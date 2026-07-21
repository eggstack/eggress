# Upstream pproxy Example: Server Configuration
#
# This fixture represents the canonical pproxy server configuration pattern
# from the upstream repository (pproxy==2.7.9). It is used as a test input
# for the paired oracle/candidate differential testing harness.
#
# Provenance: Derived from pproxy CLI help text and upstream README
# License: MIT (pproxy)
# Tested with: pproxy==2.7.9 on Python 3.11

# Pattern 1: Simple HTTP proxy on port 8080
# pproxy -l http://:8080/

# Pattern 2: SOCKS5 proxy with authentication
# pproxy -l socks5://user:pass@:1080/

# Pattern 3: Multi-protocol listener
# pproxy -l "http+socks4+socks5://:8080/"

# Pattern 4: Chain through upstream proxy
# pproxy -l http://:8080/ -r socks5://upstream:1080/

# Pattern 5: Shadowsocks server
# pproxy -l ss://aes-256-gcm:key@:8388/

# Pattern 6: UDP relay
# pproxy -l socks5://:1080/ -ul udp://:1081/

# Pattern 7: TLS listener
# pproxy -l "http+socks5://:8443/" --ssl cert.pem,key.pem

# Pattern 8: Scheduling algorithm
# pproxy -l http://:8080/ -r server1 -r server2 -s rr

# Pattern 9: Health check
# pproxy -l http://:8080/ -r server1 -r server2 -a 30

# Pattern 10: Block rules
# pproxy -l http://:8080/ -b ".*blocked.*"
