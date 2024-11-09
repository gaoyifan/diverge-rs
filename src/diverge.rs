use std::rc::Rc;

use hickory_proto::{
	op::{header::MessageType, Header, Message, Query, ResponseCode},
	rr::{DNSClass, Record, RecordType},
};
use hickory_resolver::TokioAsyncResolver;
use log::*;
use tokio::task;

use crate::{conf::DivergeConf, domain_map::DomainMap, ip_map::IpMap, resolver, utils::OrEx};

struct Upstream {
	name: String,
	resolver: Rc<TokioAsyncResolver>,
	disable_aaaa: bool,
}

pub struct Diverge {
	domain_map: DomainMap<u8>,
	ip_map: IpMap<u8>,
	upstreams: Vec<Upstream>,
}

impl Diverge {
	pub fn from(conf: &DivergeConf) -> Self {
		let mut domain_map = DomainMap::new();
		let mut ip_map = IpMap::new((conf.upstreams.len() - 1) as u8);
		let mut upstreams = Vec::new();
		for i in 0..conf.upstreams.len() {
			let upconf = &conf.upstreams[i];
			for fname in upconf.domains.iter() {
				domain_map.from_file(fname, i as u8);
			}
			for fname in upconf.ips.iter() {
				ip_map.from_file(fname, i as u8);
			}
			upstreams.push(Upstream {
				name: upconf.name.clone(),
				resolver: Rc::new(resolver::from(upconf)),
				disable_aaaa: upconf.disable_aaaa,
			})
		}
		Self {
			domain_map,
			ip_map,
			upstreams,
		}
	}

	pub async fn query(&self, q: Vec<u8>) -> Option<Vec<u8>> {
		// seriously, why not just let user send it as is and let the resolver do the work?
		let query = Message::from_vec(&q).or_debug("invalid dns message")?;
		trace!("dns query: {}", query);
		let query_header = query.header();

		let mut header = Header::response_from_request(query_header);
		let mut answers = Vec::new();

		if query_header.message_type() != MessageType::Query {
			debug!("expected query, got {}", query_header.message_type());
			header.set_response_code(ResponseCode::FormErr);
			return mk_msg(header, None, answers);
		}
		// we only support 1 question
		if query_header.query_count() == 0 {
			debug!("expected 1 question, got {}", query_header.query_count());
			header.set_response_code(ResponseCode::FormErr);
			return mk_msg(header, None, answers);
		}

		let q = &query.queries()[0];

		if query_header.query_count() > 1 {
			debug!("expected 1 question, got {}", query_header.query_count());
			header.set_response_code(ResponseCode::NotImp);
			return mk_msg(header, Some(q), answers);
		}
		if query_header.answer_count() != 0 {
			debug!("expected 0 answer, got {}", query_header.query_count());
			header.set_response_code(ResponseCode::FormErr);
			return mk_msg(header, Some(q), answers);
		}

		// to do: handle edns

		// not _really_ sure if it's supported but we don't have access to header flags from hickory
		if query_header.recursion_desired() {
			header.set_recursion_available(true);
		}

		match q.query_class() {
			DNSClass::IN => match q.query_type() {
				RecordType::A => {
					let name = q.name().to_string();
					info!("A {}", name);
					answers = self.query_ip(name, RecordType::A).await;
				}
				RecordType::AAAA => {
					let name = q.name().to_string();
					info!("AAAA {}", name);
					answers = self.query_ip(name, RecordType::AAAA).await;
				}
				RecordType::PTR => {
					let n = q.name().to_string();
					info!("PTR {}", n);
					// to do
				}
				_ => {
					debug!("unsupported query type: {:?}", q.query_type());
					header.set_response_code(ResponseCode::NotImp);
				}
			},
			DNSClass::CH => {
				// to do: diagnostic
			}
			_ => {
				debug!("unsupported class: {:?}", q.query_type());
			}
		}
		mk_msg(header, Some(q), answers)
	}

	// handles A/AAAA
	async fn query_ip(&self, name: String, rtype: RecordType) -> Vec<Record> {
		let mut ret = Vec::with_capacity(0x20);
		if let Some(i) = self.domain_map.get(&name) {
			let upstream = &self.upstreams[i as usize];
			if upstream.disable_aaaa && rtype == RecordType::AAAA {
				warn!(
					"domain map choose upstream {} for {}, but AAAA is disabled",
					upstream.name, &name
				);
				return ret;
			}
			let resolver = &upstream.resolver;
			info!("domain map choose upstream {} for {}", &upstream.name, &name);
			let resp = resolver.lookup(&name, rtype).await;
			if let Ok(resp) = resp {
				let c = self.prune(&mut ret, resp.records(), i as u8);
				if c == 0 {
					warn!(
						"domain map choose upstream {} for {}, but all records are pruned",
						upstream.name, &name
					);
					ret.clear();
				}
			} else {
				trace!("upstream {} failed to resolve {}: {:?}", &upstream.name, &name, resp);
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
				let uname = &self.upstreams[i].name;
				if let Ok(Ok(resp)) = resp {
					let c = self.prune(&mut ret, resp.records(), i as u8);
					if c > 0 {
						info!("ip map choose upstream {} for {}", uname, &name);
						break;
					} else {
						ret.clear();
					}
				} else {
					trace!("upstream {} failed to resolve {}: {:?}", uname, &name, resp);
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
					trace!("kept A {}", a);
					ret.push(r.clone());
					c += 1;
				} else {
					trace!("prune A {}", a);
				}
			} else if r.dns_class() == DNSClass::IN && r.record_type() == RecordType::AAAA {
				let a = r.data().unwrap().as_aaaa().unwrap().0;
				if self.ip_map.get_v6(&a) == v {
					trace!("keep AAAA {}", a);
					ret.push(r.clone());
					c += 1;
				} else {
					trace!("prune AAAA {}", a);
				}
			} else {
				ret.push(r.clone());
			}
		}
		c
	}
}

fn mk_msg(header: Header, q: Option<&Query>, answers: Vec<Record>) -> Option<Vec<u8>> {
	let mut resp = Message::new();
	resp.set_header(header);
	if let Some(q) = q {
		resp.add_query(q.to_owned());
	}
	resp.add_answers(answers);
	// do I need to call this?
	// resp.finalize(finalizer, inception_time)
	trace!("dns response: {}", resp);
	// to do: truncate if exceed 0xffff
	resp.to_vec().or_debug("failed to serialize dns response")
}
