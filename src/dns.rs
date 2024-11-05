use deku::prelude::*;

#[derive(Debug, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct Message {
	id: u16,
	#[deku(bits = "1")]
	qr: bool,
	#[deku(bits = "4")]
	opcode: u8,
	#[deku(bits = "1")]
	aa: bool,
	#[deku(bits = "1")]
	tc: bool,
	#[deku(bits = "1")]
	rd: bool,
	#[deku(bits = "1")]
	ra: bool,
	#[deku(bits = "3")]
	z: u8,
	#[deku(bits = "4")]
	rcode: u8,
	qdcount: u16,
	ancount: u16,
	nscount: u16,
	arcount: u16,
	#[deku(count = "qdcount")]
	questions: Vec<Question>,
	#[deku(count = "ancount")]
	answers: Vec<ResourceRecord>,
	#[deku(count = "nscount")]
	authorities: Vec<ResourceRecord>,
	#[deku(count = "arcount")]
	additional: Vec<ResourceRecord>,
}

#[derive(Debug, DekuRead, DekuWrite)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
struct Question {
	name: Name,
	qtype: u16,
	qclass: u16,
}

#[derive(Debug, DekuRead, DekuWrite)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
struct ResourceRecord {
	name: Name,
	rtype: u16,
	rclass: u16,
	ttl: u32,
	rdlength: u16,
	#[deku(count = "rdlength")]
	rdata: Vec<u8>,
}

#[derive(Debug, DekuRead, DekuWrite)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
struct Name {
	#[deku(
		until = "|l| match l { Label::Label { len, .. } => * len == 0, Label::Ptr { .. } => true }"
	)]
	label: Vec<Label>,
}

#[derive(Debug, DekuRead, DekuWrite)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
#[deku(id_type = "u8", bits = 2)]
enum Label {
	#[deku(id = 0)]
	Label {
		#[deku(bits = 6)]
		len: u8,
		#[deku(count = "len")]
		label: Vec<u8>,
	},
	#[deku(id = 0b11)]
	Ptr {
		#[deku(bits = 14)]
		ptr: u16,
	},
}
