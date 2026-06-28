__version__ = "0.1.0"

from typing import Optional, Sequence

from eggress._eggress import (
    EggressError,
    ConfigError,
    StartupError,
    ReloadError,
    ShutdownError,
    UnsupportedFeatureError,
    InternalError,
)
from eggress.config import EggressConfig
from eggress.service import EggressService, EggressHandle, AsyncEggressHandle
from eggress.pproxy import (
    TranslationResult,
    TranslationWarning,
    UnsupportedFeature,
    translate_pproxy_args,
    translate_pproxy_uri,
    check_pproxy_args,
)


def start_pproxy(
    args: Sequence[str],
    allow_partial: bool = False,
) -> EggressHandle:
    """Start an eggress service from pproxy-style CLI arguments.

    Convenience function that translates pproxy arguments, creates a service,
    starts it, and returns a handle.

    Args:
        args: pproxy-style CLI arguments.
        allow_partial: If True, start even when unsupported features exist.

    Returns:
        A handle to the running service.

    Example::

        with start_pproxy(["-l", "socks5://:1080", "-r", "http://proxy:8080"]) as handle:
            print(handle.bound_addresses)
    """
    return EggressService.from_pproxy_args(args, allow_partial=allow_partial).start()


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
    "start_pproxy",
]
