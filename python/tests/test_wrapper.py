"""Tests for python/eggress/wrapper.py — Phase C4 Workstream 4.

Tests BaseWrapper, TLS, Plugin, Chain, and normalize_chain.
"""

from __future__ import annotations

import copy
import pickle

import pytest

from eggress.protocol import BaseProtocol, Direct, HTTP, SS, Socks5, Trojan
from eggress.wrapper import (
    BaseWrapper,
    Chain,
    Plugin,
    TLS,
    normalize_chain,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


class _DummyProtocol(BaseProtocol):
    """Minimal protocol for testing wrapper delegation."""

    _SUPPORTED_IN_EGRESS = True
    _TRAFFIC_KINDS = ("tcp", "udp")
    _ROLE = "upstream"

    def __init__(
        self,
        param: str = "",
        target: str | None = "target-host",
        dest: str | None = "dest-host",
        source: str | None = "source-host",
    ) -> None:
        super().__init__(param, target=target, dest=dest, source=source)


# ---------------------------------------------------------------------------
# TestBaseWrapper
# ---------------------------------------------------------------------------


class TestBaseWrapper:
    def test_wrapper_delegates_target(self) -> None:
        proto = _DummyProtocol()
        w = TLS(proto)
        assert w.target == "target-host"

    def test_wrapper_delegates_dest(self) -> None:
        proto = _DummyProtocol()
        w = TLS(proto)
        assert w.dest == "dest-host"

    def test_wrapper_delegates_source(self) -> None:
        proto = _DummyProtocol()
        w = TLS(proto)
        assert w.source == "source-host"

    def test_wrapper_delegates_metadata(self) -> None:
        proto = _DummyProtocol()
        w = Plugin(proto)
        assert w._SUPPORTED_IN_EGRESS is True
        assert w._TRAFFIC_KINDS == ("tcp", "udp")
        assert w._ROLE == "upstream"

    def test_wrapper_delegates_metadata_default(self) -> None:
        proto = Direct()
        w = TLS(proto)
        assert w._SUPPORTED_IN_EGRESS is True
        assert w._TRAFFIC_KINDS == ("tcp",)
        assert w._ROLE == "both"

    def test_wrapper_equality(self) -> None:
        proto = Direct()
        a = TLS(proto)
        b = TLS(proto)
        assert a == b

    def test_wrapper_equality_different_type(self) -> None:
        proto = Direct()
        a = TLS(proto)
        b = Plugin(proto)
        assert a != b

    def test_wrapper_hash(self) -> None:
        proto = Direct()
        a = TLS(proto)
        b = TLS(proto)
        assert hash(a) == hash(b)

    def test_wrapper_hash_different_inner(self) -> None:
        a = TLS(Direct())
        b = TLS(HTTP())
        assert hash(a) != hash(b)


# ---------------------------------------------------------------------------
# TestTLS
# ---------------------------------------------------------------------------


class TestTLS:
    def test_tls_name(self) -> None:
        t = TLS(Direct())
        assert t.name == "tls"

    def test_tls_repr_no_secrets(self) -> None:
        t = TLS(Direct(), certfile="/etc/ssl/cert.pem", keyfile="/etc/ssl/key.pem")
        r = repr(t)
        assert "/etc/ssl/" not in r
        assert "cert.pem" in r
        assert "key.pem" in r

    def test_tls_stores_cert(self) -> None:
        t = TLS(Direct(), certfile="/path/to/cert.pem")
        assert t.certfile == "/path/to/cert.pem"

    def test_tls_stores_key(self) -> None:
        t = TLS(Direct(), keyfile="/path/to/key.pem")
        assert t.keyfile == "/path/to/key.pem"

    def test_tls_stores_sni(self) -> None:
        t = TLS(Direct(), sni="example.com")
        assert t.sni == "example.com"

    def test_tls_defaults_none(self) -> None:
        t = TLS(Direct())
        assert t.certfile is None
        assert t.keyfile is None
        assert t.sni is None

    def test_tls_pickle(self) -> None:
        original = TLS(
            SS("aes-256-gcm:pass"),
            certfile="/cert.pem",
            keyfile="/key.pem",
            sni="example.com",
        )
        restored = pickle.loads(pickle.dumps(original))
        assert restored == original
        assert restored.certfile == "/cert.pem"
        assert restored.keyfile == "/key.pem"
        assert restored.sni == "example.com"

    def test_tls_copy(self) -> None:
        original = TLS(Direct(), certfile="/cert.pem")
        copied = copy.copy(original)
        assert copied == original
        assert copied is not original

    def test_tls_deepcopy(self) -> None:
        original = TLS(SS("aes-256-gcm:pass"), sni="example.com")
        copied = copy.deepcopy(original)
        assert copied == original
        assert copied is not original

    def test_tls_inner_reference(self) -> None:
        proto = Direct()
        t = TLS(proto)
        assert t.inner is proto

    def test_tls_equality(self) -> None:
        a = TLS(Direct(), sni="a.com")
        b = TLS(Direct(), sni="a.com")
        c = TLS(Direct(), sni="b.com")
        assert a == b
        assert a != c

    def test_tls_repr_no_params(self) -> None:
        t = TLS(Direct())
        assert repr(t) == "TLS(Direct())"


# ---------------------------------------------------------------------------
# TestPlugin
# ---------------------------------------------------------------------------


class TestPlugin:
    def test_plugin_name(self) -> None:
        p = Plugin(Direct())
        assert p.name == "plugin"

    def test_plugin_repr(self) -> None:
        p = Plugin(Direct(), handler="my_handler")
        r = repr(p)
        assert "Plugin(" in r
        assert "handler=" in r

    def test_plugin_stores_handler(self) -> None:
        handler = lambda data: data  # noqa: E731
        p = Plugin(Direct(), handler=handler)
        assert p.handler is handler

    def test_plugin_handler_none(self) -> None:
        p = Plugin(Direct())
        assert p.handler is None

    def test_plugin_pickle(self) -> None:
        original = Plugin(Socks5(), handler="test_handler")
        restored = pickle.loads(pickle.dumps(original))
        assert restored == original
        assert restored.handler == "test_handler"

    def test_plugin_copy(self) -> None:
        original = Plugin(Direct(), handler="h")
        copied = copy.copy(original)
        assert copied == original
        assert copied is not original

    def test_plugin_deepcopy(self) -> None:
        original = Plugin(HTTP(), handler="h")
        copied = copy.deepcopy(original)
        assert copied == original
        assert copied is not original

    def test_plugin_inner_reference(self) -> None:
        proto = Direct()
        p = Plugin(proto)
        assert p.inner is proto

    def test_plugin_equality(self) -> None:
        a = Plugin(Direct(), handler="h")
        b = Plugin(Direct(), handler="h")
        c = Plugin(Direct(), handler="other")
        assert a == b
        assert a != c

    def test_plugin_repr_no_handler(self) -> None:
        p = Plugin(Direct())
        r = repr(p)
        assert "handler=" not in r


# ---------------------------------------------------------------------------
# TestChain
# ---------------------------------------------------------------------------


class TestChain:
    def test_chain_length(self) -> None:
        c = Chain([SS("aes-256-gcm:pass"), TLS(Direct())])
        assert len(c) == 2

    def test_chain_length_empty(self) -> None:
        c = Chain([])
        assert len(c) == 0

    def test_chain_getitem(self) -> None:
        ss = SS("aes-256-gcm:pass")
        tls = TLS(Direct())
        c = Chain([ss, tls])
        assert c[0] is ss
        assert c[1] is tls

    def test_chain_getitem_negative(self) -> None:
        ss = SS("aes-256-gcm:pass")
        c = Chain([ss])
        assert c[-1] is ss

    def test_chain_iter(self) -> None:
        items = [Direct(), HTTP(), Socks5()]
        c = Chain(items)
        assert list(c) == items

    def test_chain_equality(self) -> None:
        a = Chain([Direct(), HTTP()])
        b = Chain([Direct(), HTTP()])
        assert a == b

    def test_chain_equality_different_order(self) -> None:
        a = Chain([Direct(), HTTP()])
        b = Chain([HTTP(), Direct()])
        assert a != b

    def test_chain_hash(self) -> None:
        a = Chain([Direct(), HTTP()])
        b = Chain([Direct(), HTTP()])
        assert hash(a) == hash(b)

    def test_chain_hash_different(self) -> None:
        a = Chain([Direct()])
        b = Chain([HTTP()])
        assert hash(a) != hash(b)

    def test_chain_flat(self) -> None:
        ss = SS("aes-256-gcm:pass")
        tls = TLS(Direct())
        c = Chain([ss, tls])
        flat = c.flat()
        assert len(flat) == 2
        assert flat[0] is ss
        assert isinstance(flat[1], Direct)

    def test_chain_flat_nested_wrappers(self) -> None:
        ss = SS("aes-256-gcm:pass")
        plugin_tls = Plugin(TLS(Direct()))
        c = Chain([ss, plugin_tls])
        flat = c.flat()
        assert len(flat) == 2
        assert flat[0] is ss
        assert isinstance(flat[1], Direct)

    def test_chain_target(self) -> None:
        proto = _DummyProtocol()
        c = Chain([proto, TLS(Direct())])
        assert c.target == "target-host"

    def test_chain_dest(self) -> None:
        proto = _DummyProtocol()
        c = Chain([TLS(Direct()), proto])
        assert c.dest == "dest-host"

    def test_chain_repr(self) -> None:
        c = Chain([Direct(), HTTP()])
        r = repr(c)
        assert r.startswith("Chain([")
        assert "Direct()" in r
        assert "HTTP()" in r

    def test_chain_repr_empty(self) -> None:
        c = Chain([])
        assert repr(c) == "Chain([])"

    def test_chain_pickle(self) -> None:
        original = Chain([Direct(), HTTP()])
        restored = pickle.loads(pickle.dumps(original))
        assert restored == original

    def test_chain_contains(self) -> None:
        d = Direct()
        c = Chain([d, HTTP()])
        assert d in c
        assert Socks5() not in c

    def test_chain_protocols_tuple(self) -> None:
        c = Chain([Direct(), HTTP()])
        assert isinstance(c.protocols, tuple)
        assert len(c.protocols) == 2


# ---------------------------------------------------------------------------
# TestChainValidate
# ---------------------------------------------------------------------------


class TestChainValidate:
    def test_validate_valid_chain(self) -> None:
        c = Chain([SS("aes-256-gcm:pass"), TLS(Direct())])
        errors = c.validate()
        assert errors == []

    def test_validate_empty_chain(self) -> None:
        c = Chain([])
        errors = c.validate()
        assert len(errors) == 1
        assert "empty" in errors[0]


# ---------------------------------------------------------------------------
# TestNormalizeChain
# ---------------------------------------------------------------------------


class TestNormalizeChain:
    def test_normalize_removes_none(self) -> None:
        result = normalize_chain([Direct(), None, HTTP()])
        assert len(result) == 2
        assert result[0] == Direct()
        assert result[1] == HTTP()

    def test_normalize_removes_empty_string(self) -> None:
        result = normalize_chain([Direct(), "", HTTP()])
        assert len(result) == 2

    def test_normalize_orders_wrappers(self) -> None:
        tls = TLS(Direct())
        plugin = Plugin(HTTP())
        ss = SS("aes-256-gcm:pass")
        result = normalize_chain([tls, plugin, ss])
        assert len(result) == 3
        assert isinstance(result[0], SS)
        assert isinstance(result[1], TLS)
        assert isinstance(result[2], Plugin)

    def test_normalize_single_protocol(self) -> None:
        result = normalize_chain([Direct()])
        assert len(result) == 1
        assert result[0] == Direct()

    def test_normalize_empty(self) -> None:
        result = normalize_chain([])
        assert len(result) == 0

    def test_normalize_nested_wrappers(self) -> None:
        inner = TLS(Direct())
        plugin = Plugin(inner)
        ss = SS("aes-256-gcm:pass")
        result = normalize_chain([plugin, ss])
        assert len(result) == 2
        assert isinstance(result[0], SS)
        assert isinstance(result[1], Plugin)

    def test_normalize_all_wrappers(self) -> None:
        tls = TLS(Direct())
        plugin = Plugin(HTTP())
        result = normalize_chain([tls, plugin])
        assert len(result) == 2
        assert isinstance(result[0], TLS)
        assert isinstance(result[1], Plugin)

    def test_normalize_none_only(self) -> None:
        result = normalize_chain([None, None])
        assert len(result) == 0

    def test_normalize_preserves_protocol_order(self) -> None:
        ss = SS("aes-256-gcm:pass")
        trojan = Trojan()
        result = normalize_chain([trojan, ss])
        assert result[0] == trojan
        assert result[1] == ss

    def test_normalize_tls_chain(self) -> None:
        tls1 = TLS(Direct(), sni="a.com")
        tls2 = TLS(HTTP(), sni="b.com")
        result = normalize_chain([tls1, tls2])
        assert len(result) == 2
        assert isinstance(result[0], TLS)
        assert isinstance(result[1], TLS)

    def test_normalize_returns_chain(self) -> None:
        result = normalize_chain([Direct()])
        assert isinstance(result, Chain)


# ---------------------------------------------------------------------------
# Integration: wrapper + protocol
# ---------------------------------------------------------------------------


class TestWrapperProtocolIntegration:
    def test_tls_wrapping_ssocks5(self) -> None:
        s = Socks5()
        t = TLS(s, sni="proxy.example.com")
        assert t.inner is s
        assert t.target is None
        assert t.sni == "proxy.example.com"

    def test_plugin_wrapping_ss(self) -> None:
        ss = SS("aes-256-gcm:pass")
        p = Plugin(ss, handler=lambda x: x)
        assert p.inner is ss
        assert p._SUPPORTED_IN_EGRESS is True
        assert p._TRAFFIC_KINDS == ("tcp", "udp")

    def test_tls_plugin_chain(self) -> None:
        ss = SS("aes-256-gcm:pass")
        tls = TLS(ss, sni="example.com")
        plugin = Plugin(tls, handler="h")
        flat = Chain([plugin]).flat()
        assert len(flat) == 1
        assert isinstance(flat[0], SS)

    def test_normalize_full_composition(self) -> None:
        ss = SS("aes-256-gcm:pass")
        tls = TLS(ss, sni="example.com")
        plugin = Plugin(tls, handler="h")
        result = normalize_chain([plugin])
        assert len(result) == 1
        assert isinstance(result[0], Plugin)
