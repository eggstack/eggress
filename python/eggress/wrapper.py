"""Wrapper and composition objects for protocol chains.

Reproduces pproxy's TLS, plugin, and chain composition model.
These wrap base protocol objects to add transport layers.
"""

from __future__ import annotations

import copy
from abc import ABC, abstractmethod
from typing import Any


# ---------------------------------------------------------------------------
# Safe protocol reconstruction helpers
# ---------------------------------------------------------------------------


def _rebuild_protocol(protocol_type: type, param: str) -> Any:
    """Safely reconstruct a protocol from its class and param.

    Works around broken BaseProtocol.__reduce__/__copy__ that pass 4 positional
    args while subclasses only accept 1.
    """
    return protocol_type(param)


def _rebuild_protocol_deep(
    protocol_type: type, param: str, target: Any, dest: Any, source: Any
) -> Any:
    """Safely reconstruct a protocol, restoring target/dest/source."""
    instance = protocol_type(param)
    instance.target = target
    instance.dest = dest
    instance.source = source
    return instance


def _safe_copy_protocol(proto: Any) -> Any:
    """Copy a protocol instance safely, bypassing broken __copy__."""
    return _rebuild_protocol_deep(
        type(proto),
        proto.param,
        getattr(proto, "target", None),
        getattr(proto, "dest", None),
        getattr(proto, "source", None),
    )


def _safe_deepcopy_protocol(proto: Any, memo: dict[int, Any]) -> Any:
    """Deepcopy a protocol instance safely, bypassing broken __deepcopy__."""
    return _rebuild_protocol_deep(
        type(proto),
        copy.deepcopy(proto.param, memo),
        copy.deepcopy(getattr(proto, "target", None), memo),
        copy.deepcopy(getattr(proto, "dest", None), memo),
        copy.deepcopy(getattr(proto, "source", None), memo),
    )


def _safe_reduce_protocol(proto: Any) -> tuple[Any, tuple[str, Any, Any, Any]]:
    """Return reduce tuple for a protocol, bypassing broken __reduce__."""
    return (
        _rebuild_protocol_deep,
        (
            type(proto),
            proto.param,
            getattr(proto, "target", None),
            getattr(proto, "dest", None),
            getattr(proto, "source", None),
        ),
    )


# ---------------------------------------------------------------------------
# Base wrapper
# ---------------------------------------------------------------------------


class BaseWrapper(ABC):
    """Abstract base for all protocol wrappers.

    A wrapper delegates metadata and routing properties to its inner
    protocol while adding its own transport or security layer.
    """

    _WRAP_TYPE: str = ""

    def __init__(self, inner: Any) -> None:
        self._inner = inner

    @property
    def inner(self) -> Any:
        return self._inner

    @property
    def name(self) -> str:
        return self._WRAP_TYPE

    @property
    def target(self) -> str | None:
        return getattr(self._inner, "target", None)

    @property
    def dest(self) -> str | None:
        return getattr(self._inner, "dest", None)

    @property
    def source(self) -> str | None:
        return getattr(self._inner, "source", None)

    @property
    def _SUPPORTED_IN_EGRESS(self) -> bool:  # noqa: N802
        return getattr(self._inner, "_SUPPORTED_IN_EGRESS", True)

    @property
    def _TRAFFIC_KINDS(self) -> tuple[str, ...]:  # noqa: N802
        return getattr(self._inner, "_TRAFFIC_KINDS", ("tcp",))

    @property
    def _ROLE(self) -> str:  # noqa: N802
        return getattr(self._inner, "_ROLE", "both")

    # -- identity -----------------------------------------------------------

    def __eq__(self, other: object) -> bool:
        return (
            type(self) is type(other)
            and self._inner == getattr(other, "_inner", None)
        )

    def __hash__(self) -> int:
        return hash((type(self), self._inner))

    def __repr__(self) -> str:
        return f"{type(self).__name__}({self._inner!r})"

    # -- pickling / copying -------------------------------------------------

    def __reduce__(self) -> tuple[Any, tuple[Any]]:
        return (self.__class__, (self._inner,))

    def __copy__(self) -> BaseWrapper:
        return self.__class__(_safe_copy_protocol(self._inner))

    def __deepcopy__(self, memo: dict[int, Any]) -> BaseWrapper:
        return self.__class__(_safe_deepcopy_protocol(self._inner, memo))


# ---------------------------------------------------------------------------
# TLS wrapper
# ---------------------------------------------------------------------------


class TLS(BaseWrapper):
    """Wraps any protocol with TLS transport.

    Stores optional certificate, key, and SNI parameters.
    """

    _WRAP_TYPE = "tls"

    def __init__(
        self,
        inner: Any,
        certfile: str | None = None,
        keyfile: str | None = None,
        sni: str | None = None,
    ) -> None:
        super().__init__(inner)
        self._certfile = certfile
        self._keyfile = keyfile
        self._sni = sni

    @property
    def certfile(self) -> str | None:
        return self._certfile

    @property
    def keyfile(self) -> str | None:
        return self._keyfile

    @property
    def sni(self) -> str | None:
        return self._sni

    @property
    def name(self) -> str:
        return "tls"

    def __repr__(self) -> str:
        parts = [repr(self._inner)]
        if self._certfile:
            parts.append(f"certfile=.../{self._certfile.rsplit('/', 1)[-1]}")
        if self._keyfile:
            parts.append(f"keyfile=.../{self._keyfile.rsplit('/', 1)[-1]}")
        if self._sni:
            parts.append(f"sni={self._sni!r}")
        return f"TLS({', '.join(parts)})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, TLS):
            return NotImplemented
        return (
            self._inner == other._inner
            and self._certfile == other._certfile
            and self._keyfile == other._keyfile
            and self._sni == other._sni
        )

    def __hash__(self) -> int:
        return hash((type(self), self._inner, self._certfile, self._keyfile, self._sni))

    def __reduce__(self) -> tuple[Any, tuple[Any, str | None, str | None, str | None]]:
        return (_rebuild_tls, (self._inner, self._certfile, self._keyfile, self._sni))

    def __copy__(self) -> TLS:
        return self.__class__(
            _safe_copy_protocol(self._inner),
            certfile=self._certfile,
            keyfile=self._keyfile,
            sni=self._sni,
        )

    def __deepcopy__(self, memo: dict[int, Any]) -> TLS:
        return self.__class__(
            _safe_deepcopy_protocol(self._inner, memo),
            certfile=self._certfile,
            keyfile=self._keyfile,
            sni=self._sni,
        )


def _rebuild_tls(
    inner: Any,
    certfile: str | None,
    keyfile: str | None,
    sni: str | None,
) -> TLS:
    """Reconstruct a TLS wrapper from pickle."""
    return TLS(inner, certfile=certfile, keyfile=keyfile, sni=sni)


# ---------------------------------------------------------------------------
# Plugin wrapper
# ---------------------------------------------------------------------------


class Plugin(BaseWrapper):
    """Wraps any protocol with a plugin callback handler."""

    _WRAP_TYPE = "plugin"

    def __init__(self, inner: Any, handler: Any | None = None) -> None:
        super().__init__(inner)
        self._handler = handler

    @property
    def handler(self) -> Any | None:
        return self._handler

    @property
    def name(self) -> str:
        return "plugin"

    def __repr__(self) -> str:
        handler_info = ""
        if self._handler is not None:
            handler_info = f", handler={self._handler!r}"
        return f"Plugin({self._inner!r}{handler_info})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Plugin):
            return NotImplemented
        return (
            self._inner == other._inner
            and self._handler == other._handler
        )

    def __hash__(self) -> int:
        return hash((type(self), self._inner, self._handler))

    def __reduce__(self) -> tuple[Any, tuple[Any, Any | None]]:
        return (_rebuild_plugin, (self._inner, self._handler))

    def __copy__(self) -> Plugin:
        return self.__class__(
            _safe_copy_protocol(self._inner),
            handler=self._handler,
        )

    def __deepcopy__(self, memo: dict[int, Any]) -> Plugin:
        return self.__class__(
            _safe_deepcopy_protocol(self._inner, memo),
            handler=copy.deepcopy(self._handler, memo),
        )


def _rebuild_plugin(inner: Any, handler: Any | None) -> Plugin:
    """Reconstruct a Plugin wrapper from pickle."""
    return Plugin(inner, handler=handler)


# ---------------------------------------------------------------------------
# Chain
# ---------------------------------------------------------------------------


class Chain:
    """Represents a chain of protocols (e.g., ``[SS, TLS]``).

    Provides iteration, indexing, and metadata delegation to the
    first/last protocol in the chain.
    """

    def __init__(self, protocols: list[Any]) -> None:
        self.protocols = tuple(protocols)

    # -- properties ---------------------------------------------------------

    @property
    def target(self) -> str | None:
        if self.protocols:
            return getattr(self.protocols[0], "target", None)
        return None

    @property
    def dest(self) -> str | None:
        if self.protocols:
            return getattr(self.protocols[-1], "dest", None)
        return None

    @property
    def source(self) -> str | None:
        if self.protocols:
            return getattr(self.protocols[-1], "source", None)
        return None

    @property
    def name(self) -> str:
        return "chain"

    # -- container ----------------------------------------------------------

    def __len__(self) -> int:
        return len(self.protocols)

    def __getitem__(self, index: int) -> Any:
        return self.protocols[index]

    def __iter__(self):
        return iter(self.protocols)

    def __contains__(self, item: Any) -> bool:
        return item in self.protocols

    # -- identity -----------------------------------------------------------

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Chain):
            return NotImplemented
        return self.protocols == other.protocols

    def __hash__(self) -> int:
        return hash(self.protocols)

    def __repr__(self) -> str:
        items = ", ".join(repr(p) for p in self.protocols)
        return f"Chain([{items}])"

    # -- composition --------------------------------------------------------

    def flat(self) -> list[Any]:
        """Return a flat list of all protocols, unwrapping wrappers."""
        result: list[Any] = []
        for p in self.protocols:
            if isinstance(p, BaseWrapper):
                result.extend(_unwrap_flat(p))
            else:
                result.append(p)
        return result

    def validate(self) -> list[str]:
        """Validate the chain against composition constraints.

        Returns a list of error strings (empty if valid).
        """
        errors: list[str] = []
        flat = self.flat()
        if not flat:
            errors.append("empty chain")
            return errors
        for p in flat:
            if getattr(p, "_SUPPORTED_IN_EGRESS", True) is False:
                errors.append(
                    f"protocol {type(p).__name__} is not supported in eggress"
                )
        return errors

    # -- pickling / copying -------------------------------------------------

    def __reduce__(self) -> tuple[Any, tuple[list[Any]]]:
        return (_rebuild_chain, (list(self.protocols),))

    def __copy__(self) -> Chain:
        return Chain([_safe_copy_protocol(p) for p in self.protocols])

    def __deepcopy__(self, memo: dict[int, Any]) -> Chain:
        return Chain([_safe_deepcopy_protocol(p, memo) for p in self.protocols])


def _rebuild_chain(protocols: list[Any]) -> Chain:
    """Reconstruct a Chain from pickle."""
    return Chain(protocols)


def _unwrap_flat(wrapper: Any) -> list[Any]:
    """Recursively unwrap a wrapper to get base protocols."""
    if isinstance(wrapper, TLS):
        return _unwrap_flat(wrapper.inner)
    if isinstance(wrapper, Plugin):
        return _unwrap_flat(wrapper.inner)
    return [wrapper]


# ---------------------------------------------------------------------------
# normalize_chain
# ---------------------------------------------------------------------------


def normalize_chain(protocols: list[Any]) -> Chain:
    """Normalize a list of protocols into a properly ordered Chain.

    - Removes None and empty-string entries.
    - Orders: base protocol first, then TLS, then Plugin.

    Returns a Chain with the normalized protocol order.
    """
    cleaned: list[Any] = [p for p in protocols if p is not None and p != ""]

    base: list[Any] = []
    tls_wrappers: list[Any] = []
    plugin_wrappers: list[Any] = []

    for item in cleaned:
        if isinstance(item, Plugin):
            plugin_wrappers.append(item)
        elif isinstance(item, TLS):
            tls_wrappers.append(item)
        else:
            base.append(item)

    ordered = base + tls_wrappers + plugin_wrappers
    return Chain(ordered)
