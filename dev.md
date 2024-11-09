
log setting for debug
---
`RUST_LOG=trace,rustls=info`

ideas
---
* (dropped) multiple questions in a message
	* to handle queries other than A/AAAA
		* say if client request TXT, we'll send TXT __and__ A to upstream
		* then use ip map to test if we should use this response
	* or use an A list to handle AAAA request
	* but turns out, this is not supported
		* dnsmasq will close the connection
		* cloud/google will only answer the first question
		* 114 will just not respond, but kept the connection open