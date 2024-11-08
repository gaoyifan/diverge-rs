use std::net::IpAddr;

use hickory_proto::{op, rr::rdata::name};
use hickory_resolver::{
	config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
	TokioAsyncResolver,
};
use log::*;

use crate::conf::Section;

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
