use std::str::FromStr;

use clap::{Parser, Subcommand};
use log::*;

use hickory_proto::{op::Message, rr::RecordType};
use hickory_resolver::{
	config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
	TokioAsyncResolver,
};
use tokio::{net::UdpSocket, select, signal::ctrl_c};

#[derive(Parser)]
struct Args {
	#[command(subcommand)]
	cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
	Q {
		server: String,

		name: String,

		#[arg(default_value = "A")]
		qtype: String,

		// hickory_resolver lookup doesn't support qclass anyway
		#[arg(default_value = "IN")]
		qclass: String,
	},
	P {
		listen: String,
		origin: String,
	},
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let args = Args::parse();

	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

	match args.cmd {
		Cmd::Q {
			server,
			name,
			qtype,
			qclass,
		} => {
			query(&server, &name, &qtype, &qclass).await;
		}
		Cmd::P { listen, origin } => {
			proxy(&listen, &origin).await;
		}
	}
}

async fn proxy(listen: &str, origin: &str) {
	info!("listening on {}", listen);
	info!("origin {}", origin);

	let l = UdpSocket::bind(listen).await.unwrap();
	let o = UdpSocket::bind("0.0.0.0:0").await.unwrap();
	o.connect(origin).await.unwrap();

	let mut last_client = None;
	let mut l_buf = vec![0u8; 0x600];
	let mut o_buf = vec![0u8; 0x600];

	loop {
		select! {
			r = l.recv_from(&mut l_buf) => {
				match r {
					Ok((len, addr)) => {
						debug!("udp recv {} bytes from {}", len, addr);
						let msg = Message::from_vec(&l_buf[0..len]).unwrap();
						info!("=== from {} ===\n{}", addr, msg);
						last_client = Some(addr);
						o.send(&l_buf[0..len]).await.unwrap();
					}
					Err(e) => {
						error!("udp recv error: {}", e);
						break;
					}
				}
			}
			r = o.recv(&mut o_buf) => {
				match r {
					Ok(len) => {
						debug!("udp recv {} bytes from origin", len);
						let msg = Message::from_vec(&o_buf[0..len]).unwrap();
						info!("=== from origin ===\n{}", msg);
						if let Some(client) = &last_client {
							if let Err(e) = l.send_to(&o_buf[0..len], client).await {
								error!("udp send error: {}", e);
							}
						}
					}
					Err(e) => {
						error!("udp recv error from origin: {}", e);
						break;
					}
				}
			}
			_ = ctrl_c() => {
				info!( "ctrl-c received, exit" );
				break;
			}
		}
	}
}

async fn query(server: &str, name: &str, qtype: &str, _qclass: &str) {
	let r = resolver(server);

	let resp = r
		.lookup(name, RecordType::from_str(qtype).unwrap())
		.await
		.unwrap();
	for a in resp {
		println!("{:?}", a);
	}
}

pub fn resolver(addr: &str) -> TokioAsyncResolver {
	let mut conf = ResolverConfig::new();
	conf.add_name_server(NameServerConfig {
		socket_addr: addr.parse().unwrap(),
		protocol: Protocol::Udp,
		tls_dns_name: None,
		trust_negative_responses: false,
		tls_config: None,
		bind_addr: None,
	});
	let mut opts = ResolverOpts::default();
	opts.use_hosts_file = false;
	opts.edns0 = true;
	TokioAsyncResolver::tokio(conf, opts)
}
