use std::{
	fs::File,
	io::{BufRead, BufReader},
	path::Path,
};

use log::*;

pub trait FromLst<T: Copy> {
	fn append_line(&mut self, l: &str, v: T) -> Option<()>;

	fn append_from<L, S>(&mut self, lst: L, v: T) -> usize
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
			if self.append_line(l, v).is_some() {
				c += 1;
			} else {
				warn!("invalid line: {}", l);
			}
		}
		c
	}

	fn append_from_file<P: AsRef<Path>>(&mut self, file: P, value: T) {
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
}
