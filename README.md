
diverge
---
a DNS forwarder, with support for multiple upstream servers,
chooses results based on IP set/list.

what is this for?
---
* typical situation:
	* we have 2 links to the internet `0` and `X`.
		* typically `0` is physical from ISP, `X` is a VPN.
		* `0` only provides reliable connection to IP set/list `ip0`
		* `X` provides reliable connection to the rest, but less performant or unreliable on `ip0`.
	* we route set `ip0` via `0` and the rest via `X`.
	* each link provides it's own DNS resolver `dns0` and `dnsX`.
* when you access a website which resolves to an address out of `ip0`,
using result from `dns0` will (likely) be (geographically or otherwise) suboptimal.
	* and vice-versa.
* diverge's solution:
	* if the response from `dns0` is in `ip0`, use it.
	* otherwise, use `dnsX`.

details
---
* several measures to reduce response time:
	* queries were sent concurrently.
	* decisions were made when `dns0` responded,
	if the response qualify,
	it was returned to the client immediately without waiting for `dnsX`.
	* implemented RFC 7766 6.2.1.1 pipelining
* if the response from `dns0` contains multiple answers
and only some of them are in `ip0`, others will be pruned.
* more than 2 links are supported, like 3-way `0` `1` and `X`, or more.
	* `0` and `1` would both have their corresponding `ip0`/`ip1` and `dns0`/`dns1`
* there's an option to disable AAAA per upstream.
	* when link `X` doesn't support AAAA, but `dnsX` does.
	* still filter/prune answers from `dns0`.
		* will return no answer if all answers were pruned,
			which should be fine since clients should fallback to IPv4.
* also supports domain lists, and it takes precedence.
	* this is meant to prevent DNS leakage.
		* like you don't want `dns0` to see you're accessing some websites via `X`.

diagnostics (not implemented yet)
---
via the CHAOS class, example using dig or nslookup:
* test domain list:
	* `dig -p 1054 @127.0.0.1 -c chaos -t txt www.example.com`
	* `nslookup -port=1054 -class=chaos -type=txt www.example.com 127.0.0.1`
		* be aware, nslookup on Windows ignores `-port=` (always 53),
		but diverge typically doesn't listen on 53 (likely occupied by AdGuardHome).
* test IP set/list:
	* `dig -p 1054 @127.0.0.1 -c chaos -x 1.1.1.1`
	* `nslookup -port=1054 -class=chaos -type=ptr 1.1.1.1 127.0.0.1`

more
---
* diverge intend to be an upstream for AdGuardHome,
so certain features are omitted:
	* no cache.
* this is a port of [a previous project](https://github.com/Jimmy-Z/diverge) to Rust,
some features are different/dropped:
	* supports DoT/DoH upstreams.
	* AAAA is IP set based too, instead of based on A decision.
	* other query types fallbacks to upstream 0 if no hit in domain map.
		* instead of based on A decision.
	* (dropped) decision cache (with redis dependency).

to do
---
* sane log level
* CHAOS
