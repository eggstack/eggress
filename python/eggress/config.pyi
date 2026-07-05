"""Type stubs for the eggress.config module."""

from __future__ import annotations

from os import PathLike
from typing import Any, Union

class EggressConfig:
    def __init__(self, _inner: Any) -> None: ...
    @classmethod
    def from_toml(cls, toml: str) -> EggressConfig: ...
    @classmethod
    def from_file(cls, path: Union[str, PathLike[str]]) -> EggressConfig: ...
    def redacted_toml(self) -> str: ...
