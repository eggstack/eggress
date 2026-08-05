"""Smoke-test an installed Eggress wheel or source distribution."""

from __future__ import annotations

from pathlib import Path

import eggress
import pproxy
from eggress import EggressService, pproxy as eggress_pproxy


def _assert_installed(module: object, name: str) -> None:
    path = Path(getattr(module, "__file__", "")).resolve()
    assert "site-packages" in path.parts or "dist-packages" in path.parts, (
        f"{name} resolved outside the installed environment: {path}"
    )


def main() -> None:
    _assert_installed(eggress, "eggress")
    _assert_installed(pproxy, "pproxy")
    assert hasattr(eggress, "EggressService")
    assert hasattr(pproxy, "Server")

    toml = """
version = 1

[[listeners]]
name = "release-smoke"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""
    service = EggressService.from_toml(toml)
    with service.start() as handle:
        addresses = handle.bound_addresses
        assert addresses.get("release-smoke"), addresses
        assert handle.status().get("readiness") is True

    # EggressHandle is intentionally consumed by its context manager, so its
    # post-shutdown status is not exposed. Verify the equivalent public
    # compatibility handle's readiness transition as well.
    server = eggress_pproxy.Server(listen=["socks5://127.0.0.1:0"])
    server.start()
    assert server.is_ready is True
    assert server.addresses
    server.close()
    assert server.is_ready is False
    print(f"smoke passed: eggress={eggress.__version__} pproxy={pproxy.__file__}")


if __name__ == "__main__":
    main()
