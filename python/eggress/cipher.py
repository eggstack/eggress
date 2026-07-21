from __future__ import annotations

import hashlib
import os
from typing import Any, Callable, Optional, Tuple

try:
    from eggress._eggress import UnsupportedFeatureError
except ImportError:
    UnsupportedFeatureError = RuntimeError  # type: ignore[misc,assignment]

try:
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305

    _HAS_CRYPTOGRAPHY = True
except BaseException:
    _HAS_CRYPTOGRAPHY = False


def _rc4_crypt(key: bytes, data: bytes) -> bytes:
    """RC4 stream cipher (RFC 6229-compliant KSA + PRGA).

    Pure-Python implementation for strict compatibility mode.
    """
    S = list(range(256))
    j = 0
    for i in range(256):
        j = (j + S[i] + key[i % len(key)]) & 0xFF
        S[i], S[j] = S[j], S[i]

    out = bytearray(len(data))
    i = j = 0
    for k in range(len(data)):
        i = (i + 1) & 0xFF
        j = (j + S[i]) & 0xFF
        S[i], S[j] = S[j], S[i]
        out[k] = data[k] ^ S[(S[i] + S[j]) & 0xFF]
    return bytes(out)


def _increment_nonce(nonce: bytes) -> bytes:
    """Increment a nonce by 1 (little-endian in first 8 bytes, matching Rust backend)."""
    buf = bytearray(nonce)
    for i in range(min(8, len(buf))):
        val = buf[i] + 1
        buf[i] = val & 0xFF
        if val < 256:
            return bytes(buf)
    raise ValueError("nonce increment overflow")


def _evp_bytes_to_key(password: str | bytes, key_len: int, iv_len: int) -> Tuple[bytes, bytes]:
    """Derive key and IV from password using OpenSSL's EVP_BytesToKey with MD5."""
    data = password.encode("utf-8") if isinstance(password, str) else password
    d = b""
    while len(d) < key_len + iv_len:
        # D1 = MD5(data), D_i = MD5(D_(i-1)[-16:] || data)
        md5 = hashlib.md5(d[-16:] + data).digest() if d else hashlib.md5(data).digest()
        d += md5
    return d[:key_len], d[key_len : key_len + iv_len]


class BaseCipher:
    """Base class for all ciphers in the pproxy-compatible hierarchy.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. All encryption and decryption is handled by the
    Rust backend via protocol-level AEAD.
    """

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


class StreamCipher(BaseCipher):
    """Base class for stream ciphers with incremental encrypt/decrypt.

    Provides functional encrypt/decrypt using Python's ``cryptography``
    library when available, or pure-Python fallbacks for legacy modes
    like RC4.  Falls back to ``UnsupportedFeatureError`` if neither
    backend is available.
    """

    BLOCK_SIZE: int = 1

    def __init__(
        self,
        key: bytes | str,
        ota: bool = False,
        setup_key: bool = True,
    ) -> None:
        super().__init__(key, ota, setup_key)
        self._encryptor: Any = None
        self._decryptor: Any = None
        self._setup_state()

    def _setup_state(self) -> None:
        """Initialize cipher state.  Subclasses override to set up encryptor/decryptor."""
        pass

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        """Create an encryptor.  Override in subclasses."""
        raise NotImplementedError

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        """Create a decryptor.  Override in subclasses."""
        raise NotImplementedError

    def encrypt(self, s: bytes) -> bytes:
        """Encrypt *s* using the current cipher state."""
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"encrypt: install the 'cryptography' package for {self.name()}"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError(
                f"encrypt: no encryptor available for {self.name()}"
            )
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        """Decrypt *s* using the current cipher state."""
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"decrypt: install the 'cryptography' package for {self.name()}"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError(
                f"decrypt: no decryptor available for {self.name()}"
            )
        return self._decryptor.update(s)

    def __copy__(self) -> "StreamCipher":
        cls = self.__class__
        new = cls.__new__(cls)
        new.__dict__.update(self.__dict__)
        new._setup_state()
        return new


class AEADCipher(BaseCipher):
    """Base class for AEAD ciphers with authenticated encryption.

    Provides encrypt/decrypt using Python's ``cryptography`` library when
    available.  Falls back to ``UnsupportedFeatureError`` if the library
    is not installed.
    """

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
        self._current_nonce = self._nonce

    @property
    def nonce(self) -> bytes:
        return self._current_nonce

    def setup_nonce(self, nonce: Optional[bytes] = None) -> None:
        if nonce is not None:
            self._current_nonce = nonce
        else:
            self._current_nonce = os.urandom(self.NONCE_LENGTH)
        self._nonce = self._current_nonce

    def setup_iv(self, iv: Optional[bytes] = None) -> None:
        if iv is None:
            iv = os.urandom(self.NONCE_LENGTH)
        nonce = iv[: self.NONCE_LENGTH] if len(iv) > self.NONCE_LENGTH else iv
        super().setup_iv(nonce)
        self.setup_nonce(nonce)

    def __copy__(self) -> "AEADCipher":
        cls = self.__class__
        new = cls.__new__(cls)
        new.__dict__.update(self.__dict__)
        return new

    def _make_cipher(self) -> Any:
        """Create the underlying AEAD cipher primitive.  Override in subclasses."""
        raise NotImplementedError

    def encrypt(self, s: bytes) -> bytes:
        """Encrypt *s* using the current nonce.

        Returns ``nonce ‖ ciphertext`` where *ciphertext* includes the
        authentication tag (standard AEAD layout).  The nonce is
        incremented after each call.
        """
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"encrypt: install the 'cryptography' package for {self.name()}"
            )
        cipher = self._make_cipher()
        nonce = self._current_nonce
        ct = cipher.encrypt(nonce, s, None)
        self._current_nonce = _increment_nonce(nonce)
        self._nonce = self._current_nonce
        return nonce + ct

    def decrypt(self, s: bytes) -> bytes:
        """Decrypt *s* (``nonce ‖ ciphertext``) using the embedded nonce."""
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"decrypt: install the 'cryptography' package for {self.name()}"
            )
        if len(s) < self.NONCE_LENGTH:
            raise ValueError("ciphertext too short")
        cipher = self._make_cipher()
        nonce = s[: self.NONCE_LENGTH]
        ct = s[self.NONCE_LENGTH :]
        return cipher.decrypt(nonce, ct, None)

    def encrypt_chunk(self, chunk: bytes) -> bytes:
        return self.encrypt(chunk)

    def decrypt_chunk(self, chunk: bytes) -> bytes:
        return self.decrypt(chunk)

    def encrypt_and_digest(self, plaintext: bytes) -> tuple[bytes, bytes]:
        """Encrypt and return ``(ciphertext, tag)`` as separate values.

        This matches the pproxy API.  Internally the tag is appended to
        the ciphertext by the AEAD primitive and split for the return.
        """
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"encrypt_and_digest: install the 'cryptography' package for {self.name()}"
            )
        cipher = self._make_cipher()
        nonce = self._current_nonce
        ct = cipher.encrypt(nonce, plaintext, None)
        self._current_nonce = _increment_nonce(nonce)
        self._nonce = self._current_nonce
        tag = ct[-self.TAG_LENGTH :]
        raw_ct = ct[: -self.TAG_LENGTH]
        return raw_ct, tag

    def decrypt_and_verify(self, ciphertext: bytes, tag: bytes) -> bytes:
        """Verify *tag* and decrypt *ciphertext* (pproxy API)."""
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"decrypt_and_verify: install the 'cryptography' package for {self.name()}"
            )
        cipher = self._make_cipher()
        nonce = self._current_nonce
        ct_with_tag = ciphertext + tag
        return cipher.decrypt(nonce, ct_with_tag, None)

    def __repr__(self) -> str:
        return (
            f"<{type(self).__name__} name={self.name()!r} "
            f"NONCE_LENGTH={self.NONCE_LENGTH} TAG_LENGTH={self.TAG_LENGTH}>"
        )


class PacketCipher:
    """Wraps a cipher for UDP datagram use, adding nonce/tag framing.

    For AEAD ciphers, encrypt/decrypt operate on individual UDP packets
    with the standard AEAD framing (nonce + ciphertext + tag).
    """

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
        """Encrypt a UDP packet.  Uses the underlying AEAD cipher's encrypt."""
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"PacketCipher.encrypt: install the 'cryptography' package for {self._name}"
            )
        if isinstance(self._cipher, AEADCipher):
            return self._cipher.encrypt(data)
        raise UnsupportedFeatureError(
            f"PacketCipher.encrypt: only AEAD ciphers are supported for UDP, got {self._name}"
        )

    def decrypt(self, data: bytes) -> bytes:
        """Decrypt a UDP packet.  Uses the underlying AEAD cipher's decrypt."""
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                f"PacketCipher.decrypt: install the 'cryptography' package for {self._name}"
            )
        if isinstance(self._cipher, AEADCipher):
            return self._cipher.decrypt(data)
        raise UnsupportedFeatureError(
            f"PacketCipher.decrypt: only AEAD ciphers are supported for UDP, got {self._name}"
        )

    def __repr__(self) -> str:
        return f"<PacketCipher name={self._name!r}>"


class AES_256_GCM_Cipher(AEADCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 32
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF

    def _make_cipher(self) -> Any:
        return AESGCM(self._key)


class AES_192_GCM_Cipher(AEADCipher):
    KEY_LENGTH = 24
    IV_LENGTH = 24
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF

    def _make_cipher(self) -> Any:
        return AESGCM(self._key)


class AES_128_GCM_Cipher(AEADCipher):
    KEY_LENGTH = 16
    IV_LENGTH = 16
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF

    def _make_cipher(self) -> Any:
        return AESGCM(self._key)


class ChaCha20_IETF_POLY1305_Cipher(AEADCipher):
    KEY_LENGTH = 32
    IV_LENGTH = 32
    NONCE_LENGTH = 12
    TAG_LENGTH = 16
    PACKET_LIMIT = 0x3FFF

    def _make_cipher(self) -> Any:
        return ChaCha20Poly1305(self._key)


class RC4_Cipher(StreamCipher):
    """RC4 stream cipher.

    Uses a pure-Python RC4 implementation for strict pproxy 2.7.9
    compatibility.  RC4 is insecure and should not be used for new
    configurations.
    """

    KEY_LENGTH = 16
    IV_LENGTH = 0

    def _setup_state(self) -> None:
        self._rc4_state = None
        self._rc4_pos = 0
        self._rc4_j = 0

    def _rc4_init(self, key: bytes) -> None:
        """Initialize RC4 state from key."""
        S = list(range(256))
        j = 0
        for i in range(256):
            j = (j + S[i] + key[i % len(key)]) & 0xFF
            S[i], S[j] = S[j], S[i]
        self._rc4_state = S
        self._rc4_pos = 0
        self._rc4_j = 0

    def _rc4_process(self, data: bytes) -> bytes:
        """Process data through RC4 stream cipher."""
        if self._rc4_state is None:
            self._rc4_init(self._key)
        S = self._rc4_state
        i = self._rc4_pos
        j = self._rc4_j
        out = bytearray(len(data))
        for k in range(len(data)):
            i = (i + 1) & 0xFF
            j = (j + S[i]) & 0xFF
            S[i], S[j] = S[j], S[i]
            out[k] = data[k] ^ S[(S[i] + S[j]) & 0xFF]
        self._rc4_pos = i
        self._rc4_j = j
        return bytes(out)

    def encrypt(self, s: bytes) -> bytes:
        return self._rc4_process(s)

    def decrypt(self, s: bytes) -> bytes:
        return self._rc4_process(s)

    def __copy__(self) -> "RC4_Cipher":
        new = RC4_Cipher(self._key, ota=self.ota, setup_key=False)
        new._rc4_init(self._key)
        return new


class RC4_MD5_Cipher(StreamCipher):
    """RC4-MD5 stream cipher.

    RC4 with the first 16 bytes of the key stream XORed with MD5(key + IV).
    Used by some legacy Shadowsocks implementations.
    """

    KEY_LENGTH = 16
    IV_LENGTH = 16

    def _setup_state(self) -> None:
        self._rc4_state = None
        self._rc4_pos = 0
        self._rc4_j = 0

    def _rc4_init(self, key: bytes, iv: bytes) -> None:
        """Initialize RC4-MD5 state."""
        derived = hashlib.md5(key + iv).digest()
        S = list(range(256))
        j = 0
        for i in range(256):
            j = (j + S[i] + derived[i % len(derived)]) & 0xFF
            S[i], S[j] = S[j], S[i]
        self._rc4_state = S
        self._rc4_pos = 0
        self._rc4_j = 0

    def _rc4_process(self, data: bytes) -> bytes:
        """Process data through RC4-MD5 stream cipher."""
        if self._rc4_state is None:
            iv = self._iv if self._iv else b"\x00" * 16
            self._rc4_init(self._key, iv)
        S = self._rc4_state
        i = self._rc4_pos
        j = self._rc4_j
        out = bytearray(len(data))
        for k in range(len(data)):
            i = (i + 1) & 0xFF
            j = (j + S[i]) & 0xFF
            S[i], S[j] = S[j], S[i]
            out[k] = data[k] ^ S[(S[i] + S[j]) & 0xFF]
        self._rc4_pos = i
        self._rc4_j = j
        return bytes(out)

    def encrypt(self, s: bytes) -> bytes:
        return self._rc4_process(s)

    def decrypt(self, s: bytes) -> bytes:
        return self._rc4_process(s)

    def __copy__(self) -> "RC4_MD5_Cipher":
        new = RC4_MD5_Cipher(self._key, ota=self.ota, setup_key=False)
        iv = self._iv if self._iv else b"\x00" * 16
        new._rc4_init(self._key, iv)
        return new


class ChaCha20_Cipher(StreamCipher):
    """ChaCha20 stream cipher (non-IETF, 8-byte IV).

    Uses the ``cryptography`` library's ChaCha20 implementation.
    The 8-byte IV is zero-padded to 16 bytes for the backend.
    """

    KEY_LENGTH = 32
    IV_LENGTH = 8

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        # ChaCha20 backend requires 16-byte nonce; pad 8-byte IV
        padded = iv.ljust(16, b"\x00")
        cipher = Cipher(algorithms.ChaCha20(key, padded), mode=None)
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        padded = iv.ljust(16, b"\x00")
        cipher = Cipher(algorithms.ChaCha20(key, padded), mode=None)
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 8
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for ChaCha20"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: ChaCha20 unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for ChaCha20"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: ChaCha20 unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "ChaCha20_Cipher":
        new = ChaCha20_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class ChaCha20_IETF_Cipher(StreamCipher):
    """ChaCha20-IETF stream cipher (12-byte nonce).

    Uses the ``cryptography`` library's ChaCha20 implementation with
    the IETF 12-byte nonce variant.
    """

    KEY_LENGTH = 32
    IV_LENGTH = 12

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        # ChaCha20-IETF uses 12-byte nonce; pad to 16 bytes for backend
        padded = iv.ljust(16, b"\x00")
        cipher = Cipher(algorithms.ChaCha20(key, padded), mode=None)
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        padded = iv.ljust(16, b"\x00")
        cipher = Cipher(algorithms.ChaCha20(key, padded), mode=None)
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 12
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for ChaCha20-IETF"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: ChaCha20-IETF unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for ChaCha20-IETF"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: ChaCha20-IETF unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "ChaCha20_IETF_Cipher":
        new = ChaCha20_IETF_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class Salsa20_Cipher(BaseCipher):
    """Salsa20 stream cipher -- unsupported (no ``cryptography`` backend).

    Salsa20 is not provided by the ``cryptography`` library and is
    intentionally deferred to Milestone D or later.  Use ChaCha20 or
    an AEAD method instead.
    """

    KEY_LENGTH = 32
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "Salsa20 is not supported (no cryptography backend); "
            "use chacha20, chacha20-ietf-poly1305, or an AEAD method"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "Salsa20 is not supported (no cryptography backend); "
            "use chacha20, chacha20-ietf-poly1305, or an AEAD method"
        )


class AES_256_CFB_Cipher(StreamCipher):
    """AES-256-CFB stream cipher (CFB = Cipher Feedback, full-block).

    Uses the ``cryptography`` library's AES-CFB implementation.
    """

    KEY_LENGTH = 32
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-256-CFB"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-256-CFB unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-256-CFB"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-256-CFB unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_256_CFB_Cipher":
        new = AES_256_CFB_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_192_CFB_Cipher(StreamCipher):
    """AES-192-CFB stream cipher."""

    KEY_LENGTH = 24
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-192-CFB"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-192-CFB unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-192-CFB"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-192-CFB unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_192_CFB_Cipher":
        new = AES_192_CFB_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_128_CFB_Cipher(StreamCipher):
    """AES-128-CFB stream cipher."""

    KEY_LENGTH = 16
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-128-CFB"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-128-CFB unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-128-CFB"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-128-CFB unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_128_CFB_Cipher":
        new = AES_128_CFB_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_256_CFB8_Cipher(StreamCipher):
    """AES-256-CFB8 stream cipher (8-bit cipher feedback)."""

    KEY_LENGTH = 32
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB8(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB8(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-256-CFB8"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-256-CFB8 unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-256-CFB8"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-256-CFB8 unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_256_CFB8_Cipher":
        new = AES_256_CFB8_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_192_CFB8_Cipher(StreamCipher):
    """AES-192-CFB8 stream cipher (8-bit cipher feedback)."""

    KEY_LENGTH = 24
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB8(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB8(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-192-CFB8"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-192-CFB8 unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-192-CFB8"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-192-CFB8 unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_192_CFB8_Cipher":
        new = AES_192_CFB8_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_128_CFB8_Cipher(StreamCipher):
    """AES-128-CFB8 stream cipher (8-bit cipher feedback)."""

    KEY_LENGTH = 16
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB8(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CFB8(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-128-CFB8"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-128-CFB8 unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-128-CFB8"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-128-CFB8 unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_128_CFB8_Cipher":
        new = AES_128_CFB8_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_256_OFB_Cipher(StreamCipher):
    """AES-256-OFB stream cipher (Output Feedback mode)."""

    KEY_LENGTH = 32
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.OFB(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.OFB(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-256-OFB"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-256-OFB unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-256-OFB"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-256-OFB unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_256_OFB_Cipher":
        new = AES_256_OFB_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_192_OFB_Cipher(StreamCipher):
    """AES-192-OFB stream cipher (Output Feedback mode)."""

    KEY_LENGTH = 24
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.OFB(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.OFB(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-192-OFB"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-192-OFB unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-192-OFB"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-192-OFB unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_192_OFB_Cipher":
        new = AES_192_OFB_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_128_OFB_Cipher(StreamCipher):
    """AES-128-OFB stream cipher (Output Feedback mode)."""

    KEY_LENGTH = 16
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.OFB(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.OFB(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-128-OFB"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-128-OFB unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-128-OFB"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-128-OFB unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_128_OFB_Cipher":
        new = AES_128_OFB_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_256_CTR_Cipher(StreamCipher):
    """AES-256-CTR stream cipher (Counter mode)."""

    KEY_LENGTH = 32
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CTR(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CTR(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-256-CTR"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-256-CTR unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-256-CTR"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-256-CTR unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_256_CTR_Cipher":
        new = AES_256_CTR_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_192_CTR_Cipher(StreamCipher):
    """AES-192-CTR stream cipher (Counter mode)."""

    KEY_LENGTH = 24
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CTR(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CTR(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-192-CTR"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-192-CTR unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-192-CTR"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-192-CTR unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_192_CTR_Cipher":
        new = AES_192_CTR_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class AES_128_CTR_Cipher(StreamCipher):
    """AES-128-CTR stream cipher (Counter mode)."""

    KEY_LENGTH = 16
    IV_LENGTH = 16

    def _make_encryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CTR(iv))
        return cipher.encryptor()

    def _make_decryptor(self, key: bytes, iv: bytes) -> Any:
        if not _HAS_CRYPTOGRAPHY:
            return None
        cipher = Cipher(algorithms.AES(key), modes.CTR(iv))
        return cipher.decryptor()

    def _setup_state(self) -> None:
        if not _HAS_CRYPTOGRAPHY:
            self._encryptor = None
            self._decryptor = None
            return
        iv = self._iv if self._iv else b"\x00" * 16
        self._encryptor = self._make_encryptor(self._key, iv)
        self._decryptor = self._make_decryptor(self._key, iv)

    def encrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "encrypt: install the 'cryptography' package for AES-128-CTR"
            )
        if self._encryptor is None:
            self._setup_state()
        if self._encryptor is None:
            raise UnsupportedFeatureError("encrypt: AES-128-CTR unavailable")
        return self._encryptor.update(s)

    def decrypt(self, s: bytes) -> bytes:
        if not _HAS_CRYPTOGRAPHY:
            raise UnsupportedFeatureError(
                "decrypt: install the 'cryptography' package for AES-128-CTR"
            )
        if self._decryptor is None:
            self._setup_state()
        if self._decryptor is None:
            raise UnsupportedFeatureError("decrypt: AES-128-CTR unavailable")
        return self._decryptor.update(s)

    def __copy__(self) -> "AES_128_CTR_Cipher":
        new = AES_128_CTR_Cipher(self._key, ota=self.ota, setup_key=False)
        new._iv = self._iv
        new._setup_state()
        return new


class BF_CFB_Cipher(BaseCipher):
    """Blowfish-CFB stream cipher -- unsupported (no ``cryptography`` backend).

    Blowfish is not provided by the ``cryptography`` library and is
    intentionally deferred to Milestone D or later.
    """

    KEY_LENGTH = 16
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "Blowfish-CFB is not supported (no cryptography backend); "
            "use an AES or ChaCha20 method instead"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "Blowfish-CFB is not supported (no cryptography backend); "
            "use an AES or ChaCha20 method instead"
        )


class CAST5_CFB_Cipher(BaseCipher):
    """CAST5-CFB stream cipher -- unsupported (no ``cryptography`` backend).

    CAST5 is not provided by the ``cryptography`` library and is
    intentionally deferred to Milestone D or later.
    """

    KEY_LENGTH = 16
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "CAST5-CFB is not supported (no cryptography backend); "
            "use an AES or ChaCha20 method instead"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "CAST5-CFB is not supported (no cryptography backend); "
            "use an AES or ChaCha20 method instead"
        )


class DES_CFB_Cipher(BaseCipher):
    """DES-CFB stream cipher -- unsupported (no ``cryptography`` backend).

    DES is insecure and not provided by the ``cryptography`` library.
    Intentionally deferred to Milestone D or later.
    """

    KEY_LENGTH = 8
    IV_LENGTH = 8

    def encrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "DES-CFB is not supported (insecure, no cryptography backend); "
            "use an AES or ChaCha20 method instead"
        )

    def decrypt(self, s: bytes) -> bytes:
        raise UnsupportedFeatureError(
            "DES-CFB is not supported (insecure, no cryptography backend); "
            "use an AES or ChaCha20 method instead"
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
    # pproxy 2.7.9 aliases: -py variants point to the same implementation
    "aes-256-gcm-py": AES_256_GCM_Cipher,
    "aes-128-gcm-py": AES_128_GCM_Cipher,
    "chacha20-ietf-poly1305-py": ChaCha20_IETF_POLY1305_Cipher,
    "rc4-md5-py": RC4_MD5_Cipher,
    "chacha20-py": ChaCha20_Cipher,
    "salsa20-py": Salsa20_Cipher,
    "aes-256-cfb-py": AES_256_CFB_Cipher,
    "aes-128-cfb-py": AES_128_CFB_Cipher,
    "aes-256-cfb8-py": AES_256_CFB8_Cipher,
    "aes-128-cfb8-py": AES_128_CFB8_Cipher,
    "aes-256-ofb-py": AES_256_OFB_Cipher,
    "aes-128-ofb-py": AES_128_OFB_Cipher,
    "aes-256-ctr-py": AES_256_CTR_Cipher,
    "aes-128-ctr-py": AES_128_CTR_Cipher,
    "bf-cfb-py": BF_CFB_Cipher,
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
    "StreamCipher",
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
