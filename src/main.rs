use std::net::SocketAddr;

// use deku::prelude::*;
use hickory_proto::{
	op::{Message, Query},
	rr::Name,
};
use log::*;
use tokio::{net::UdpSocket, select};

mod ipset;
use ipset::IpSet;

struct Upstream {
	name: String,
	// resolver: AsyncResolver<GenericConnector<TokioRuntimeProvider>>,
	ipset: Option<IpSet>,
}

struct Diverge(Vec<Upstream>);

impl Diverge {
	fn new() -> Self {
		Self(Vec::new())
	}
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

	let d = UdpSocket::bind("127.0.0.1:1053").await.unwrap();
	info!("downstream listening on {}", d.local_addr().unwrap());

	let u = UdpSocket::bind("0.0.0.0:0").await.unwrap();
	u.connect("1.1.1.1:53").await.unwrap();
	info!(
		"upstream connected to {} from {}",
		u.peer_addr().unwrap(),
		u.local_addr().unwrap()
	);

	let mut u_buf = [0u8; 4096];
	let mut d_buf = [0u8; 4096];
	let mut last_client: Option<SocketAddr> = None;

	loop {
		select! {
			r = u.recv(&mut u_buf) => {
				match r {
					Ok(len) => {
						trace!("{} bytes from upstream", len);
						// dump u_buf to a file
						std::fs::write("dump.bin", &u_buf[0..len]).unwrap();
						let msg = Message::from_vec(&u_buf[0..len]).unwrap();
						trace!("DNS Message: {:?}", msg);
						if let Some(last_client) = last_client {
							d.send_to(&u_buf[0..len], last_client).await.unwrap();
						}
					}
					Err(e) => {
						error!("recv error: {:?}", e);
					}
				}
			}
			r = d.recv_from(&mut d_buf) => {
				match r {
					Ok((len, src)) => {
						trace!("{} bytes from {}", len, src);
						last_client = Some(src);
						let msg = Message::from_vec(&d_buf[0..len]).unwrap();
						trace!("DNS Message: {:?}", msg);
						u.send(&d_buf[0..len]).await.unwrap();
					}
					Err(e) => {
						error!("recv error: {:?}", e);
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use hickory_proto::op::Message;

	#[test]
	fn it_works() {
		let mut msg = Message::new();
		// msg.add_answer(records);
	}
}
