use log::warn;

pub trait Section {
	fn new(name: &str) -> Self;
	fn set(&mut self, k: &str, v: &str);
}

// yes I wrote my own parser
//	since I want to warn on unknown keys and it's simple enough
pub fn from_conf<T: Section + Sized>(conf: &str) -> Vec<T> {
	let mut r = Vec::new();
	let mut sec = None;
	for l in conf.lines() {
		let l = l.trim_ascii();
		if l.is_empty() || l.starts_with('#') {
			// empty line or comment
		} else if l.starts_with("[") && l.ends_with("]") {
			if let Some(sec) = sec.take() {
				r.push(sec);
			}
			// section name
			let name = l[1..l.len() - 1].trim_ascii();
			sec = Some(T::new(name));
			continue;
		} else if let Some(sec) = sec.as_mut() {
			// k = v
			match l.split_once('=') {
				None => panic!("invalid line: {}", l),
				Some((k, v)) => {sec.set(k, v);}
			}
		} else {
			warn!("invalid line, not in a section: {}", l);
		}
	}
	if let Some(sec) = sec.take() {
		r.push(sec);
	}
	r
}
