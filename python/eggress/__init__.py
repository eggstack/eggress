try:
    from eggress._eggress import __version__ as __version__
except ImportError:
    __version__ = "0.1.0"

import sys
from typing import Any, Sequence

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
            "https",
            "ss",
            "trojan",
        ],
        "supported_schedulers": [
            "round_robin",
            "least_connections",
            "first_available",
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
    "start_pproxy",
    "version",
    "capabilities",
]
