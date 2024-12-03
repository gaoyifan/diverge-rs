
log setting for debug
---
`RUST_LOG=trace,rustls=info`

to do
---
* log partial prune to file, for science

thoughts
---
* better handling for queries other than A/AAA/PTR?
	* send an A query too
	* if the upstream supports pipelining, should not hurt response time
	* do we really need this though
* NXDOMAIN handling
	* currently just returns a no error with no answer
	* maybe we should trust NXDOMAIN from some upstream?
* maybe ditch `hickory_resolver`
	* the interface is not low enough
		* we want to send/recv `hickory_proto::op::Message`
* maybe ditch `hickory_proto` too
	* interface is a bit clunky
	* we might only need a (partial) deserialize
		* to filter response
		* not able to prune answers
			* need more data/experiment on this
* optimize ip/domain map with fst

dropped
---
* multiple questions in a message
	* to handle queries other than A/AAAA/PTR
		* say if client request TXT, we'll send TXT __and__ A to upstream
		* then use ip map to test if we should use this response
	* or use an A list to handle AAAA request
	* but turns out, this is not supported
		* dnsmasq will close the connection
		* cloud/google will only answer the first question
		* 114 will just not respond, but kept the connection open
