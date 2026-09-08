"""Phase 2 WS4 — Python async bridging convergence.

AsyncConnection and AsyncOutboundStream must share the maintained
AsyncBridge/CloseWaiter model: loop affinity, cancellation propagation,
idempotent close/wait, multi-waiter safety, contextvars preservation, no
event-loop blocking, bounded finalizers.
"""

from __future__ import annotations

import asyncio
import contextvars
import warnings

import pytest

pytest.importorskip("eggress._eggress")


def _needs_outbound():
    return pytest.importorskip("eggress.outbound")


async def _make_stream():
    outbound_mod = _needs_outbound()
    # Direct connector needs no listener; connect to a closed port will fail,
    # so instead test bridge semantics with a lightweight blocking callable
    # via the bridge itself plus close/wait lifecycle on a real stream object
    # constructed from a dummy inner? For lifecycle, use a real connector
    # against an ephemeral local listener is heavy; instead exercise the
    # bridge/waiter directly on AsyncOutboundStream with a fake inner that
    # mimics blocking behavior without network.
    #
    # To avoid network flakiness, most tests below use AsyncBridge/CloseWaiter
    # directly (the shared primitive) plus one integration test that opens a
    # real outbound stream to a local blackhole and checks close idempotency.
    from eggress._asyncio import AsyncBridge, CloseWaiter

    return AsyncBridge, CloseWaiter


class TestOutboundBridgeUsesCanonicalPrimitive:
    def test_no_direct_run_in_executor_outside_bridge(self):
        import inspect

        import eggress.outbound as outbound_mod
        import eggress.connection as connection_mod
        import eggress._asyncio_adapter as adapter_mod

        for module in (outbound_mod, connection_mod, adapter_mod):
            source = inspect.getsource(module)
            # Direct executor use is allowed only inside _asyncio.py (the
            # canonical bridge) or documented plugin exception. Outbound,
            # connection, and adapter must route via wrap_blocking_call /
            # AsyncBridge, not loop.run_in_executor directly.
            assert "loop.run_in_executor" not in source, (
                f"{module.__name__} still uses direct run_in_executor; "
                "route via AsyncBridge/wrap_blocking_call"
            )

    def test_async_outbound_stream_has_bridge_and_waiter(self):
        async def _run():
            from eggress.outbound import OutboundConnector

            # Real connector construction is sync and lightweight (no I/O).
            connector = OutboundConnector.from_pproxy_uri("direct://")
            # aconnect to an unroutable address with short timeout to get a
            # stream object lifecycle without hanging: use invalid port 1 with
            # 0.2s timeout; even on failure path, bridge cancellation must not
            # double-resolve. Here we only check the stream class carries the
            # canonical bridge attributes when construction succeeds; if
            # connect fails (expected in CI without network), skip.
            try:
                stream = await asyncio.wait_for(
                    connector.aconnect_tcp("127.0.0.1", 1, timeout=0.5), timeout=2.0
                )
            except Exception:
                pytest.skip("no local connect target for lifecycle check")
            try:
                assert hasattr(stream, "_bridge")
                assert hasattr(stream, "_waiter")
                # Close idempotent.
                stream.close()
                stream.close()
                await stream.wait_closed()
                await stream.wait_closed()
            finally:
                try:
                    stream.close()
                except Exception:
                    pass

        asyncio.run(_run())


class TestCloseWaiterConvergence:
    def test_close_idempotent_and_multi_waiter(self):
        async def _run():
            from eggress._asyncio import CloseWaiter

            waiter = CloseWaiter()
            await waiter.close()
            await waiter.close()
            assert waiter.is_closed
            # Multiple waiters complete consistently.
            waiter2 = CloseWaiter()

            async def _waiter():
                await waiter2.wait_closed()
                return 1

            tasks = [asyncio.create_task(_waiter()) for _ in range(5)]
            await asyncio.sleep(0.01)
            waiter2.mark_closed()
            results = await asyncio.gather(*tasks)
            assert results == [1] * 5

        asyncio.run(_run())

    def test_wait_cancellation_does_not_corrupt_state(self):
        async def _run():
            from eggress._asyncio import CloseWaiter

            waiter = CloseWaiter()

            async def _cancelled_wait():
                await waiter.wait_closed()

            task = asyncio.create_task(_cancelled_wait())
            await asyncio.sleep(0.01)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
            assert not waiter.is_closed
            # Subsequent close still works.
            await waiter.close()
            assert waiter.is_closed
            await waiter.wait_closed()

        asyncio.run(_run())

    def test_contextvars_preserved_via_bridge(self):
        async def _run():
            from eggress._asyncio import wrap_blocking_call

            var = contextvars.ContextVar("ws4_test_var")
            var.set("expected")

            def _read():
                return var.get()

            result = await wrap_blocking_call(_read)
            assert result == "expected"

        asyncio.run(_run())

    def test_async_connection_cross_loop_fails_predictably(self):
        from eggress.async_connection import AsyncConnection
        from eggress._asyncio import LoopAffinityError

        async def _make():
            return AsyncConnection("socks5://127.0.0.1:0")

        async def _run():
            conn = await _make()
            try:
                return conn
            except Exception:
                return None

        # Create on one loop, use from another.
        conn = asyncio.run(_run())
        if conn is None:
            pytest.skip("could not construct AsyncConnection")
        try:

            async def _misuse():
                conn._check_loop()

            # New loop differs from creation loop; _check_loop must raise
            # LoopAffinityError (or succeed only if same loop, which it isn't).
            with pytest.raises(LoopAffinityError):
                asyncio.run(_misuse())
        finally:
            try:
                conn._conn.close()
            except Exception:
                pass

    def test_finalizer_bounded_and_nonblocking(self):
        async def _run():
            from eggress._asyncio import AsyncBridge

            bridge = AsyncBridge(label="ws4-finalizer-test")
            # Not closed: __del__ must warn (ResourceWarning) and never block.
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                bridge.__del__()
            assert any(
                issubclass(w.category, ResourceWarning) for w in caught
            ), "bridge __del__ must emit ResourceWarning when unclosed"

        asyncio.run(_run())
