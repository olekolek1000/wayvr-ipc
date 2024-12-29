pub mod ipc;
pub mod packet_client;
pub mod packet_server;
mod util;

#[cfg(feature = "client")]
pub mod client;

#[cfg(test)]
mod tests {

	#[test]
	fn it_works() {
		assert_eq!(4, 4);
	}
}
