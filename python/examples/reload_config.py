"""Reload an eggress service configuration at runtime."""

from eggress import EggressService

INITIAL_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""

RELOAD_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""

with EggressService.from_toml(INITIAL_TOML).start() as handle:
    print("Started, generation:", handle.status()["generation"])

    # Hot-reload (routing/upstreams only; listener topology unchanged)
    result = handle.reload_toml(RELOAD_TOML)
    print("Reloaded, new generation:", result["generation"])
    print("Upstreams:", result["upstreams"])
