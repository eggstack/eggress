import pytest
from eggress import EggressService, EggressError, __version__
from eggress._eggress import __version__ as _native_version

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


def test_version_matches_native_module():
    """Python __version__ must match the native module's Cargo-derived version."""
    assert __version__ == _native_version
    assert isinstance(__version__, str)
    parts = __version__.split(".")
    assert len(parts) >= 2, f"Version string has fewer than 2 dot-separated parts: {__version__}"


def test_shutdown_is_idempotent():
    """Calling shutdown() twice must not raise."""
    handle = EggressService.from_toml(VALID_TOML).start()
    assert handle.status()["readiness"] is True
    handle.shutdown()
    # Second call must be a safe no-op
    handle.shutdown()


def test_context_manager_on_exception():
    """Context manager must shut down even when the body raises."""
    exited = False
    try:
        with EggressService.from_toml(VALID_TOML).start() as handle:
            assert handle.status()["readiness"] is True
            raise ValueError("intentional")
    except ValueError:
        exited = True
    assert exited, "ValueError should have propagated"
    # Handle should already be shut down; status() should fail
    with pytest.raises(EggressError):
        handle.status()

