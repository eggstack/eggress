"""Compatibility import for Eggress' bounded plugin bridge."""

from eggress.plugin import *  # noqa: F401,F403
from eggress.pproxy import UnsupportedPProxyFeature

PLUGIN = {}

def get_plugin(name):
    raise UnsupportedPProxyFeature(
        f"plugin({name!r})",
        alternative="pproxy plugins are not supported by eggress; use Eggress plugin bridge",
    )
