use std::{cell::RefCell, rc::Rc, time::Duration};

use log::*;
use tokio::{
	io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
	net::{TcpListener, TcpStream},
	select,
	signal::ctrl_c,
	task,
	time::timeout,
};

mod conf;
mod diverge;
mod domain_map;
mod ipset;
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

	let d = TcpListener::bind("127.0.0.1:1053")
		.await
		.or_debug("tcp bind error")?;
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
					Duration::from_secs(5)
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
) -> Option<()> {
	let mut buf = [0u8; BUF_LEN];
	let (r, w) = s.into_split();
	let mut r = BufReader::new(r);
	let w = Rc::new(RefCell::new(w));
	loop {
		if *quit.borrow() {
			debug!("tcp handle task quit");
			break;
		}
		let len = timeout(d_timeout, r.read_u16())
			.await
			.or_debug("tcp timeout while reading length")?
			.or_debug("tcp error while reading length")?;
		let _ = timeout(d_timeout, r.read_exact(&mut buf[0..len as usize]))
			.await
			.or_debug("tcp timeout while reading dns message")?
			.or_debug("tcp error while reading dns message")?;
		// pipelining
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
	let mut w = w.borrow_mut();
	w.write_u16(a.len() as u16)
		.await
		.or_debug("tcp write error")?;
	w.write_all(&a).await.or_debug("tcp write error")?;
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
