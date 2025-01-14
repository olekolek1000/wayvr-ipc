// Contents of this file should be the same as on wlx-overlay-s.

use serde::{Deserialize, Serialize};

use super::ipc::Serial;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeSuccess {
	pub runtime: String, // Runtime name, for example "wlx-overlay-s"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disconnect {
	pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrDisplayHandle {
	pub idx: u32,
	pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WvrProcessHandle {
	pub idx: u32,
	pub generation: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WvrDisplay {
	pub width: u16,
	pub height: u16,
	pub name: String,
	pub visible: bool,
	pub handle: WvrDisplayHandle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WvrDisplayList {
	pub list: Vec<WvrDisplay>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WvrProcess {
	pub name: String,
	pub display_handle: WvrDisplayHandle,
	pub handle: WvrProcessHandle,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WvrProcessList {
	pub list: Vec<WvrProcess>,
}

// "Wvr" prefixes are WayVR-specific

#[derive(Debug, Serialize, Deserialize)]
pub enum PacketServer {
	HandshakeSuccess(HandshakeSuccess),
	Disconnect(Disconnect),
	WvrDisplayCreateResponse(Serial, WvrDisplayHandle),
	WvrDisplayGetResponse(Serial, Option<WvrDisplay>),
	WvrDisplayListResponse(Serial, WvrDisplayList),
	WvrProcessLaunchResponse(Serial, Result<WvrProcessHandle, String>),
	WvrProcessListResponse(Serial, WvrProcessList),
}

impl PacketServer {
	pub fn serial(&self) -> Option<&Serial> {
		match self {
			PacketServer::HandshakeSuccess(_) => None,
			PacketServer::Disconnect(_) => None,
			PacketServer::WvrDisplayCreateResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayGetResponse(serial, _) => Some(serial),
			PacketServer::WvrDisplayListResponse(serial, _) => Some(serial),
			PacketServer::WvrProcessLaunchResponse(serial, _) => Some(serial),
			PacketServer::WvrProcessListResponse(serial, _) => Some(serial),
		}
	}
}
