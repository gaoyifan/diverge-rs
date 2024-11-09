use std::collections::HashMap;

pub struct DomainMap<T>(HashMap<String, T>);

impl<T: Copy> DomainMap<T> {
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn insert(&mut self, k: &str, v: T) {
		self.0.insert(k.to_string(), v);
	}

	pub fn from_file(&mut self, filename: &str, v: T) {
		self.from_str(&std::fs::read_to_string(filename).unwrap(), v);
	}

	pub fn from_str(&mut self, list: &str, v: T) {
		for line in list.lines() {
			let line = line.trim_ascii();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			self.insert(line, v);
		}
	}

	pub fn get(&self, mut k: &str) -> Option<T> {
		if k.ends_with(".") {
			k = &k[0..k.len() - 1];
		}
		loop {
			if let Some(v) = self.0.get(k) {
				return Some(*v);
			}
			// "a.com" -> "com"
			match k.find('.') {
				Some(i) => k = &k[i + 1..],
				None => return None
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test() {
		let mut m = DomainMap::new();
		m.insert("a.a", ());
		for (t, e) in [
			("a.a", Some(())),
			("a.a.a", Some(())),
			("a", None),
			("b.a", None),
			("aa.a", None),
		] {
			assert_eq!(m.get(t), e);
		}
	}
}