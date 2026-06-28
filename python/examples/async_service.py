"""Async usage of the eggress Python bindings."""

import asyncio

from eggress import EggressService

TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


async def main():
    svc = EggressService.from_toml(TOML)

    async with await svc.astart() as handle:
        print("Listening on", await handle.bound_addresses)
        print("Status:", await handle.status())

        # Async metrics
        metrics = await handle.metrics_text()
        print("Metrics preview:", metrics[:200])


if __name__ == "__main__":
    asyncio.run(main())
