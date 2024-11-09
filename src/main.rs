use std::{cell::RefCell, net::SocketAddr, rc::Rc, time::Duration};

use log::*;
use tokio::{
	io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
	net::{TcpSocket, TcpStream},
	select,
	signal::ctrl_c,
	task,
	time::timeout,
};

mod conf;
mod diverge;
mod domain_map;
mod ipmap;
mod resolver;
mod utils;

use diverge::Diverge;
use utils::OrEx;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

	// to do: setup diverge
	let diverge = Diverge::new();

	let local = task::LocalSet::new();
	local.run_until(main_ls(diverge)).await;
	local.await;
}

// the real main running in a local set
async fn main_ls(diverge: Diverge) -> Option<()> {
	let diverge = Rc::new(diverge);
	let quit = Rc::new(RefCell::new(false));

	let addr: SocketAddr = "127.0.0.1:1053".parse().unwrap();
	let s = match addr {
		SocketAddr::V4(_) => TcpSocket::new_v4().unwrap(),
		SocketAddr::V6(_) => TcpSocket::new_v6().unwrap(),
	};
	s.set_nodelay(true).unwrap();
	s.bind(addr).unwrap();

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
	let (r, w) = s.into_split();
	let mut r = BufReader::new(r);
	// RFC 7766 6.2.1.1 pipelining
	let w = Rc::new(RefCell::new(w));
	loop {
		if *quit.borrow() {
			debug!("tcp handle task quit");
			break;
		}
		let len = timeout(d_timeout, r.read_u16())
			.await
			// to do: peaceful eof handling
			.or_debug("tcp timeout while reading length")?
			.or_debug("tcp error while reading length")?;
		let _ = timeout(r_timeout, r.read_exact(&mut buf[0..len as usize]))
			.await
			.or_debug("tcp timeout while reading dns message")?
			.or_debug("tcp error while reading dns message")?;
		task::spawn_local(handle_q(
			diverge.clone(),
			w.clone(),
			(&buf[0..len as usize]).to_vec(),
		));
	}
	Some(())
}

async fn handle_q<W: AsyncWrite + Unpin>(
	diverge: Rc<Diverge>,
	w: Rc<RefCell<W>>,
	q: Vec<u8>,
) -> Option<()> {
	let a = diverge.query(q).await.or_debug("diverge error")?;
	if a.len() > 0xffff {
		error!("answer too large: {}", a.len());
		return None
	}
	// RFC 7766 8
	let mut buf = Vec::with_capacity(2 + a.len());
	buf.extend_from_slice(&(a.len() as u16).to_be_bytes()[..]);
	buf.extend_from_slice(&a);
	w.borrow_mut()
		.write_all(&buf)
		.await
		.or_debug("tcp write error")?;
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
