"""Phase 4 contract checks for the complete pproxy module namespace."""

from __future__ import annotations

import asyncio
import inspect
import os
import pkgutil
import subprocess
import sys

import pproxy
import pytest

from scripts.pproxy_surface_probe import TRACKED_SYMBOLS


def test_pinned_module_namespace_is_importable():
    expected = {
        "pproxy",
        "pproxy.__doc__",
        "pproxy.__main__",
        "pproxy.cipher",
        "pproxy.cipherpy",
        "pproxy.plugin",
        "pproxy.proto",
        "pproxy.server",
        "pproxy.sysproxy",
        "pproxy.verbose",
    }
    actual = {pproxy.__name__}
    actual.update(info.name for info in pkgutil.iter_modules(pproxy.__path__, "pproxy."))
    assert expected <= actual
    for module_name in expected:
        __import__(module_name)


def test_tracked_symbols_and_callable_shapes():
    for module_name, names in TRACKED_SYMBOLS.items():
        module = __import__(module_name, fromlist=["*"])
        for name in names:
            assert hasattr(module, name), f"missing {module_name}.{name}"
            value = getattr(module, name)
            if callable(value):
                assert inspect.signature(value) is not None

    import pproxy.server as server
    import pproxy.sysproxy as sysproxy
    import pproxy.verbose as verbose

    assert str(inspect.signature(server.main)) == "(args=None)"
    assert inspect.iscoroutinefunction(server.check_server_alive)
    assert inspect.iscoroutinefunction(server.prepare_ciphers)
    assert inspect.iscoroutinefunction(server.datagram_handler)
    assert inspect.iscoroutinefunction(server.stream_handler)
    assert inspect.iscoroutinefunction(server.test_url)
    assert inspect.iscoroutinefunction(verbose.realtime_stat)
    assert not inspect.iscoroutinefunction(verbose.setup)
    assert not inspect.iscoroutinefunction(sysproxy.setup)


def test_metadata_keeps_eggress_version_truthful():
    from pproxy import __doc__ as metadata

    assert metadata.__title__ == "pproxy"
    assert metadata.__url__.endswith("qwj/python-proxy")
    assert metadata.__version__ == pproxy.__version__
    assert metadata.__version__ != "2.7.9"


def test_python_module_entry_point_reports_compatibility_version():
    result = subprocess.run(
        [sys.executable, "-m", "pproxy", "--version"],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0
    assert result.stdout.strip() == f"eggress-pproxy-compat {pproxy.__version__}"


def test_server_helpers_delegate_or_fail_at_the_feature_boundary():
    import pproxy.server as server

    assert asyncio.run(server.prepare_ciphers(None, None, None)) == (None, None)

    async def cancel_health_probe():
        task = asyncio.create_task(server.check_server_alive(0, [], lambda *_: None))
        await asyncio.sleep(0)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task

    asyncio.run(cancel_health_probe())

    request = server.test_url("http://127.0.0.1:1/", [])
    assert inspect.iscoroutine(request)
    request.close()


def test_modern_cipherpy_aead_names_delegate_to_native_implementations():
    from eggress.cipher import AEADCipher
    from pproxy.cipherpy import (
        AES_128_GCM_Cipher,
        AES_192_GCM_Cipher,
        AES_256_GCM_Cipher,
        ChaCha20_IETF_POLY1305_Cipher,
    )

    for cipher_type, key_length in (
        (AES_128_GCM_Cipher, 16),
        (AES_192_GCM_Cipher, 24),
        (AES_256_GCM_Cipher, 32),
        (ChaCha20_IETF_POLY1305_Cipher, 32),
    ):
        assert isinstance(cipher_type(b"k" * key_length), AEADCipher)


def test_console_adapter_starts_a_minimal_listener():
    from eggress.pproxy import PPProxyService

    service = PPProxyService.from_args(["-l", "socks5://127.0.0.1:0"])
    handle = service.start()
    try:
        assert handle.bound_addresses
    finally:
        handle.shutdown()


def test_python_compat_runtime_options_use_native_parser_and_supervisor():
    from eggress._eggress import pproxy_runtime_options
    from eggress.pproxy import PPProxyService

    options = pproxy_runtime_options(
        ["-l", "socks5://127.0.0.1:0", "--auth", "30", "--sys", "-vv"]
    )
    assert options["auth_timeout_seconds"] == 30
    assert options["system_proxy"] is True
    assert options["verbose_level"] == 2
    assert options["default_log_level"] == "debug"

    # --auth and -vv must be carried into the native compatibility supervisor
    # instead of being accepted and silently dropped by the Python wrapper.
    service = PPProxyService.from_args(
        ["-l", "socks5://127.0.0.1:0", "--auth", "30", "-vv"]
    )
    handle = service.start()
    try:
        assert handle.bound_addresses
    finally:
        handle.shutdown()


def test_system_proxy_uses_native_bridge_or_platform_refusal():
    import pproxy.sysproxy as sysproxy

    if sys.platform not in ("darwin", "win32"):
        assert sysproxy.setup(type("Args", (), {"listen": []})()) is None

    import eggress._eggress as native

    assert hasattr(native, "apply_system_proxy")
