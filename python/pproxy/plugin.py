"""Compatibility import for Eggress' bounded plugin bridge."""

from eggress.plugin import *  # noqa: F401,F403

PLUGIN = {}

def get_plugin(name):
    raise NotImplementedError(
        f"pproxy plugin {name!r} is not supported by the Eggress adapter"
    )
