"""pproxy system-proxy compatibility adapter.

Mutation and rollback are delegated to the same native backend used by the
compatibility runtime.  Unsupported platforms fail clearly, matching the
upstream Linux behavior instead of changing the process environment.
"""

from __future__ import annotations

import sys
from typing import Any

from eggress.pproxy import UnsupportedPProxyFeature


def _listener(args: Any, protocol: str) -> Any:
    for option in getattr(args, "listen", ()):
        protos = {getattr(proto, "name", "") for proto in getattr(option, "protos", ())}
        if getattr(option, "unix", False) or {"ssl", "secure"} & protos:
            continue
        if protocol in protos:
            return option
    return None


class _NativeSetting:
    _kind = "http"

    def __init__(self, args: Any) -> None:
        option = _listener(args, self._kind)
        if option is None:
            raise UnsupportedPProxyFeature(
                "system-proxy",
                alternative=f"configure a local {self._kind} or socks5 listener",
            )
        self.listen = option
        self._native = self._apply(option)

    def _apply(self, option: Any) -> Any:
        try:
            from eggress._eggress import apply_system_proxy

            port = int(option.port)
            return apply_system_proxy(self._kind, f"127.0.0.1:{port}")
        except (AttributeError, ImportError, ValueError) as exc:
            raise UnsupportedPProxyFeature(
                "system-proxy",
                alternative="use the native compatibility runtime --sys path",
            ) from exc
        except Exception as exc:
            raise UnsupportedPProxyFeature("system-proxy", alternative=str(exc)) from exc

    def clear(self) -> None:
        if self._native is not None:
            self._native.restore()
            self._native = None


class MacSetting(_NativeSetting):
    _kind = "socks5"


class WindowsSetting(_NativeSetting):
    _kind = "http"


def setup(args: Any) -> Any:
    if sys.platform == "darwin":
        return MacSetting(args)
    if sys.platform == "win32":
        return WindowsSetting(args)
    print(f'System proxy setting: platform "{sys.platform}" not supported')
    return None


__all__ = ["MacSetting", "WindowsSetting", "setup"]
