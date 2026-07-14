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
