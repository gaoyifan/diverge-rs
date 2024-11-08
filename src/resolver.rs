use std::net::IpAddr;

use hickory_proto::{op, rr::rdata::name};
use hickory_resolver::{
	config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
	TokioAsyncResolver,
};
use log::*;

use crate::conf::Section;

struct ResolverConf {
	name: String,
	protocol: Protocol,
	addrs: Vec<IpAddr>,
	port: Option<u16>,
	ips: Vec<String>,
	domains: Vec<String>,
}

impl Section for ResolverConf {
	fn new(name: &str) -> Self {
		Self {
			name: name.to_string(),
			protocol: Protocol::Tcp,
			addrs: Vec::new(),
			port: None,
			ips: Vec::new(),
			domains: Vec::new(),
		}
	}

	fn set(&mut self, k: &str, v: &str) {
		match k.to_ascii_lowercase().as_str() {
			"addresses" => {
				self.addrs = v
					.split(' ')
					.filter_map(|e| {
						let e = e.trim_ascii();
						if e.is_empty() {
							return None;
						}
						match e.parse() {
							Ok(e) => Some(e),
							Err(e) => panic!("invalid address: {}", e),
						}
					})
					.collect();
			}
			"protocol" => match v.to_ascii_lowercase().as_str() {
				"tcp" => self.protocol = Protocol::Tcp,
				"tls" => self.protocol = Protocol::Tls,
				_ => panic!("unsupported protocol: {}", v),
			},
			"port" => match v.parse() {
				Ok(v) => self.port = Some(v),
				Err(e) => panic!("invalid port {}: {}", v, e),
			},
			"ips" => self.ips = v.split_ascii_whitespace().map(|s| s.to_string()).collect(),
			"domains" => self.domains = v.split_ascii_whitespace().map(|s| s.to_string()).collect(),
			_ => warn!("unknown key: {}", k),
		}
	}
}

fn default_port(protocol: Protocol) -> u16 {
	match protocol {
		Protocol::Tcp => 53,
		Protocol::Tls => 853,
		_ => panic!("unsupported protocol: {}", protocol),
	}
}

pub fn from(url: &str) -> TokioAsyncResolver {
	let mut config = ResolverConfig::new();

	config.add_name_server(NameServerConfig {
		socket_addr: "1.1.1.1:853".parse().unwrap(),
		protocol: Protocol::Tls,
		// tls_dns_name: Some("cloudflare-dns.com".to_string()),
		tls_dns_name: Some("1.1.1.1".to_string()),
		// tls_dns_name: None,
		trust_negative_responses: true,
		tls_config: None,
		bind_addr: None,
	});

	let mut opts = ResolverOpts::default();
	// default 5 seconds
	opts.timeout = std::time::Duration::from_secs(5);
	// default 2
	opts.attempts = 2;
	// default 32
	opts.cache_size = 32;
	// default true
	opts.use_hosts_file = false;
	// default 2
	opts.num_concurrent_reqs = 2;

	TokioAsyncResolver::tokio(config, opts)
}

#[cfg(test)]
mod tests {
	// use hickory_proto::rr::RecordType;
	use super::*;

	#[tokio::test]
	async fn test() {
		// let r = TokioAsyncResolver::tokio(ResolverConfig::cloudflare_tls(), ResolverOpts::default());
		// let r = TokioAsyncResolver::tokio(ResolverConfig::google_tls(), ResolverOpts::default());
		let r = from("");
		let resp = r.lookup_ip("www.google.com").await.unwrap();
		for a in resp {
			println!("{:?}", a);
		}
	}

	#[tokio::test]
	async fn test2() {
	}
}
