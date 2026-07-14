from __future__ import annotations

import asyncio
import warnings
from enum import Enum
from typing import Any, Optional, Sequence

from eggress._eggress import (
    EggressError,
    UnsupportedFeatureError,
    PyConnection as _PyConnection,
)


class ConnectionState(str, Enum):
    CREATED = "created"
    CONNECTING = "connecting"
    CONNECTED = "connected"
    CLOSING = "closing"
    CLOSED = "closed"
    FAILED = "failed"


class ConnectionError(EggressError):
    pass


class ConnectionClosedError(ConnectionError):
    pass


class TimeoutError(ConnectionError):
    pass


class DnsError(ConnectionError):
    pass


class AuthError(ConnectionError):
    pass


class TlsError(ConnectionError):
    pass


class LoopMismatchError(EggressError):
    pass


class Connection:
    __slots__ = ("_inner", "_state", "_closed")

    def __init__(self, *uris: str, **kwargs: Any) -> None:
        if not uris:
            raise ConnectionError("at least one URI argument is required")

        try:
            self._inner = _PyConnection(list(uris))
        except UnsupportedFeatureError as e:
            raise ConnectionError(str(e)) from e
        except EggressError as e:
            raise ConnectionError(str(e)) from e

        self._state = ConnectionState.CREATED
        self._closed = False

    @property
    def state(self) -> str:
        if self._closed:
            return ConnectionState.CLOSED.value
        try:
            return self._inner.state
        except Exception:
            return self._state.value

    @property
    def closed(self) -> bool:
        return self._closed or self._inner.closed

    @property
    def config(self) -> str:
        return self._inner.config

    @property
    def peername(self) -> tuple[str, int] | None:
        raw = self._inner.peername
        if raw is None:
            return None
        try:
            host, port_str = raw.rsplit(":", 1)
            return (host.strip("[]"), int(port_str))
        except (ValueError, IndexError):
            return None

    @property
    def sockname(self) -> tuple[str, int] | None:
        raw = self._inner.sockname
        if raw is None:
            return None
        try:
            host, port_str = raw.rsplit(":", 1)
            return (host.strip("[]"), int(port_str))
        except (ValueError, IndexError):
            return None

    def extra_info(self) -> dict[str, Any]:
        return self._inner.extra_info()

    def close(self) -> None:
        if self._closed:
            return
        self._state = ConnectionState.CLOSING
        try:
            self._inner.close()
        except Exception as e:
            self._state = ConnectionState.FAILED
            raise ConnectionError(f"close failed: {e}") from e
        self._closed = True
        self._state = ConnectionState.CLOSED

    def wait_closed(self) -> None:
        if self._closed:
            return
        self.close()

    async def aclose(self) -> None:
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self.close)

    async def await_closed(self) -> None:
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self.wait_closed)

    def __enter__(self) -> Connection:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> bool:
        self.close()
        return False

    async def __aenter__(self) -> Connection:
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
        if not getattr(self, "_closed", True):
            warnings.warn(
                "Connection was not properly closed. Use 'with' statement or call close().",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self._inner.close()
            except Exception:
                pass
            self._closed = True

    def __repr__(self) -> str:
        state = "closed" if self._closed else self.state
        try:
            sock = self._inner.sockname
        except Exception:
            sock = None
        return f"Connection(state='{state}', sockname={sock!r})"

    def __bool__(self) -> bool:
        return not self._closed
