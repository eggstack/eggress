"""Start an eggress SOCKS5 proxy from a TOML configuration string."""

from eggress import EggressService

TOML = """
version = 1

[[listeners]]
name = "proxy"
bind = "127.0.0.1:1080"
protocols = ["socks5"]
"""

with EggressService.from_toml(TOML).start() as handle:
    print("Listening on", handle.bound_addresses)
    print("Status:", handle.status()["readiness"])
    print("Press Ctrl+C to stop")
    try:
        while True:
            pass
    except KeyboardInterrupt:
        pass
