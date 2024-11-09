
diverge
---
a DNS forwarder, with support for multiple upstream servers,
chooses results based on IP set/list.

what is this for?
---
* typical situation:
	* we have 2 links to the internet `A` and `X`.
		* typically `A` is physical from ISP, `X` is a VPN.
		* `A` only provides reliable connection to IP set/list `ipA`
		* `X` provides reliable connection to the rest, but less performant or unreliable on `ipA`.
	* we route set `ipA` via `A` and the rest via `X`.
	* each link provides it's own DNS resolver `dnsA` and `dnsX`.
* when you access a website which resolves to an address out of `ipA`,
using result from `dnsA` will (likely) be (geographically or otherwise) suboptimal.
	* and vice-versa.
* diverge's solution:
	* if the response from `dnsA` is in `ipA`, use it.
	* otherwise, use `dnsX`.

details
---
* several measures to reduce response time.
	* queries were sent concurrently.
	* decisions were made when `dnsA` responded,
	if the response qualify,
	it was returned to the client immediately without waiting for `dnsX`.
* if the response from `dnsA` contains multiple answers
and only some of them are in `ipA`, others will be pruned.
* more than 2 links are supported, like 3-way `A` `B` and `X`, or more.
	* `A` and `B` would both have their corresponding `ipA`/`ipB` and `dnsA`/`dnsB`
* there's an option to disable AAAA per upstream.
	* when link `X` doesn't support AAAA, but `dnsX` does.
	* still filter/prune AAAA results from `dnsA`.
	* will return NXDOMAIN for addresses out of `ipA`,
		which should be fine since clients should fallback to IPv4.
* also supports domain lists, and it takes precedence.
	* this is meant to prevent DNS leakage.
		* like you don't want `dnsA` to see you're accessing some websites via `X`.

diagnostics (planned)
---
via the CHAOS class, example using dig or nslookup:
* test domain list:
	* `dig +tcp -p 1053 @127.0.0.1 -c chaos -t txt www.example.com`
	* `nslookup -vc -port=1053 -class=chaos -type=txt www.example.com 127.0.0.1`
		* be aware, nslookup on Windows ignores `-port=` (always 53),
		but diverge typically doesn't listen on 53 (likely occupied by AdGuardHome).
* test IP set/list:
	* `dig +tcp -p 1053 @127.0.0.1 -c chaos -x 1.1.1.1`
	* `nslookup -vc -port=1053 -class=chaos -type=ptr 1.1.1.1 127.0.0.1`

more
---
* diverge intend to be an upstream for AdGuardHome,
so certain features are omitted:
	* no cache.
	* listens on TCP only.
* this is a port of [a previous project](https://github.com/Jimmy-Z/diverge) to Rust,
some features are different/dropped:
	* AAAA is IP set based too, instead of based on A decision.
	* (dropped) decision cache (with redis dependency).
	* (dropped) QTYPE other than A/AAAA/PTR.
	* (dropped) listen on UDP.

to do
---
* sane log level
* maybe ditch `hickory_resolver`
	* since it doesn't support `hickory_proto::op::Message` anyway
* maybe ditch `hickory_proto` too
* TXT support?
* maybe optimize ipset using a bitmap like the go version
