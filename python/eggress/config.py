from __future__ import annotations

from os import PathLike
from typing import Union

from eggress._eggress import PyEggressConfig, ConfigError


class EggressConfig:
    """Parsed and validated eggress configuration."""

    __slots__ = ("_inner",)

    def __init__(self, _inner: PyEggressConfig) -> None:
        object.__setattr__(self, "_inner", _inner)

    @classmethod
    def from_toml(cls, toml: str) -> EggressConfig:
        """Parse a TOML configuration string."""
        return cls(PyEggressConfig.from_toml(toml))

    @classmethod
    def from_file(cls, path: Union[str, PathLike[str]]) -> EggressConfig:
        """Load and validate a TOML configuration file."""
        return cls(PyEggressConfig.from_file(str(path)))

    def redacted_toml(self) -> str:
        """Return the TOML source with credentials redacted."""
        return self._inner.redacted_toml()

    def __repr__(self) -> str:
        return "EggressConfig(...)"
