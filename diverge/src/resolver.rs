use std::net::SocketAddr;

use hickory_resolver::{
	config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
	TokioAsyncResolver,
};

use crate::conf::UpstreamSec;

fn default_port(protocol: Protocol) -> u16 {
	match protocol {
		Protocol::Udp => 53,
		Protocol::Tcp => 53,
		Protocol::Tls => 853,
		_ => panic!("unsupported protocol: {}", protocol),
	}
}

pub fn from(conf: &UpstreamSec) -> TokioAsyncResolver {
	let mut config = ResolverConfig::new();
	let port = conf.port.unwrap_or(default_port(conf.protocol));
	for addr in &conf.addrs {
		config.add_name_server(NameServerConfig {
			socket_addr: SocketAddr::new(*addr, port),
			protocol: conf.protocol,
			tls_dns_name: if conf.protocol == Protocol::Tls {
				Some(addr.to_string())
			} else {
				None
			},
			trust_negative_responses: conf.protocol == Protocol::Tls,
			tls_config: None,
			bind_addr: None,
		});
	}

	let mut opts = ResolverOpts::default();
	// default 5 seconds
	// opts.timeout = std::time::Duration::from_secs(5);
	// default 2
	// opts.attempts = 2;
	// default 32
	// opts.cache_size = 32;
	// default true
	opts.use_hosts_file = false;
	// default 2
	// opts.num_concurrent_reqs = 2;
	// default false
	opts.edns0 = true;

	TokioAsyncResolver::tokio(config, opts)
}

#[cfg(test)]
mod tests {
	use super::*;
	use hickory_proto::rr::RecordType;

	#[tokio::test]
	async fn test() {
		let r = from(&UpstreamSec {
			name: "".to_string(),
			protocol: Protocol::Tls,
			addrs: vec!["1.1.1.1".parse().unwrap()],
			port: None,
			ips: vec![],
			domains: vec![],
			disable_aaaa: false,
		});
		let resp = r.lookup("www.example.com", RecordType::A).await.unwrap();
		for a in resp {
			println!("{:?}", a);
		}
	}
}
