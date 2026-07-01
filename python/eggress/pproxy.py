from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

from eggress._eggress import (
    PyTranslationResult,
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


try:
    from eggress._eggress import describe_reverse_pproxy_uri as _describe_reverse_pproxy_uri
except ImportError:
    _describe_reverse_pproxy_uri = None


@dataclass(frozen=True)
class ReverseUriSummary:
    role: str  # "server" | "client" | "unknown"
    scheme: str
    target: str  # redacted "host:port" or "****@host:port"
    has_auth: bool
    toml_section: str  # "reverse_servers" | "reverse_clients" | "unknown"
    tls: bool
    modifiers: tuple[str, ...]


def describe_reverse_pproxy_uri(uri: str) -> ReverseUriSummary:
    """Inspect a pproxy reverse URI and summarize how eggress would translate it.

    Supported pproxy reverse URI forms:
        * ``bind://[user:pass@]host:port`` / ``listen://...`` / ``backward://...`` /
          ``rebind://...``  -> eggress ``reverse_servers`` entry
        * ``socks5+in://...`` / ``http+in://...`` / ``ss+in://...`` etc.
          -> eggress ``reverse_clients`` entry

    The returned ``target`` is always redacted; credentials are never exposed.
    """
    if _describe_reverse_pproxy_uri is None:
        raise RuntimeError(
            "describe_reverse_pproxy_uri requires a newer eggress native module"
        )
    inner = _describe_reverse_pproxy_uri(uri)
    return ReverseUriSummary(
        role=inner.role,
        scheme=inner.scheme,
        target=inner.target,
        has_auth=inner.has_auth,
        toml_section=inner.toml_section,
        tls=inner.tls,
        modifiers=tuple(inner.modifiers),
    )
