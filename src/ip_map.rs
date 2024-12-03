use std::{
	fs::File,
	io::{BufRead, BufReader},
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
	path::Path,
};

use log::*;
use treebitmap::IpLookupTable;

pub struct IpMap<T> {
	v4: IpLookupTable<Ipv4Addr, T>,
	v6: IpLookupTable<Ipv6Addr, T>,
	default: T,
}

impl<T: Copy> IpMap<T> {
	pub fn new(default: T) -> Self {
		Self {
			v4: IpLookupTable::new(),
			v6: IpLookupTable::new(),
			default,
		}
	}

	pub fn append_from_file<P: AsRef<Path>>(&mut self, file: P, value: T) {
		let f = match File::open(file.as_ref()) {
			Ok(f) => f,
			Err(e) => {
				warn!("failed to open file: {:?}", e);
				return;
			}
		};
		let c = self.append_from(BufReader::new(f).lines().map_while(Result::ok), value);
		info!("loaded {} entries from {}", c, file.as_ref().display());
	}

	pub fn append_from<L, S>(&mut self, lst: L, value: T) -> usize
	where
		L: Iterator<Item = S>,
		S: AsRef<str>,
	{
		let mut c = 0;
		for l in lst {
			let l = l.as_ref();
			let l = l.trim();
			if l.is_empty() || l.starts_with('#') {
				continue;
			}
			// use a closure so we can enjoy ? for a while
			let (addr, len) = match |l: &str| -> Option<(IpAddr, u32)> {
				let (a, b) = l.split_once('/')?;
				Some((a.parse().ok()?, b.parse().ok()?))
			}(l)
			{
				Some(l) => l,
				None => {
					warn!("invalid line: {}", l);
					continue;
				}
			};
			self.insert(addr, len, value);
			c += 1;
		}
		c
	}

	pub fn insert(&mut self, addr: IpAddr, cidr_len: u32, value: T) {
		match addr {
			IpAddr::V4(addr) => self.v4.insert(addr, cidr_len, value),
			IpAddr::V6(addr) => self.v6.insert(addr, cidr_len, value),
		};
	}

	pub fn get4(&self, addr: Ipv4Addr) -> T {
		self.v4
			.longest_match(addr)
			.map_or(self.default, |(_, _, v)| *v)
	}

	pub fn get6(&self, addr: Ipv6Addr) -> T {
		self.v6
			.longest_match(addr)
			.map_or(self.default, |(_, _, v)| *v)
	}

	pub fn get(&self, addr: IpAddr) -> T {
		match addr {
			IpAddr::V4(addr) => self.get4(addr),
			IpAddr::V6(addr) => self.get6(addr),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn size() {
		println!("size_of::<IpSet>: {}", std::mem::size_of::<IpMap<u8>>());
	}

	#[test]
	fn test_ip_map() {
		let mut m = IpMap::new(false);
		for (net, len) in [("127.0.0.1", 24), ("2001:db8::", 32)] {
			m.insert(net.parse().unwrap(), len, true);
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
			assert_eq!(m.get(ip.parse().unwrap()), *expected);
		}
	}
}
