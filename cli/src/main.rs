use std::{
	str::FromStr,
	time::{Duration, Instant},
};

use clap::Parser;
use hickory_proto::{
	op::{Header, Message, MessageType, OpCode, Query},
	rr::RecordType,
};
use log::*;
use tokio::{net::UdpSocket, select, signal::ctrl_c, time::sleep};

use diverge::{conf, dohc::Dohc, resolver};

mod args;
use args::*;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let args = CliArgs::parse();

	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

	match args.cmd {
		Cmd::Query(args) => {
			if &args.proto == "dohc" {
				dohc_query(args).await;
			} else {
				query(args).await;
			}
		}
		Cmd::Proxy { listen, origin } => {
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
						info!("udp recv {} bytes from {}", len, addr);
						let msg = Message::from_vec(&l_buf[0..len]).unwrap();
						debug!("=== from {} ===\n{}", addr, msg);
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
						info!("udp recv {} bytes from origin", len);
						let msg = Message::from_vec(&o_buf[0..len]).unwrap();
						debug!("=== from origin ===\n{}", msg);
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

async fn query(args: QArgs) {
	let r = resolver::from(&conf::UpstreamSec {
		name: "".to_string(),
		addrs: vec![args.server.parse().unwrap()],
		protocol: conf::parse_proto(&args.proto),
		port: args.port,
		tls_dns_name: args.tls_dns_name.clone(),
		disable_aaaa: false,
		ips: vec![],
		domains: vec![],
	});

	let mut intv = args.interval;
	for i in 0..args.repeat {
		let t0 = Instant::now();
		let resp = r
			.lookup(&args.name, RecordType::from_str(&args.qtype).unwrap())
			.await
			.unwrap();
		let cost = t0.elapsed().as_secs_f32();
		info!(
			"lookup {} {} cost {:.3} ms",
			args.name,
			args.qtype,
			cost * 1000.0
		);
		for a in resp {
			trace!("{:?}", a);
		}
		if i < args.repeat - 1 && intv > 0. {
			info!("sleep for {:.02} seconds", intv);
			sleep(Duration::from_secs_f32(intv)).await;
			intv *= args.backoff;
		}
	}
}

async fn dohc_query(args: QArgs) {
	let dohc = Dohc::new(
		args.tls_dns_name.unwrap_or(args.server.clone()),
		vec![args.server.parse().unwrap()],
		args.port,
		args.proxy,
	);

	let mut h = Header::new();
	h.set_message_type(MessageType::Query);
	h.set_op_code(OpCode::Query);
	h.set_recursion_desired(true);

	let mut q = Query::new();
	q.set_name(args.name.parse().unwrap());
	q.set_query_type(RecordType::from_str(&args.qtype).unwrap());

	let mut query = Message::new();
	query.add_query(q);

	let mut intv = args.interval;
	for i in 0..args.repeat {
		h.set_id(((0x2a01 + i) & 0xffff) as u16);
		query.set_header(h);
		let qbuf = query.to_vec().unwrap();

		let t0 = Instant::now();
		let resp = dohc.exchange(qbuf).await.unwrap();
		info!(
			"lookup {} {} cost {:.02} ms",
			args.name,
			args.qtype,
			t0.elapsed().as_secs_f32() * 1000.0
		);
		trace!("{:?}", Message::from_vec(&resp).unwrap());
		if i < args.repeat - 1 && intv > 0. {
			info!("sleep for {:.02} seconds", intv);
			sleep(Duration::from_secs_f32(intv)).await;
			intv *= args.backoff;
		}
	}
}
