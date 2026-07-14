"""Contract validation tests for pproxy API compatibility.

Compares eggress's compatibility module against the extracted pproxy API
contract. Rejects undocumented drift and validates explicit divergences.

See python/compat/extract_api.py for the contract generator.
See python/compat/classification.json for the classification mapping.
"""
from __future__ import annotations

import ast
import inspect
import json
import re
import textwrap
from pathlib import Path
from typing import Any, Optional, Sequence

import pytest

CONTRACT_PATH = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "compat" / "pproxy_api_contract.json"
)
CLASSIFICATION_PATH = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "compat" / "classification.json"
)
EGGRESS_INIT = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "__init__.py"
)
EGGRESS_PPROXY = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "pproxy.py"
)
EGGRESS_SERVICE = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "service.py"
)
EGGRESS_CONFIG = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "config.py"
)
EGGRESS_INIT_STUB = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "__init__.pyi"
)
EGGRESS_PPROXY_STUB = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "pproxy.pyi"
)
EGGRESS_SERVICE_STUB = (
    Path(__file__).resolve().parent.parent.parent
    / "python" / "eggress" / "service.pyi"
)

# Standard-library re-exports from pproxy internal modules that do not
# need classification entries (they are Python stdlib, not pproxy API).
_STDLIB_REEXPORTS = frozenset({
    "asyncio", "base64", "hashlib", "hmac", "io", "os", "re",
    "socket", "struct", "time", "urllib", "argparse", "functools",
    "random", "proto",
})

# Bare module names that appear in the contract but are not classified as
# individual symbols (they are the module containers themselves).
_UNCLASSIFIED_BARE_MODULES = frozenset({"pproxy", "pproxy.cipher"})


# ──────────────────────────────────────────────────────────────────────
# Allowlist of explicit divergences with stable IDs.
#
# Each entry documents a known difference between the pproxy API contract
# and eggress's implementation. Tests in TestDivergenceAllowlist verify
# that only allowlisted divergences exist.
#
# Keys: stable divergence ID (kebab-case, lowercase)
# Values: rationale, owner_phase, expiration, and the affected symbol(s)
# ──────────────────────────────────────────────────────────────────────
ALLOWED_DIVERGENCES: dict[str, dict[str, str]] = {
    "pproxy-connection-is-alias": {
        "rationale": (
            "pproxy.Connection is an alias for proxies_by_uri; "
            "eggress uses PPProxyService.from_args with pproxy-style CLI arg translation"
        ),
        "owner_phase": "C1",
        "expires": "never",
        "symbol": "pproxy.Connection",
    },
    "pproxy-direct-global-sentinel": {
        "rationale": (
            "pproxy.DIRECT is a ProxyDirect sentinel object; "
            "eggress encodes 'direct' via URI scheme, not a module-level constant"
        ),
        "owner_phase": "C1",
        "expires": "never",
        "symbol": "pproxy.DIRECT",
    },
    "pproxy-rule-is-function": {
        "rationale": (
            "pproxy.Rule is compile_rule(filename); "
            "eggress translates rulefile lines to reject rules via PproxyRuleFile::load()"
        ),
        "owner_phase": "C1",
        "expires": "never",
        "symbol": "pproxy.Rule",
    },
    "pproxy-proto-internal-only": {
        "rationale": (
            "pproxy.proto is an internal implementation module; "
            "eggress protocol handling is in Rust core"
        ),
        "owner_phase": "C1",
        "expires": "never",
        "symbol": "pproxy.proto",
    },
    "pproxy-server-internal-only": {
        "rationale": (
            "pproxy.server is an internal implementation module; "
            "eggress server orchestration is in Rust core"
        ),
        "owner_phase": "C1",
        "expires": "never",
        "symbol": "pproxy.server",
    },
    "pproxy-cipher-internal-only": {
        "rationale": (
            "pproxy.cipher is an internal module; "
            "eggress Shadowsocks ciphers are in Rust core"
        ),
        "owner_phase": "C1",
        "expires": "never",
        "symbol": "pproxy.cipher",
    },
}


# ──────────────────────────────────────────────────────────────────────
# Helpers
# ──────────────────────────────────────────────────────────────────────


def _load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def _parse_assign_names(filepath: Path) -> set[str]:
    """Extract top-level assigned names from a Python file."""
    source = filepath.read_text()
    tree = ast.parse(source)
    names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    names.add(target.id)
    return names


def _parse_all_names(filepath: Path) -> set[str]:
    """Extract the ``__all__`` list from a Python file."""
    source = filepath.read_text()
    tree = ast.parse(source)
    all_names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == "__all__":
                    if isinstance(node.value, ast.List):
                        for elt in node.value.elts:
                            if isinstance(elt, ast.Constant):
                                all_names.add(elt.value)
    return all_names


def _parse_module_names(filepath: Path) -> set[str]:
    """Parse a Python file and return all top-level names.

    Covers class definitions, function definitions, and variable
    assignments — **not** import statements.  For re-export checking
    (e.g. ``from eggress.pproxy import Foo``), use ``_parse_all_names``
    or ``_parse_imported_names``.
    """
    source = filepath.read_text()
    tree = ast.parse(source)
    names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.ClassDef):
            names.add(node.name)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    names.add(target.id)
    return names


def _parse_imported_names(filepath: Path) -> set[str]:
    """Return the set of names brought into scope via import statements.

    Handles both ``import X`` and ``from X import Y, Z, ...`` at the
    top level (not inside try/except or other blocks).
    """
    source = filepath.read_text()
    tree = ast.parse(source)
    names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                names.add(alias.asname or alias.name)
        elif isinstance(node, ast.ImportFrom):
            for alias in node.names:
                names.add(alias.asname or alias.name)
    return names


def _parse_all_names_in_scope(filepath: Path) -> set[str]:
    """All names accessible at module scope (definitions + imports)."""
    return _parse_module_names(filepath) | _parse_imported_names(filepath)


def _parse_stub_names(filepath: Path) -> set[str]:
    """Parse a .pyi stub file and return all declared names.

    Covers class/function definitions, variable assignments, *and*
    import re-exports (``from X import Y as Y``).
    """
    source = filepath.read_text()
    tree = ast.parse(source)
    names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.ClassDef):
            names.add(node.name)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    names.add(target.id)
        elif isinstance(node, ast.ImportFrom):
            for alias in node.names:
                # ``from X import Y as Y`` is an explicit re-export
                if alias.asname is not None:
                    names.add(alias.asname)
                else:
                    names.add(alias.name)
        elif isinstance(node, ast.Import):
            for alias in node.names:
                names.add(alias.asname or alias.name)
    return names


def _parse_function_params(filepath: Path, func_name: str) -> dict[str, Any] | None:
    """Extract parameter info for a function from a Python file."""
    source = filepath.read_text()
    tree = ast.parse(source)
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == func_name:
            params: dict[str, Any] = {}
            args = node.args
            # positional-only args
            for i, arg in enumerate(args.args):
                name = arg.arg
                default = None
                # defaults align to the end of args list
                num_defaults = len(args.defaults)
                offset = i - (len(args.args) - num_defaults)
                if offset >= 0:
                    default_node = args.defaults[offset]
                    if isinstance(default_node, ast.Constant):
                        default = default_node.value
                    elif isinstance(default_node, ast.NameConstant):
                        default = default_node.value
                    else:
                        default = f"<ast:{type(default_node).__name__}>"
                has_default = offset >= 0
                params[name] = {"default": default, "has_default": has_default}
            # keyword-only args
            for i, arg in enumerate(args.kwonlyargs):
                name = arg.arg
                default = None
                if i < len(args.kw_defaults) and args.kw_defaults[i] is not None:
                    default_node = args.kw_defaults[i]
                    if isinstance(default_node, ast.Constant):
                        default = default_node.value
                    elif isinstance(default_node, ast.NameConstant):
                        default = default_node.value
                    else:
                        default = f"<ast:{type(default_node).__name__}>"
                    params[name] = {"default": default, "has_default": True}
                else:
                    params[name] = {"default": None, "has_default": False}
            return params
    return None


def _parse_class_members(filepath: Path, class_name: str) -> set[str]:
    """Extract method/property/field names for a class.

    Handles regular ``def``, ``async def``, plain assignments, *and*
    annotated assignments (``x: int = 5``) used by dataclasses.
    """
    source = filepath.read_text()
    tree = ast.parse(source)
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef) and node.name == class_name:
            members: set[str] = set()
            for item in node.body:
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    members.add(item.name)
                elif isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            members.add(target.id)
                elif isinstance(item, ast.AnnAssign):
                    if isinstance(item.target, ast.Name):
                        members.add(item.target.id)
            return members
    return set()


# Keep backward-compat alias for existing call-sites
_parse_class_methods = _parse_class_members


# ──────────────────────────────────────────────────────────────────────
# Fixtures
# ──────────────────────────────────────────────────────────────────────


@pytest.fixture(scope="module")
def contract() -> dict:
    """Load the extracted pproxy API contract."""
    return _load_json(CONTRACT_PATH)


@pytest.fixture(scope="module")
def classification() -> dict:
    """Load the classification mapping."""
    return _load_json(CLASSIFICATION_PATH)


@pytest.fixture(scope="module")
def classified_symbols(classification: dict) -> dict[str, dict]:
    """Build a lookup from pproxy_symbol to its classification entry."""
    return {
        entry["pproxy_symbol"]: entry
        for entry in classification["classifications"]
    }


@pytest.fixture(scope="module")
def eggress_pproxy_names() -> set[str]:
    """Top-level names defined in eggress.pproxy."""
    return _parse_module_names(EGGRESS_PPROXY)


@pytest.fixture(scope="module")
def eggress_init_names() -> set[str]:
    """Names accessible in eggress.__init__ (definitions + imports)."""
    return _parse_all_names_in_scope(EGGRESS_INIT)


@pytest.fixture(scope="module")
def eggress_service_names() -> set[str]:
    """Top-level names defined in eggress.service."""
    return _parse_module_names(EGGRESS_SERVICE)


# ──────────────────────────────────────────────────────────────────────
# Tier predicates
# ──────────────────────────────────────────────────────────────────────


def _is_actionable(entry: dict) -> bool:
    """Return True if this classification tier requires an eggress mapping."""
    return entry["tier"] in ("adapted_target", "exact_target")


def _is_intentional_non_parity(entry: dict) -> bool:
    return entry["tier"] == "intentional_non_parity"


def _is_internal_observed(entry: dict) -> bool:
    return entry["tier"] == "internal_observed"


def _is_unsupported_blocker(entry: dict) -> bool:
    return entry["tier"] == "unsupported_release_blocker"


# ──────────────────────────────────────────────────────────────────────
# Tests
# ──────────────────────────────────────────────────────────────────────


class TestSymbolPresence:
    """Every classified symbol has an eggress mapping or is explicitly excluded."""

    def test_classification_has_entries(self, classification: dict):
        assert "classifications" in classification
        assert len(classification["classifications"]) > 0

    def test_actionable_symbols_have_eggress_mapping(
        self, classified_symbols: dict[str, dict]
    ):
        """Every adapted_target or exact_target symbol must have a non-NOT_IMPLEMENTED location."""
        actionable = {
            sym: entry
            for sym, entry in classified_symbols.items()
            if _is_actionable(entry)
        }
        for sym, entry in actionable.items():
            loc = entry["eggress_location"]
            assert loc != "NOT_IMPLEMENTED", (
                f"Actionable symbol {sym} (tier={entry['tier']}) "
                f"has no eggress mapping"
            )

    def test_no_unsupported_release_blockers(
        self, classified_symbols: dict[str, dict]
    ):
        """No symbol should be classified as unsupported_release_blocker."""
        blockers = {
            sym: entry
            for sym, entry in classified_symbols.items()
            if _is_unsupported_blocker(entry)
        }
        assert blockers == {}, (
            f"Found unsupported release blockers: {list(blockers.keys())}"
        )

    @pytest.mark.parametrize(
        "tier",
        ["adapted_target", "exact_target", "intentional_non_parity",
         "internal_observed", "unsupported_release_blocker"],
    )
    def test_tier_is_in_known_set(self, classified_symbols: dict[str, dict], tier: str):
        """All classification tiers in the file belong to the known set."""
        known_tiers = {
            "adapted_target", "exact_target", "intentional_non_parity",
            "internal_observed", "unsupported_release_blocker",
        }
        for sym, entry in classified_symbols.items():
            assert entry["tier"] in known_tiers, (
                f"{sym} has unknown tier: {entry['tier']}"
            )

    def test_eggress_reverse_mapping_exists(self, classification: dict):
        """The reverse mapping section must be present and non-empty."""
        assert "eggress_reverse_mapping" in classification
        assert len(classification["eggress_reverse_mapping"]) > 0

    def test_reverse_mapping_has_required_fields(self, classification: dict):
        required = {"eggress_symbol", "pproxy_concept", "relationship", "notes"}
        for entry in classification["eggress_reverse_mapping"]:
            missing = required - set(entry.keys())
            assert not missing, (
                f"Reverse mapping entry missing fields: {missing}"
            )

    def test_all_pproxy_contract_symbols_are_classified(
        self, contract: dict, classified_symbols: dict[str, dict]
    ):
        """Every pproxy module export from the contract has a classification entry."""
        unclassified = []
        for module_name, module_data in contract.get("modules", {}).items():
            # Skip bare module names that are containers, not API surface
            if module_name in _UNCLASSIFIED_BARE_MODULES:
                continue
            for export in module_data.get("exports", []):
                full_name = f"{module_name}.{export}"
                # Skip stdlib re-exports — they are Python builtins, not
                # pproxy API surface that needs classification.
                if export in _STDLIB_REEXPORTS:
                    continue
                if full_name not in classified_symbols:
                    unclassified.append(full_name)
        assert unclassified == [], (
            f"Unclassified contract symbols: {unclassified}"
        )


class TestImportPaths:
    """eggress imports resolve correctly."""

    def test_eggress_init_imports_pproxy_classes(self, eggress_init_names: set[str]):
        expected = {
            "TranslationResult", "TranslationWarning", "UnsupportedFeature",
            "UriInfo", "Diagnostic", "CompatibilityReport", "FeatureInfo",
            "ReverseUriSummary", "AlreadyStartedError", "Server", "PPProxyService",
        }
        missing = expected - eggress_init_names
        assert not missing, f"Missing imports in __init__: {missing}"

    def test_eggress_init_imports_pproxy_functions(self, eggress_init_names: set[str]):
        expected = {
            "translate_pproxy_args", "translate_pproxy_uri",
            "check_pproxy_args", "check_pproxy_uri", "redact_pproxy_uri",
            "diagnostics_for_uri", "supported_features",
            "compatibility_version", "describe_reverse_pproxy_uri",
            "explain_config_toml", "explain_pproxy_args",
            "explain_pproxy_uri", "route_explain", "check_upstream",
            "start_pproxy", "version", "capabilities",
        }
        missing = expected - eggress_init_names
        assert not missing, f"Missing functions in __init__: {missing}"

    def test_eggress_init_imports_exceptions(self, eggress_init_names: set[str]):
        expected = {
            "EggressError", "ConfigError", "StartupError", "ReloadError",
            "ShutdownError", "UnsupportedFeatureError", "InternalError",
        }
        missing = expected - eggress_init_names
        assert not missing, f"Missing exceptions in __init__: {missing}"

    def test_eggress_init_imports_service_classes(self, eggress_init_names: set[str]):
        expected = {
            "EggressConfig", "EggressService", "EggressHandle",
            "AsyncEggressHandle",
        }
        missing = expected - eggress_init_names
        assert not missing, f"Missing service classes in __init__: {missing}"

    def test_eggress_pproxy_has_all_public_symbols(
        self, eggress_pproxy_names: set[str]
    ):
        """pproxy.py defines all symbols that __init__.py re-exports from it."""
        expected = {
            "TranslationResult", "TranslationWarning", "UnsupportedFeature",
            "UriInfo", "Diagnostic", "CompatibilityReport", "FeatureInfo",
            "ReverseUriSummary", "AlreadyStartedError", "Server", "PPProxyService",
            "translate_pproxy_args", "translate_pproxy_uri",
            "check_pproxy_args", "check_pproxy_uri", "redact_pproxy_uri",
            "diagnostics_for_uri", "supported_features",
            "compatibility_version", "describe_reverse_pproxy_uri",
            "explain_config_toml", "explain_pproxy_args",
            "explain_pproxy_uri", "route_explain", "check_upstream",
            "redact_config_toml",
        }
        missing = expected - eggress_pproxy_names
        assert not missing, f"Missing in pproxy.py: {missing}"

    def test_eggress_init_has_all_alphabetical_exports(
        self, eggress_init_names: set[str]
    ):
        """__init__.py should re-export everything it declares in __all__."""
        all_names = _parse_all_names(EGGRESS_INIT)
        missing = all_names - eggress_init_names
        assert not missing, (
            f"__all__ lists symbols not defined in __init__: {missing}"
        )


class TestSignatureCompatibility:
    """Adapted targets preserve parameter names and defaults where applicable."""

    def test_translate_pproxy_args_params(self):
        params = _parse_function_params(EGGRESS_PPROXY, "translate_pproxy_args")
        assert params is not None, "translate_pproxy_args not found"
        assert "args" in params
        assert params["args"]["has_default"] is False

    def test_translate_pproxy_uri_params(self):
        params = _parse_function_params(EGGRESS_PPROXY, "translate_pproxy_uri")
        assert params is not None, "translate_pproxy_uri not found"
        assert "local" in params
        assert params["local"]["has_default"] is False
        assert "remotes" in params
        assert params["remotes"]["has_default"] is True

    def test_check_pproxy_args_params(self):
        params = _parse_function_params(EGGRESS_PPROXY, "check_pproxy_args")
        assert params is not None, "check_pproxy_args not found"
        assert "args" in params

    def test_check_pproxy_uri_params(self):
        params = _parse_function_params(EGGRESS_PPROXY, "check_pproxy_uri")
        assert params is not None, "check_pproxy_uri not found"
        assert "uri" in params

    def test_redact_pproxy_uri_params(self):
        params = _parse_function_params(EGGRESS_PPROXY, "redact_pproxy_uri")
        assert params is not None, "redact_pproxy_uri not found"
        assert "uri" in params

    def test_server_init_params(self):
        source = EGGRESS_PPROXY.read_text()
        tree = ast.parse(source)
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef) and node.name == "Server":
                for item in node.body:
                    if isinstance(item, ast.FunctionDef) and item.name == "__init__":
                        arg_names = [a.arg for a in item.args.args]
                        kw_names = [a.arg for a in item.args.kwonlyargs]
                        all_names = set(arg_names) | set(kw_names)
                        assert "listen" in all_names, f"Missing 'listen' param, got {all_names}"
                        assert "remote" in all_names, f"Missing 'remote' param, got {all_names}"
                        assert "config" in all_names, f"Missing 'config' param, got {all_names}"
                        assert "allow_partial" in all_names, (
                            f"Missing 'allow_partial' param, got {all_names}"
                        )
                        return
        pytest.fail("Server.__init__ not found")

    def test_ppproxyservice_from_args_params(self):
        source = EGGRESS_PPROXY.read_text()
        tree = ast.parse(source)
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef) and node.name == "PPProxyService":
                for item in node.body:
                    if isinstance(item, ast.FunctionDef) and item.name == "from_args":
                        arg_names = [a.arg for a in item.args.args if a.arg != "cls"]
                        kw_names = [a.arg for a in item.args.kwonlyargs]
                        all_names = set(arg_names) | set(kw_names)
                        assert "args" in all_names
                        assert "allow_partial" in all_names
                        return
        pytest.fail("PPProxyService.from_args not found")

    def test_start_pproxy_params(self):
        params = _parse_function_params(EGGRESS_INIT, "start_pproxy")
        assert params is not None, "start_pproxy not found"
        expected = {"args", "local", "remote", "config", "config_path",
                     "allow_partial", "background", "log_format"}
        assert expected.issubset(set(params.keys())), (
            f"Missing params: {expected - set(params.keys())}"
        )

    def test_check_upstream_default_timeout(self):
        params = _parse_function_params(EGGRESS_PPROXY, "check_upstream")
        assert params is not None, "check_upstream not found"
        assert "uri" in params
        assert "timeout" in params
        assert params["timeout"]["has_default"] is True
        assert params["timeout"]["default"] == 5.0


class TestExceptionHierarchy:
    """eggress exceptions inherit from correct base classes."""

    def test_already_started_error_inherits_exception(self):
        """AlreadyStartedError is defined in pproxy.py and inherits from Exception."""
        source = EGGRESS_PPROXY.read_text()
        tree = ast.parse(source)
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef) and node.name == "AlreadyStartedError":
                base_names = []
                for b in node.bases:
                    if isinstance(b, ast.Name):
                        base_names.append(b.id)
                    elif isinstance(b, ast.Attribute):
                        base_names.append(b.attr)
                assert "Exception" in base_names, (
                    f"AlreadyStartedError bases: {base_names}"
                )
                return
        pytest.fail("AlreadyStartedError class not found in pproxy.py")

    def test_eggress_error_hierarchy_is_coherent(self):
        """EggressError and subclasses should be importable and form a hierarchy."""
        try:
            from eggress._eggress import (
                EggressError, ConfigError, StartupError, ReloadError,
                ShutdownError, UnsupportedFeatureError, InternalError,
            )
        except ImportError:
            pytest.skip("_eggress native module not available")

        assert issubclass(ConfigError, EggressError)
        assert issubclass(StartupError, EggressError)
        assert issubclass(ReloadError, EggressError)
        assert issubclass(ShutdownError, EggressError)
        assert issubclass(InternalError, EggressError)
        assert issubclass(UnsupportedFeatureError, EggressError)
        assert issubclass(EggressError, Exception)

    def test_already_started_error_is_subclass_of_exception(self):
        """AlreadyStartedError is importable from eggress.pproxy and is an Exception."""
        try:
            from eggress.pproxy import AlreadyStartedError
        except ImportError:
            pytest.skip("eggress.pproxy not importable")
        assert issubclass(AlreadyStartedError, Exception)

    def test_exceptions_in_all(self, eggress_init_names: set[str]):
        expected = {
            "EggressError", "ConfigError", "StartupError", "ReloadError",
            "ShutdownError", "UnsupportedFeatureError", "InternalError",
        }
        missing = expected - eggress_init_names
        assert not missing, f"Exceptions missing from __init__: {missing}"

    def test_already_started_error_in_init_all(self):
        """AlreadyStartedError is re-exported from __init__.py."""
        all_names = _parse_all_names(EGGRESS_INIT)
        assert "AlreadyStartedError" in all_names


class TestMethodAvailability:
    """Key methods exist on Server, PPProxyService, etc."""

    def test_server_sync_methods(self):
        expected = {"start", "close", "stop", "run", "__enter__", "__exit__"}
        actual = _parse_class_members(EGGRESS_PPROXY, "Server")
        missing = expected - actual
        assert not missing, f"Server missing sync methods: {missing}"

    def test_server_async_methods(self):
        expected = {"astart", "aclose", "wait_closed", "__aenter__", "__aexit__"}
        actual = _parse_class_members(EGGRESS_PPROXY, "Server")
        missing = expected - actual
        assert not missing, f"Server missing async methods: {missing}"

    def test_server_properties(self):
        expected = {"addresses", "config", "is_ready", "listener_info", "metrics_text"}
        actual = _parse_class_members(EGGRESS_PPROXY, "Server")
        missing = expected - actual
        assert not missing, f"Server missing properties: {missing}"

    def test_ppproxyservice_classmethods(self):
        expected = {"from_args", "from_uri", "from_toml", "from_file"}
        actual = _parse_class_members(EGGRESS_PPROXY, "PPProxyService")
        missing = expected - actual
        assert not missing, f"PPProxyService missing classmethods: {missing}"

    def test_pproxyservice_has_start(self):
        actual = _parse_class_members(EGGRESS_PPROXY, "PPProxyService")
        assert "start" in actual

    def test_pproxyservice_has_config_property(self):
        actual = _parse_class_members(EGGRESS_PPROXY, "PPProxyService")
        assert "config" in actual

    def test_eggress_handle_methods(self):
        expected = {
            "bound_addresses", "status", "metrics_text",
            "reload_toml", "shutdown", "__enter__", "__exit__",
        }
        actual = _parse_class_members(EGGRESS_SERVICE, "EggressHandle")
        missing = expected - actual
        assert not missing, f"EggressHandle missing methods: {missing}"

    def test_async_eggress_handle_methods(self):
        expected = {
            "bound_addresses", "status", "metrics_text",
            "reload_toml", "shutdown", "__aenter__", "__aexit__",
        }
        actual = _parse_class_members(EGGRESS_SERVICE, "AsyncEggressHandle")
        missing = expected - actual
        assert not missing, f"AsyncEggressHandle missing methods: {missing}"

    def test_eggress_service_methods(self):
        expected = {"from_toml", "from_file", "from_pproxy_args", "start", "astart"}
        actual = _parse_class_members(EGGRESS_SERVICE, "EggressService")
        missing = expected - actual
        assert not missing, f"EggressService missing methods: {missing}"

    def test_translation_result_properties(self):
        expected = {"toml", "warnings", "unsupported", "ok", "config"}
        actual = _parse_class_members(EGGRESS_PPROXY, "TranslationResult")
        missing = expected - actual
        assert not missing, f"TranslationResult missing properties: {missing}"

    def test_diagnostic_fields(self):
        expected = {"code", "feature_id", "tier", "message", "suggestion"}
        actual = _parse_class_members(EGGRESS_PPROXY, "Diagnostic")
        missing = expected - actual
        assert not missing, f"Diagnostic missing fields: {missing}"

    def test_compatibility_report_fields(self):
        expected = {
            "tier", "ok", "warnings", "unsupported", "diagnostics",
            "features", "toml", "parsed_uris", "raw_args",
        }
        actual = _parse_class_members(EGGRESS_PPROXY, "CompatibilityReport")
        missing = expected - actual
        assert not missing, f"CompatibilityReport missing fields: {missing}"


class TestStubConsistency:
    """Type stubs match runtime signatures."""

    def _stub_names(self, stub_path: Path) -> set[str]:
        return _parse_stub_names(stub_path)

    def test_pproxy_stub_has_all_runtime_symbols(self, eggress_pproxy_names: set[str]):
        stub = self._stub_names(EGGRESS_PPROXY_STUB)
        # Stubs must cover all public names.  Filter out:
        # - names starting with _ (private/internal)
        # - internal module-level constants (ALL_CAPS) that are not part of
        #   the public API contract
        public = {
            n for n in eggress_pproxy_names
            if not n.startswith("_") and not (n.isupper() and "_" in n)
        }
        missing = public - stub
        assert not missing, f"pproxy.pyi missing symbols: {missing}"

    def test_service_stub_has_all_runtime_symbols(self, eggress_service_names: set[str]):
        stub = self._stub_names(EGGRESS_SERVICE_STUB)
        public = {n for n in eggress_service_names if not n.startswith("_")}
        missing = public - stub
        assert not missing, f"service.pyi missing symbols: {missing}"

    def test_init_stub_has_all_init_symbols(self):
        """__init__.pyi stub covers the __all__ list from __init__.py."""
        stub = self._stub_names(EGGRESS_INIT_STUB)
        all_names = _parse_all_names(EGGRESS_INIT)
        missing = all_names - stub
        assert not missing, f"__init__.pyi missing __all__ symbols: {missing}"

    def test_pproxy_stub_declarations_match_runtime_signatures(self):
        """Verify key function signatures in the stub match the runtime."""
        stub_source = EGGRESS_PPROXY_STUB.read_text()
        runtime_source = EGGRESS_PPROXY.read_text()

        key_functions = [
            "translate_pproxy_args",
            "translate_pproxy_uri",
            "check_pproxy_args",
            "check_pproxy_uri",
            "redact_pproxy_uri",
            "diagnostics_for_uri",
            "supported_features",
            "describe_reverse_pproxy_uri",
            "explain_config_toml",
            "explain_pproxy_args",
            "explain_pproxy_uri",
            "route_explain",
            "check_upstream",
            "compatibility_version",
        ]

        for func_name in key_functions:
            stub_has = f"def {func_name}(" in stub_source
            runtime_has = f"def {func_name}(" in runtime_source
            assert stub_has == runtime_has, (
                f"{func_name}: stub={'defined' if stub_has else 'missing'}, "
                f"runtime={'defined' if runtime_has else 'missing'}"
            )

    def test_stub_dataclass_fields_match_runtime(self):
        """Verify key dataclass fields match between stubs and runtime."""
        key_classes = {
            "TranslationWarning": ["category", "message"],
            "UnsupportedFeature": ["feature", "message"],
            "UriInfo": [
                "scheme", "host", "port", "tls", "ssl", "inbound",
                "backward_num", "has_auth", "has_rule", "is_reverse_listener",
                "redacted_display", "error",
            ],
            "Diagnostic": ["code", "feature_id", "tier", "message", "suggestion"],
            "FeatureInfo": ["feature_id", "tier", "supported"],
            "ReverseUriSummary": [
                "role", "scheme", "target", "has_auth",
                "toml_section", "tls", "modifiers",
            ],
        }
        stub_source = EGGRESS_PPROXY_STUB.read_text()
        runtime_source = EGGRESS_PPROXY.read_text()

        for class_name, fields in key_classes.items():
            for field in fields:
                stub_has = f"{field}:" in stub_source
                runtime_has = f"{field}:" in runtime_source
                assert stub_has == runtime_has, (
                    f"{class_name}.{field}: stub={'found' if stub_has else 'missing'}, "
                    f"runtime={'found' if runtime_has else 'missing'}"
                )


class TestDivergenceAllowlist:
    """Only documented divergences are permitted."""

    def test_allowlist_is_non_empty(self):
        assert len(ALLOWED_DIVERGENCES) > 0, "Allowlist should not be empty"

    def test_allowlist_entries_have_required_fields(self):
        required = {"rationale", "owner_phase", "expires", "symbol"}
        for div_id, entry in ALLOWED_DIVERGENCES.items():
            missing = required - set(entry.keys())
            assert not missing, (
                f"Divergence '{div_id}' missing fields: {missing}"
            )

    def test_allowlist_symbol_exists_in_classification(
        self, classified_symbols: dict[str, dict]
    ):
        """Every divergence references a symbol that exists in the classification."""
        for div_id, entry in ALLOWED_DIVERGENCES.items():
            symbol = entry["symbol"]
            # Bare module names (pproxy.proto, pproxy.cipher, pproxy.server)
            # and the top-level pproxy module are not individual classified
            # symbols — they are module containers.  They are valid as
            # divergence targets but do not appear in classification keys.
            if symbol in _UNCLASSIFIED_BARE_MODULES or symbol == "pproxy":
                continue
            assert symbol in classified_symbols, (
                f"Divergence '{div_id}' references unknown symbol: {symbol}"
            )

    def test_allowlist_symbols_have_expected_tiers(
        self, classified_symbols: dict[str, dict]
    ):
        """Divergence symbols should be adapted_target, exact_target,
        intentional_non_parity, or internal_observed (for module-level
        entries that are internal by design)."""
        for div_id, entry in ALLOWED_DIVERGENCES.items():
            symbol = entry["symbol"]
            if symbol not in classified_symbols:
                # Bare module name — skip tier check for these
                continue
            tier = classified_symbols[symbol]["tier"]
            assert tier in (
                "adapted_target", "exact_target",
                "intentional_non_parity", "internal_observed",
            ), (
                f"Divergence '{div_id}' for {symbol} has unexpected tier: {tier}"
            )

    def test_no_undocumented_actionable_divergences(
        self, classified_symbols: dict[str, dict]
    ):
        """Every actionable symbol that is NOT a direct 1:1 mapping must be
        in the allowlist."""
        actionable = {
            sym: entry
            for sym, entry in classified_symbols.items()
            if _is_actionable(entry)
        }
        allowlisted_symbols = {d["symbol"] for d in ALLOWED_DIVERGENCES.values()}

        for sym, entry in actionable.items():
            loc = entry["eggress_location"]
            if loc == "NOT_IMPLEMENTED":
                assert sym in allowlisted_symbols, (
                    f"Actionable symbol {sym} has NOT_IMPLEMENTED but is not in allowlist"
                )

    def test_internal_observed_modules_in_allowlist_are_valid(
        self, classified_symbols: dict[str, dict]
    ):
        """If an internal_observed symbol IS in the allowlist, it must be
        a module-level entry (pproxy.proto, pproxy.cipher, pproxy.server)
        that is explicitly called out as internal-only."""
        allowlisted = {
            d["symbol"]: div_id
            for div_id, d in ALLOWED_DIVERGENCES.items()
        }
        for symbol, div_id in allowlisted.items():
            if symbol in classified_symbols:
                tier = classified_symbols[symbol]["tier"]
                if tier == "internal_observed":
                    # Only module-level internal entries may be in the allowlist
                    assert symbol in ("pproxy.proto", "pproxy.server", "pproxy.cipher"), (
                        f"Internal symbol {symbol} should not be in divergence "
                        f"allowlist (divergence ID: {div_id})"
                    )

    def test_divergence_ids_are_kebab_case(self):
        for div_id in ALLOWED_DIVERGENCES:
            assert re.match(r'^[a-z0-9]+(-[a-z0-9]+)*$', div_id), (
                f"Divergence ID '{div_id}' is not kebab-case"
            )

    def test_divergence_owners_are_phase_strings(self):
        for div_id, entry in ALLOWED_DIVERGENCES.items():
            phase = entry["owner_phase"]
            assert isinstance(phase, str) and len(phase) > 0, (
                f"Divergence '{div_id}' has invalid owner_phase: {phase}"
            )
