from __future__ import annotations

import asyncio
from os import PathLike
from typing import Any, Optional, Sequence, Union

from eggress._eggress import (
    PyEggressService,
    PyEggressHandle,
    UnsupportedFeatureError,
)
from eggress.config import EggressConfig


class EggressService:
    """Pre-start service builder."""

    __slots__ = ("_inner",)

    def __init__(self, config: EggressConfig) -> None:
        object.__setattr__(
            self, "_inner", PyEggressService(config._inner)
        )

    @classmethod
    def from_toml(cls, toml: str) -> EggressService:
        """Parse TOML and create a service."""
        return cls(EggressConfig.from_toml(toml))

    @classmethod
    def from_file(cls, path: Union[str, PathLike[str]]) -> EggressService:
        """Load file and create a service."""
        return cls(EggressConfig.from_file(path))

    @classmethod
    def from_pproxy_args(
        cls,
        args: Sequence[str],
        allow_partial: bool = False,
    ) -> EggressService:
        """Create a service from pproxy-style CLI arguments.

        Translates pproxy arguments to eggress TOML configuration and creates
        a service from the result.

        Args:
            args: pproxy-style CLI arguments (e.g. ["-l", "socks5://:1080", "-r", "http://proxy:8080"]).
            allow_partial: If True, start even when unsupported features are detected.
                If False (default), raise UnsupportedFeatureError on unsupported features.

        Returns:
            A pre-start EggressService.

        Raises:
            UnsupportedFeatureError: If unsupported features exist and allow_partial is False.
            ConfigError: If the translated configuration is invalid.
        """
        from eggress.pproxy import translate_pproxy_args

        result = translate_pproxy_args(args)
        if not allow_partial and not result.ok:
            features = ", ".join(
                f"{u.feature}: {u.message}" for u in result.unsupported
            )
            raise UnsupportedFeatureError(
                f"unsupported pproxy features: {features}"
            )
        return cls(result.config())

    def start(self) -> EggressHandle:
        """Start the service and return a handle."""
        handle = self._inner.start()
        return EggressHandle(handle)

    async def astart(self) -> AsyncEggressHandle:
        """Start the service asynchronously and return an async handle.

        The blocking start() runs in a thread executor to avoid blocking
        the asyncio event loop.
        """
        handle = await asyncio.to_thread(self._inner.start)
        return AsyncEggressHandle(handle)


class EggressHandle:
    """Handle to a running eggress service."""

    __slots__ = ("_inner",)

    def __init__(self, _inner: PyEggressHandle) -> None:
        object.__setattr__(self, "_inner", _inner)

    @property
    def bound_addresses(self) -> dict[str, str]:
        """Addresses the service is listening on."""
        return self._inner.bound_addresses()

    def status(self) -> dict[str, Any]:
        """Current service status."""
        return self._inner.status()

    def metrics_text(self) -> str:
        """Prometheus metrics text."""
        return self._inner.metrics_text()

    def reload_toml(self, toml: str) -> dict[str, Any]:
        """Reload configuration from a TOML string."""
        return self._inner.reload_toml(toml)

    def shutdown(self) -> None:
        """Initiate graceful shutdown."""
        self._inner.shutdown()

    def __enter__(self) -> EggressHandle:
        self._inner.__enter__()
        return self

    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: Optional[Any],
    ) -> bool:
        return self._inner.__exit__(exc_type, exc_val, exc_tb)

    def __repr__(self) -> str:
        return "EggressHandle(...)"


class AsyncEggressHandle:
    """Async handle to a running eggress service.

    All blocking operations are delegated to a thread executor to avoid
    blocking the asyncio event loop. Supports the async context manager protocol.
    """

    __slots__ = ("_inner",)

    def __init__(self, _inner: PyEggressHandle) -> None:
        object.__setattr__(self, "_inner", _inner)

    async def bound_addresses(self) -> dict[str, str]:
        """Addresses the service is listening on."""
        return await asyncio.to_thread(self._inner.bound_addresses)

    async def status(self) -> dict[str, Any]:
        """Current service status."""
        return await asyncio.to_thread(self._inner.status)

    async def metrics_text(self) -> str:
        """Prometheus metrics text."""
        return await asyncio.to_thread(self._inner.metrics_text)

    async def reload_toml(self, toml: str) -> dict[str, Any]:
        """Reload configuration from a TOML string."""
        return await asyncio.to_thread(self._inner.reload_toml, toml)

    async def shutdown(self) -> None:
        """Initiate graceful shutdown."""
        await asyncio.to_thread(self._inner.shutdown)

    async def __aenter__(self) -> AsyncEggressHandle:
        return self

    async def __aexit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: Optional[Any],
    ) -> bool:
        await self.shutdown()
        return False

    def __repr__(self) -> str:
        return "AsyncEggressHandle(...)"


# Type alias for the pproxy-compatible handle
PPProxyHandle = EggressHandle
