"""Low-maintenance agreement checks for maintained package exports."""

import eggress


def test_all_public_exports_are_bound_in_native_test_environment():
    missing = [name for name in eggress.__all__ if not hasattr(eggress, name)]
    assert not missing, f"__all__ contains missing exports: {missing}"


def test_capabilities_contract_is_stable():
    capabilities = eggress.capabilities()
    assert capabilities["supported_protocols"] == [
        "http", "socks4", "socks4a", "socks5", "shadowsocks", "trojan"
    ]
    assert capabilities["supported_schedulers"] == [
        "round_robin", "least_connections", "first_available", "random"
    ]
