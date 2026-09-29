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
except ImportError:
    from eggress import (
        EggressError,
        ConfigError,
        StartupError,
        ReloadError,
        ShutdownError,
        UnsupportedFeatureError,
        InternalError,
    )

from eggress.pproxy import AlreadyStartedError

try:
    from eggress.connection import (
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
    ConnectionBaseError = EggressError  # type: ignore[misc]
    ConnectionClosedError = EggressError  # type: ignore[misc]
    ConnectionTimeoutError = EggressError  # type: ignore[misc]
    ConnectionDnsError = EggressError  # type: ignore[misc]
    ConnectionAuthError = EggressError  # type: ignore[misc]
    ConnectionTlsError = EggressError  # type: ignore[misc]
    LoopMismatchError = EggressError  # type: ignore[misc]
    ConnectionCancelledError = EggressError  # type: ignore[misc]
    UseAfterCloseError = EggressError  # type: ignore[misc]
    UdpAssociationError = EggressError  # type: ignore[misc]
    UnsupportedCompositionError = EggressError  # type: ignore[misc]

__all__ = [
    "AlreadyStartedError",
    "EggressError",
    "ConfigError",
    "StartupError",
    "ReloadError",
    "ShutdownError",
    "UnsupportedFeatureError",
    "InternalError",
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
]
