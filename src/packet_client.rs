// Contents of this file should be the same as on wlx-overlay-s.

use serde::{Deserialize, Serialize};

use super::{ipc::Serial, packet_server};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Handshake {
	pub protocol_version: u32, // always set to PROTOCOL_VERSION
	pub magic: String,         // always set to CONNECTION_MAGIC
	pub client_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum AttachTo {
	None,
	HandLeft,
	HandRight,
	Head,
	Stage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WvrProcessLaunchParams {
	pub name: String,
	pub exec: String,
	pub target_display: packet_server::WvrDisplayHandle,
	pub env: Vec<String>,
	pub args: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WvrDisplayCreateParams {
	pub width: u16,
	pub height: u16,
	pub name: String,
	pub scale: Option<f32>,
	pub attach_to: AttachTo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WlxHapticsParams {
	pub intensity: f32,
	pub duration: f32,
	pub frequency: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PacketClient {
	Handshake(Handshake),
	WvrDisplayCreate(Serial, WvrDisplayCreateParams),
	WvrDisplayGet(Serial, packet_server::WvrDisplayHandle),
	WvrDisplayList(Serial),
	WvrDisplayRemove(Serial, packet_server::WvrDisplayHandle),
	WvrDisplaySetVisible(packet_server::WvrDisplayHandle, bool),
	WvrProcessLaunch(Serial, WvrProcessLaunchParams),
	WvrProcessList(Serial),
	WvrProcessTerminate(packet_server::WvrProcessHandle),
	WlxHaptics(WlxHapticsParams),
}
