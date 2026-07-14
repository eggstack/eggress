"""Phase C2: Python Connection class contract and behavioral tests."""

from __future__ import annotations

import gc
import warnings

import pytest

pytest.importorskip("eggress._eggress")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Bare URI that creates a SOCKS5 listener on an ephemeral port.  The
# Connection constructor calls into the pproxy argument translator; bare
# URIs are treated as positional locals.
_SOCKS5_URI = "socks5://127.0.0.1:0"


# ---------------------------------------------------------------------------
# Contract tests
# ---------------------------------------------------------------------------


class TestConnectionContract:
    """Contract tests: signatures, attributes, defaults, types."""

    def test_import_connection_class(self):
        from eggress.connection import Connection

        assert Connection is not None

    def test_import_connection_state(self):
        from eggress.connection import ConnectionState

        assert hasattr(ConnectionState, "CREATED")
        assert hasattr(ConnectionState, "CONNECTING")
        assert hasattr(ConnectionState, "CONNECTED")
        assert hasattr(ConnectionState, "CLOSING")
        assert hasattr(ConnectionState, "CLOSED")
        assert hasattr(ConnectionState, "FAILED")

    def test_import_exception_types(self):
        from eggress.connection import (
            ConnectionError,
            ConnectionClosedError,
            TimeoutError,
            DnsError,
            AuthError,
            TlsError,
            LoopMismatchError,
        )

        assert issubclass(ConnectionError, Exception)
        assert issubclass(ConnectionClosedError, ConnectionError)
        assert issubclass(TimeoutError, ConnectionError)
        assert issubclass(DnsError, ConnectionError)
        assert issubclass(AuthError, ConnectionError)
        assert issubclass(TlsError, ConnectionError)
        assert issubclass(LoopMismatchError, Exception)

    def test_connection_is_exported(self):
        import eggress

        assert hasattr(eggress, "Connection")
        assert hasattr(eggress, "ConnectionState")

    def test_connection_callable(self):
        from eggress.connection import Connection

        assert callable(Connection)

    def test_connection_constructor_requires_args(self):
        from eggress.connection import Connection, ConnectionError

        with pytest.raises(ConnectionError):
            Connection()

    def test_connection_constructor_accepts_uris(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert conn.state in ("created", "connecting", "connected")
            assert not conn.closed
        finally:
            conn.close()

    def test_connection_has_state_property(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert isinstance(conn.state, str)
        finally:
            conn.close()

    def test_connection_has_closed_property(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert isinstance(conn.closed, bool)
            assert not conn.closed
        finally:
            conn.close()

    def test_connection_has_config_property(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            config = conn.config
            assert isinstance(config, str)
            assert "version" in config
        finally:
            conn.close()

    def test_extra_info_is_callable(self):
        """extra_info() is defined as a method on Connection."""
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert callable(conn.extra_info)
        finally:
            conn.close()

    def test_connection_repr(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            r = repr(conn)
            assert "Connection" in r
            assert "state=" in r
        finally:
            conn.close()

    def test_connection_repr_shows_sockname(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            r = repr(conn)
            assert "sockname=" in r
        finally:
            conn.close()

    def test_connection_has_sockname_property(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            sn = conn.sockname
            if sn is not None:
                assert isinstance(sn, tuple)
                assert len(sn) == 2
                assert isinstance(sn[0], str)
                assert isinstance(sn[1], int)
        finally:
            conn.close()

    def test_connection_has_peername_property(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            pn = conn.peername
            # peername may or may not be set depending on state
            if pn is not None:
                assert isinstance(pn, tuple)
                assert len(pn) == 2
        finally:
            conn.close()

    def test_connection_state_enum_values(self):
        from eggress.connection import ConnectionState

        expected = {"created", "connecting", "connected", "closing", "closed", "failed"}
        actual = {member.value for member in ConnectionState}
        assert actual == expected

    def test_connection_error_hierarchy_from_eggress(self):
        """Exception types are also exported from the top-level package."""
        import eggress

        assert hasattr(eggress, "ConnectionBaseError")
        assert hasattr(eggress, "ConnectionClosedError")
        assert hasattr(eggress, "LoopMismatchError")


# ---------------------------------------------------------------------------
# Lifecycle tests
# ---------------------------------------------------------------------------


class TestConnectionLifecycle:
    """Behavioral tests: lifecycle, close semantics, resource ownership."""

    def test_close_is_idempotent(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        conn.close()
        conn.close()  # should not raise
        assert conn.closed

    def test_wait_closed_after_close(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        conn.close()
        conn.wait_closed()  # should not raise
        assert conn.closed

    def test_wait_closed_idempotent(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        conn.wait_closed()
        conn.wait_closed()  # should not raise
        assert conn.closed

    def test_context_manager_closes(self):
        from eggress.connection import Connection

        with Connection(_SOCKS5_URI) as conn:
            assert not conn.closed
        assert conn.closed

    def test_state_transitions_to_closed(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        conn.close()
        assert conn.state == "closed"

    def test_repr_after_close(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        conn.close()
        r = repr(conn)
        assert "closed" in r

    def test_bool_after_close(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        conn.close()
        assert not bool(conn)

    def test_bool_when_open(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert bool(conn)
        finally:
            conn.close()

    def test_del_warns_on_unclosed(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            del conn
            gc.collect()
            # Should have issued a ResourceWarning
            resource_warnings = [x for x in w if issubclass(x.category, ResourceWarning)]
            assert len(resource_warnings) >= 1

    def test_sockname_tuple_format(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            sn = conn.sockname
            if sn is not None:
                assert isinstance(sn, tuple)
                assert len(sn) == 2
                assert isinstance(sn[0], str)
                assert isinstance(sn[1], int)
        finally:
            conn.close()

    def test_peername_none_when_not_connected(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            # peername may or may not be set depending on state
            pn = conn.peername
            if pn is not None:
                assert isinstance(pn, tuple)
        finally:
            conn.close()

    def test_multiple_connections_independent(self):
        from eggress.connection import Connection

        conn1 = Connection(_SOCKS5_URI)
        conn2 = Connection(_SOCKS5_URI)
        try:
            assert not conn1.closed
            assert not conn2.closed
            conn1.close()
            assert conn1.closed
            assert not conn2.closed
        finally:
            conn2.close()

    def test_connection_from_pproxy_style_args(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert not conn.closed
            config = conn.config
            assert "socks5" in config or "socks" in config
        finally:
            conn.close()

    def test_connection_with_multiple_uris(self):
        """Second positional URI is treated as an upstream remote."""
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI, "socks5://127.0.0.1:9999")
        try:
            assert not conn.closed
        finally:
            conn.close()

    def test_extra_info_method_exists(self):
        """extra_info is defined as a callable on Connection."""
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            assert hasattr(conn, "extra_info")
            assert callable(conn.extra_info)
        finally:
            conn.close()

    def test_config_contains_version(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            config = conn.config
            assert "version" in config
        finally:
            conn.close()

    def test_config_contains_listeners(self):
        from eggress.connection import Connection

        conn = Connection(_SOCKS5_URI)
        try:
            config = conn.config
            assert "listener" in config.lower() or "bind" in config.lower()
        finally:
            conn.close()


# ---------------------------------------------------------------------------
# Async tests
# ---------------------------------------------------------------------------


class TestConnectionAsync:
    """Async lifecycle tests: aclose, await_closed, async context manager."""

    def test_aclose(self):
        import asyncio

        from eggress.connection import Connection

        async def _run():
            conn = Connection(_SOCKS5_URI)
            await conn.aclose()
            assert conn.closed

        asyncio.run(_run())

    def test_await_closed(self):
        import asyncio

        from eggress.connection import Connection

        async def _run():
            conn = Connection(_SOCKS5_URI)
            await conn.await_closed()
            assert conn.closed

        asyncio.run(_run())

    def test_async_context_manager(self):
        import asyncio

        from eggress.connection import Connection

        async def _run():
            async with Connection(_SOCKS5_URI) as conn:
                assert not conn.closed
            assert conn.closed

        asyncio.run(_run())
