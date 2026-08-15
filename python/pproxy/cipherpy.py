"""Compatibility names for pproxy's optional pure-Python cipher module.

The upstream module contains a second implementation of many legacy
ciphers.  Eggress deliberately keeps one cipher implementation and exposes
the supported AEAD base through the Rust-backed compatibility cipher module.
Legacy pure-Python names are present for import compatibility and fail with a
named feature error when constructed; they are never silently substituted.
"""

from __future__ import annotations

from eggress.cipher import (
    AEADCipher,
    AES_128_GCM_Cipher,
    AES_192_GCM_Cipher,
    AES_256_GCM_Cipher,
    ChaCha20_IETF_POLY1305_Cipher,
    get_cipher,
)
from eggress.pproxy import UnsupportedPProxyFeature


class _LegacyCipher:
    """Import-compatible marker for an unsupported legacy cipher."""

    PYTHON = True
    KEY_LENGTH = 0
    IV_LENGTH = 0

    def __init__(self, *args, **kwargs):
        del args, kwargs
        raise UnsupportedPProxyFeature(
            f"pproxy.cipherpy.{type(self).__name__}",
            alternative="native Shadowsocks AEAD or the pproxy-legacy feature",
        )


def _legacy_class(name: str) -> type[_LegacyCipher]:
    return type(name, (_LegacyCipher,), {"__module__": __name__})


_LEGACY_NAMES = (
    "Table_Cipher",
    "RC4_Cipher",
    "RC4_MD5_Cipher",
    "ChaCha20_Cipher",
    "ChaCha20_IETF_Cipher",
    "XChaCha20_Cipher",
    "XChaCha20_IETF_Cipher",
    "XChaCha20_IETF_POLY1305_Cipher",
    "Salsa20_Cipher",
    "AES_128_CFB_Cipher",
    "AES_192_CFB_Cipher",
    "AES_256_CFB_Cipher",
    "AES_128_CFB8_Cipher",
    "AES_192_CFB8_Cipher",
    "AES_256_CFB8_Cipher",
    "AES_128_CTR_Cipher",
    "AES_192_CTR_Cipher",
    "AES_256_CTR_Cipher",
    "AES_128_OFB_Cipher",
    "AES_192_OFB_Cipher",
    "AES_256_OFB_Cipher",
    "BF_CFB_Cipher",
    "Camellia_128_CFB_Cipher",
    "Camellia_192_CFB_Cipher",
    "Camellia_256_CFB_Cipher",
    "IDEA_CFB_Cipher",
    "SEED_CFB_Cipher",
    "RC2_CFB_Cipher",
)

for _name in _LEGACY_NAMES:
    if _name not in globals():
        globals()[_name] = _legacy_class(_name)


__all__ = ["AEADCipher", "get_cipher", *_LEGACY_NAMES]
