import pytest
from eggress import EggressService, EggressError

VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_from_toml():
    svc = EggressService.from_toml(VALID_TOML)
    assert svc is not None


def test_start_and_stop():
    with EggressService.from_toml(VALID_TOML).start() as handle:
        addrs = handle.bound_addresses
        assert "socks" in addrs
        assert addrs["socks"] != ""


def test_context_manager_shuts_down():
    svc = EggressService.from_toml(VALID_TOML)
    with svc.start() as handle:
        assert handle.status()["readiness"] is True
    with pytest.raises(EggressError):
        handle.status()

