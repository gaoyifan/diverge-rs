use std::{
	collections::BTreeMap,
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use log::*;

pub struct IpMap<T> {
	v4: BTreeMap<usize, BTreeMap<u32, T>>,
	v6: BTreeMap<usize, BTreeMap<u64, T>>,
	default: T,
}

impl<T: Copy> IpMap<T> {
	pub fn new(default: T) -> Self {
		Self {
			v4: BTreeMap::new(),
			v6: BTreeMap::new(),
			default,
		}
	}

	pub fn append_from_file(&mut self, filename: &str, value: T) {
		let c = self.append_from_str(&std::fs::read_to_string(filename).unwrap(), value);
		info!("loaded {} entries from {}", c, filename);
	}

	pub fn append_from_str(&mut self, list: &str, value: T) -> usize {
		let mut c = 0;
		for line in list.lines() {
			let line = line.trim();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			let (addr, len) = line.split_once('/').unwrap();
			let addr: IpAddr = addr.parse().unwrap();
			let len: usize = len.parse().unwrap();
			self.insert(&addr, len, value);
			c += 1;
		}
		c
	}

	pub fn insert(&mut self, addr: &IpAddr, cidr_len: usize, value: T) {
		match addr {
			IpAddr::V4(net4) => self.insert_v4(net4, cidr_len, value),
			IpAddr::V6(net6) => self.insert_v6(net6, cidr_len, value),
		}
	}

	pub fn get(&self, addr: &IpAddr) -> T {
		match addr {
			IpAddr::V4(addr4) => self.get_v4(addr4),
			IpAddr::V6(addr6) => self.get_v6(addr6),
		}
	}

	pub fn insert_v4(&mut self, addr: &Ipv4Addr, cidr_len: usize, value: T) {
		if cidr_len > 32 {
			warn!("CIDR length too large: {}", cidr_len);
			return;
		}
		let net = ipv4_to_u32(addr) >> (32 - cidr_len);
		self.v4.entry(cidr_len).or_default().insert(net, value);
	}

	pub fn insert_v6(&mut self, addr: &Ipv6Addr, cidr_len: usize, value: T) {
		if cidr_len > 64 {
			warn!("CIDR length too large: {}", cidr_len);
			return;
		}
		let net = ipv6_to_u64(addr) >> (64 - cidr_len);
		self.v6.entry(cidr_len).or_default().insert(net, value);
	}

	pub fn get_v4(&self, addr: &Ipv4Addr) -> T {
		let addr = ipv4_to_u32(addr);
		for (len, map) in self.v4.iter() {
			if let Some(v) = map.get(&(addr >> (32 - *len))) {
				return *v;
			}
		}
		self.default
	}

	pub fn get_v6(&self, addr: &Ipv6Addr) -> T {
		let addr = ipv6_to_u64(addr);
		for (len, map) in self.v6.iter() {
			if let Some(v) = map.get(&(addr >> (64 - *len))) {
				return *v;
			}
		}
		self.default
	}
}

fn ipv4_to_u32(addr: &Ipv4Addr) -> u32 {
	let a = addr.octets();
	((a[0] as u32) << 24) + ((a[1] as u32) << 16) + ((a[2] as u32) << 8) + (a[3] as u32)
}

// just the first 8 bytes
fn ipv6_to_u64(addr: &Ipv6Addr) -> u64 {
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
		println!("size_of::<IpSet>: {}", std::mem::size_of::<IpMap<u8>>());
	}

	#[test]
	fn test() {
		let mut m = IpMap::new(false);
		for (net, len) in [("127.0.0.1", 24), ("2001:db8::", 32)] {
			let net: IpAddr = net.parse().unwrap();
			m.insert(&net, len, true);
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
			assert_eq!(m.get(&a), *expected);
		}
	}
}
