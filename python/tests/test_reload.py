import pytest
from eggress import EggressService, ReloadError

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

BAD_RELOAD_TOML = """
not valid toml {{{
"""


def test_reload_applied():
    with EggressService.from_toml(INITIAL_TOML).start() as handle:
        result = handle.reload_toml(RELOAD_TOML)
        assert "generation" in result
        assert result["generation"] >= 0


def test_bad_reload_preserves_service():
    with EggressService.from_toml(INITIAL_TOML).start() as handle:
        with pytest.raises(Exception):
            handle.reload_toml(BAD_RELOAD_TOML)
        assert handle.status()["readiness"] is True
