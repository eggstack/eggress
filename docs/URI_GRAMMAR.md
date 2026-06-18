# EggProxy URI Grammar

> Status: Implemented in Phase 1, Milestone 1.2.

## Grammar

```
proxy_chain = hop ( '__' hop )*
hop = protocols '://' [credentials '@'] endpoint [ '?' query ] [ '@' local_bind ]
protocols = protocol ( '+' protocol )*
protocol = 'http' | 'socks4' | 'socks5'
endpoint = host ':' port | '[' ipv6 ']' ':' port
host = hostname | ipv4 | (empty)
credentials = username ':' password
query = param ( '&' param )*
param = 'rule' '=' value
```

## Example URI Format

```
socks5://user:pass@upstream:1080
http://proxy:8080
socks4://proxy:1080
http+socks4+socks5://:8080
socks5://hop1:1080__http://hop2:8080
http://[::1]:8080
http://[2001:db8::1]:1080
http://proxy:8080?rule=regex
```
