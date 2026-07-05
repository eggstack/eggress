"""Type stubs for the eggress public API."""

from __future__ import annotations

from typing import Any, Sequence

from eggress.config import EggressConfig
from eggress.service import EggressService, EggressHandle, AsyncEggressHandle, PPProxyHandle
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
from eggress._eggress import (
    EggressError,
    ConfigError,
    StartupError,
    ReloadError,
    ShutdownError,
    UnsupportedFeatureError,
    InternalError,
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
