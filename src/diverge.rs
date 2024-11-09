use std::{
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
	rc::Rc,
};

use hickory_proto::{
	op::{header::MessageType, Header, Message, OpCode, ResponseCode},
	rr::{DNSClass, Name, Record, RecordType},
};
use hickory_resolver::TokioAsyncResolver;
use log::*;
use tokio::task;

use crate::ipmap::IpMap;
use crate::utils::OrEx;
use crate::{domain_map::DomainMap, resolver};

struct Upstream {
	name: String,
	resolver: Rc<TokioAsyncResolver>,
	disable_aaaa: bool,
}

impl Upstream {
	/*
	fn new(name: &str) -> Self {
		Self {
			name: name.to_string(),
			ipset: None,
			resolver: TokioAsyncResolver::tokio_from_system_conf().unwrap(),
			disable_aaaa: false,
		}
	}
	*/
}

pub struct Diverge {
	domain_map: DomainMap,
	ip_map: IpMap<u8>,
	upstreams: Vec<Upstream>,
}

impl Diverge {
	pub fn new() -> Self {
		Self {
			domain_map: DomainMap::new(),
			ip_map: IpMap::new(0),
			upstreams: Vec::new(),
		}
	}

	pub async fn query(&self, q: Vec<u8>) -> Option<Vec<u8>> {
		// seriously, why not just let user send it as is and let the resolver do the work?
		let query = Message::from_vec(&q).or_debug("invalid dns message")?;
		debug!("dns query: {}", query);
		let query_header = query.header();

		let mut header = Header::response_from_request(query_header);
		let mut answers = Vec::new();

		if query_header.message_type() != MessageType::Query {
			debug!("expected query, got {}", query_header.message_type());
			header.set_response_code(ResponseCode::FormErr);
			return mk_msg(header, answers);
		}
		// we only support 1 question
		if query_header.query_count() == 0 {
			debug!("expected 1 question, got {}", query_header.query_count());
			header.set_response_code(ResponseCode::FormErr);
			return mk_msg(header, answers);
		} else if query_header.query_count() > 1 {
			debug!("expected 1 question, got {}", query_header.query_count());
			header.set_response_code(ResponseCode::NotImp);
			return mk_msg(header, answers);
		}
		if query_header.answer_count() != 0 {
			debug!("expected 0 answer, got {}", query_header.query_count());
			header.set_response_code(ResponseCode::FormErr);
			return mk_msg(header, answers);
		}

		let q = &query.queries()[0];
		match q.query_class() {
			DNSClass::IN => match q.query_type() {
				RecordType::A => {
					let name = q.name().to_string();
					info!("A {}", name);
					answers = self.query_a(name, RecordType::A).await;
				}
				RecordType::AAAA => {
					let name = q.name().to_string();
					info!("AAAA {}", name);
					answers = self.query_a(name, RecordType::AAAA).await;
				}
				RecordType::PTR => {
					let n = q.name().to_string();
					info!("PTR {}", n);
					// to do
				}
				_ => {
					debug!("unsupported query type: {:?}", q.query_type());
					header.set_response_code(ResponseCode::FormErr);
				}
			},
			DNSClass::CH => {
				// to do: diagnostic
			}
			_ => {
				debug!("unsupported class: {:?}", q.query_type());
			}
		}
		mk_msg(header, answers)
	}

	// actually A/AAAA
	async fn query_a(&self, name: String, rtype: RecordType) -> Vec<Record> {
		let mut ret = Vec::new();
		if let Some(i) = self.domain_map.get(&name) {
			let upstream = &self.upstreams[i];
			if upstream.disable_aaaa && rtype == RecordType::AAAA {
				warn!(
					"domain map choose upstream {} for {}, but AAAA is disabled",
					upstream.name, &name
				);
				return ret;
			}
			let resolver = &upstream.resolver;
			let resp = resolver.lookup(&name, rtype).await;
			if let Ok(resp) = resp {
				let c = self.prune(&mut ret, resp.records(), i as u8);
				if c == 0 {
					warn!(
						"domain map choose upstream {} for {}, but it returned no A records after pruning",
						upstream.name, &name
					);
				}
			}
		} else {
			let mut tasks = Vec::new();
			for i in 0..self.upstreams.len() {
				let upstream = &self.upstreams[i];
				if upstream.disable_aaaa && rtype == RecordType::AAAA {
					continue;
				}
				let resolver = (&upstream.resolver).clone();
				let name = name.clone();
				tasks.push((
					i,
					task::spawn_local(async move { resolver.lookup(&name, rtype).await }),
				));
			}
			for (i, task) in tasks.into_iter() {
				let resp = task.await;
				if let Ok(Ok(resp)) = resp {
					let c = self.prune(&mut ret, resp.records(), i as u8);
					if c > 0 {
						break;
					}
				}
			}
		}
		ret
	}

	// prune A/AAAA records, retain the rest, and return the number of remain A/AAAA records
	fn prune(&self, ret: &mut Vec<Record>, records: &[Record], v: u8) -> usize {
		let mut c = 0;
		for r in records {
			if r.dns_class() == DNSClass::IN && r.record_type() == RecordType::A {
				let a = r.data().unwrap().as_a().unwrap().0;
				if self.ip_map.get_v4(&a) == v {
					ret.push(r.clone());
					c += 1;
				}
			} else if r.dns_class() == DNSClass::IN && r.record_type() == RecordType::AAAA {
				let a = r.data().unwrap().as_aaaa().unwrap().0;
				if self.ip_map.get_v6(&a) == v {
					ret.push(r.clone());
					c += 1;
				}
			} else {
				ret.push(r.clone());
			}
		}
		c
	}
}

fn mk_msg(header: Header, answers: Vec<Record>) -> Option<Vec<u8>> {
	let mut resp = Message::new();
	resp.set_header(header);
	resp.add_answers(answers);
	// do I need to call this?
	// resp.finalize(finalizer, inception_time)
	debug!("dns response: {}", resp);
	resp.to_vec().or_debug("failed to serialize dns response")
}
