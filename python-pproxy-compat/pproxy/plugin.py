"""Plugin registry and bridge for pproxy-compatible callback extension points.

Re-exports from ``eggress.plugin`` for the pproxy compatibility namespace.
Also provides the oracle-compatible simple plugin classes (BasePlugin,
Plain_Plugin, Http_Simple_Plugin, etc.) and the PLUGIN registry.
"""

from __future__ import annotations

from typing import Any

try:
    from eggress.plugin import (
        PluginBridge,
        PluginError,
        PluginRejectedError,
        PluginRegistry,
        PluginShutdownError,
        PluginTimeoutError,
    )
except ImportError:
    # Minimal stubs when the Rust backend is unavailable
    class PluginError(Exception):
        pass

    class PluginTimeoutError(PluginError):
        pass

    class PluginRejectedError(PluginError):
        pass

    class PluginShutdownError(PluginError):
        pass

    class PluginReentrantError(PluginError):
        pass

    class PluginRegistry:
        """Stub plugin registry when eggress.plugin is unavailable."""

        def __init__(self) -> None:
            self._hooks: dict[str, list] = {}

        def register(self, hook_name: str, callback) -> None:
            self._hooks.setdefault(hook_name, []).append(callback)

        def unregister(self, hook_name: str, callback) -> None:
            if hook_name in self._hooks:
                self._hooks[hook_name] = [
                    c for c in self._hooks[hook_name] if c is not callback
                ]

        def get(self, hook_name: str) -> list:
            return list(self._hooks.get(hook_name, []))

        def has(self, hook_name: str) -> bool:
            return bool(self._hooks.get(hook_name))

        def list_hooks(self) -> list[str]:
            return sorted(self._hooks.keys())

        def clear(self) -> None:
            self._hooks.clear()

    class PluginBridge:
        """Stub plugin bridge when eggress.plugin is unavailable."""

        def __init__(self, registry=None, max_concurrent=10, default_timeout=5.0):
            self._registry = registry or PluginRegistry()

        async def submit_async(self, hook_name: str, **kwargs):
            hooks = self._registry.get(hook_name)
            results = []
            for hook in hooks:
                try:
                    result = hook(**kwargs)
                    if hasattr(result, "__await__"):
                        result = await result
                    results.append(result)
                except Exception as e:
                    results.append(e)
            return results

        def submit_sync(self, hook_name: str, **kwargs):
            hooks = self._registry.get(hook_name)
            results = []
            for hook in hooks:
                try:
                    results.append(hook(**kwargs))
                except Exception as e:
                    results.append(e)
            return results

        def shutdown(self) -> None:
            pass


__all__ = [
    "PluginBridge",
    "PluginError",
    "PluginRejectedError",
    "PluginRegistry",
    "PluginShutdownError",
    "PluginTimeoutError",
    # Oracle-compatible plugin classes
    "BasePlugin",
    "Plain_Plugin",
    "Origin_Plugin",
    "Http_Simple_Plugin",
    "Verify_Simple_Plugin",
    "Verify_Deflate_Plugin",
    "Tls1__2_Ticket_Auth_Plugin",
    "PLUGIN",
    "get_plugin",
]


# ---------------------------------------------------------------------------
# Oracle-compatible simple plugin classes
# ---------------------------------------------------------------------------


class BasePlugin:
    """Base class for pproxy cipher/auth plugins (oracle-compatible)."""

    @property
    def name(self) -> str:
        return "base"

    def add_cipher(self, cipher: Any) -> None:
        pass

    async def init_client_data(
        self, reader: Any, writer: Any, cipher: Any
    ) -> None:
        pass

    async def init_server_data(
        self, reader: Any, writer: Any, cipher: Any, raddr: Any
    ) -> None:
        pass


class Plain_Plugin(BasePlugin):
    """Plain plugin — no-op cipher/auth (default)."""

    @property
    def name(self) -> str:
        return "plain"


class Origin_Plugin(BasePlugin):
    """Origin plugin — allows connections from any origin."""

    @property
    def name(self) -> str:
        return "origin"


class Http_Simple_Plugin(BasePlugin):
    """HTTP Simple plugin — sends HTTP request as obfuscation."""

    @property
    def name(self) -> str:
        return "http_simple"

    async def init_server_data(
        self, reader: Any, writer: Any, cipher: Any, raddr: Any
    ) -> None:
        host = raddr[0] if isinstance(raddr, tuple) else str(raddr)
        writer.write(
            f"GET / HTTP/1.1\r\nHost: {host}\r\n"
            f"User-Agent: curl\r\nAccept-Encoding: gzip, deflate\r\n"
            f"Connection: keep-alive\r\n\r\n".encode()
        )
        await reader.read_until(b"\r\n\r\n")


class Verify_Simple_Plugin(BasePlugin):
    """Verify Simple plugin — verifies simple HTTP response."""

    @property
    def name(self) -> str:
        return "verify_simple"

    async def init_client_data(
        self, reader: Any, writer: Any, cipher: Any
    ) -> None:
        await reader.read_until(b"\r\n\r\n")


class Verify_Deflate_Plugin(BasePlugin):
    """Verify Deflate plugin — verifies deflated HTTP response."""

    @property
    def name(self) -> str:
        return "verify_deflate"

    async def init_client_data(
        self, reader: Any, writer: Any, cipher: Any
    ) -> None:
        import zlib
        await reader.read_until(b"\r\n\r\n")


class Tls1__2_Ticket_Auth_Plugin(BasePlugin):
    """TLS 1.2 Ticket Auth plugin — session ticket authentication."""

    @property
    def name(self) -> str:
        return "tls1.2_ticket_auth"


# Plugin registry — maps names to plugin classes
PLUGIN: dict[str, type[BasePlugin]] = {
    "plain": Plain_Plugin,
    "origin": Origin_Plugin,
    "http_simple": Http_Simple_Plugin,
    "verify_simple": Verify_Simple_Plugin,
    "verify_deflate": Verify_Deflate_Plugin,
    "tls1.2_ticket_auth": Tls1__2_Ticket_Auth_Plugin,
}


def get_plugin(plugin_name: str) -> BasePlugin:
    """Return an instance of the named plugin.

    Raises ``KeyError`` if the plugin name is not registered.
    """
    cls = PLUGIN.get(plugin_name)
    if cls is None:
        raise KeyError(f"Unknown plugin: {plugin_name}")
    return cls()
