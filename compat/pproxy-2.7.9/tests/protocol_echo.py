# Upstream pproxy Test Fixtures: Protocol Echo Tests
#
# These fixtures represent canonical pproxy protocol interaction patterns
# used for differential testing between oracle (pproxy==2.7.9) and candidate
# (eggress) implementations.
#
# Provenance: Derived from pproxy test suite and protocol specifications
# License: MIT (pproxy)
# Tested with: pproxy==2.7.9 on Python 3.11

# SOCKS5 Handshake Test
# Client sends: version, auth methods
# Server responds: chosen auth method
# Client sends: connect request (host, port)
# Server responds: success/failure reply
# Expected: byte-level wire compatibility

# HTTP CONNECT Test
# Client sends: CONNECT host:port HTTP/1.1
# Server responds: HTTP/1.1 200 Connection Established
# Client sends: tunneled data
# Expected: HTTP response code compatibility

# SOCKS4/4a Test
# Client sends: version, port, ip, userid
# Server responds: granted/rejected reply
# Expected: reply code compatibility

# Shadowsocks AEAD Test
# Client sends: encrypted stream header (salt + encrypted length + encrypted payload)
# Server decrypts and forwards
# Expected: AEAD cipher interoperability

# UDP ASSOCIATE Test
# Client sends: SOCKS5 UDP ASSOCIATE request
# Server responds with UDP relay address
# Client sends: UDP datagrams with SOCKS5 UDP header
# Expected: datagram relay compatibility
