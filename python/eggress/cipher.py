from __future__ import annotations

import hashlib
import os
from typing import Any, Callable, Optional, Tuple

try:
    from eggress._eggress import UnsupportedFeatureError
except ImportError:
    UnsupportedFeatureError = RuntimeError  # type: ignore[misc,assignment]


def _evp_bytes_to_key(password: str | bytes, key_len: int, iv_len: int) -> Tuple[bytes, bytes]:
    """Derive key and IV from password using OpenSSL's EVP_BytesToKey with MD5."""
    data = password.encode("utf-8") if isinstance(password, str) else password
    d = b""
    while len(d) < key_len + iv_len:
        md5 = hashlib.md5(d[-16:] if d else b"").digest()
        d += md5
    return d[:key_len], d[key_len : key_len + iv_len]


class BaseCipher:
    """Base class for all ciphers in the pproxy-compatible hierarchy."""

    KEY_LENGTH: int = 0
    IV_LENGTH: int = 0
    PYTHON: bool = False

    def __init__(
        self,
        key: bytes | str,
        ota: bool = False,
        setup_key: bool = True,
    ) -> None:
        if isinstance(key, str):
            key = key.encode("utf-8")
        self._key = key
        self.ota = ota
        self._iv: Optional[bytes] = None
        if setup_key and self.IV_LENGTH > 0:
            self._iv = os.urandom(self.IV_LENGTH)
        elif setup_key:
            self._iv = b""

    @property
    def key(self) -> bytes:
        return self._key

    def setup_iv(self, iv: Optional[bytes] = None) -> None:
        if iv is not None:
            self._iv = iv
        elif self.IV_LENGTH > 0:
            self._iv = os.urandom(self.IV_LENGTH)
        else:
            self._iv = b""

    @property
    def iv(self) -> Optional[bytes]:
        return self._iv

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"encrypt: use protocol-level AEAD via Rust for {self.name()}"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"decrypt: use protocol-level AEAD via Rust for {self.name()}"
        )

    @classmethod
    def name(cls) -> str:
        n = cls.__name__
        if n.endswith("_Cipher"):
            n = n[: -len("_Cipher")]
        return n.replace("_", "-")

    def __repr__(self) -> str:
        return f"<{type(self).__name__} name={self.name()!r}>"

    def __str__(self) -> str:
        return self.name()

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, BaseCipher):
            return NotImplemented
        return self.name() == other.name() and self.key == other.key

    def __hash__(self) -> int:
        return hash((self.name(), self.key))

    def __reduce__(self):
        raise TypeError(
            "Cannot pickle cipher objects: key material would be exposed"
        )

    def __copy__(self):
        return self.__class__(self.key)

    def __deepcopy__(self, memo):
        return self.__class__(self.key)

    def __del__(self) -> None:
        # Best-effort zeroing of key material
        if hasattr(self, "_key") and isinstance(self._key, (bytes, bytearray)):
            if isinstance(self._key, bytearray):
                for i in range(len(self._key)):
                    self._key[i] = 0


class AEADCipher(BaseCipher):
    """Base class for AEAD ciphers with authenticated encryption."""

    NONCE_LENGTH: int = 12
    TAG_LENGTH: int = 16
    PACKET_LIMIT: int = 0x3FFF  # 16383 bytes

    def __init__(
        self,
        key: bytes | str,
        ota: bool = False,
        setup_key: bool = True,
    ) -> None:
        super().__init__(key, ota, setup_key)
        self._nonce = os.urandom(self.NONCE_LENGTH)

    @property
    def nonce(self) -> bytes:
        return self._nonce

    def setup_nonce(self, nonce: Optional[bytes] = None) -> None:
        if nonce is not None:
            self._nonce = nonce
        else:
            self._nonce = os.urandom(self.NONCE_LENGTH)

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"encrypt: use protocol-level AEAD via Rust for {self.name()}"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"decrypt: use protocol-level AEAD via Rust for {self.name()}"
        )

    def encrypt_chunk(self, chunk: bytes) -> bytes:
        return self.encrypt(chunk)

    def decrypt_chunk(self, chunk: bytes) -> bytes:
        return self.decrypt(chunk)

    def encrypt_and_digest(self, plaintext: bytes) -> tuple[bytes, bytes]:
        raise UnsupportedFeatureError(
            "AEAD encryption is handled by the Rust backend"
        )

    def decrypt_and_verify(self, ciphertext: bytes, tag: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "AEAD decryption is handled by the Rust backend"
        )

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} name={self.name()!r} "
            f"NONCE_LENGTH={self.NONCE_LENGTH} TAG_LENGTH={self.TAG_LENGTH}>"
        )


class PacketCipher:
    """Wraps a cipher for UDP datagram use, adding nonce/tag framing."""

    def __init__(self, cipher: BaseCipher, key: bytes | str, name: str) -> None:
        if isinstance(key, str):
            key = key.encode("utf-8")
        self._cipher = cipher
        self._key = key
        self._name = name

    @property
    def cipher(self) -> BaseCipher:
        return self._cipher

    @property
    def key(self) -> bytes:
        return self._key

    @property
    def name(self) -> str:
        return self._name

    def encrypt(self, data: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"PacketCipher.encrypt: use protocol-level AEAD via Rust for {self._name}"
        )

    def decrypt(self, data: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"PacketCipher.decrypt: use protocol-level AEAD via Rust for {self._name}"
        )

    def __repr__(self) -> str:
        return f"<PacketCipher name={self._name!r}>"


class AES_256_GCM_Cipher(AEADCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 32
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF


class AES_192_GCM_Cipher(AEADCipher):
    KEY_LENGTH = 24
    IV_LENGTH = 24
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF


class AES_128_GCM_Cipher(AEADCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF


class ChaCha20_IETF_POLY1305_Cipher(AEADCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 32
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF


class RC4_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 0

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"RC4 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"RC4 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class RC4_MD5_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"RC4-MD5 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"RC4-MD5 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class ChaCha20_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"ChaCha20 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"ChaCha20 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class ChaCha20_IETF_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 12

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"ChaCha20-IETF stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"ChaCha20-IETF stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class Salsa20_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"Salsa20 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"Salsa20 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_256_CFB_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_192_CFB_Cipher(BaseCipher):
    KEY_LENGTH = 24
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_128_CFB_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_256_CFB8_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-CFB8 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-CFB8 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_192_CFB8_Cipher(BaseCipher):
    KEY_LENGTH = 24
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-CFB8 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-CFB8 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_128_CFB8_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-CFB8 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-CFB8 stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_256_OFB_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-OFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-OFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_192_OFB_Cipher(BaseCipher):
    KEY_LENGTH = 24
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-OFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-OFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_128_OFB_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-OFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-OFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_256_CTR_Cipher(BaseCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-CTR stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-256-CTR stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_192_CTR_Cipher(BaseCipher):
    KEY_LENGTH = 24
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-CTR stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-192-CTR stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class AES_128_CTR_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-CTR stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"AES-128-CTR stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class BF_CFB_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"Blowfish-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"Blowfish-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class CAST5_CFB_Cipher(BaseCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"CAST5-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"CAST5-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


class DES_CFB_Cipher(BaseCipher):
    KEY_LENGTH = 8
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"DES-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            f"DES-CFB stream cipher is not supported (Track F); "
            f"use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )


MAP: dict[str, type[BaseCipher]] = {
    "aes-256-gcm": AES_256_GCM_Cipher,
    "aes-192-gcm": AES_192_GCM_Cipher,
    "aes-128-gcm": AES_128_GCM_Cipher,
    "chacha20-ietf-poly1305": ChaCha20_IETF_POLY1305_Cipher,
    "rc4": RC4_Cipher,
    "rc4-md5": RC4_MD5_Cipher,
    "chacha20": ChaCha20_Cipher,
    "chacha20-ietf": ChaCha20_IETF_Cipher,
    "salsa20": Salsa20_Cipher,
    "aes-256-cfb": AES_256_CFB_Cipher,
    "aes-192-cfb": AES_192_CFB_Cipher,
    "aes-128-cfb": AES_128_CFB_Cipher,
    "aes-256-cfb8": AES_256_CFB8_Cipher,
    "aes-192-cfb8": AES_192_CFB8_Cipher,
    "aes-128-cfb8": AES_128_CFB8_Cipher,
    "aes-256-ofb": AES_256_OFB_Cipher,
    "aes-192-ofb": AES_192_OFB_Cipher,
    "aes-128-ofb": AES_128_OFB_Cipher,
    "aes-256-ctr": AES_256_CTR_Cipher,
    "aes-192-ctr": AES_192_CTR_Cipher,
    "aes-128-ctr": AES_128_CTR_Cipher,
    "bf-cfb": BF_CFB_Cipher,
    "cast5-cfb": CAST5_CFB_Cipher,
    "des-cfb": DES_CFB_Cipher,
}


class _ApplyCipher:
    """Callable returned by get_cipher that carries cipher metadata."""

    __slots__ = ("cipher", "_key", "_name", "_ota", "_plugins", "_datagram")

    def __init__(
        self,
        cipher: BaseCipher,
        key: bytes,
        name: str,
        ota: bool = False,
        plugins: Optional[list] = None,
        datagram: Optional[PacketCipher] = None,
    ) -> None:
        self.cipher = cipher
        self._key = key
        self._name = name
        self._ota = ota
        self._plugins = plugins or []
        self._datagram = datagram

    @property
    def key(self) -> bytes:
        return self._key

    @property
    def name(self) -> str:
        return self._name

    @property
    def ota(self) -> bool:
        return self._ota

    @property
    def plugins(self) -> list:
        return self._plugins

    @property
    def datagram(self) -> Optional[PacketCipher]:
        return self._datagram

    def __call__(self, data: bytes) -> bytes:
        return data

    def __repr__(self) -> str:
        return f"<ApplyCipher name={self._name!r} ota={self._ota}>"


def get_cipher(
    cipher_key: str,
) -> Tuple[Optional[str], Optional[_ApplyCipher]]:
    """Parse a 'cipher_name:password[!ota]' string and return (error, apply_fn).

    On success returns (None, apply_cipher_fn).
    On failure returns (error_message, None).
    """
    if not cipher_key:
        return ("empty cipher specification", None)

    ota = False
    key_str = cipher_key
    if cipher_key.endswith("!ota"):
        ota = True
        key_str = cipher_key[: -len("!ota")]

    parts = key_str.split(":", 1)
    if len(parts) != 2:
        return (f"invalid cipher format '{cipher_key}': expected 'name:password'", None)

    cipher_name, password = parts
    cipher_name = cipher_name.strip().lower()
    password = password.strip()

    if not password:
        return (f"empty password for cipher '{cipher_name}'", None)

    cipher_cls = MAP.get(cipher_name)
    if cipher_cls is None:
        known = ", ".join(sorted(MAP.keys()))
        return (
            f"unknown cipher '{cipher_name}'; known ciphers: {known}",
            None,
        )

    key_material, iv_material = _evp_bytes_to_key(
        password, cipher_cls.KEY_LENGTH, cipher_cls.IV_LENGTH
    )

    try:
        cipher = cipher_cls(key_material, ota=ota, setup_key=False)
    except Exception as exc:
        return (f"failed to instantiate cipher '{cipher_name}': {exc}", None)

    if iv_material:
        cipher.setup_iv(iv_material)

    is_aead = issubclass(cipher_cls, AEADCipher)
    datagram: Optional[PacketCipher] = None
    if is_aead:
        datagram = PacketCipher(cipher, key_material, cipher_name)

    apply_fn = _ApplyCipher(
        cipher=cipher,
        key=key_material,
        name=cipher_name,
        ota=ota,
        datagram=datagram,
    )

    return (None, apply_fn)


__all__ = [
    "BaseCipher",
    "AEADCipher",
    "PacketCipher",
    "AES_256_GCM_Cipher",
    "AES_192_GCM_Cipher",
    "AES_128_GCM_Cipher",
    "ChaCha20_IETF_POLY1305_Cipher",
    "RC4_Cipher",
    "RC4_MD5_Cipher",
    "ChaCha20_Cipher",
    "ChaCha20_IETF_Cipher",
    "Salsa20_Cipher",
    "AES_256_CFB_Cipher",
    "AES_192_CFB_Cipher",
    "AES_128_CFB_Cipher",
    "AES_256_CFB8_Cipher",
    "AES_192_CFB8_Cipher",
    "AES_128_CFB8_Cipher",
    "AES_256_OFB_Cipher",
    "AES_192_OFB_Cipher",
    "AES_128_OFB_Cipher",
    "AES_256_CTR_Cipher",
    "AES_192_CTR_Cipher",
    "AES_128_CTR_Cipher",
    "BF_CFB_Cipher",
    "CAST5_CFB_Cipher",
    "DES_CFB_Cipher",
    "MAP",
    "get_cipher",
    "_ApplyCipher",
]
