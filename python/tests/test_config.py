import pytest
from eggress import EggressConfig, ConfigError

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
