use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hickory_proto::{
	op::{header::MessageType, Header, Message, Query, ResponseCode},
	rr::{DNSClass, Name, Record, RecordType},
};
use hickory_resolver::TokioAsyncResolver;
use log::*;
use tokio::task;

use crate::{conf::DivergeConf, domain_map::DomainMap, ip_map::IpMap, resolver, utils::FromLst};

struct Upstream {
	name: String,
	resolver: TokioAsyncResolver,
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
		let upstreams: Vec<_> = conf
			.upstreams
			.iter()
			.enumerate()
			.map(|(i, upconf)| {
				for fname in upconf.domains.iter() {
					domain_map.append_from_file(fname, i as u8);
				}
				for fname in upconf.ips.iter() {
					ip_map.append_from_file(fname, i as u8);
				}
				info!("upstream {} configured", &upconf.name);
				Upstream {
					name: upconf.name.clone(),
					resolver: resolver::from(upconf),
					disable_aaaa: upconf.disable_aaaa,
				}
			})
			.collect();
		Self {
			domain_map,
			ip_map,
			upstreams,
		}
	}

	pub async fn query(&self, q: Vec<u8>) -> Option<Vec<u8>> {
		// seriously, why not just let user send it as is and let the resolver do the work?
		let query = Message::from_vec(&q)
			.map_err(|e| error!("invalid dns message: {}", e))
			.ok()?;
		trace!("dns query: {}", query);
		let query_header = query.header();

		let mut header = Header::response_from_request(query_header);
		let mut answers = None;

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

		// to do: handle edns (RFC 6891)

		// not _really_ sure if it's supported, but let's assume it is
		// also we don't have access to response header flags from hickory::lookup
		if query_header.recursion_desired() {
			header.set_recursion_available(true);
		}

		match q.query_class() {
			DNSClass::IN => match q.query_type() {
				RecordType::A => {
					let name = q.name();
					info!("A {}", name);
					answers = Some(self.query_ip(name, RecordType::A).await);
				}
				RecordType::AAAA => {
					let name = q.name();
					info!("AAAA {}", name);
					answers = Some(self.query_ip(name, RecordType::AAAA).await);
				}
				RecordType::PTR => {
					if let Some(a) = parse_ptr_verbose(&q.name().to_ascii()) {
						info!("PTR {}", a);
						answers = self.query_ptr(a).await;
					} else {
						header.set_response_code(ResponseCode::FormErr);
					}
				}
				_ => {
					let qtype = q.query_type();
					let name = q.name();
					info!("{} {}", qtype, name);
					answers = self.query_other(name, qtype).await;
				}
			},
			DNSClass::CH => {
				// to do: diagnostic
				info!("CHAOS {} {}", q.query_type(), q.name());
				header.set_response_code(ResponseCode::NotImp);
			}
			_ => {
				warn!("unsupported class: {}", q.query_type());
				header.set_response_code(ResponseCode::NotImp);
			}
		}
		mk_msg(header, Some(q), answers)
	}

	// handles A/AAAA
	async fn query_ip(&self, name: &Name, rtype: RecordType) -> Vec<Record> {
		let mut ret = Vec::with_capacity(0x10);
		if let Some(i) = self.domain_map.get(&name.to_utf8()) {
			let upstream = &self.upstreams[i as usize];
			if upstream.disable_aaaa && rtype == RecordType::AAAA {
				info!(
					"domain map choose upstream {} for {} but AAAA is disabled",
					upstream.name, name
				);
				return ret;
			}
			info!("domain map choose upstream {} for {}", &upstream.name, name);
			let resp = upstream.resolver.lookup(&name.to_ascii(), rtype).await;
			match resp {
				Ok(resp) => {
					let c = self.prune(&mut ret, resp.records(), i);
					if c == 0 {
						warn!(
							"domain map choose upstream {} for {} but all records are pruned",
							upstream.name, &name
						);
						ret.clear();
					}
				}
				Err(e) => {
					log_resolve_error(&upstream.name, name, e);
				}
			}
		} else {
			let mut tasks = Vec::new();
			let name = name.to_ascii();
			for i in 0..self.upstreams.len() {
				let upstream = &self.upstreams[i];
				if upstream.disable_aaaa && rtype == RecordType::AAAA {
					continue;
				}
				let resolver = upstream.resolver.clone();
				let name = name.clone();
				tasks.push((
					i,
					task::spawn_local(async move { resolver.lookup(&name, rtype).await }),
				));
			}
			for (i, task) in tasks.into_iter() {
				let resp = task.await;
				let uname = &self.upstreams[i].name;
				match resp {
					Ok(Ok(resp)) => {
						let c = self.prune(&mut ret, resp.records(), i as u8);
						if c > 0 {
							info!("ip map choose upstream {} for {}", uname, &name);
							break;
						} else {
							ret.clear();
						}
					}
					Ok(Err(e)) => {
						log_resolve_error(uname, &name, e);
					}
					Err(e) => {
						warn!(
							"failed to join task (resolve {} via {}): {:?}",
							name, uname, e
						);
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
			match (r.dns_class(), r.record_type()) {
				(DNSClass::IN, RecordType::A) => {
					let a = r.data().unwrap().as_a().unwrap().0;
					if self.ip_map.get4(a) == v {
						trace!("keep A {}", a);
						ret.push(r.to_owned());
						c += 1;
					} else {
						trace!("prune A {}", a);
					}
				}
				(DNSClass::IN, RecordType::AAAA) => {
					let a = r.data().unwrap().as_aaaa().unwrap().0;
					if self.ip_map.get6(a) == v {
						trace!("keep AAAA {}", a);
						ret.push(r.to_owned());
						c += 1;
					} else {
						trace!("prune AAAA {}", a);
					}
				}
				_ => {
					trace!("skip {} record", r.record_type());
					ret.push(r.to_owned());
				}
			}
		}
		c
	}

	async fn query_ptr(&self, q: IpAddr) -> Option<Vec<Record>> {
		let i = self.ip_map.get(q);
		let upstream = &self.upstreams[i as usize];
		info!("ip map choose upstream {} for {} PTR", upstream.name, q);
		let resp = upstream.resolver.reverse_lookup(q).await;
		match resp {
			Ok(resp) => Some(resp.as_lookup().records().to_vec()),
			Err(err) => {
				log_resolve_error(&upstream.name, q, err);
				None
			}
		}
	}

	async fn query_other(&self, q: &Name, rtype: RecordType) -> Option<Vec<Record>> {
		let upstream = match self.domain_map.get(&q.to_utf8()) {
			Some(i) => {
				let u = &self.upstreams[i as usize];
				info!("domain map choose upstream {} for {} {}", &u.name, q, rtype);
				u
			}
			None => {
				let u = &self.upstreams[0];
				info!(
					"domain map miss, fallback to upstream {} for {} {}",
					&u.name, q, rtype
				);
				u
			}
		};
		// CAUTION: hickory warned this interface may change in the future
		// interesting, hickory_proto::rr::Name does not satisfy hickory_resolver::IntoName
		let resp = upstream.resolver.lookup(q.to_ascii(), rtype).await;
		match resp {
			Ok(resp) => Some(resp.records().to_vec()),
			Err(err) => {
				log_resolve_error(&upstream.name, q, err);
				None
			}
		}
	}
}

fn mk_msg(header: Header, q: Option<&Query>, answers: Option<Vec<Record>>) -> Option<Vec<u8>> {
	let mut resp = Message::new();
	resp.set_header(header);
	if let Some(q) = q {
		resp.add_query(q.to_owned());
	}
	if let Some(a) = answers {
		resp.add_answers(a);
	}
	// it seems finalize() is not necessary
	trace!("dns response: {}", resp);
	// to do: truncate if exceed 0xffff
	resp.to_vec()
		.map_err(|e| error!("dns response encode error: {}", e))
		.ok()
}

fn parse_ptr_verbose(q: &str) -> Option<IpAddr> {
	let ptr = parse_ptr(q);
	if ptr.is_none() {
		warn!("invalid PTR query: {}", q);
	}
	ptr
}

fn parse_ptr(q: &str) -> Option<IpAddr> {
	if let Some(q) = q.strip_suffix(".in-addr.arpa.") {
		// v4
		let octets: [u8; 4] = q
			.split('.')
			.rev()
			.map(|s| s.parse())
			.collect::<Result<Vec<u8>, _>>()
			.ok()?
			.try_into()
			.ok()?;
		Some(IpAddr::V4(Ipv4Addr::from(octets)))
	} else if let Some(q) = q.strip_suffix(".ip6.arpa.") {
		// v6, what a weired format...
		let mut o: [u8; 32] = q
			.split('.')
			.rev()
			.map(|o| u8::from_str_radix(o, 16))
			.collect::<Result<Vec<u8>, _>>()
			.ok()?
			.try_into()
			.ok()?;
		for i in 0..16 {
			o[i] = o[i * 2] << 4 | o[i * 2 + 1];
		}
		let o: [u8; 16] = o[0..16].try_into().unwrap();
		Some(IpAddr::V6(Ipv6Addr::from(o)))
	} else {
		None
	}
}

use hickory_resolver::error::{ResolveError, ResolveErrorKind};

fn log_resolve_error<N: std::fmt::Display>(upname: &str, name: N, err: ResolveError) {
	match err.kind() {
		ResolveErrorKind::NoRecordsFound { query, .. } => {
			let qtype = query.query_type();
			let level = match qtype {
				RecordType::A => log::Level::Warn,
				_ => log::Level::Info,
			};
			log!(
				level,
				"upstream {}: {} - no {} records found",
				upname,
				name,
				qtype,
			);
		}
		_ => {
			warn!("upstream {} failed to resolve {}: {:?}", upname, name, err);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_ptr() {
		assert_eq!(
			parse_ptr_verbose("1.2.3.4.in-addr.arpa."),
			Some(IpAddr::V4(Ipv4Addr::new(4, 3, 2, 1)))
		);
		assert_eq!(
			parse_ptr_verbose(
				"1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.ip6.arpa."
			),
			Some(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
		);
	}
}
