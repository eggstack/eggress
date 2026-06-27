from eggress import EggressService

VALID_TOML = """
version = 1

[[listeners]]
name = "socks"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"""


def test_metrics_text():
    with EggressService.from_toml(VALID_TOML).start() as handle:
        metrics = handle.metrics_text()
        assert "eggress_connections_total" in metrics


def test_status():
    with EggressService.from_toml(VALID_TOML).start() as handle:
        status = handle.status()
        assert status["readiness"] is True
        assert status["generation"] == 0
        assert status["listener_count"] == 1
        assert status["udp_associations_active"] == 0
        assert status["upstream_count"] == 0
