use std::collections::HashMap;

pub struct DomainMap(HashMap<String, usize>);

impl DomainMap {
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn insert(&mut self, k: &str, v: usize) {
		self.0.insert(k.to_string(), v);
	}

	pub fn from(&mut self, list: &str, v: usize) {
		for line in list.lines() {
			let line = line.trim_ascii();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			self.insert(line, v);
		}
	}

	pub fn get(&self, mut k: &str) -> Option<usize> {
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
		m.insert("a.a", 1);
		for (t, e) in [
			("a.a", Some(1)),
			("a.a.a", Some(1)),
			("a", None),
			("b.a", None),
			("aa.a", None),
		] {
			assert_eq!(m.get(t), e);
		}
	}
}