
use std::{
	rc::Rc,
};

use hickory_proto::op::Message;
use hickory_resolver::TokioAsyncResolver;

use crate::ipset::IpSet;
use crate::domain_map::DomainMap;
use crate::utils::OrEx;


struct Upstream {
	name: String,
	ipset: IpSet,
	resolver: TokioAsyncResolver,
}

pub struct Diverge {
	domain_map: DomainMap,
	upstreams: Vec<Upstream>,
}

impl Diverge {
	pub fn new() -> Self {
		Self {
			domain_map: DomainMap::new(),
			upstreams: Vec::new(),
		}
	}

	pub async fn query(&self, q: Vec<u8>) -> Option<Vec<u8>> {
		let q = Message::from_vec(&q).or_debug("invalid dns message")?;
		// seriously, why not just let me send it as is and let the resolver do the work?
		let id = q.id();

		let mut a = Message::new();
		a.set_id(id);

		// do I need to call this?
		// a.finalize(finalizer, inception_time)
		a.to_vec().or_debug("failed to serialize dns message")
	}

}
