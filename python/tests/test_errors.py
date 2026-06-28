import pytest
from eggress import (
    EggressError,
    ConfigError,
    StartupError,  # noqa: F401
    ReloadError,  # noqa: F401
    ShutdownError,  # noqa: F401
    UnsupportedFeatureError,  # noqa: F401
    InternalError,  # noqa: F401
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


CREDENTIAL_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "password"
username = "admin"
password = "s3cret_p@ssw0rd"
"""


def test_redacted_toml_redacts_password():
    from eggress import EggressConfig
    config = EggressConfig.from_toml(CREDENTIAL_TOML)
    redacted = config.redacted_toml()
    assert "s3cret_p@ssw0rd" not in redacted
    assert "****" in redacted
    # username is not a secret — should be preserved
    assert "admin" in redacted


def test_error_message_does_not_leak_password():
    """Validation error on a config with credentials should not include them in the error string."""
    from eggress import EggressConfig
    bad_toml = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.auth]
type = "badauth"
username = "admin"
password = "hunter2_leaked"
"""
    with pytest.raises(ConfigError) as exc_info:
        EggressConfig.from_toml(bad_toml)
    assert "hunter2_leaked" not in str(exc_info.value)
