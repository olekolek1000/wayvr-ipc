use bytes::BufMut;
use interprocess::local_socket::{
	self,
	tokio::{prelude::*, Stream},
	GenericNamespaced,
};
use smallvec::SmallVec;
use std::sync::{Arc, Weak};
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use crate::{
	gen_id,
	ipc::{self, binary_decode, binary_encode, Serial},
	packet_client::{self, PacketClient},
	packet_server::{self, PacketServer},
	util::notifier::Notifier,
};

pub struct QueuedPacket {
	notifier: Notifier,
	serial: Serial,
	packet: Option<PacketServer>,
}

gen_id!(
	QueuedPacketVec,
	QueuedPacket,
	QueuedPacketCell,
	QueuedPacketHandle
);

pub struct WayVRClient {
	receiver: ReceiverMutex,
	sender: SenderMutex,
	cancel_token: CancellationToken,
	exiting: bool,
	queued_packets: QueuedPacketVec,
}

pub async fn send_packet(sender: &SenderMutex, data: &[u8]) -> anyhow::Result<()> {
	let mut bytes = bytes::BytesMut::new();

	// packet size
	bytes.put_u32(data.len() as u32);

	// packet data
	bytes.put_slice(data);

	sender.lock().await.write_all(&bytes).await?;

	Ok(())
}

async fn send_handshake(sender: &SenderMutex) -> anyhow::Result<()> {
	let handshake = ipc::Handshake::new();
	send_packet(sender, &binary_encode(&handshake)).await?;

	Ok(())
}

pub type WayVRClientMutex = Arc<Mutex<WayVRClient>>;
pub type WayVRClientWeak = Weak<Mutex<WayVRClient>>;

type ReceiverMutex = Arc<Mutex<local_socket::tokio::RecvHalf>>;
type SenderMutex = Arc<Mutex<local_socket::tokio::SendHalf>>;

async fn client_runner(client: WayVRClientMutex) -> anyhow::Result<()> {
	loop {
		WayVRClient::tick(client.clone()).await?;
	}
}

type Payload = SmallVec<[u8; 64]>;

async fn read_payload(
	conn: &mut local_socket::tokio::RecvHalf,
	size: u32,
) -> anyhow::Result<Payload> {
	let mut payload = Payload::new();
	payload.resize(size as usize, 0);
	conn.read_exact(&mut payload).await?;
	Ok(payload)
}

macro_rules! bail_unexpected_response {
	() => {
		anyhow::bail!("unexpected response");
	};
}

impl WayVRClient {
	pub async fn new() -> anyhow::Result<WayVRClientMutex> {
		let printname = "/tmp/wayvr_ipc.sock";
		let name = printname.to_ns_name::<GenericNamespaced>()?;

		let stream = match Stream::connect(name).await {
			Ok(c) => c,
			Err(e) => {
				anyhow::bail!("Failed to connect to the WayVR IPC: {}. Make sure you have wlx-overlay-s already running. Check README at https://github.com/olekolek1000/wayvr-dashboard", e)
			}
		};
		let (receiver, sender) = stream.split();

		let receiver = Arc::new(Mutex::new(receiver));
		let sender = Arc::new(Mutex::new(sender));

		send_handshake(&sender).await?;

		let cancel_token = CancellationToken::new();

		let client = Arc::new(Mutex::new(Self {
			receiver,
			sender,
			exiting: false,
			cancel_token: cancel_token.clone(),
			queued_packets: QueuedPacketVec::new(),
		}));

		WayVRClient::start_runner(client.clone(), cancel_token);

		Ok(client)
	}

	fn start_runner(client: WayVRClientMutex, cancel_token: CancellationToken) {
		tokio::spawn(async move {
			loop {
				tokio::select! {
						_ = cancel_token.cancelled() => {
								log::info!("Exiting WayVRClient runner");
								break;
						}
						_ = client_runner(client.clone()) => {
								log::info!("start_runner select failed");
						}
				}
			}
		});
	}

	async fn tick(client_mtx: WayVRClientMutex) -> anyhow::Result<()> {
		let receiver = {
			let client = client_mtx.lock().await;
			client.receiver.clone()
		};

		// read packet
		let packet = {
			let mut receiver = receiver.lock().await;
			let packet_size = receiver.read_u32().await?;
			log::trace!("packet size {}", packet_size);
			if packet_size > 128 * 1024 {
				anyhow::bail!("packet size too large");
			}
			let payload = read_payload(&mut receiver, packet_size).await?;
			let packet: PacketServer = binary_decode(&payload)?;
			packet
		};

		{
			let mut client = client_mtx.lock().await;
			// queue packet to read if it contains a serial response
			if let Some(serial) = packet.serial() {
				for qpacket in &mut client.queued_packets.vec {
					let Some(qpacket) = qpacket else {
						continue;
					};

					let qpacket = &mut qpacket.obj;
					if qpacket.serial != *serial {
						continue; //skip
					}

					// found response serial, fill it and notify the receiver
					qpacket.packet = Some(packet);
					let notifier = qpacket.notifier.clone();

					drop(client);
					notifier.notify();
					break;
				}
			}
		}

		Ok(())
	}

	// Send packet without feedback
	async fn send_payload(client_mtx: WayVRClientMutex, payload: &[u8]) -> anyhow::Result<()> {
		let client = client_mtx.lock().await;
		let sender = client.sender.clone();
		drop(client);
		send_packet(&sender, payload).await?;
		Ok(())
	}

	async fn queue_wait_packet(
		client_mtx: WayVRClientMutex,
		serial: Serial,
		payload: &[u8],
	) -> anyhow::Result<PacketServer> {
		let notifier = Notifier::new();

		// Send packet to the server
		let queued_packet_handle = {
			let mut client = client_mtx.lock().await;
			let handle = client.queued_packets.add(QueuedPacket {
				notifier: notifier.clone(),
				packet: None, // will be filled after notify
				serial,
			});

			let sender = client.sender.clone();

			drop(client);

			send_packet(&sender, payload).await?;
			handle
		};

		// Wait for response message
		notifier.wait().await;

		// Fetch response packet
		{
			let mut client = client_mtx.lock().await;

			let cell = client
				.queued_packets
				.get_mut(&queued_packet_handle)
				.ok_or(anyhow::anyhow!(
					"missing packet cell, this shouldn't happen"
				))?;

			let Some(packet) = cell.packet.take() else {
				anyhow::bail!("packet is None, this shouldn't happen");
			};

			client.queued_packets.remove(&queued_packet_handle);

			Ok(packet)
		}
	}

	pub async fn fn_wvr_display_list(
		client_mtx: WayVRClientMutex,
		serial: Serial,
	) -> anyhow::Result<Vec<packet_server::WvrDisplay>> {
		let PacketServer::WvrDisplayListResponse(_, display_list) = WayVRClient::queue_wait_packet(
			client_mtx,
			serial,
			&binary_encode(&PacketClient::WvrDisplayList(serial)),
		)
		.await?
		else {
			bail_unexpected_response!();
		};
		Ok(display_list.list)
	}

	pub async fn fn_wvr_display_get(
		client_mtx: WayVRClientMutex,
		serial: Serial,
		handle: packet_server::WvrDisplayHandle,
	) -> anyhow::Result<Option<packet_server::WvrDisplay>> {
		let PacketServer::WvrDisplayGetResponse(_, display) = WayVRClient::queue_wait_packet(
			client_mtx,
			serial,
			&binary_encode(&PacketClient::WvrDisplayGet(serial, handle)),
		)
		.await?
		else {
			bail_unexpected_response!();
		};
		Ok(display)
	}

	pub async fn fn_wvr_display_create(
		client_mtx: WayVRClientMutex,
		serial: Serial,
		params: packet_client::WvrDisplayCreateParams,
	) -> anyhow::Result<packet_server::WvrDisplayHandle> {
		let PacketServer::WvrDisplayCreateResponse(_, handle) = WayVRClient::queue_wait_packet(
			client_mtx,
			serial,
			&binary_encode(&PacketClient::WvrDisplayCreate(serial, params)),
		)
		.await?
		else {
			bail_unexpected_response!();
		};
		Ok(handle)
	}

	pub async fn fn_wvr_process_list(
		client_mtx: WayVRClientMutex,
		serial: Serial,
	) -> anyhow::Result<Vec<packet_server::WvrProcess>> {
		let PacketServer::WvrProcessListResponse(_, process_list) = WayVRClient::queue_wait_packet(
			client_mtx,
			serial,
			&binary_encode(&PacketClient::WvrProcessList(serial)),
		)
		.await?
		else {
			bail_unexpected_response!();
		};

		Ok(process_list.list)
	}

	pub async fn fn_wvr_process_terminate(
		client_mtx: WayVRClientMutex,
		handle: packet_server::WvrProcessHandle,
	) -> anyhow::Result<()> {
		WayVRClient::send_payload(
			client_mtx,
			&binary_encode(&PacketClient::WvrProcessTerminate(handle)),
		)
		.await?;
		Ok(())
	}

	pub async fn fn_wvr_process_launch(
		client_mtx: WayVRClientMutex,
		serial: Serial,
		params: packet_client::WvrProcessLaunchParams,
	) -> anyhow::Result<packet_server::WvrProcessHandle> {
		let PacketServer::WvrProcessLaunchResponse(_, handle) = WayVRClient::queue_wait_packet(
			client_mtx,
			serial,
			&binary_encode(&PacketClient::WvrProcessLaunch(serial, params)),
		)
		.await?
		else {
			bail_unexpected_response!();
		};

		Ok(handle.map_err(|e| anyhow::anyhow!("{}", e))?)
	}
}

impl Drop for WayVRClient {
	fn drop(&mut self) {
		self.exiting = true;
		self.cancel_token.cancel();
	}
}
