use std::{
	collections::{BTreeMap, BTreeSet},
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use log::*;

pub struct IpSet(
	BTreeMap<usize, BTreeSet<u32>>,
	BTreeMap<usize, BTreeSet<u64>>,
);

impl IpSet {
	pub fn new() -> Self {
		Self(BTreeMap::new(), BTreeMap::new())
	}

	pub fn from(&mut self, filename: &str) {
		for line in std::fs::read_to_string(filename).unwrap().lines() {
			let line = line.trim();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			let (addr, len) = line.split_once('/').unwrap();
			let addr: IpAddr = addr.parse().unwrap();
			let len: usize = len.parse().unwrap();
			self.insert(addr, len);
		}
	}

	pub fn insert(&mut self, addr: IpAddr, cidr_len: usize) {
		match addr {
			IpAddr::V4(net4) => self.insert_v4(net4, cidr_len),
			IpAddr::V6(net6) => self.insert_v6(net6, cidr_len),
		}
	}

	pub fn contains(&self, addr: IpAddr) -> bool {
		match addr {
			IpAddr::V4(addr4) => self.contains_v4(addr4),
			IpAddr::V6(addr6) => self.contains_v6(addr6),
		}
	}

	pub fn insert_v4(&mut self, addr: Ipv4Addr, cidr_len: usize) {
		if cidr_len > 32 {
			warn!("CIDR length too large: {}", cidr_len);
			return;
		}
		let net = ipv4_to_u32(addr) >> (32 - cidr_len);
		self.0
			.entry(cidr_len)
			.or_insert_with(BTreeSet::new)
			.insert(net);
	}

	pub fn insert_v6(&mut self, addr: Ipv6Addr, cidr_len: usize) {
		if cidr_len > 64 {
			warn!("CIDR length too large: {}", cidr_len);
			return;
		}
		let net = ipv6_to_u64(addr) >> (64 - cidr_len);
		self.1
			.entry(cidr_len)
			.or_insert_with(BTreeSet::new)
			.insert(net);
	}

	pub fn contains_v4(&self, addr: Ipv4Addr) -> bool {
		let addr = ipv4_to_u32(addr);
		for (len, set) in self.0.iter() {
			if set.contains(&(addr >> (32 - *len))) {
				return true;
			}
		}
		return false;
	}

	pub fn contains_v6(&self, addr: Ipv6Addr) -> bool {
		let addr = ipv6_to_u64(addr);
		for (len, set) in self.1.iter() {
			if set.contains(&(addr >> (64 - *len))) {
				return true;
			}
		}
		return false;
	}
}

fn ipv4_to_u32(addr: Ipv4Addr) -> u32 {
	let a = addr.octets();
	((a[0] as u32) << 24) + ((a[1] as u32) << 16) + ((a[2] as u32) << 8) + (a[3] as u32)
}

// just the first 8 bytes
fn ipv6_to_u64(addr: Ipv6Addr) -> u64 {
	let a = addr.octets();
	((a[0] as u64) << 56)
		+ ((a[1] as u64) << 48)
		+ ((a[2] as u64) << 40)
		+ ((a[3] as u64) << 32)
		+ ((a[4] as u64) << 24)
		+ ((a[5] as u64) << 16)
		+ ((a[6] as u64) << 8)
		+ (a[7] as u64)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn size() {
		println!("size_of::<IpSet>: {}", std::mem::size_of::<IpSet>());
	}

	#[test]
	fn test() {
		let mut s = IpSet::new();
		for (net, len) in [
			("127.0.0.1", 24),
			("2001:db8::", 32),

		] {
			let net: IpAddr = net.parse().unwrap();
			s.insert(net, len);
		}

		let tests = [
			("126.255.255.255", false),
			("127.0.0.0", true),
			("127.0.0.255", true),
			("127.0.1.0", false),
			("2001:db7:ffff:ffff:ffff:ffff:ffff:ffff", false),
			("2001:db8::0", true),
			("2001:db8:ffff:ffff:ffff:ffff:ffff:ffff", true),
			("2001:db9::0", false),
		];
		for (ip, expected) in tests.iter() {
			let a: IpAddr = ip.parse().unwrap();
			assert_eq!(s.contains(a), *expected);
		}
	}
}
