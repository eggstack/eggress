"""Plugin registry and bridge for pproxy-compatible callback extension points.

Re-exports from ``eggress.plugin`` for the pproxy compatibility namespace.
"""

from __future__ import annotations

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
]
