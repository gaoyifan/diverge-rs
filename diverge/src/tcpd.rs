// this is deprecated, since support for DNS over TCP is so bad

pub async fn tcpd(listen: SocketAddr, diverge: Diverge) -> Option<()> {
	let diverge = Rc::new(diverge);
	let quit = Rc::new(Cell::new(false));

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
				let (socket, addr) = d.map_err(|e| error!("tcp accept error: {}", e)).ok()?;
				debug!("new connection from {}", addr);
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
				break;
			}
		}
	}
	quit.set(true);
	Some(())
}

async fn handle_conn(
	diverge: Rc<Diverge>,
	s: TcpStream,
	quit: Rc<Cell<bool>>,
	d_timeout: Duration,
	r_timeout: Duration,
) -> Option<()> {
	let (mut r, mut w) = s.into_split();

	// spawn a task to handle writing with a channel
	let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1);
	task::spawn_local(async move {
		// RFC 7766 8 says we SHOULD pass them in a single write
		let mut buf = Vec::with_capacity(0x1000);
		while let Some(msg) = rx.recv().await {
			buf.truncate(0);
			if msg.len() > u16::MAX as usize {
				error!("message too large: {}", msg.len());
				continue;
			}
			buf.extend_from_slice(&(msg.len() as u16).to_be_bytes()[..]);
			buf.extend_from_slice(&msg);
			match w.write_all(&buf).await {
				Ok(_) => trace!("client write task wrote {} bytes", buf.len()),
				Err(e) => trace!("client write task write error: {}", e),
			}
		}
		trace!("client write task ended")
	});

	// read client requests
	let mut buf = vec![0u8; 0x1000];
	loop {
		if quit.get() {
			debug!("tcp handle task quit");
			break;
		}
		let len = match timeout(d_timeout, r.read_u16()).await {
			Ok(Ok(len)) => len,
			Err(_) => {
				info!("tcp timeout while waiting client request, connection closed");
				break;
			}
			Ok(Err(e)) if e.kind() == ErrorKind::UnexpectedEof => {
				debug!("tcp eof, client closed");
				break;
			}
			Ok(Err(e)) => {
				warn!("tcp error while waiting client request: {}", e);
				return None;
			}
		};
		if buf.len() < len as usize {
			buf.resize(align_to(len as usize, 0x1000), 0);
		}
		match timeout(r_timeout, r.read_exact(&mut buf[0..len as usize])).await {
			Err(_) => {
				debug!("tcp timeout while reading dns request");
				break;
			}
			Ok(Err(e)) => {
				debug!("tcp error while reading dns request: {}", e);
			}
			Ok(Ok(_)) => {}
		}
		// RFC 7766 6.2.1.1 pipelining
		let diverge = diverge.clone();
		let tx = tx.clone();
		let buf = buf[0..len as usize].to_vec();
		task::spawn_local(async move {
			if let Some(a) = diverge.query(buf).await {
				if tx.send(a).await.is_err() {
					debug!("channel write error");
				}
			} else {
				debug!("diverge error");
			}
		});
	}
	Some(())
}
