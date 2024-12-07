use std::{
	fs::File,
	io::{BufRead, BufReader},
	path::Path,
};

use log::*;

pub trait FromLst<T: Copy> {
	fn append_line(&mut self, l: &str, v: T) -> Option<()>;

	fn append_from(&mut self, lst: impl IntoIterator<Item = impl AsRef<str>>, v: T) -> usize {
		let mut c = 0;
		for l in lst {
			let l = l.as_ref();
			let l = l.trim_ascii();
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

	fn append_from_file(&mut self, file: impl AsRef<Path>, value: T) -> Option<usize> {
		let file = file.as_ref();
		let c = self.append_from(read_lines(file)?, value);
		info!("loaded {} entries from {}", c, file.display());
		Some(c)
	}
}

pub fn read_lines(f: impl AsRef<Path>) -> Option<impl Iterator<Item = impl AsRef<str>>> {
	let f = f.as_ref();
	match File::open(f) {
		Err(e) => {
			warn!("failed to open {}: {:?}", f.display(), e);
			None
		}
		Ok(f) => Some(BufReader::new(f).lines().map_while(Result::ok)),
	}
}
