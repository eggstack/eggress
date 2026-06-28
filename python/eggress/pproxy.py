from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

from eggress._eggress import (
    PyTranslationResult,
    PyTranslationWarning,
    PyUnsupportedFeature,
    translate_pproxy_args as _translate_pproxy_args,
    translate_pproxy_uri as _translate_pproxy_uri,
    check_pproxy_args as _check_pproxy_args,
)


@dataclass(frozen=True)
class TranslationWarning:
    category: str
    message: str


@dataclass(frozen=True)
class UnsupportedFeature:
    feature: str
    message: str


class TranslationResult:
    __slots__ = ("_inner",)

    def __init__(self, _inner: PyTranslationResult) -> None:
        object.__setattr__(self, "_inner", _inner)

    @property
    def toml(self) -> str:
        return self._inner.toml

    @property
    def warnings(self) -> list[TranslationWarning]:
        return [
            TranslationWarning(category=w.category, message=w.message)
            for w in self._inner.warnings
        ]

    @property
    def unsupported(self) -> list[UnsupportedFeature]:
        return [
            UnsupportedFeature(feature=u.feature, message=u.message)
            for u in self._inner.unsupported
        ]

    @property
    def ok(self) -> bool:
        return self._inner.ok

    def config(self):
        from eggress.config import EggressConfig

        return EggressConfig(self._inner.config())

    def __repr__(self) -> str:
        return (
            f"TranslationResult(warnings={len(self.warnings)}, "
            f"unsupported={len(self.unsupported)})"
        )


def translate_pproxy_args(args: Sequence[str]) -> TranslationResult:
    return TranslationResult(_translate_pproxy_args(list(args)))


def translate_pproxy_uri(
    local: str, remotes: Sequence[str] = ()
) -> TranslationResult:
    return TranslationResult(_translate_pproxy_uri(local, list(remotes)))


def check_pproxy_args(args: Sequence[str]) -> TranslationResult:
    return TranslationResult(_check_pproxy_args(list(args)))
