use std::collections::HashMap;

use crate::utils::FromLst;

pub struct DomainMap<T>(HashMap<String, T>);

impl<T: Copy> DomainMap<T> {
	#[allow(clippy::new_without_default)]
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn insert(&mut self, k: &str, v: T) {
		self.0.insert(k.to_string(), v);
	}

	pub fn get(&self, mut k: &str) -> Option<T> {
		if k.ends_with('.') {
			k = &k[0..k.len() - 1];
		}
		loop {
			if let Some(v) = self.0.get(k) {
				return Some(*v);
			}
			// "a.com" -> "com"
			match k.find('.') {
				Some(i) => k = &k[i + 1..],
				None => return None,
			}
		}
	}
}

impl<T: Copy> FromLst<T> for DomainMap<T> {
	fn append_line(&mut self, l: &str, v: T) -> Option<()> {
		self.insert(l, v);
		Some(())
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
