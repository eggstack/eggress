"""Metadata exposed by the pinned pproxy 2.7.9 package.

The compatibility distribution remains named ``eggress``.  In particular,
``__version__`` is the Eggress distribution version, not a false packaging
claim that this wheel is the upstream pproxy release.
"""

__title__ = "pproxy"
__license__ = "MIT"
__description__ = "Proxy server that can tunnel among remote servers by regex rules."
__keywords__ = "proxy socks http shadowsocks shadowsocksr ssr redirect pf tunnel cipher ssl udp"
__author__ = "Qian Wenjie"
__email__ = "qianwenjie@gmail.com"
__url__ = "https://github.com/qwj/python-proxy"

try:
    from eggress import __version__ as _eggress_version
except ImportError:  # pragma: no cover - source-only metadata probe
    _eggress_version = "unknown"

__version__ = _eggress_version

__all__ = ["__version__", "__description__", "__url__"]
