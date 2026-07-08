from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Sequence

from eggress._eggress import (
    PyTranslationResult,
    UnsupportedFeatureError,
    translate_pproxy_args as _translate_pproxy_args,
    translate_pproxy_uri as _translate_pproxy_uri,
    check_pproxy_args as _check_pproxy_args,
    check_pproxy_uri as _check_pproxy_uri,
    redact_pproxy_uri as _redact_pproxy_uri,
    diagnostics_for_uri as _diagnostics_for_uri,
    supported_features as _supported_features,
    explain_config_toml as _explain_config_toml,
    explain_pproxy_args as _explain_pproxy_args,
    explain_pproxy_uri as _explain_pproxy_uri,
    route_explain as _route_explain,
    test_upstream_connect as _test_upstream_connect,
)

_PPROXY_COMPATIBILITY_VERSION = "2.7.9"


def _redact_config_toml(toml_str: str) -> str:
    """Redact credentials from TOML config for safe display."""
    import re

    def _redact_uri(match: re.Match) -> str:
        uri = match.group(0)
        scheme_end = uri.find("://")
        if scheme_end < 0:
            return uri
        after_scheme = uri[scheme_end + 3:]
        # The userinfo separator is the LAST unbracketed '@' after the
        # scheme; a raw password containing '@' must not be split.
        last_at = -1
        bracket_depth = 0
        for i, c in enumerate(after_scheme):
            if c == "[":
                bracket_depth += 1
            elif c == "]":
                bracket_depth = max(0, bracket_depth - 1)
            elif c == "@" and bracket_depth == 0:
                last_at = i
        if last_at >= 0:
            return f"{uri[:scheme_end + 3]}****{uri[scheme_end + 3 + last_at:]}"
        return uri

    def _redact_kv(match: re.Match) -> str:
        key = match.group(1)
        val = match.group(2)
        sensitive = ("password", "secret", "key", "token", "auth")
        if any(s in key.lower() for s in sensitive):
            return f'{key} = "****"'
        return match.group(0)

    result = re.sub(r'\w+://[^\s"]+', _redact_uri, toml_str)
    result = re.sub(r'^(\w+)\s*=\s*"([^"]*)"', _redact_kv, result, flags=re.MULTILINE)
    return result


def redact_config_toml(toml_str: str) -> str:
    """Redact credentials from TOML config for safe display."""
    return _redact_config_toml(toml_str)


def compatibility_version() -> str:
    """Return the target pproxy version that eggress aims to be compatible with.

    Returns:
        The pproxy version string, currently ``"2.7.9"``.

    This does not dynamically detect or require the upstream ``pproxy`` package.
    It is the version against which eggress pproxy compatibility is tested.
    """
    return _PPROXY_COMPATIBILITY_VERSION


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


def _manifest_tier_for_diagnostic(code: str) -> str:
    """Map a translator warning code to its manifest-aligned tier.

    The translator emits structured warning codes for known modifier flags
    and unsupported cases. This helper classifies them according to the
    same five-tier vocabulary as
    ``docs/parity/pproxy_capability_manifest.toml``:

    - ``intentional_non_parity`` — flag parsed, no plan to implement
    - ``native_equivalent`` — outcome same as pproxy, different mechanism
    - ``compatible_with_warning`` — works but emits a diagnostic
    - ``unsupported`` — flag or feature not implemented
    - ``drop_in`` — no warning expected
    """
    # Intentional non-parity: connection pooling, etc.
    intentional = {
        "reuse-connection",
    }
    # Native equivalent: same outcome through different mechanism.
    native_equivalent = {
        "alive-check",
        "pac-serving",
        "test-mode",
        "system-proxy",
        "log-file",
        "verbose-mode",
    }
    # Compatible with warning: works but emits a diagnostic.
    compatible_with_warning = {
        "scheduler",
        "credential-in-toml",
        "rulefile-partial",
        "rulefile-parse",
        "rulefile-read",
        "direct-mode",
        "ul-no-listener",
    }
    if code in intentional:
        return "intentional_non_parity"
    if code in native_equivalent:
        return "native_equivalent"
    if code in compatible_with_warning:
        return "compatible_with_warning"
    return "unsupported"


def _classify_aggregate_tier(
    warnings: list[Diagnostic], unsupported: list[Diagnostic]
) -> str:
    """Pick the worst manifest-aligned tier from the diagnostics.

    Deterministic severity order (worst first):
        1. any unsupported hard failure -> ``unsupported``
        2. any intentional non-parity   -> ``intentional_non_parity``
        3. any native-equivalent warning -> ``native_equivalent``
        4. any compatible-with-warning  -> ``compatible_with_warning``
        5. no diagnostics               -> ``drop_in``
    """
    if unsupported:
        return "unsupported"
    has_intentional = any(
        (w.tier or "") == "intentional_non_parity" for w in warnings
    )
    if has_intentional:
        return "intentional_non_parity"
    has_native = any(
        (w.tier or "") == "native_equivalent" for w in warnings
    )
    if has_native:
        return "native_equivalent"
    if warnings:
        return "compatible_with_warning"
    return "drop_in"


def check_pproxy_args(args: Sequence[str]) -> CompatibilityReport:
    """Translate pproxy args and return a full compatibility report.

    Returns a :class:`CompatibilityReport` with tier classification,
    diagnostics, parsed URIs, and generated TOML.

    The aggregate ``tier`` field uses the same five-tier vocabulary as
    ``docs/parity/pproxy_capability_manifest.toml``:

    - ``drop_in``
    - ``compatible_with_warning``
    - ``native_equivalent``
    - ``intentional_non_parity``
    - ``unsupported``

    Each ``Diagnostic`` also carries its own ``tier`` so callers can
    attribute the aggregate classification to specific warnings. The
    ``features`` list reflects only capabilities actually referenced by
    the supplied args (not the full ``supported_features()`` set), so a
    feature is only marked ``unsupported`` when the user's args triggered
    an unsupported diagnostic for it.
    """
    result = _translate_pproxy_args(list(args))

    warn_diags: list[Diagnostic] = []
    for w in result.warnings:
        code = getattr(w, "category", "warning")
        warn_diags.append(Diagnostic(
            code=code,
            feature_id=None,
            tier=_manifest_tier_for_diagnostic(code),
            message=getattr(w, "message", str(w)),
            suggestion=None,
        ))

    unsupported_diags: list[Diagnostic] = []
    for u in result.unsupported:
        unsupported_diags.append(Diagnostic(
            code="unsupported_protocol",
            feature_id=getattr(u, "feature", None),
            tier="unsupported",
            message=getattr(u, "message", str(u)),
            suggestion=None,
        ))

    diagnostics = list(warn_diags) + list(unsupported_diags)
    tier = _classify_aggregate_tier(warn_diags, unsupported_diags)

    parsed_uris: dict[str, UriInfo] = {}
    i = 0
    arg_list = list(args)
    while i < len(arg_list):
        if arg_list[i] in ("-l", "-r", "--listen", "--remote") and i + 1 < len(arg_list):
            uri = arg_list[i + 1]
            parsed_uris[uri] = check_pproxy_uri(uri)
            i += 2
        else:
            i += 1

    # Only mark features as "unsupported" when the args actually triggered
    # an unsupported diagnostic for them. A bare "-l socks5://:0" should
    # not mark every known pproxy feature as "compatible by default";
    # that conflated "convenience list" with "evidence-based capability
    # classification" and is replaced by leaving unrelated features out.
    features: list[FeatureInfo] = []
    unsupported_feature_ids = {
        u.feature for u in result.unsupported if getattr(u, "feature", None)
    }
    for feat_id in unsupported_feature_ids:
        features.append(
            FeatureInfo(
                feature_id=feat_id, tier="unsupported", supported=False
            )
        )

    return CompatibilityReport(
        tier=tier,
        ok=result.ok,
        warnings=warn_diags,
        unsupported=unsupported_diags,
        diagnostics=diagnostics,
        features=features,
        toml=redact_config_toml(result.toml) if result.toml else None,
        parsed_uris=parsed_uris,
        raw_args=list(args),
    )


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


@dataclass(frozen=True)
class UriInfo:
    """Result of parsing a single pproxy URI. Never raises on invalid input."""

    scheme: str
    host: str
    port: int
    tls: bool
    ssl: bool
    inbound: bool
    backward_num: int
    has_auth: bool
    has_rule: bool
    is_reverse_listener: bool
    redacted_display: str
    error: Optional[str]

    @property
    def ok(self) -> bool:
        return self.error is None

    def __repr__(self) -> str:
        if self.error:
            return f"UriInfo(error={self.error!r})"
        return (
            f"UriInfo(scheme={self.scheme!r}, host={self.host!r}, "
            f"port={self.port}, tls={self.tls})"
        )


def check_pproxy_uri(uri: str) -> UriInfo:
    """Parse a pproxy URI and return structured info. Never raises."""
    inner = _check_pproxy_uri(uri)
    return UriInfo(
        scheme=inner.scheme,
        host=inner.host,
        port=inner.port,
        tls=inner.tls,
        ssl=inner.ssl,
        inbound=inner.inbound,
        backward_num=inner.backward_num,
        has_auth=inner.has_auth,
        has_rule=inner.has_rule,
        is_reverse_listener=inner.is_reverse_listener,
        redacted_display=inner.redacted_display,
        error=inner.error,
    )


def redact_pproxy_uri(uri: str) -> str:
    """Return the redacted display of a pproxy URI. Raises on invalid URI."""
    return _redact_pproxy_uri(uri)


@dataclass(frozen=True)
class Diagnostic:
    """A structured diagnostic from the pproxy compatibility layer."""

    code: str
    feature_id: Optional[str]
    tier: Optional[str]
    message: str
    suggestion: Optional[str]

    def __repr__(self) -> str:
        return f"[{self.code}] {self.message}"


@dataclass(frozen=True)
class FeatureInfo:
    """A single feature from the pproxy compatibility manifest."""
    feature_id: str
    tier: str  # "compatible", "partial", "unsupported"
    supported: bool


@dataclass(frozen=True)
class CompatibilityReport:
    """Structured compatibility report for pproxy args.

    Aligns with ``eggress pproxy check --json`` output and the Phase 37
    pproxy capability manifest.
    """
    tier: str
    ok: bool
    warnings: list[Diagnostic]
    unsupported: list[Diagnostic]
    diagnostics: list[Diagnostic]
    features: list[FeatureInfo]
    toml: str | None
    parsed_uris: dict[str, UriInfo]
    raw_args: list[str]


def diagnostics_for_uri(uri: str) -> list[Diagnostic]:
    """Return diagnostics produced when translating a pproxy URI."""
    return [
        Diagnostic(
            code=d.code,
            feature_id=d.feature_id,
            tier=d.tier,
            message=d.message,
            suggestion=d.suggestion,
        )
        for d in _diagnostics_for_uri(uri)
    ]


def supported_features() -> list[str]:
    """Return the list of pproxy protocol features supported by eggress."""
    return list(_supported_features())


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
        self._config = eggress_config
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
        """Start and block until interrupted (SIGINT/SIGTERM).

        Must be called from the main thread because Python signal handlers
        can only be installed from the main thread.
        """
        import signal
        import threading

        if threading.current_thread() is not threading.main_thread():
            raise RuntimeError(
                "Server.run() must be called from the main thread; "
                "use Server.start() from other threads"
            )

        self.start()
        event = threading.Event()

        def _handler(sig, frame):
            event.set()

        old_sigint = signal.signal(signal.SIGINT, _handler)
        try:
            old_sigterm = signal.signal(signal.SIGTERM, _handler)
        except BaseException:
            signal.signal(signal.SIGINT, old_sigint)
            raise
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

    @property
    def config(self):
        """The :class:`EggressConfig` used to construct this server.

        Available before and after ``start()``. If the server was constructed
        from ``listen``/remote URIs, this is the translated config.
        """
        return self._config

    @property
    def is_ready(self) -> bool:
        """Whether the server is started and ready."""
        if self._handle is None:
            return False
        return self._handle.status().get("readiness", False)

    @property
    def listener_info(self) -> list[dict]:
        """Listener details from the running service. Empty list if not started."""
        if self._handle is None:
            return []
        return self._handle.status().get("listeners", [])

    @property
    def metrics_text(self) -> str:
        """Prometheus-format metrics text. Empty string if not started."""
        if self._handle is None:
            return ""
        return self._handle.metrics_text()

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


class PPProxyService:
    """pproxy-compatible service builder.

    Accepts pproxy-style listen/remote URIs, pre-built TOML config, or a
    TOML file path, translates them to eggress configuration, and manages
    the service lifecycle.

    Example::

        # From pproxy CLI args
        with PPProxyService.from_args(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
            print(handle.bound_addresses)

        # From local/remote URIs
        with PPProxyService.from_uri("socks5://127.0.0.1:0") as handle:
            print(handle.bound_addresses)

        # From TOML string
        with PPProxyService.from_toml(toml_str) as handle:
            print(handle.bound_addresses)

        # From TOML file
        with PPProxyService.from_file("config.toml") as handle:
            print(handle.bound_addresses)
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
        self._config = eggress_config
        self._handle = None

    @classmethod
    def from_args(
        cls, args: Sequence[str], allow_partial: bool = False
    ) -> PPProxyService:
        """Create a service from pproxy-style CLI arguments.

        The full pproxy argument vector is preserved and routed through
        :func:`translate_pproxy_args`, so modifier flags such as ``--ssl``,
        ``-b``, ``--pac``, ``-s``, ``-a``, ``--rulefile``, ``-ul``/``-ur``,
        ``--test``, ``--sys``, ``--log``, ``--reuse``, and ``--get`` are not
        silently dropped. This matches the behavior of the top-level
        :func:`eggress.start_pproxy` entry point.

        Args:
            args: pproxy-style CLI arguments (e.g. ["-l", "socks5://:1080"]).
            allow_partial: If True, start even when unsupported features exist.

        Returns:
            A pre-start PPProxyService.

        Raises:
            UnsupportedFeatureError: If unsupported features exist and
                ``allow_partial`` is False.
        """
        from eggress.config import EggressConfig
        from eggress.service import EggressService

        result = translate_pproxy_args(list(args))
        if not allow_partial and not result.ok:
            features = ", ".join(
                f"{u.feature}: {u.message}" for u in result.unsupported
            )
            raise UnsupportedFeatureError(
                f"unsupported pproxy features: {features}"
            )
        # Build a service directly from the translated config rather than
        # reconstructing the listen/remote URI list. This is the only way
        # to preserve --ssl, -b, --pac, -s, -a, --rulefile, -ul, -ur,
        # --test, --sys, --log, --reuse, and --get semantics.
        eggress_config: EggressConfig = result.config()
        instance = cls.__new__(cls)
        instance._service = EggressService(eggress_config)
        instance._config = eggress_config
        instance._handle = None
        return instance

    @classmethod
    def from_uri(
        cls,
        local: str,
        remotes: Sequence[str] = (),
        allow_partial: bool = False,
    ) -> PPProxyService:
        """Create a service from a local URI and optional remote URIs.

        Args:
            local: Local listener URI (e.g. "socks5://127.0.0.1:1080").
            remotes: Optional list of remote upstream URIs.
            allow_partial: If True, start even when unsupported features exist.

        Returns:
            A pre-start PPProxyService.
        """
        return cls(
            listen=[local],
            remote=list(remotes) or None,
            allow_partial=allow_partial,
        )

    @classmethod
    def from_toml(cls, toml: str) -> PPProxyService:
        """Create a service from a TOML configuration string.

        Args:
            toml: TOML configuration string.

        Returns:
            A pre-start PPProxyService.
        """
        from eggress.config import EggressConfig

        return cls(config=EggressConfig.from_toml(toml))

    @classmethod
    def from_file(cls, path: str) -> PPProxyService:
        """Create a service from a TOML configuration file.

        Args:
            path: Path to a TOML configuration file.

        Returns:
            A pre-start PPProxyService.
        """
        from eggress.config import EggressConfig

        return cls(config=EggressConfig.from_file(path))

    def start(self) -> EggressHandle:
        """Start the service and return a handle.

        Returns:
            A started EggressHandle (supports bound_addresses, status,
            metrics_text, reload_toml, shutdown, context manager).
        """
        if self._handle is not None:
            raise AlreadyStartedError("service is already running")
        self._handle = self._service.start()
        return self._handle

    @property
    def config(self) -> "EggressConfig":
        """The eggress configuration produced from the pproxy inputs.

        Useful for inspecting the translated TOML config that the service
        will run (for example, to verify that ``--ssl`` translated to a
        ``listeners.tls`` block). Available before and after ``start()``.
        """
        return self._config

    def __enter__(self) -> EggressHandle:
        self.start()
        return self._handle

    def __exit__(self, exc_type, exc_val, exc_tb) -> bool:
        if self._handle is not None:
            self._handle.shutdown()
            self._handle = None
        return False

    def __repr__(self) -> str:
        state = "running" if self._handle is not None else "stopped"
        return f"PPProxyService({state})"


# --- Config explanation helpers ---


def explain_config_toml(toml_str: str) -> dict:
    """Parse an eggress TOML config and return a structured explanation dict.

    Returns a dict with keys: listeners, upstreams, upstream_groups, rules,
    reverse_servers, reverse_clients, security_notes.
    """
    return _explain_config_toml(toml_str)


def explain_pproxy_args(args: list[str]) -> dict:
    """Translate pproxy args and return structured explanation + metadata.

    Returns a dict with keys: listeners, upstreams, upstream_groups, rules,
    reverse_servers, reverse_clients, security_notes, warnings, unsupported,
    toml, ok.
    """
    return _explain_pproxy_args(args)


def explain_pproxy_uri(uri: str) -> dict:
    """Translate a pproxy URI and return structured explanation + metadata.

    Returns a dict with keys: listeners, upstreams, upstream_groups, rules,
    reverse_servers, reverse_clients, security_notes, warnings, unsupported,
    toml, ok.
    """
    return _explain_pproxy_uri(uri)


# --- Route explanation and upstream test helpers ---


def route_explain(config_toml: str, target: str) -> dict:
    """Explain which rule/upstream group/scheduler applies for a target.

    Compiles the given TOML config and runs the routing engine against the
    target address. Returns a dict with: target, listener, protocol, transport,
    matched_rule, action, upstream_group, scheduler, eligible_upstreams,
    selected_upstream, chain, generation.
    """
    return _route_explain(config_toml, target)


def check_upstream(uri: str, timeout: float = 5.0) -> dict:
    """Test TCP connectivity to an upstream URI.

    Attempts a TCP connection to the upstream and returns a dict with:
    host, port, scheme, has_auth, redacted_uri, connected, latency_us, error.
    """
    return _test_upstream_connect(uri, timeout)
