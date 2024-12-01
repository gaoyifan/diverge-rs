use std::{cell::RefCell, io::ErrorKind, net::SocketAddr, rc::Rc, time::Duration};

use conf::DivergeConf;
use log::*;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt, BufReader},
	net::{TcpSocket, TcpStream},
	select,
	signal::ctrl_c,
	sync::mpsc::{self, Sender},
	task,
	time::timeout,
};

mod conf;
mod diverge;
mod domain_map;
mod ip_map;
mod resolver;
mod utils;

use diverge::Diverge;
use utils::OrEx;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

	#[cfg(debug_assertions)]
	let default_conf = "tests/test.conf".to_string();
	#[cfg(not(debug_assertions))]
	let default_conf = "diverge.conf".to_string();

	let conf_fn = if std::env::args().len() < 2 {
		default_conf
	} else {
		std::env::args().nth(1).unwrap()
	};

	info!("read config from {}", &conf_fn);
	let conf_str = std::fs::read_to_string(conf_fn).unwrap();
	let conf: DivergeConf = conf_str.parse().unwrap();

	let diverge = Diverge::from(&conf);

	let local = task::LocalSet::new();
	local.run_until(main_ls(conf.global.listen, diverge)).await;
	local.await;
}

// the real main running in a local set
async fn main_ls(listen: SocketAddr, diverge: Diverge) -> Option<()> {
	let diverge = Rc::new(diverge);
	let quit = Rc::new(RefCell::new(false));

	let s = match listen {
		SocketAddr::V4(_) => TcpSocket::new_v4().unwrap(),
		SocketAddr::V6(_) => TcpSocket::new_v6().unwrap(),
	};
	s.set_nodelay(true).unwrap();
	s.bind(listen).unwrap();

	let d = s.listen(64).unwrap();
	info!("listening on {}", d.local_addr().unwrap());

	loop {
		select! {
			d = d.accept() => {
				let (socket, addr) = d.or_debug("tcp accept error")?;
				info!("new connection from {}", addr);
				let _ = task::spawn_local(handle_conn(
					diverge.clone(),
					socket,
					quit.clone(),
					// RFC 1035 4.2.2 recommends 120s
					Duration::from_secs(120),
					Duration::from_secs(7)
				)).await;
			}
			_ = ctrl_c() => {
				info!("ctrl-c received, exiting");
				*quit.borrow_mut() = true;
				break;
			}
		}
	}
	Some(())
}

const BUF_LEN: usize = 0x10000;

async fn handle_conn(
	diverge: Rc<Diverge>,
	s: TcpStream,
	quit: Rc<RefCell<bool>>,
	d_timeout: Duration,
	r_timeout: Duration,
) -> Option<()> {
	let mut buf = [0u8; BUF_LEN];
	let (r, mut w) = s.into_split();

	// spawn a task to handle writing with a channel
	let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1);
	task::spawn_local(async move {
		while let Some(msg) = rx.recv().await {
			let _ = w.write_all(&msg).await.or_debug("tcp write error");
		}
	});

	// read client requests
	let mut r = BufReader::new(r);
	loop {
		if *quit.borrow() {
			debug!("tcp handle task quit");
			break;
		}
		let len = match timeout(d_timeout, r.read_u16())
			.await
			.or_info("tcp timeout while waiting client request, connection closed")?
		{
			Ok(len) => len,
			Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
				info!("tcp eof, client closed");
				break;
			}
			Err(e) => {
				warn!("tcp error while waiting client request: {}", e);
				return None;
			}
		};
		let _ = timeout(r_timeout, r.read_exact(&mut buf[0..len as usize]))
			.await
			.or_debug("tcp timeout while reading dns message")?
			.or_debug("tcp error while reading dns message")?;
		// RFC 7766 6.2.1.1 pipelining
		task::spawn_local(handle_q(
			diverge.clone(),
			tx.clone(),
			buf[0..len as usize].to_vec(),
		));
	}
	Some(())
}

async fn handle_q(diverge: Rc<Diverge>, w: Sender<Vec<u8>>, q: Vec<u8>) -> Option<()> {
	let a = diverge.query(q).await.or_debug("diverge error")?;
	if a.len() > 0xffff {
		error!("answer too large: {}", a.len());
		return None;
	}
	// RFC 7766 8
	let mut buf = Vec::with_capacity(2 + a.len());
	buf.extend_from_slice(&(a.len() as u16).to_be_bytes()[..]);
	buf.extend_from_slice(&a);

	w.send(buf).await.or_debug("channel write error")?;
	Some(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use hickory_proto::op::Message;

	#[test]
	fn sizes() {
		println!("Duration: {}", std::mem::size_of::<std::time::Duration>());
		println!("Message: {}", std::mem::size_of::<Message>());
		println!(
			"TokioAsyncResolver: {}",
			std::mem::size_of::<hickory_resolver::TokioAsyncResolver>()
		);
		println!("Diverge: {}", std::mem::size_of::<Diverge>());
	}
}
