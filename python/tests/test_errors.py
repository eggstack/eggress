import pytest
from eggress import (
    EggressError,
    ConfigError,
    StartupError,
    ReloadError,
    ShutdownError,
    UnsupportedFeatureError,
    InternalError,
)

VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_invalid_config_raises_config_error():
    with pytest.raises(ConfigError):
        from eggress import EggressConfig
        EggressConfig.from_toml("not valid {{{")


def test_invalid_config_is_eggress_error():
    with pytest.raises(EggressError):
        from eggress import EggressConfig
        EggressConfig.from_toml("not valid {{{")


def test_shutdown_idempotent():
    from eggress import EggressService
    handle = EggressService.from_toml(VALID_TOML).start()
    handle.shutdown()
    handle.shutdown()
