use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use futures::{stream::FuturesUnordered, StreamExt};
use hickory_proto::{
	op::{header::MessageType, Header, Message, Query, ResponseCode},
	rr::{DNSClass, Name, Record, RecordType},
};
use hickory_resolver::{error::ResolveError, TokioAsyncResolver};
use log::*;
use tokio::time::{timeout, Duration};

use crate::{conf::DivergeConf, domain_map::DomainMap, ip_map::IpMap, resolver, utils::FromLst};

const UPSTREAM_LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

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
			match lookup_records(upstream.resolver.clone(), name.to_ascii(), rtype).await {
				LookupOutcome::Records(records) => {
					let c = self.prune(&mut ret, &records, i);
					if c == 0 {
						warn!(
							"domain map choose upstream {} for {} but all records are pruned; returning unfiltered records",
							upstream.name, &name
						);
						ret = records;
					}
				}
				LookupOutcome::Error(e) => {
					log_resolve_error(&upstream.name, name, e);
				}
				LookupOutcome::Timeout => log_resolve_timeout(&upstream.name, name, rtype),
				LookupOutcome::Skipped => {}
			}
		} else {
			let name = name.to_ascii();
			let mut outcomes = Vec::with_capacity(self.upstreams.len());
			outcomes.resize_with(self.upstreams.len(), || None);
			let mut tasks = FuturesUnordered::new();

			for (i, upstream) in self.upstreams.iter().enumerate() {
				if upstream.disable_aaaa && rtype == RecordType::AAAA {
					outcomes[i] = Some(LookupOutcome::Skipped);
					continue;
				}
				let resolver = upstream.resolver.clone();
				let name = name.clone();
				tasks.push(async move { (i, lookup_records(resolver, name, rtype).await) });
			}

			let mut next = 0;
			while let Some((i, outcome)) = tasks.next().await {
				outcomes[i] = Some(outcome);

				while next < outcomes.len() {
					let Some(outcome) = outcomes[next].take() else {
						break;
					};
					let uname = &self.upstreams[next].name;
					match outcome {
						LookupOutcome::Records(records) => {
							let c = self.prune(&mut ret, &records, next as u8);
							if c > 0 {
								info!("ip map choose upstream {} for {}", uname, &name);
								return ret;
							}
							ret.clear();
						}
						LookupOutcome::Error(e) => {
							log_resolve_error(uname, &name, e);
						}
						LookupOutcome::Timeout => {
							log_resolve_timeout(uname, &name, rtype);
						}
						LookupOutcome::Skipped => {}
					}
					next += 1;
				}
			}

			while next < outcomes.len() {
				if let Some(outcome) = outcomes[next].take() {
					let uname = &self.upstreams[next].name;
					match outcome {
						LookupOutcome::Records(records) => {
							let c = self.prune(&mut ret, &records, next as u8);
							if c > 0 {
								info!("ip map choose upstream {} for {}", uname, &name);
								return ret;
							}
							ret.clear();
						}
						LookupOutcome::Error(e) => {
							log_resolve_error(uname, &name, e);
						}
						LookupOutcome::Timeout => log_resolve_timeout(uname, &name, rtype),
						LookupOutcome::Skipped => {}
					};
				}
				next += 1;
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

enum LookupOutcome {
	Records(Vec<Record>),
	Error(ResolveError),
	Timeout,
	Skipped,
}

async fn lookup_records(
	resolver: TokioAsyncResolver,
	name: String,
	rtype: RecordType,
) -> LookupOutcome {
	match timeout(UPSTREAM_LOOKUP_TIMEOUT, resolver.lookup(name, rtype)).await {
		Ok(Ok(resp)) => LookupOutcome::Records(resp.records().to_vec()),
		Ok(Err(e)) => LookupOutcome::Error(e),
		Err(_) => LookupOutcome::Timeout,
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

use hickory_resolver::error::ResolveErrorKind;

fn log_resolve_timeout<N: std::fmt::Display>(upname: &str, name: N, rtype: RecordType) {
	warn!(
		"upstream {} timed out resolving {} {} after {:?}",
		upname, name, rtype, UPSTREAM_LOOKUP_TIMEOUT
	);
}

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
	use crate::conf::{DivergeConf, GlobalSec, UpstreamSec};
	use hickory_proto::op::OpCode;
	use hickory_resolver::config::Protocol;
	use tokio::net::UdpSocket;

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

	#[tokio::test(flavor = "current_thread")]
	async fn query_returns_when_later_upstream_hangs() {
		let responsive = no_records_server().await;
		let hanging = hanging_server().await;
		let diverge = Diverge::from(&DivergeConf {
			global: GlobalSec {
				listen: "127.0.0.1:0".parse().unwrap(),
			},
			upstreams: vec![
				UpstreamSec {
					name: "CN".to_string(),
					protocol: Protocol::Udp,
					addrs: vec![responsive.ip()],
					port: Some(responsive.port()),
					tls_dns_name: None,
					ips: vec![],
					domains: vec![],
					disable_aaaa: false,
				},
				UpstreamSec {
					name: "X".to_string(),
					protocol: Protocol::Udp,
					addrs: vec![hanging.ip()],
					port: Some(hanging.port()),
					tls_dns_name: None,
					ips: vec![],
					domains: vec![],
					disable_aaaa: false,
				},
			],
		});

		let query = query_message("api.github.com.", RecordType::AAAA);
		let response = timeout(Duration::from_secs(5), diverge.query(query))
			.await
			.expect("diverge query should be bounded by upstream timeout")
			.expect("valid query should produce a DNS response");
		let response = Message::from_vec(&response).unwrap();

		assert_eq!(response.response_code(), ResponseCode::NoError);
		assert_eq!(response.answer_count(), 0);
		assert_eq!(response.query_count(), 1);
	}

	fn query_message(name: &str, rtype: RecordType) -> Vec<u8> {
		let mut query = Query::new();
		query.set_name(Name::from_ascii(name).unwrap());
		query.set_query_type(rtype);
		query.set_query_class(DNSClass::IN);

		let mut msg = Message::new();
		msg.set_id(0x1234);
		msg.set_message_type(MessageType::Query);
		msg.set_op_code(OpCode::Query);
		msg.set_recursion_desired(true);
		msg.add_query(query);
		msg.to_vec().unwrap()
	}

	async fn no_records_server() -> std::net::SocketAddr {
		let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
		let addr = socket.local_addr().unwrap();
		tokio::spawn(async move {
			let mut buf = vec![0u8; 512];
			while let Ok((len, peer)) = socket.recv_from(&mut buf).await {
				let request = Message::from_vec(&buf[..len]).unwrap();
				let mut response = Message::new();
				response.set_header(Header::response_from_request(request.header()));
				response.set_recursion_available(true);
				for query in request.queries() {
					response.add_query(query.clone());
				}
				socket
					.send_to(&response.to_vec().unwrap(), peer)
					.await
					.unwrap();
			}
		});
		addr
	}

	async fn hanging_server() -> std::net::SocketAddr {
		let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
		let addr = socket.local_addr().unwrap();
		tokio::spawn(async move {
			let mut buf = vec![0u8; 512];
			while socket.recv_from(&mut buf).await.is_ok() {}
		});
		addr
	}
}
