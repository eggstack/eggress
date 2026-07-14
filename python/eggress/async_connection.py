from __future__ import annotations

import asyncio
import warnings
from typing import Any, Optional

from eggress.connection import Connection, ConnectionState, LoopMismatchError


class AsyncConnection:
    """Async wrapper around Connection with loop affinity.

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

    __slots__ = ("_conn", "_loop", "_closed")

    def __init__(self, *uris: str, **kwargs: Any) -> None:
        try:
            self._loop = asyncio.get_running_loop()
        except RuntimeError:
            raise RuntimeError(
                "AsyncConnection must be created inside a running event loop. "
                "Use Connection for synchronous usage."
            )
        self._conn = Connection(*uris, **kwargs)
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
            raise LoopMismatchError(
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
        """Async close."""
        self._check_loop()
        if self._closed:
            return
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._conn.close)
        self._closed = True

    async def await_closed(self) -> None:
        """Async wait for close."""
        self._check_loop()
        if self._closed:
            return
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._conn.wait_closed)
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
        if not self._closed and hasattr(self, "_conn") and not self._conn.closed:
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

    def __repr__(self) -> str:
        state = "closed" if self._closed else self.state
        try:
            sock = self._conn.sockname
        except Exception:
            sock = None
        return f"AsyncConnection(state='{state}', sockname={sock!r})"

    def __bool__(self) -> bool:
        return not self._closed
