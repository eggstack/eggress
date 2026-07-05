"""Type stubs for the eggress.pproxy module."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Sequence

from eggress._eggress import UnsupportedFeatureError

_PPROXY_COMPATIBILITY_VERSION: str

def compatibility_version() -> str: ...

@dataclass(frozen=True)
class TranslationWarning:
    category: str
    message: str

@dataclass(frozen=True)
class UnsupportedFeature:
    feature: str
    message: str

class TranslationResult:
    @property
    def toml(self) -> str: ...
    @property
    def warnings(self) -> list[TranslationWarning]: ...
    @property
    def unsupported(self) -> list[UnsupportedFeature]: ...
    @property
    def ok(self) -> bool: ...
    def config(self) -> Any: ...

def translate_pproxy_args(args: Sequence[str]) -> TranslationResult: ...
def translate_pproxy_uri(local: str, remotes: Sequence[str] = ()) -> TranslationResult: ...
def check_pproxy_args(args: Sequence[str]) -> CompatibilityReport: ...

@dataclass(frozen=True)
class ReverseUriSummary:
    role: str
    scheme: str
    target: str
    has_auth: bool
    toml_section: str
    tls: bool
    modifiers: tuple[str, ...]

def describe_reverse_pproxy_uri(uri: str) -> ReverseUriSummary: ...

@dataclass(frozen=True)
class UriInfo:
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
    def ok(self) -> bool: ...

def check_pproxy_uri(uri: str) -> UriInfo: ...
def redact_pproxy_uri(uri: str) -> str: ...

@dataclass(frozen=True)
class Diagnostic:
    code: str
    feature_id: Optional[str]
    tier: Optional[str]
    message: str
    suggestion: Optional[str]

@dataclass(frozen=True)
class FeatureInfo:
    feature_id: str
    tier: str
    supported: bool

@dataclass(frozen=True)
class CompatibilityReport:
    tier: str
    ok: bool
    warnings: list[Diagnostic]
    unsupported: list[Diagnostic]
    diagnostics: list[Diagnostic]
    features: list[FeatureInfo]
    toml: str | None
    parsed_uris: dict[str, UriInfo]
    raw_args: list[str]

def diagnostics_for_uri(uri: str) -> list[Diagnostic]: ...
def supported_features() -> list[str]: ...

class AlreadyStartedError(Exception): ...

class Server:
    def __init__(
        self,
        listen: list[str] | None = None,
        remote: list[str] | None = None,
        *,
        config: Any | None = None,
        allow_partial: bool = False,
    ) -> None: ...
    def start(self) -> Server: ...
    def close(self) -> None: ...
    def stop(self) -> None: ...
    def run(self) -> None: ...
    async def astart(self) -> Server: ...
    async def aclose(self) -> None: ...
    async def wait_closed(self) -> None: ...
    @property
    def addresses(self) -> dict[str, str]: ...
    @property
    def config(self) -> Any: ...
    @property
    def is_ready(self) -> bool: ...
    @property
    def listener_info(self) -> list[dict]: ...
    @property
    def metrics_text(self) -> str: ...
    def __enter__(self) -> Server: ...
    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool: ...
    async def __aenter__(self) -> Server: ...
    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool: ...

class PPProxyService:
    def __init__(
        self,
        listen: list[str] | None = None,
        remote: list[str] | None = None,
        *,
        config: Any | None = None,
        allow_partial: bool = False,
    ) -> None: ...
    @classmethod
    def from_args(cls, args: Sequence[str], allow_partial: bool = False) -> PPProxyService: ...
    @classmethod
    def from_uri(cls, local: str, remotes: Sequence[str] = (), allow_partial: bool = False) -> PPProxyService: ...
    @classmethod
    def from_toml(cls, toml: str) -> PPProxyService: ...
    @classmethod
    def from_file(cls, path: str) -> PPProxyService: ...
    def start(self) -> Server: ...
    def __enter__(self) -> Server: ...
    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool: ...

def explain_config_toml(toml_str: str) -> dict: ...
def explain_pproxy_args(args: list[str]) -> dict: ...
def explain_pproxy_uri(uri: str) -> dict: ...
def route_explain(config_toml: str, target: str) -> dict: ...
def check_upstream(uri: str, timeout: float = 5.0) -> dict: ...
