use std::{
	net::Ipv4Addr,
	collections::{BTreeMap, BTreeSet},
};

pub struct Ip4Set(BTreeMap<isize, BTreeSet<u32>>);

fn ipv4_to_u32(ip: Ipv4Addr) -> u32 {
	let ip = ip.octets();
	((ip[0] as u32) << 24) + ((ip[1] as u32) << 16) + ((ip[2] as u32) << 8) + (ip[3] as u32)
}

impl Ip4Set {
	pub fn new() -> Self {
		Self(BTreeMap::new())
	}
	
	pub fn from(filename: &str) -> Self {
		let mut s = Self::new();
		for line in std::fs::read_to_string(filename).unwrap().lines() {
			let line = line.trim();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			let (net, len) = line.split_once('/').unwrap();
			let net: Ipv4Addr = net.parse().unwrap();
			let len: isize = len.parse().unwrap();
			s.insert(net, len);
		}
		s
	}

	pub fn insert(&mut self, ip: Ipv4Addr, cidr_len: isize) {
		let net = ipv4_to_u32(ip) >> (32 - cidr_len);
		self.0.entry(cidr_len).or_insert_with(BTreeSet::new).insert(net);
	}

	pub fn contains(&self, ip: Ipv4Addr) -> bool {
		let ip = ipv4_to_u32(ip);
		for (len, set) in self.0.iter() {
			if set.contains(&(ip >> (32 - *len))) {
				return true;
			}
		}
		return false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test() {
		let mut s = Ip4Set::new();
		for (net, len) in [
			("127.0.0.1", 24),

		] {
			let net: Ipv4Addr = net.parse().unwrap();
			s.insert(net, len);
		}

		let tests = [
			("126.255.255.255", false),
			("127.0.0.1", true),
			("127.0.0.255", true),
			("127.0.1.1", false),
		];
		for (ip, expected) in tests.iter() {
			let a: Ipv4Addr = ip.parse().unwrap();
			assert_eq!(s.contains(a), *expected);
		}
	}
}
