"""Pure-Python reference cipher implementations for pproxy 2.7.9 compatibility.

Workstream C13 of Milestone C.  Re-exports the cipher classes from
``eggress.cipher`` under the short pproxy 2.7.9 names (without the
``_Cipher`` suffix).  When ``eggress.cipher`` is unavailable, basic
stub classes are provided so that imports always succeed.
"""

from __future__ import annotations

from typing import Any

try:
    from eggress.cipher import (
        AEADCipher as _AEADCipher,
        BaseCipher as _BaseCipher,
        PacketCipher as _PacketCipher,
        StreamCipher as _StreamCipher,
        _evp_bytes_to_key as _evp_bytes_to_key_impl,
        get_cipher as _get_cipher_impl,
        MAP as _EGGRESS_MAP,
        AES_256_GCM_Cipher as _AES_256_GCM_Cipher,
        AES_192_GCM_Cipher as _AES_192_GCM_Cipher,
        AES_128_GCM_Cipher as _AES_128_GCM_Cipher,
        ChaCha20_IETF_POLY1305_Cipher as _ChaCha20_IETF_POLY1305_Cipher,
        RC4_Cipher as _RC4_Cipher,
        RC4_MD5_Cipher as _RC4_MD5_Cipher,
        ChaCha20_Cipher as _ChaCha20_Cipher,
        ChaCha20_IETF_Cipher as _ChaCha20_IETF_Cipher,
        Salsa20_Cipher as _Salsa20_Cipher,
        AES_256_CFB_Cipher as _AES_256_CFB_Cipher,
        AES_192_CFB_Cipher as _AES_192_CFB_Cipher,
        AES_128_CFB_Cipher as _AES_128_CFB_Cipher,
        AES_256_CFB8_Cipher as _AES_256_CFB8_Cipher,
        AES_192_CFB8_Cipher as _AES_192_CFB8_Cipher,
        AES_128_CFB8_Cipher as _AES_128_CFB8_Cipher,
        AES_256_OFB_Cipher as _AES_256_OFB_Cipher,
        AES_192_OFB_Cipher as _AES_192_OFB_Cipher,
        AES_128_OFB_Cipher as _AES_128_OFB_Cipher,
        AES_256_CTR_Cipher as _AES_256_CTR_Cipher,
        AES_192_CTR_Cipher as _AES_192_CTR_Cipher,
        AES_128_CTR_Cipher as _AES_128_CTR_Cipher,
        BF_CFB_Cipher as _BF_CFB_Cipher,
        CAST5_CFB_Cipher as _CAST5_CFB_Cipher,
        DES_CFB_Cipher as _DES_CFB_Cipher,
    )

    _HAS_EGGRESS_CIPHER = True
except ImportError:
    _HAS_EGGRESS_CIPHER = False

if _HAS_EGGRESS_CIPHER:
    # Aliases with short pproxy 2.7.9 names (no _Cipher suffix).
    BaseCipher = _BaseCipher
    StreamCipher = _StreamCipher
    AEADCipher = _AEADCipher
    PacketCipher = _PacketCipher

    RC4 = _RC4_Cipher
    RC4_MD5 = _RC4_MD5_Cipher
    ChaCha20 = _ChaCha20_Cipher
    ChaCha20_IETF = _ChaCha20_IETF_Cipher
    ChaCha20_IETF_POLY1305 = _ChaCha20_IETF_POLY1305_Cipher

    AES_256_GCM = _AES_256_GCM_Cipher
    AES_192_GCM = _AES_192_GCM_Cipher
    AES_128_GCM = _AES_128_GCM_Cipher

    AES_256_CFB = _AES_256_CFB_Cipher
    AES_192_CFB = _AES_192_CFB_Cipher
    AES_128_CFB = _AES_128_CFB_Cipher
    AES_256_CFB8 = _AES_256_CFB8_Cipher
    AES_192_CFB8 = _AES_192_CFB8_Cipher
    AES_128_CFB8 = _AES_128_CFB8_Cipher

    AES_256_OFB = _AES_256_OFB_Cipher
    AES_192_OFB = _AES_192_OFB_Cipher
    AES_128_OFB = _AES_128_OFB_Cipher

    AES_256_CTR = _AES_256_CTR_Cipher
    AES_192_CTR = _AES_192_CTR_Cipher
    AES_128_CTR = _AES_128_CTR_Cipher

    Salsa20 = _Salsa20_Cipher
    BF_CFB = _BF_CFB_Cipher
    CAST5_CFB = _CAST5_CFB_Cipher
    DES_CFB = _DES_CFB_Cipher

    _evp_bytes_to_key = _evp_bytes_to_key_impl
    get_cipher = _get_cipher_impl

    # Build MAP using short names as values, keeping the same string keys.
    # Includes pproxy 2.7.9 `-py` variant aliases.
    MAP: dict[str, type[BaseCipher]] = {
        "aes-256-gcm": AES_256_GCM,
        "aes-192-gcm": AES_192_GCM,
        "aes-128-gcm": AES_128_GCM,
        "chacha20-ietf-poly1305": ChaCha20_IETF_POLY1305,
        "rc4": RC4,
        "rc4-md5": RC4_MD5,
        "chacha20": ChaCha20,
        "chacha20-ietf": ChaCha20_IETF,
        "salsa20": Salsa20,
        "aes-256-cfb": AES_256_CFB,
        "aes-192-cfb": AES_192_CFB,
        "aes-128-cfb": AES_128_CFB,
        "aes-256-cfb8": AES_256_CFB8,
        "aes-192-cfb8": AES_192_CFB8,
        "aes-128-cfb8": AES_128_CFB8,
        "aes-256-ofb": AES_256_OFB,
        "aes-192-ofb": AES_192_OFB,
        "aes-128-ofb": AES_128_OFB,
        "aes-256-ctr": AES_256_CTR,
        "aes-192-ctr": AES_192_CTR,
        "aes-128-ctr": AES_128_CTR,
        "bf-cfb": BF_CFB,
        "cast5-cfb": CAST5_CFB,
        "des-cfb": DES_CFB,
        # pproxy 2.7.9 aliases: -py variants point to the same implementation
        "aes-256-gcm-py": AES_256_GCM,
        "aes-128-gcm-py": AES_128_GCM,
        "chacha20-ietf-poly1305-py": ChaCha20_IETF_POLY1305,
        "rc4-md5-py": RC4_MD5,
        "chacha20-py": ChaCha20,
        "salsa20-py": Salsa20,
        "aes-256-cfb-py": AES_256_CFB,
        "aes-128-cfb-py": AES_128_CFB,
        "aes-256-cfb8-py": AES_256_CFB8,
        "aes-128-cfb8-py": AES_128_CFB8,
        "aes-256-ofb-py": AES_256_OFB,
        "aes-128-ofb-py": AES_128_OFB,
        "aes-256-ctr-py": AES_256_CTR,
        "aes-128-ctr-py": AES_128_CTR,
        "bf-cfb-py": BF_CFB,
    }

else:
    # Fallback: stub classes so imports always succeed.
    class BaseCipher:  # type: ignore[no-redef]
        KEY_LENGTH: int = 0
        IV_LENGTH: int = 0
        PYTHON: bool = False

        def __init__(self, key: bytes | str, ota: bool = False, setup_key: bool = True) -> None:
            if isinstance(key, str):
                key = key.encode("utf-8")
            self._key = key
            self.ota = ota

        @property
        def key(self) -> bytes:
            return self._key

        def setup_iv(self, iv: Any = None) -> None:
            pass

        def encrypt(self, s: bytes) -> bytes:
            raise NotImplementedError("eggress.cipher is unavailable")

        def decrypt(self, s: bytes) -> bytes:
            raise NotImplementedError("eggress.cipher is unavailable")

    class StreamCipher(BaseCipher):  # type: ignore[no-redef]
        BLOCK_SIZE: int = 1

    class AEADCipher(BaseCipher):  # type: ignore[no-redef]
        NONCE_LENGTH: int = 12
        TAG_LENGTH: int = 16
        PACKET_LIMIT: int = 0x3FFF

    class PacketCipher:  # type: ignore[no-redef]
        def __init__(self, cipher: Any, key: bytes | str, name: str) -> None:
            self._cipher = cipher
            self._name = name

    RC4 = type("RC4", (StreamCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 0})
    RC4_MD5 = type("RC4_MD5", (StreamCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 16})
    ChaCha20 = type("ChaCha20", (StreamCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 8})
    ChaCha20_IETF = type("ChaCha20_IETF", (StreamCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 12})
    ChaCha20_IETF_POLY1305 = type(
        "ChaCha20_IETF_POLY1305", (AEADCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 32, "NONCE_LENGTH": 12, "TAG_LENGTH": 16}
    )

    AES_256_GCM = type("AES_256_GCM", (AEADCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 32, "NONCE_LENGTH": 12, "TAG_LENGTH": 16})
    AES_192_GCM = type("AES_192_GCM", (AEADCipher,), {"KEY_LENGTH": 24, "IV_LENGTH": 24, "NONCE_LENGTH": 12, "TAG_LENGTH": 16})
    AES_128_GCM = type("AES_128_GCM", (AEADCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 16, "NONCE_LENGTH": 12, "TAG_LENGTH": 16})

    AES_256_CFB = type("AES_256_CFB", (StreamCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 16})
    AES_192_CFB = type("AES_192_CFB", (StreamCipher,), {"KEY_LENGTH": 24, "IV_LENGTH": 16})
    AES_128_CFB = type("AES_128_CFB", (StreamCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 16})
    AES_256_CFB8 = type("AES_256_CFB8", (StreamCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 16})
    AES_192_CFB8 = type("AES_192_CFB8", (StreamCipher,), {"KEY_LENGTH": 24, "IV_LENGTH": 16})
    AES_128_CFB8 = type("AES_128_CFB8", (StreamCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 16})

    AES_256_OFB = type("AES_256_OFB", (StreamCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 16})
    AES_192_OFB = type("AES_192_OFB", (StreamCipher,), {"KEY_LENGTH": 24, "IV_LENGTH": 16})
    AES_128_OFB = type("AES_128_OFB", (StreamCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 16})

    AES_256_CTR = type("AES_256_CTR", (StreamCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 16})
    AES_192_CTR = type("AES_192_CTR", (StreamCipher,), {"KEY_LENGTH": 24, "IV_LENGTH": 16})
    AES_128_CTR = type("AES_128_CTR", (StreamCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 16})

    Salsa20 = type("Salsa20", (BaseCipher,), {"KEY_LENGTH": 32, "IV_LENGTH": 8})
    BF_CFB = type("BF_CFB", (BaseCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 8})
    CAST5_CFB = type("CAST5_CFB", (BaseCipher,), {"KEY_LENGTH": 16, "IV_LENGTH": 8})
    DES_CFB = type("DES_CFB", (BaseCipher,), {"KEY_LENGTH": 8, "IV_LENGTH": 8})

    def _evp_bytes_to_key(password: str | bytes, key_len: int, iv_len: int) -> tuple[bytes, bytes]:
        import hashlib as _hashlib

        data = password.encode("utf-8") if isinstance(password, str) else password
        d = b""
        while len(d) < key_len + iv_len:
            md5 = _hashlib.md5(d[-16:] + data).digest() if d else _hashlib.md5(data).digest()
            d += md5
        return d[:key_len], d[key_len : key_len + iv_len]

    def get_cipher(cipher_key: str) -> tuple[str | None, None]:
        return ("eggress.cipher is unavailable", None)

    MAP: dict[str, type[BaseCipher]] = {
        "aes-256-gcm": AES_256_GCM,
        "aes-192-gcm": AES_192_GCM,
        "aes-128-gcm": AES_128_GCM,
        "chacha20-ietf-poly1305": ChaCha20_IETF_POLY1305,
        "rc4": RC4,
        "rc4-md5": RC4_MD5,
        "chacha20": ChaCha20,
        "chacha20-ietf": ChaCha20_IETF,
        "salsa20": Salsa20,
        "aes-256-cfb": AES_256_CFB,
        "aes-192-cfb": AES_192_CFB,
        "aes-128-cfb": AES_128_CFB,
        "aes-256-cfb8": AES_256_CFB8,
        "aes-192-cfb8": AES_192_CFB8,
        "aes-128-cfb8": AES_128_CFB8,
        "aes-256-ofb": AES_256_OFB,
        "aes-192-ofb": AES_192_OFB,
        "aes-128-ofb": AES_128_OFB,
        "aes-256-ctr": AES_256_CTR,
        "aes-192-ctr": AES_192_CTR,
        "aes-128-ctr": AES_128_CTR,
        "bf-cfb": BF_CFB,
        "cast5-cfb": CAST5_CFB,
        "des-cfb": DES_CFB,
        # pproxy 2.7.9 aliases: -py variants
        "aes-256-gcm-py": AES_256_GCM,
        "aes-128-gcm-py": AES_128_GCM,
        "chacha20-ietf-poly1305-py": ChaCha20_IETF_POLY1305,
        "rc4-md5-py": RC4_MD5,
        "chacha20-py": ChaCha20,
        "salsa20-py": Salsa20,
        "aes-256-cfb-py": AES_256_CFB,
        "aes-128-cfb-py": AES_128_CFB,
        "aes-256-cfb8-py": AES_256_CFB8,
        "aes-128-cfb8-py": AES_128_CFB8,
        "aes-256-ofb-py": AES_256_OFB,
        "aes-128-ofb-py": AES_128_OFB,
        "aes-256-ctr-py": AES_256_CTR,
        "aes-128-ctr-py": AES_128_CTR,
        "bf-cfb-py": BF_CFB,
    }


__all__ = [
    "BaseCipher",
    "StreamCipher",
    "AEADCipher",
    "PacketCipher",
    "RC4",
    "RC4_MD5",
    "ChaCha20",
    "ChaCha20_IETF",
    "ChaCha20_IETF_POLY1305",
    "AES_256_GCM",
    "AES_192_GCM",
    "AES_128_GCM",
    "AES_256_CFB",
    "AES_192_CFB",
    "AES_128_CFB",
    "AES_256_CFB8",
    "AES_192_CFB8",
    "AES_128_CFB8",
    "AES_256_OFB",
    "AES_192_OFB",
    "AES_128_OFB",
    "AES_256_CTR",
    "AES_192_CTR",
    "AES_128_CTR",
    "Salsa20",
    "BF_CFB",
    "CAST5_CFB",
    "DES_CFB",
    "MAP",
    "get_cipher",
    "_evp_bytes_to_key",
]
