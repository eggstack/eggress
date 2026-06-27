import pytest
import eggress
from eggress import EggressConfig, ConfigError


def test_import():
    """Verify package imports cleanly and exposes expected public API."""
    assert hasattr(eggress, "EggressConfig")
    assert hasattr(eggress, "EggressService")
    assert hasattr(eggress, "EggressHandle")
    assert hasattr(eggress, "EggressError")
    assert hasattr(eggress, "ConfigError")
    assert hasattr(eggress, "StartupError")
    assert hasattr(eggress, "ReloadError")
    assert hasattr(eggress, "ShutdownError")
    assert hasattr(eggress, "UnsupportedFeatureError")
    assert hasattr(eggress, "InternalError")

VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_from_toml():
    config = EggressConfig.from_toml(VALID_TOML)
    assert config is not None


def test_redacted_toml():
    config = EggressConfig.from_toml(VALID_TOML)
    redacted = config.redacted_toml()
    assert "127.0.0.1" in redacted


def test_invalid_toml():
    with pytest.raises(ConfigError):
        EggressConfig.from_toml("not valid toml {{{")


def test_invalid_version():
    with pytest.raises(ConfigError):
        EggressConfig.from_toml('version = 99\n\n[[listeners]]\nname = "x"\nbind = "127.0.0.1:0"\nprotocols = ["socks5"]')
