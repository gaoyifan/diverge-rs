// Dual Overhead Camshaft, oops, I mean, DNS over HTTPS Client
// based on reqwest, has the benefit of supporting proxies

use std::{
	net::{IpAddr, SocketAddr},
	time::Duration,
};

use bytes::Bytes;
use log::*;
use reqwest::{header, tls::Version, Client, Proxy, Url};

pub struct Dohc {
	url: Url,
	reqwest_client: Client,
}

impl Dohc {
	pub fn new(
		host: impl AsRef<str>,
		addrs: impl AsRef<[IpAddr]>,
		port: Option<u16>,
		proxy: Option<impl AsRef<str>>,
	) -> Self {
		let mut headers = header::HeaderMap::new();
		headers.insert(
			header::CONTENT_TYPE,
			"application/dns-message".parse().unwrap(),
		);

		let mut url = Url::parse(&format!("https://{}/dns-query", host.as_ref())).unwrap();
		url.set_port(port).unwrap();

		let mut b = Client::builder()
			.default_headers(headers)
			.resolve_to_addrs(
				host.as_ref(),
				&addrs
					.as_ref()
					.iter()
					.map(|a| SocketAddr::new(*a, 0))
					.collect::<Vec<SocketAddr>>(),
			)
			.pool_idle_timeout(Some(Duration::from_secs(2501)))
			.pool_max_idle_per_host(1)
			.tcp_keepalive(Some(Duration::from_secs_f32(25.01)))
			.min_tls_version(Version::TLS_1_2)
			.http2_prior_knowledge()
			.http2_keep_alive_interval(Some(Duration::from_secs_f32(25.01)))
			.http2_keep_alive_timeout(Duration::from_secs_f32(2.501))
			.http2_keep_alive_while_idle(true);

		if let Some(proxy) = proxy {
			b = b.proxy(Proxy::all(proxy.as_ref()).unwrap());
		}

		Self {
			url,
			reqwest_client: b.build().unwrap(),
		}
	}

	pub async fn exchange(&self, msg: Vec<u8>) -> reqwest::Result<Bytes> {
		let res = self
			.reqwest_client
			.post(self.url.clone())
			.body(msg)
			.send()
			.await?;
		trace!("reqwest: {}", res.status());
		res.bytes().await
	}
}
