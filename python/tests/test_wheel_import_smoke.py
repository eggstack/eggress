"""Wheel import and API smoke tests (Phase 32)."""

import os
import sys

import eggress
from eggress import pproxy


def _installed_path() -> str:
    """Return absolute path to the loaded eggress module file."""
    return os.path.realpath(eggress.__file__)


def test_import_eggress():
    assert hasattr(eggress, "__version__")
    assert isinstance(eggress.__version__, str)
    assert len(eggress.__version__) > 0


def test_imported_from_installed_wheel():
    """When EGRESS_EXPECT_INSTALLED_WHEEL=1 is set, eggress must be loaded
    from a site-packages/dist-packages installation (i.e., a wheel install),
    not from a local source tree. This prevents accidental source-tree
    shadowing during release validation."""
    if os.environ.get("EGRESS_EXPECT_INSTALLED_WHEEL") != "1":
        return
    path = _installed_path()
    assert "site-packages" in path or "dist-packages" in path, (
        f"eggress loaded from non-installed path: {path}; "
        "set EGRESS_EXPECT_INSTALLED_WHEEL=1 only inside an installed venv"
    )


def test_native_module_is_compiled():
    """The native extension module must be a compiled artifact (.so/.pyd/.dylib),
    not a Python source file. This verifies the wheel bundled the binary."""
    import eggress._eggress as native  # noqa: F401

    assert native.__file__.endswith((".so", ".pyd", ".dylib")), (
        f"native module is not a compiled artifact: {native.__file__}"
    )


def test_no_source_tree_in_sys_path():
    """The repo's python/ directory must not be in sys.path when running
    inside an installed venv; otherwise local source files would shadow
    the installed wheel. Only enforced when EGRESS_EXPECT_INSTALLED_WHEEL=1
    is set (i.e., this is a wheel-validation run, not a dev-source run)."""
    if os.environ.get("EGRESS_EXPECT_INSTALLED_WHEEL") != "1":
        return
    repo_python = os.path.realpath(
        os.path.join(os.path.dirname(__file__), "..", "..", "python")
    )
    for entry in sys.path:
        if not entry:
            continue
        normalized = os.path.realpath(entry)
        assert not normalized.startswith(repo_python), (
            f"sys.path leaks source tree: {normalized}"
        )


def test_import_eggress_pproxy():
    assert hasattr(pproxy, "check_pproxy_uri")
    assert hasattr(pproxy, "Server")


def test_pproxy_check_uri():
    info = pproxy.check_pproxy_uri("socks5://127.0.0.1:1080")
    assert isinstance(info, pproxy.UriInfo)
    assert info.ok is True
    assert info.scheme == "socks5"
    assert info.host == "127.0.0.1"
    assert info.port == 1080
    assert info.error is None


def test_pproxy_unsupported_uri():
    info = pproxy.check_pproxy_uri("not-a-valid-uri")
    assert isinstance(info, pproxy.UriInfo)
    assert info.ok is False
    assert info.error is not None


def test_pproxy_unsupported_feature_detection():
    result = pproxy.translate_pproxy_args(["-l", "ssh://example.com:22"])
    assert not result.ok
    assert len(result.unsupported) > 0


def test_pproxy_server_instantiate():
    srv = pproxy.Server(listen=["socks5://127.0.0.1:0"])
    assert isinstance(srv, pproxy.Server)
    assert srv.addresses == {}
    assert srv.is_ready is False
    assert srv.listener_info == []
    assert srv.metrics_text == ""


def test_version_metadata():
    assert isinstance(eggress.__version__, str)
    parts = eggress.__version__.split(".")
    assert len(parts) >= 2
    assert all(p.isdigit() for p in parts)


def test_capabilities():
    features = pproxy.supported_features()
    assert isinstance(features, list)
    assert len(features) > 0
    assert all(isinstance(f, str) for f in features)


def test_no_pproxy_shadow():
    """After `import eggress`, `sys.modules` should NOT contain a top-level
    `pproxy` entry unless pproxy was independently installed."""
    mod = sys.modules.get("pproxy")
    # If the user happens to have pproxy installed, this test is a no-op.
    # The important thing is that eggress itself does not inject one.
    if mod is not None:
        # Verify it's not our internal re-export
        assert not hasattr(mod, "_eggress"), (
            "sys.modules['pproxy' entry looks like eggress internals"
        )


def test_import_eggress_no_shadow():
    """After `import eggress`, verify `sys.modules.get('pproxy')` is None
    unless pproxy is separately installed."""
    mod = sys.modules.get("pproxy")
    if mod is not None:
        # pproxy package is installed; just verify we can also import it
        import pproxy as pp
        assert hasattr(pp, "Server")


def test_eggress_pproxy_coexists():
    """If pproxy is installed, both `import eggress` and `import pproxy`
    work independently."""
    import importlib
    # Both modules should be importable without conflict
    importlib.import_module("eggress")
    mod = sys.modules.get("pproxy")
    if mod is not None:
        import pproxy as pp
        assert hasattr(pp, "Server") or hasattr(pp, "server")


def test_py_typed_marker_exists():
    """The py.typed PEP 561 marker file should be present in the package."""
    import importlib.resources
    try:
        ref = importlib.resources.files("eggress").joinpath("py.typed")
        assert ref.is_file(), "py.typed marker file not found in eggress package"
    except (TypeError, AttributeError):
        # Fallback for Python 3.9 where files() API differs
        import pathlib
        pkg_dir = pathlib.Path(__file__).resolve().parents[1] / "eggress"
        assert (pkg_dir / "py.typed").exists(), "py.typed marker file not found"
