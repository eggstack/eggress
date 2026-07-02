from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Sequence

from eggress._eggress import (
    PyTranslationResult,
    UnsupportedFeatureError,
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


class AlreadyStartedError(Exception):
    """Raised when trying to start an already-running server."""


class Server:
    """pproxy-compatible server wrapper.

    Accepts pproxy-style listen/remote URIs or a pre-built ``EggressConfig``,
    translates them to eggress configuration, and manages the service lifecycle.

    Example::

        # Sync
        with Server(listen=["socks5://127.0.0.1:0"], remote=["http://proxy:8080"]) as srv:
            print(srv.addresses)

        # Async
        async with Server(config=my_config) as srv:
            print(srv.addresses)

        # Blocking
        server = Server(listen=["socks5://:1080"], remote=["http://proxy:8080"])
        server.run()  # blocks until SIGINT/SIGTERM
    """

    def __init__(
        self,
        listen: Optional[list[str]] = None,
        remote: Optional[list[str]] = None,
        *,
        config=None,
        allow_partial: bool = False,
    ) -> None:
        from eggress.config import EggressConfig
        from eggress.service import EggressService

        if config is not None and (listen is not None or remote is not None):
            raise ValueError(
                "config is mutually exclusive with listen/remote"
            )
        if config is None and listen is None and remote is None:
            raise ValueError(
                "either listen/remote or config must be provided"
            )

        if config is not None:
            if not isinstance(config, EggressConfig):
                raise TypeError("config must be an EggressConfig instance")
            eggress_config = config
        else:
            args: list[str] = []
            if listen is not None:
                for uri in listen:
                    args.extend(["-l", uri])
            if remote is not None:
                for uri in remote:
                    args.extend(["-r", uri])
            result = translate_pproxy_args(args)
            if not allow_partial and not result.ok:
                features = ", ".join(
                    f"{u.feature}: {u.message}" for u in result.unsupported
                )
                raise UnsupportedFeatureError(
                    f"unsupported pproxy features: {features}"
                )
            eggress_config = result.config()

        self._service = EggressService(eggress_config)
        self._handle = None

    def start(self):
        """Start the server. Returns self for chaining."""
        if self._handle is not None:
            raise AlreadyStartedError("server is already running")
        self._handle = self._service.start()
        return self

    def close(self) -> None:
        """Stop the server. Idempotent."""
        if self._handle is not None:
            self._handle.shutdown()
            self._handle = None

    def stop(self) -> None:
        """Stop the server. Alias for close()."""
        self.close()

    def run(self) -> None:
        """Start and block until interrupted (SIGINT/SIGTERM)."""
        import signal
        import threading

        self.start()
        event = threading.Event()

        def _handler(sig, frame):
            event.set()

        old_sigint = signal.signal(signal.SIGINT, _handler)
        old_sigterm = signal.signal(signal.SIGTERM, _handler)
        try:
            event.wait()
        finally:
            signal.signal(signal.SIGINT, old_sigint)
            signal.signal(signal.SIGTERM, old_sigterm)
            self.close()

    async def astart(self):
        """Start the server asynchronously. Returns self for chaining."""
        if self._handle is not None:
            raise AlreadyStartedError("server is already running")
        import asyncio

        self._handle = await asyncio.to_thread(self._service.start)
        return self

    async def aclose(self) -> None:
        """Stop the server asynchronously. Idempotent."""
        if self._handle is not None:
            import asyncio

            await asyncio.to_thread(self._handle.shutdown)
            self._handle = None

    async def wait_closed(self) -> None:
        """Wait for the server to close."""
        if self._handle is not None:
            await self.aclose()

    @property
    def addresses(self) -> dict[str, str]:
        """Bound listener addresses. Empty dict if not started."""
        if self._handle is None:
            return {}
        return self._handle.bound_addresses

    def __enter__(self):
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> bool:
        self.close()
        return False

    async def __aenter__(self):
        await self.astart()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> bool:
        await self.aclose()
        return False

    def __repr__(self) -> str:
        state = "running" if self._handle is not None else "stopped"
        return f"Server({state})"
