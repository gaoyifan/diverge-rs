use std::{net::SocketAddr, rc::Rc};

use log::*;
use tokio::{net::UdpSocket, select, signal::ctrl_c, task};

use crate::diverge::Diverge;

pub async fn udpd(listen: SocketAddr, diverge: Diverge) {
	let diverge = Rc::new(diverge);

	let s = Rc::new(UdpSocket::bind(listen).await.unwrap());
	info!("listening on UDP {}", s.local_addr().unwrap());

	let mut buf = vec![0u8; 0x600];
	loop {
		select! {
			r = s.recv_from(&mut buf) => {
				match r {
					Ok((len, addr)) => {
						trace!("udp recv {} bytes from {}", len, addr);
						let diverge = diverge.clone();
						let w = s.clone();
						let buf = buf[0..len].to_vec();
						task::spawn_local(async move {
							if let Some(a) = diverge.query(buf).await {
								if let Err(e) = w.send_to(&a, addr).await {
									error!("udp send error: {}", e);
								}
							} else {
								error!("diverge error");
							}
						});
					}
					Err(e) => {
						error!("udp recv error: {}", e);
						break;
					}
				}
			}
			_ = ctrl_c() => {
				info!("ctrl-c received, exiting");
				break;
			}
		}
	}
}
