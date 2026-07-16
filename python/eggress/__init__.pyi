"""Type stubs for the eggress public API."""

from __future__ import annotations

from typing import Any, Sequence

from eggress.config import EggressConfig as EggressConfig
from eggress.service import EggressService as EggressService, EggressHandle as EggressHandle, AsyncEggressHandle as AsyncEggressHandle, PPProxyHandle as PPProxyHandle
from eggress.pproxy import (
    TranslationResult as TranslationResult,
    TranslationWarning as TranslationWarning,
    UnsupportedFeature as UnsupportedFeature,
    translate_pproxy_args as translate_pproxy_args,
    translate_pproxy_uri as translate_pproxy_uri,
    check_pproxy_args as check_pproxy_args,
    describe_reverse_pproxy_uri as describe_reverse_pproxy_uri,
    ReverseUriSummary as ReverseUriSummary,
    UriInfo as UriInfo,
    check_pproxy_uri as check_pproxy_uri,
    redact_pproxy_uri as redact_pproxy_uri,
    Diagnostic as Diagnostic,
    diagnostics_for_uri as diagnostics_for_uri,
    supported_features as supported_features,
    compatibility_version as compatibility_version,
    AlreadyStartedError as AlreadyStartedError,
    Server as Server,
    explain_config_toml as explain_config_toml,
    explain_pproxy_args as explain_pproxy_args,
    explain_pproxy_uri as explain_pproxy_uri,
    route_explain as route_explain,
    check_upstream as check_upstream,
    CompatibilityReport as CompatibilityReport,
    FeatureInfo as FeatureInfo,
    PPProxyService as PPProxyService,
)
from eggress.protocol import (
    BaseProtocol as BaseProtocol,
    Direct as Direct,
    HTTP as HTTP,
    HTTPOnly as HTTPOnly,
    Socks4 as Socks4,
    Socks5 as Socks5,
    SS as SS,
    SSR as SSR,
    Trojan as Trojan,
    WS as WS,
    H2 as H2,
    H3 as H3,
    SSH as SSH,
    Transparent as Transparent,
    Redir as Redir,
    Pf as Pf,
    Tunnel as Tunnel,
    Echo as Echo,
    MAPPINGS as PROTOCOL_MAPPINGS,
    get_protos as get_protos,
    packstr as packstr,
    netloc_split as netloc_split,
    HTTP_LINE as HTTP_LINE,
)
from eggress.cipher import (
    BaseCipher as BaseCipher,
    AEADCipher as AEADCipher,
    PacketCipher as PacketCipher,
    AES_256_GCM_Cipher as AES_256_GCM_Cipher,
    AES_192_GCM_Cipher as AES_192_GCM_Cipher,
    AES_128_GCM_Cipher as AES_128_GCM_Cipher,
    ChaCha20_IETF_POLY1305_Cipher as ChaCha20_IETF_POLY1305_Cipher,
    MAP as CIPHER_MAP,
    get_cipher as get_cipher,
)
from eggress.plugin import (
    PluginRegistry as PluginRegistry,
    PluginBridge as PluginBridge,
    PluginError as PluginError,
    PluginTimeoutError as PluginTimeoutError,
    PluginRejectedError as PluginRejectedError,
    PluginShutdownError as PluginShutdownError,
    PluginReentrantError as PluginReentrantError,
    CallbackResult as CallbackResult,
    CallbackMetrics as CallbackMetrics,
    CallbackWrapper as CallbackWrapper,
)
from eggress.wrapper import (
    BaseWrapper as BaseWrapper,
    TLS as TLS,
    Plugin as PluginWrapper,
    Chain as Chain,
    normalize_chain as normalize_chain,
)
from eggress.connection import (
    Connection as Connection,
    ConnectionState as ConnectionState,
    ConnectionError as ConnectionBaseError,
    ConnectionClosedError as ConnectionClosedError,
    TimeoutError as ConnectionTimeoutError,
    DnsError as ConnectionDnsError,
    AuthError as ConnectionAuthError,
    TlsError as ConnectionTlsError,
    LoopMismatchError as LoopMismatchError,
    ConnectionCancelledError as ConnectionCancelledError,
    UseAfterCloseError as UseAfterCloseError,
    UdpAssociationError as UdpAssociationError,
    UnsupportedCompositionError as UnsupportedCompositionError,
)
from eggress.async_connection import AsyncConnection as AsyncConnection
from eggress.pproxy_connection import ProxyConnection as ProxyConnection
from eggress.outbound import (
    AsyncOutboundStream as AsyncOutboundStream,
    OutboundConnector as OutboundConnector,
    OutboundStream as OutboundStream,
)
from eggress._asyncio import (
    AsyncBridge as AsyncBridge,
    CloseWaiter as CloseWaiter,
    LoopAffinityError as LoopAffinityError,
    PY_VERSION as PY_VERSION,
    PY_MAJOR as PY_MAJOR,
    PY_MINOR as PY_MINOR,
    HAS_TASKGROUP as HAS_TASKGROUP,
    HAS_EXCEPTIONGROUP as HAS_EXCEPTIONGROUP,
)
from eggress._eggress import (
    EggressError as EggressError,
    ConfigError as ConfigError,
    StartupError as StartupError,
    ReloadError as ReloadError,
    ShutdownError as ShutdownError,
    UnsupportedFeatureError as UnsupportedFeatureError,
    InternalError as InternalError,
)

__version__: str

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
) -> EggressHandle: ...
def version() -> str: ...
def capabilities() -> dict[str, Any]: ...
