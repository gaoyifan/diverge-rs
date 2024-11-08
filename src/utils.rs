use std::error::Error;

use log::{log, Level};

pub trait Or<T> {
	fn or(self, level: Level, msg: &str) -> T;
}

pub trait OrEx<T> {
	fn or_err(self, msg: &str) -> T;
	fn or_warn(self, msg: &str) -> T;
	fn or_info(self, msg: &str) -> T;
	fn or_debug(self, msg: &str) -> T;
	fn or_trace(self, msg: &str) -> T;
}

// can't impl these in Or since Or is not Sized
impl<T, O> OrEx<O> for T
where
	T: Or<O> + Sized,
	O: Sized,
{
	fn or_err(self, msg: &str) -> O {
		self.or(Level::Error, msg)
	}
	fn or_warn(self, msg: &str) -> O {
		self.or(Level::Warn, msg)
	}
	fn or_info(self, msg: &str) -> O {
		self.or(Level::Info, msg)
	}
	fn or_debug(self, msg: &str) -> O {
		self.or(Level::Debug, msg)
	}
	fn or_trace(self, msg: &str) -> O {
		self.or(Level::Trace, msg)
	}
}

impl<T, E: Error> Or<Option<T>> for Result<T, E> {
	fn or(self, level: Level, msg: &str) -> Option<T> {
		match self {
			Ok(e) => Some(e),
			Err(e) => {
				log!(level, "{} {}", msg, e);
				None
			}
		}
	}
}

impl<T> Or<Option<T>> for Option<T> {
	fn or(self, level: Level, msg: &str) -> Option<T> {
		if self.is_none() {
			log!(level, "{}", msg);
		}
		self
	}
}
