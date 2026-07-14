from __future__ import annotations

import asyncio
import warnings
from typing import Any, Optional

from eggress._asyncio import AsyncBridge, CloseWaiter, LoopAffinityError
from eggress.connection import Connection, ConnectionState


class AsyncConnection:
    """Async wrapper around Connection with loop affinity.

    Lifecycle invariants (Phase C5):

    - Created inside a running event loop; binds on first use.
    - Cross-loop use raises :class:`LoopAffinityError` before any native
      work begins.
    - Cancellation propagates to the executor future (best-effort).
    - ``aclose()`` / ``await_closed()`` are idempotent and race-safe.
    - Multiple ``await_closed()`` callers all unblock simultaneously.
    - ``__del__`` never blocks; issues a ``ResourceWarning`` and best-
      effort closes the underlying connection.

    Usage::

        conn = await AsyncConnection.open("socks5://127.0.0.1:0")
        try:
            # use connection
            pass
        finally:
            await conn.aclose()

    Or::

        async with AsyncConnection("socks5://127.0.0.1:0") as conn:
            pass
    """

    __slots__ = ("_conn", "_loop", "_bridge", "_waiter", "_closed")

    def __init__(self, *uris: str, **kwargs: Any) -> None:
        try:
            self._loop = asyncio.get_running_loop()
        except RuntimeError:
            raise RuntimeError(
                "AsyncConnection must be created inside a running event loop. "
                "Use Connection for synchronous usage."
            )
        self._conn = Connection(*uris, **kwargs)
        self._bridge = AsyncBridge(label="AsyncConnection")
        self._waiter = CloseWaiter()
        self._closed = False

    @classmethod
    async def open(cls, *uris: str, **kwargs: Any) -> AsyncConnection:
        """Create an AsyncConnection on the current running loop."""
        return cls(*uris, **kwargs)

    def _check_loop(self) -> None:
        """Verify we're being called from the correct event loop."""
        try:
            current_loop = asyncio.get_running_loop()
        except RuntimeError:
            return  # No running loop, skip check (sync context)
        if current_loop is not self._loop:
            raise LoopAffinityError(
                f"AsyncConnection was created on loop {self._loop!r} "
                f"but called from loop {current_loop!r}"
            )

    @property
    def state(self) -> str:
        self._check_loop()
        return self._conn.state

    @property
    def closed(self) -> bool:
        return self._closed or self._conn.closed

    @property
    def config(self) -> str:
        self._check_loop()
        return self._conn.config

    @property
    def peername(self) -> tuple[str, int] | None:
        self._check_loop()
        return self._conn.peername

    @property
    def sockname(self) -> tuple[str, int] | None:
        self._check_loop()
        return self._conn.sockname

    def extra_info(self) -> dict[str, Any]:
        self._check_loop()
        return self._conn.extra_info()

    async def aclose(self) -> None:
        """Async close. Idempotent and race-safe."""
        self._check_loop()
        if self._closed:
            return

        async def _cleanup() -> None:
            await self._bridge.run(self._conn.close)

        await self._waiter.close(_cleanup)
        self._closed = True
        self._bridge.close()

    async def await_closed(self) -> None:
        """Async wait for close. Idempotent and multi-waiter safe."""
        self._check_loop()
        if self._closed and self._waiter.is_closed:
            return
        await self._waiter.wait_closed()
        self._closed = True

    async def __aenter__(self) -> AsyncConnection:
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> bool:
        await self.aclose()
        return False

    def __del__(self) -> None:
        if getattr(self, "_closed", True):
            return
        if not hasattr(self, "_conn") or self._conn.closed:
            return
        warnings.warn(
            "AsyncConnection was not properly closed. "
            "Use 'async with' or call await aclose().",
            ResourceWarning,
            stacklevel=2,
        )
        try:
            self._conn.close()
        except Exception:
            pass
        self._closed = True
        self._bridge.close()

    def __repr__(self) -> str:
        state = "closed" if self._closed else self.state
        try:
            sock = self._conn.sockname
        except Exception:
            sock = None
        return f"AsyncConnection(state='{state}', sockname={sock!r})"

    def __bool__(self) -> bool:
        return not self._closed
