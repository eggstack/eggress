# Eggress URI Grammar

> Status: Native grammar. Shared lexical primitives (`eggress-uri::syntax`)
> are reused by the pproxy compatibility grammar, but the grammars stay
> separate: `shared lexical primitives != shared grammar`. Compatibility-only
> constructs (`+in`, `bind`/`listen`/`backward`/`rebind`, `https`, `direct`,
> `redir`, `echo`, plugins, fragments, fixed targets, rule files) never enter
> the native AST.

## Grammar

```
proxy_chain = hop ( '__' hop )*
hop = protocols '://' [credentials '@'] endpoint [ '?' query ] [ '/' plugins ] [ '#' auth_prefix ] [ '@' local_bind ]
protocols = protocol ( '+' protocol )*   -- 'tls' in the list sets hop.tls
protocol = 'http' | 'httponly' | 'socks4' | 'socks4a' | 'socks5'
         | 'shadowsocks' | 'ss' | 'ssr' | 'trojan'
         | 'h2' | 'h3' | 'quic' | 'ws' | 'wss'
         | 'raw' | 'tunnel' | 'ssh' | 'unix'
endpoint = host ':' port | '[' ipv6 ']' ':' port
host = hostname | ipv4 | ipv6-literal (bracketed)   -- empty hosts rejected for proxy hops
credentials = username ':' password
trojan_credentials = password
query = param ( '&' param )*
param = 'rule' '=' value | 'insecure' | 'insecure=true'
plugins = name ( ',' name )*
```

Canonical names (`ProtocolSpec::canonical_name`): `http`, `httponly`,
`socks4`, `socks5`, `shadowsocks`, `ssr`, `trojan`, `h2`, `h3`, `quic`,
`ws`, `raw`, `ssh`, `unix`. Aliases: `socks4a`→`socks4`, `ss`→`shadowsocks`,
`wss`→`ws` (WebSocket), `tunnel`→`raw`.

Rules: `__` separates hops (`___` rejected as duplicate separator);
`+` stacks protocols within a scheme; userinfo separator is the last
unbracketed `@`; credentials are percent-decoded; SSH defaults to port 22;
port 0 rejected (except Unix); unmatched brackets fail closed.

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
http+tls://proxy:443
shadowsocks://aes-256-gcm:secret@proxy:8388
trojan://password@proxy:443
```
