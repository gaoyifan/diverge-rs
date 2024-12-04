// yes I wrote my own conf parser
//	since I want:
//		warn on unknown keys
//		keep section order
//	and it's simple enough
//		at least I thought it would be
//	well it's a good practice, I think

#[cfg(debug_assertions)]
use std::fmt::Debug;
use std::str::FromStr;

use log::warn;

// this is the part that's generic

pub trait Section {
	fn set(&mut self, k: &str, v: &str);
}

pub trait Conf: Sized {
	fn new() -> Self;
	fn sec_mut(&mut self, name: &str) -> &mut dyn Section;
	// I'd very much like to impl FromStr for Conf, but that's not allowed
	fn from_str(conf: &str) -> Self {
		let mut ret = Self::new();
		let mut sec = None;
		for l in conf.lines() {
			let l = l.trim_ascii();
			if l.is_empty() || l.starts_with('#') {
				// empty line or comment
			} else if l.starts_with("[") && l.ends_with("]") {
				// section name
				let name = l[1..l.len() - 1].trim_ascii();
				sec = Some(ret.sec_mut(name));
				continue;
			} else if let Some(sec) = sec.as_mut() {
				// k = v
				match l.split_once('=') {
					None => panic!("invalid line: {}", l),
					Some((k, v)) => {
						sec.set(k.trim_ascii_end(), v.trim_ascii_start());
					}
				}
			} else {
				warn!("invalid line, not in a section: {}", l);
			}
		}
		ret
	}
}

// the following is specific to diverge's conf

use std::net::{IpAddr, SocketAddr};

use hickory_resolver::config::Protocol;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct DivergeConf {
	pub global: GlobalSec,
	pub upstreams: Vec<UpstreamSec>,
}

impl Conf for DivergeConf {
	fn new() -> Self {
		Self {
			global: GlobalSec::new(),
			upstreams: Vec::new(),
		}
	}
	fn sec_mut(&mut self, name: &str) -> &mut dyn Section {
		match name {
			"global" => &mut self.global,
			_ => {
				self.upstreams.push(UpstreamSec::new(name));
				let len = self.upstreams.len();
				&mut self.upstreams[len - 1]
			}
		}
	}
}

impl FromStr for DivergeConf {
	type Err = ();
	fn from_str(conf: &str) -> Result<Self, Self::Err> {
		Ok(<Self as Conf>::from_str(conf))
	}
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct GlobalSec {
	pub listen: SocketAddr,
}

impl GlobalSec {
	fn new() -> Self {
		Self {
			listen: SocketAddr::from(([127, 0, 0, 1], 1054)),
		}
	}
}

impl Section for GlobalSec {
	fn set(&mut self, k: &str, v: &str) {
		match k.to_ascii_lowercase().as_str() {
			"listen" => self.listen = v.parse().unwrap(),
			_ => warn!("unknown key: {}", k),
		}
	}
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct UpstreamSec {
	pub name: String,
	pub protocol: Protocol,
	pub addrs: Vec<IpAddr>,
	pub port: Option<u16>,
	pub ips: Vec<String>,
	pub domains: Vec<String>,
	pub disable_aaaa: bool,
}

impl UpstreamSec {
	fn new(name: &str) -> Self {
		Self {
			name: name.to_string(),
			protocol: Protocol::Udp,
			addrs: Vec::new(),
			port: None,
			ips: Vec::new(),
			domains: Vec::new(),
			disable_aaaa: false,
		}
	}
}

impl Section for UpstreamSec {
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
				"udp" => self.protocol = Protocol::Udp,
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
			"disable_aaaa" => self.disable_aaaa = v.parse().unwrap(),
			_ => warn!("unknown key: \"{}\"", k),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test() {
		env_logger::builder()
			.is_test(true)
			.filter_level(log::LevelFilter::Trace)
			.try_init()
			.unwrap();
		// read "example.conf" into a string
		let conf = std::fs::read_to_string("example.conf").unwrap();
		let dc: DivergeConf = conf.parse().unwrap();
		println!("{:?}", dc);
	}
}
