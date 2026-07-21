# Upstream pproxy Example: Client Connection Patterns
#
# This fixture represents the canonical pproxy Python client connection
# patterns from the upstream repository (pproxy==2.7.9). It is used as
# a test input for the paired oracle/candidate differential testing harness.
#
# Provenance: Derived from pproxy API contract and upstream tests
# License: MIT (pproxy)
# Tested with: pproxy==2.7.9 on Python 3.11

import asyncio
from pproxy import Connection, Server

# Pattern 1: Simple connection via URI
# conn = Connection('http://proxy:8080/')

# Pattern 2: Connection with auth
# conn = Connection('socks5://user:pass@proxy:1080/')

# Pattern 3: Chain of proxies
# conn = Connection('http://proxy1:8080/ -> socks5://proxy2:1080/')

# Pattern 4: Direct connection
# conn = Connection('direct://')

# Pattern 5: Shadowsocks connection
# conn = Connection('ss://aes-256-gcm:key@server:8388/')

# Pattern 6: Server with multiple listeners
# server = Server(['http://:8080/', 'socks5://:1080/'])

# Pattern 7: Server with rules
# server = Server(['http://:8080/'], rule='rules.txt')

# Pattern 8: Async usage
# async def main():
#     conn = Connection('http://proxy:8080/')
#     reader, writer = await conn.tcp_connect('example.com', 80)
#     writer.write(b'GET / HTTP/1.1\r\nHost: example.com\r\n\r\n')
#     data = await reader.read(4096)
#     print(data)

# Pattern 9: UDP association
# async def udp_main():
#     conn = Connection('socks5://proxy:1080/')
#     transport, protocol = await conn.udp_associate('target:53')

# Pattern 10: TLS connection
# conn = Connection('https://proxy:8443/')
