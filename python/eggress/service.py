from __future__ import annotations

from os import PathLike
from typing import Any, Optional, Union

from eggress._eggress import (
    PyEggressConfig,
    PyEggressService,
    PyEggressHandle,
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

    def start(self) -> EggressHandle:
        """Start the service and return a handle."""
        handle = self._inner.start()
        return EggressHandle(handle)


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
