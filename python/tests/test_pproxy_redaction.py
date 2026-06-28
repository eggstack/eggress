"""Security and redaction tests for pproxy translation helpers.

Ensures Python ergonomics do not leak credentials.
"""

import pytest
from eggress import (
    EggressConfig,
    EggressService,
    TranslationResult,
    translate_pproxy_args,
    translate_pproxy_uri,
)


CREDENTIAL_ARGS = [
    "-l", "socks5://admin:s3cret_p@ssw0rd@127.0.0.1:1080",
    "-r", "http://user:pass123@proxy:8080",
]

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


def test_translation_result_repr_redacts_credentials():
    """repr(TranslationResult) should not expose raw credentials."""
    result = translate_pproxy_args(CREDENTIAL_ARGS)
    r = repr(result)
    assert "s3cret_p@ssw0rd" not in r
    assert "pass123" not in r
    assert "TranslationResult" in r


def test_eggress_config_repr_redacts_credentials():
    """repr(EggressConfig) should not expose raw credentials."""
    config = EggressConfig.from_toml(CREDENTIAL_TOML)
    r = repr(config)
    assert "s3cret_p@ssw0rd" not in r
    assert "EggressConfig" in r


def test_redacted_toml_hides_password():
    """redacted_toml() should replace passwords with ****."""
    config = EggressConfig.from_toml(CREDENTIAL_TOML)
    redacted = config.redacted_toml()
    assert "s3cret_p@ssw0rd" not in redacted
    assert "****" in redacted
    # Username is not a secret
    assert "admin" in redacted


def test_translation_warnings_mention_plaintext_without_printing_secret():
    """Warnings about plaintext credentials should not include the actual password."""
    result = translate_pproxy_args(CREDENTIAL_ARGS)
    for w in result.warnings:
        assert "s3cret_p@ssw0rd" not in w.message
        assert "pass123" not in w.message


def test_exception_messages_redact_credentials():
    """Error/exception messages should not leak credentials."""
    from eggress import ConfigError

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


def test_generated_toml_contains_credentials_only_when_expected():
    """Generated TOML may contain credentials (they're needed for config),
    but the translation warning confirms this."""
    result = translate_pproxy_args(CREDENTIAL_ARGS)
    # The generated TOML will contain the password since it's a valid config
    assert "s3cret_p@ssw0rd" in result.toml or "password" in result.toml
    # But there should be a warning about plaintext credentials
    assert any(w.category == "credential-in-toml" for w in result.warnings)


def test_metrics_status_no_credentials():
    """Metrics and status should not expose credentials."""
    with EggressService.from_toml("""
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
""").start() as handle:
        metrics = handle.metrics_text()
        status = handle.status()

        # Metrics should contain no credential data
        assert "password" not in metrics.lower() or "password_bytes" in metrics.lower()
        assert "secret" not in metrics.lower()

        # Status should not expose credentials
        status_str = str(status)
        assert "password" not in status_str


def test_translation_warning_category_plaintext():
    """Translation of credentials should produce a credential-in-toml warning."""
    result = translate_pproxy_args([
        "-l", "socks5://user:pass@127.0.0.1:1080",
    ])
    assert any(w.category == "credential-in-toml" for w in result.warnings)
