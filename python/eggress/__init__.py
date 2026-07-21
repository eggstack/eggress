from __future__ import annotations

try:
    from eggress._eggress import __version__ as __version__
except ImportError:
    __version__ = "0.1.0"

import sys
from typing import Any, Sequence

try:
    from eggress._eggress import (
        EggressError,
        ConfigError,
        StartupError,
        ReloadError,
        ShutdownError,
        UnsupportedFeatureError,
        InternalError,
    )
    _HAS_NATIVE = True
except ImportError:
    _HAS_NATIVE = False
    class EggressError(Exception): pass  # type: ignore[no-redef]
    class ConfigError(EggressError): pass  # type: ignore[no-redef]
    class StartupError(EggressError): pass  # type: ignore[no-redef]
    class ReloadError(EggressError): pass  # type: ignore[no-redef]
    class ShutdownError(EggressError): pass  # type: ignore[no-redef]
    class UnsupportedFeatureError(EggressError): pass  # type: ignore[no-redef]
    class InternalError(EggressError): pass  # type: ignore[no-redef]

try:
    from eggress.config import EggressConfig
except ImportError:
    EggressConfig = None  # type: ignore[assignment,misc]

try:
    from eggress.service import EggressService, EggressHandle, AsyncEggressHandle, PPProxyHandle
except ImportError:
    EggressService = EggressHandle = AsyncEggressHandle = PPProxyHandle = None  # type: ignore[assignment,misc]

try:
    from eggress.pproxy import (
        TranslationResult,
        TranslationWarning,
        UnsupportedFeature,
        translate_pproxy_args,
        translate_pproxy_uri,
        check_pproxy_args,
        describe_reverse_pproxy_uri,
        ReverseUriSummary,
        UriInfo,
        check_pproxy_uri,
        redact_pproxy_uri,
        Diagnostic,
        diagnostics_for_uri,
        supported_features,
        compatibility_version,
        AlreadyStartedError,
        Server,
        explain_config_toml,
        explain_pproxy_args,
        explain_pproxy_uri,
        route_explain,
        check_upstream,
        CompatibilityReport,
        FeatureInfo,
        PPProxyService,
    )
except ImportError:
    pass  # pproxy module not available without native extension

try:
    from eggress.connection import (
        Connection,
        ConnectionState,
        ConnectionError as ConnectionBaseError,
        ConnectionClosedError,
        TimeoutError as ConnectionTimeoutError,
        DnsError as ConnectionDnsError,
        AuthError as ConnectionAuthError,
        TlsError as ConnectionTlsError,
        LoopMismatchError,
        ConnectionCancelledError,
        UseAfterCloseError,
        UdpAssociationError,
        UnsupportedCompositionError,
    )
except ImportError:
    pass

try:
    from eggress.pproxy_connection import ProxyConnection
except ImportError:
    ProxyConnection = None  # type: ignore[assignment,misc]

try:
    from eggress.async_connection import AsyncConnection
except ImportError:
    AsyncConnection = None  # type: ignore[assignment,misc]

try:
    from eggress.outbound import AsyncOutboundStream, OutboundConnector, OutboundStream
except ImportError:
    AsyncOutboundStream = OutboundConnector = OutboundStream = None  # type: ignore[assignment,misc]

try:
    from eggress._asyncio import AsyncBridge, CloseWaiter, LoopAffinityError
except ImportError:
    AsyncBridge = CloseWaiter = LoopAffinityError = None  # type: ignore[assignment,misc]

try:
    from eggress._compat import (
        PY_VERSION,
        PY_MAJOR,
        PY_MINOR,
        HAS_TASKGROUP,
        HAS_EXCEPTIONGROUP,
    )
except ImportError:
    pass

try:
    from eggress.protocol import (
        BaseProtocol,
        Direct,
        HTTP,
        HTTPOnly,
        Socks4,
        Socks5,
        SS,
        SSR,
        Trojan,
        WS,
        H2,
        H3,
        SSH,
        Transparent,
        Redir,
        Pf,
        Tunnel,
        Echo,
        MAPPINGS as PROTOCOL_MAPPINGS,
        get_protos,
        packstr,
        netloc_split,
        HTTP_LINE,
    )
except ImportError:
    pass

try:
    from eggress.cipher import (
        BaseCipher,
        AEADCipher,
        StreamCipher,
        PacketCipher,
        AES_256_GCM_Cipher,
        AES_192_GCM_Cipher,
        AES_128_GCM_Cipher,
        ChaCha20_IETF_POLY1305_Cipher,
        MAP as CIPHER_MAP,
        get_cipher,
    )
except ImportError:
    pass

try:
    from eggress.plugin import (
        PluginRegistry,
        PluginBridge,
        PluginError,
        PluginTimeoutError,
        PluginRejectedError,
        PluginShutdownError,
        PluginReentrantError,
        CallbackResult,
        CallbackMetrics,
        CallbackWrapper,
    )
except ImportError:
    pass

try:
    from eggress.wrapper import (
        BaseWrapper,
        TLS,
        Plugin as PluginWrapper,
        Chain,
        normalize_chain,
    )
except ImportError:
    pass

try:
    from eggress._asyncio_adapter import (
        CompatibleStreamReader,
        CompatibleStreamWriter,
        open_tcp_connection,
    )
except ImportError:
    pass

try:
    from eggress._pproxy_proxy import (
        AuthTable,
        ProxyDirect,
        ProxySimple,
        ProxyBackward,
        ProxyH2,
        ProxySSH,
        ProxyQUIC,
        ProxyH3,
        DIRECT as PROXY_DIRECT,
    )
except ImportError:
    pass


def start_pproxy(
    args: Sequence[str] | None = None,
    *,
    local: str | None = None,
    remote: str | Sequence[str] | None = None,
    config: str | None = None,
    config_path: str | None = None,
    allow_partial: bool = False,
    background: bool = True,
    log_format: str | None = None,
) -> EggressHandle:
    """Start an eggress service from pproxy-style CLI arguments.

    Supports multiple input modes (mutually exclusive):

    - ``args``: pproxy CLI-style arguments (e.g. ``["-l", "socks5://:1080"]``)
    - ``local``: shorthand for a single local listener URI
    - ``remote``: shorthand for remote upstream URI(s)
    - ``config``: a TOML configuration string
    - ``config_path``: path to a TOML configuration file

    Args:
        args: pproxy-style CLI arguments (excluding argv[0]).
        local: Single local listener URI (e.g. ``"socks5://127.0.0.1:1080"``).
        remote: Remote upstream URI string or list of strings.
        config: TOML configuration string.
        config_path: Path to a TOML configuration file.
        allow_partial: If True, start even when unsupported features exist.
        background: Reserved for API compatibility (currently ignored).
        log_format: Reserved for future use.

    Returns:
        A handle to the running service.

    Raises:
        ValueError: If conflicting input modes are provided.
        ConfigError: If configuration is invalid.
        UnsupportedFeatureError: If unsupported features exist and allow_partial is False.

    Example::

        with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
            print(handle.bound_addresses)

        with start_pproxy(local="socks5://127.0.0.1:0") as handle:
            print(handle.bound_addresses)
    """
    provided = []
    if args is not None:
        provided.append("args")
    if local is not None:
        provided.append("local")
    if config is not None:
        provided.append("config")
    if config_path is not None:
        provided.append("config_path")
    if len(provided) > 1:
        raise ValueError(
            f"conflicting input modes: {', '.join(provided)} are mutually exclusive"
        )

    if args is not None:
        return EggressService.from_pproxy_args(
            args, allow_partial=allow_partial
        ).start()

    if local is not None:
        remote_list = [remote] if isinstance(remote, str) else list(remote or [])
        return PPProxyService.from_uri(
            local, remote_list, allow_partial=allow_partial
        ).start()

    if config is not None:
        return EggressService.from_toml(config).start()

    if config_path is not None:
        return EggressService.from_file(config_path).start()

    raise ValueError(
        "at least one of args, local, config, or config_path must be provided"
    )


def version() -> str:
    """Return the eggress version string.

    Returns:
        The version string, e.g. ``"0.1.0"``.
    """
    return __version__


def capabilities() -> dict[str, Any]:
    """Return a dict describing eggress capabilities and metadata.

    Returns a dict with the following keys:

    - ``version``: eggress version string
    - ``python_version``: Python runtime version (e.g. ``"3.12.4"``)
    - ``pproxy_compatibility_version``: target pproxy version (e.g. ``"2.7.9"``)
    - ``supported_protocols``: list of supported proxy protocol names
    - ``supported_schedulers``: list of supported scheduler names

    Example::

        >>> import eggress
        >>> caps = eggress.capabilities()
        >>> caps["version"]
        '0.1.0'
        >>> caps["pproxy_compatibility_version"]
        '2.7.9'
    """
    return {
        "version": __version__,
        "python_version": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        "pproxy_compatibility_version": compatibility_version(),
        "supported_protocols": [
            "http",
            "socks4",
            "socks4a",
            "socks5",
            "shadowsocks",
            "trojan",
        ],
        "supported_schedulers": [
            "round_robin",
            "least_connections",
            "first_available",
            "random",
        ],
    }


__all__ = [
    "EggressConfig",
    "EggressService",
    "EggressHandle",
    "AsyncEggressHandle",
    "EggressError",
    "ConfigError",
    "StartupError",
    "ReloadError",
    "ShutdownError",
    "UnsupportedFeatureError",
    "InternalError",
    "TranslationResult",
    "TranslationWarning",
    "UnsupportedFeature",
    "translate_pproxy_args",
    "translate_pproxy_uri",
    "check_pproxy_args",
    "describe_reverse_pproxy_uri",
    "ReverseUriSummary",
    "UriInfo",
    "check_pproxy_uri",
    "redact_pproxy_uri",
    "Diagnostic",
    "diagnostics_for_uri",
    "supported_features",
    "compatibility_version",
    "AlreadyStartedError",
    "Server",
    "explain_config_toml",
    "explain_pproxy_args",
    "explain_pproxy_uri",
    "route_explain",
    "check_upstream",
    "start_pproxy",
    "version",
    "capabilities",
    "CompatibilityReport",
    "FeatureInfo",
    "PPProxyService",
    "PPProxyHandle",
    "Connection",
    "ProxyConnection",
    "ConnectionState",
    "ConnectionBaseError",
    "ConnectionClosedError",
    "ConnectionTimeoutError",
    "ConnectionDnsError",
    "ConnectionAuthError",
    "ConnectionTlsError",
    "LoopMismatchError",
    "ConnectionCancelledError",
    "UseAfterCloseError",
    "UdpAssociationError",
    "UnsupportedCompositionError",
    "AsyncConnection",
    "OutboundConnector",
    "OutboundStream",
    "AsyncOutboundStream",
    # Phase C5 async bridge
    "AsyncBridge",
    "CloseWaiter",
    "LoopAffinityError",
    "PY_VERSION",
    "PY_MAJOR",
    "PY_MINOR",
    "HAS_TASKGROUP",
    "HAS_EXCEPTIONGROUP",
    # Protocol objects (Phase C4)
    "BaseProtocol",
    "Direct",
    "HTTP",
    "HTTPOnly",
    "Socks4",
    "Socks5",
    "SS",
    "SSR",
    "Trojan",
    "WS",
    "H2",
    "H3",
    "SSH",
    "Transparent",
    "Redir",
    "Pf",
    "Tunnel",
    "Echo",
    "PROTOCOL_MAPPINGS",
    "get_protos",
    "packstr",
    "netloc_split",
    "HTTP_LINE",
    # Cipher objects (Phase C4)
    "BaseCipher",
    "AEADCipher",
    "PacketCipher",
    "AES_256_GCM_Cipher",
    "AES_192_GCM_Cipher",
    "AES_128_GCM_Cipher",
    "ChaCha20_IETF_POLY1305_Cipher",
    "CIPHER_MAP",
    "get_cipher",
    # Plugin bridge (Phase C4)
    "PluginRegistry",
    "PluginBridge",
    "PluginError",
    "PluginTimeoutError",
    "PluginRejectedError",
    "PluginShutdownError",
    "PluginReentrantError",
    "CallbackResult",
    "CallbackMetrics",
    "CallbackWrapper",
    # Wrapper/composition objects (Phase C4)
    "BaseWrapper",
    "TLS",
    "PluginWrapper",
    "Chain",
    "normalize_chain",
    # Asyncio adapter (Milestone B4)
    "CompatibleStreamReader",
    "CompatibleStreamWriter",
    "open_tcp_connection",
    # Pproxy server proxy objects (Milestone B3)
    "AuthTable",
    "ProxyDirect",
    "ProxySimple",
    "ProxyBackward",
    "ProxyH2",
    "ProxySSH",
    "ProxyQUIC",
    "ProxyH3",
    "PROXY_DIRECT",
]
