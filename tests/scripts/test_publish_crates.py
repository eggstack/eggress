"""Focused tests for scripts/publish-crates.py (no crates.io writes)."""

import importlib.util
import sys
import types
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "publish-crates.py"


def load_helper():
    spec = importlib.util.spec_from_file_location("publish_crates", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules["publish_crates"] = module
    spec.loader.exec_module(module)
    return module


pc = load_helper()

VERSION = "1.0.7"


def _pkg(name, version=VERSION, publish=None, pid=None):
    return {
        "name": name,
        "version": version,
        "manifest_path": f"/repo/crates/{name}/Cargo.toml",
        "id": pid or f"{name} {version} (path+file:///repo/crates/{name})",
        "publish": publish,
    }


def _dep(name, req=f"={VERSION}", kind=None, optional=False):
    return {"name": name, "req": req, "kind": kind, "optional": optional}


def _metadata(packages, deps_by_crate):
    pkgs = []
    for p in packages:
        entry = dict(p)
        entry["dependencies"] = list(deps_by_crate.get(entry["name"], []))
        pkgs.append(entry)
    return {
        "packages": pkgs,
        "workspace_members": [p["id"] for p in pkgs],
    }


def _realistic_metadata():
    """Mirror the real workspace shape at a small scale."""
    packages = [
        _pkg("eggress-bench", publish=False),
        _pkg("eggress-relay"),
        _pkg("eggress-uri"),
        _pkg("eggress-core"),
        _pkg("eggress-config"),
        _pkg("eggress-pproxy-compat"),
        _pkg("eggress-outbound"),
        _pkg("eggress-server"),
        _pkg("eggress-python"),
    ]
    deps = {
        "eggress-bench": [],
        "eggress-relay": [],
        "eggress-uri": [],
        "eggress-core": [_dep("eggress-relay")],
        "eggress-config": [_dep("eggress-core"), _dep("eggress-uri")],
        "eggress-pproxy-compat": [_dep("eggress-config")],
        # Optional normal deps must still constrain order.
        "eggress-outbound": [
            _dep("eggress-config", optional=True),
            _dep("eggress-core"),
            _dep("eggress-pproxy-compat", optional=True),
        ],
        "eggress-server": [_dep("eggress-outbound")],
        "eggress-python": [_dep("eggress-core")],
    }
    return _metadata(packages, deps)


def test_publish_false_root_excluded():
    md = _realistic_metadata()
    packages = pc.discover_packages(md)
    assert "eggress-bench" not in packages
    assert set(packages) == {
        "eggress-relay",
        "eggress-uri",
        "eggress-core",
        "eggress-config",
        "eggress-pproxy-compat",
        "eggress-outbound",
        "eggress-server",
        "eggress-python",
    }


def test_real_workspace_discovers_28_crates():
    md = pc.load_metadata()
    packages = pc.discover_packages(md)
    assert len(packages) == 28
    assert "eggress-bench" not in packages
    assert "eggress-relay" in packages and "eggress-python" in packages


def test_real_workspace_order_respects_internal_edges():
    md = pc.load_metadata()
    packages = pc.discover_packages(md)
    edges = pc.internal_edges(md, packages)
    order = pc.topological_order(edges)
    assert len(order) == 28
    pos = {name: i for i, name in enumerate(order)}
    for consumer, prereqs in edges.items():
        for dep in prereqs:
            assert pos[dep] < pos[consumer], f"{dep} must precede {consumer}"


def test_optional_normal_deps_constrain_order():
    md = _realistic_metadata()
    packages = pc.discover_packages(md)
    edges = pc.internal_edges(md, packages)
    assert "eggress-config" in edges["eggress-outbound"]
    assert "eggress-pproxy-compat" in edges["eggress-outbound"]
    order = pc.topological_order(edges)
    pos = {n: i for i, n in enumerate(order)}
    assert pos["eggress-config"] < pos["eggress-outbound"]
    assert pos["eggress-pproxy-compat"] < pos["eggress-outbound"]


def test_build_deps_constrain_order():
    packages = [_pkg("eggress-a"), _pkg("eggress-b")]
    deps = {"eggress-a": [], "eggress-b": [_dep("eggress-a", kind="build", optional=True)]}
    md = _metadata(packages, deps)
    discovered = pc.discover_packages(md)
    edges = pc.internal_edges(md, discovered)
    assert edges["eggress-b"] == {"eggress-a"}
    assert pc.topological_order(edges) == ["eggress-a", "eggress-b"]


def test_path_only_dev_deps_do_not_constrain_order():
    packages = [_pkg("eggress-a"), _pkg("eggress-b")]
    deps = {
        "eggress-a": [],
        # Path-only dev edge with a wildcard req must not order publication.
        "eggress-b": [{"name": "eggress-a", "req": "*", "kind": "dev", "optional": False}],
    }
    md = _metadata(packages, deps)
    discovered = pc.discover_packages(md)
    edges = pc.internal_edges(md, discovered)
    assert edges["eggress-b"] == set()
    assert set(pc.topological_order(edges)) == {"eggress-a", "eggress-b"}


def test_unpinned_normal_dep_fails_closed():
    packages = [_pkg("eggress-a"), _pkg("eggress-b")]
    deps = {"eggress-a": [], "eggress-b": [{"name": "eggress-a", "req": "*", "kind": None}]}
    md = _metadata(packages, deps)
    discovered = pc.discover_packages(md)
    with pytest.raises(pc.PublishError, match="unpinned"):
        pc.internal_edges(md, discovered)


def test_cycle_fails_closed():
    with pytest.raises(pc.PublishError, match="cycle"):
        pc.topological_order({"eggress-a": {"eggress-b"}, "eggress-b": {"eggress-a"}})


def test_version_mismatch_fails_closed():
    packages = [_pkg("eggress-a"), _pkg("eggress-b", version="9.9.9")]
    deps = {"eggress-a": [], "eggress-b": []}
    md = _metadata(packages, deps)
    discovered = pc.discover_packages(md)
    with pytest.raises(pc.PublishError, match="differ from workspace"):
        pc.validate_versions(md, discovered, VERSION)


def test_pin_mismatch_fails_closed():
    packages = [_pkg("eggress-a"), _pkg("eggress-b")]
    deps = {"eggress-a": [], "eggress-b": [_dep("eggress-a", req="=9.9.9")]}
    md = _metadata(packages, deps)
    discovered = pc.discover_packages(md)
    with pytest.raises(pc.PublishError, match="expected"):
        pc.validate_versions(md, discovered, VERSION)


class _Proc:
    def __init__(self, returncode=0, stdout="", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


def test_already_published_subset_skipped():
    published = {"eggress-a"}
    queries = []

    def fake_query(crate, version):
        queries.append(crate)
        return "present" if crate in published else "missing"

    calls = []

    def fake_runner(cmd):
        calls.append(cmd)
        return _Proc(returncode=0)

    # 'eggress-a' is already published: no cargo invocation.
    assert pc.publish_one("eggress-a", runner=fake_runner, sleep=lambda s: None,
                           query=fake_query, version=VERSION) == "skipped-present"
    assert calls == []

    # 'eggress-b' is missing and becomes visible after upload: it publishes.
    visibility = {"calls": 0}

    def maturing_query(crate, version):
        # First call (pre-publish check) reports missing; afterwards the
        # version is visible, which also satisfies the post-publish wait.
        visibility["calls"] += 1
        return "missing" if visibility["calls"] == 1 else "present"

    assert pc.publish_one("eggress-b", runner=fake_runner, sleep=lambda s: None,
                           query=maturing_query,
                           version=VERSION) == "published"
    assert calls and calls[0][:3] == ["cargo", "publish", "-p"]


def test_execute_command_never_bypasses_verification():
    seen = []

    def fake_runner(cmd):
        seen.append(cmd)
        return _Proc(returncode=0)

    states = {"calls": 0}

    def maturing_query(crate, version):
        states["calls"] += 1
        return "missing" if states["calls"] == 1 else "present"

    pc.publish_one("eggress-core", runner=fake_runner, sleep=lambda s: None,
                   query=maturing_query, version=VERSION)
    assert seen, "expected at least one cargo publish invocation"
    for cmd in seen:
        assert "--no-verify" not in cmd
        assert "--allow-dirty" not in cmd
    assert pc.publish_command("eggress-core") == [
        "cargo", "publish", "-p", "eggress-core", "--locked",
    ]


def test_rate_limit_retry_bounded_without_real_sleep():
    attempts = []
    sleeps = []

    def flaky_runner(cmd):
        attempts.append(cmd)
        if len(attempts) < 3:
            return _Proc(returncode=1, stderr="error: 429 Too Many Requests, retry after 1")
        return _Proc(returncode=0)

    states = {"calls": 0}

    def maturing_query(crate, version):
        states["calls"] += 1
        # Pre-publish check + retry re-checks stay missing until the third
        # publish attempt succeeds; the post-publish visibility wait then sees
        # the version. Calls: 1=pre-check, 2=retry-2 re-check,
        # 3=retry-3 re-check, 4+=visibility present.
        if states["calls"] <= 3:
            return "missing"
        return "present"

    outcome = pc.publish_one(
        "eggress-core",
        runner=flaky_runner,
        sleep=lambda s: sleeps.append(s),
        query=maturing_query,
        version=VERSION,
    )
    assert outcome == "published"
    assert len(attempts) == 3
    # Bounded backoff honored the registry Retry-After without real minutes.
    assert sleeps and all(s <= pc.BACKOFF_CAP_SECONDS for s in sleeps)


def test_rate_limit_gives_up_after_bounded_budget():
    def always_throttled(cmd):
        return _Proc(returncode=1, stderr="error: 429 rate limit exceeded")

    with pytest.raises(pc.PublishError, match="attempt"):
        pc.publish_one(
            "eggress-core",
            runner=always_throttled,
            sleep=lambda s: None,
            query=lambda c, v: "missing",
            version=VERSION,
        )


def test_manifest_failure_does_not_retry():
    calls = []

    def failing_runner(cmd):
        calls.append(cmd)
        return _Proc(returncode=1, stderr="error: failed to verify package")

    with pytest.raises(pc.PublishError, match="verification"):
        pc.publish_one(
            "eggress-core",
            runner=failing_runner,
            sleep=lambda s: None,
            query=lambda c, v: "missing",
            version=VERSION,
        )
    assert len(calls) == 1


def test_dry_run_never_invokes_publish(monkeypatch):
    calls = []
    monkeypatch.setattr(pc, "check_preflight", lambda: None)
    monkeypatch.setattr(pc, "check_clean_tree", lambda: None)
    monkeypatch.setattr(pc, "compute_plan", lambda: (
        VERSION, ["a", "b"], {"a": set(), "b": {"a"}}, {"a": {}, "b": {}},
    ))
    monkeypatch.setattr(pc, "query_registry", lambda c, v: "missing")

    def fake_run(cmd, **kwargs):
        calls.append(cmd)
        return _Proc(returncode=0)

    monkeypatch.setattr(pc, "run", fake_run)
    assert pc.cmd_dry_run(skip_package_verify=True) == 0
    assert all("publish" not in cmd for cmd in calls)
