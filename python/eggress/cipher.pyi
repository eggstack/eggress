"""Type stubs for eggress.cipher module."""

from __future__ import annotations

from typing import Any, Optional, Tuple

class BaseCipher:
    """Base class for all ciphers in the pproxy-compatible hierarchy.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. All encryption and decryption is handled by the
    Rust backend via protocol-level AEAD.
    """
    KEY_LENGTH: int
    IV_LENGTH: int
    PYTHON: bool
    def __init__(self, key: bytes | str, ota: bool = ..., setup_key: bool = ...) -> None: ...
    @property
    def key(self) -> bytes: ...
    @property
    def iv(self) -> Optional[bytes]: ...
    def setup_iv(self, iv: Optional[bytes] = ...) -> None: ...
    def encrypt(self, s: bytes) -> bytes: ...
    def decrypt(self, s: bytes) -> bytes: ...
    @classmethod
    def name(cls) -> str: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def __reduce__(self) -> tuple[type[BaseCipher], tuple[bytes]]: ...
    def __copy__(self) -> BaseCipher: ...
    def __deepcopy__(self, memo: dict[int, Any]) -> BaseCipher: ...

class AEADCipher(BaseCipher):
    """Base class for AEAD ciphers with authenticated encryption.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. All encryption and decryption is handled by the
    Rust backend via protocol-level AEAD.
    """
    NONCE_LENGTH: int
    TAG_LENGTH: int
    PACKET_LIMIT: int
    @property
    def nonce(self) -> bytes: ...
    def setup_nonce(self, nonce: Optional[bytes] = ...) -> None: ...
    def encrypt_chunk(self, data: bytes) -> bytes: ...
    def decrypt_chunk(self, data: bytes) -> bytes: ...
    def encrypt_and_digest(self, buffer: bytes) -> tuple[bytes, bytes]: ...
    def decrypt_and_verify(self, buffer: bytes, tag: bytes) -> bytes: ...

class AES_256_GCM_Cipher(AEADCipher): ...
class AES_192_GCM_Cipher(AEADCipher): ...
class AES_128_GCM_Cipher(AEADCipher): ...
class ChaCha20_IETF_POLY1305_Cipher(AEADCipher): ...

class RC4_Cipher(BaseCipher):
    """RC4 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. RC4 is rejected with UnsupportedFeatureError.
    """
class RC4_MD5_Cipher(BaseCipher):
    """RC4-MD5 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. RC4-MD5 is rejected with UnsupportedFeatureError.
    """
class ChaCha20_Cipher(BaseCipher):
    """ChaCha20 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. ChaCha20 is rejected with UnsupportedFeatureError.
    """
class ChaCha20_IETF_Cipher(BaseCipher):
    """ChaCha20-IETF stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. ChaCha20-IETF is rejected with UnsupportedFeatureError.
    """
class Salsa20_Cipher(BaseCipher):
    """Salsa20 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. Salsa20 is rejected with UnsupportedFeatureError.
    """
class AES_256_CFB_Cipher(BaseCipher):
    """AES-256-CFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-256-CFB is rejected with UnsupportedFeatureError.
    """
class AES_192_CFB_Cipher(BaseCipher):
    """AES-192-CFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-192-CFB is rejected with UnsupportedFeatureError.
    """
class AES_128_CFB_Cipher(BaseCipher):
    """AES-128-CFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-128-CFB is rejected with UnsupportedFeatureError.
    """
class AES_256_CFB8_Cipher(BaseCipher):
    """AES-256-CFB8 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-256-CFB8 is rejected with UnsupportedFeatureError.
    """
class AES_192_CFB8_Cipher(BaseCipher):
    """AES-192-CFB8 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-192-CFB8 is rejected with UnsupportedFeatureError.
    """
class AES_128_CFB8_Cipher(BaseCipher):
    """AES-128-CFB8 stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-128-CFB8 is rejected with UnsupportedFeatureError.
    """
class AES_256_OFB_Cipher(BaseCipher):
    """AES-256-OFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-256-OFB is rejected with UnsupportedFeatureError.
    """
class AES_192_OFB_Cipher(BaseCipher):
    """AES-192-OFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-192-OFB is rejected with UnsupportedFeatureError.
    """
class AES_128_OFB_Cipher(BaseCipher):
    """AES-128-OFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-128-OFB is rejected with UnsupportedFeatureError.
    """
class AES_256_CTR_Cipher(BaseCipher):
    """AES-256-CTR stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-256-CTR is rejected with UnsupportedFeatureError.
    """
class AES_192_CTR_Cipher(BaseCipher):
    """AES-192-CTR stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-192-CTR is rejected with UnsupportedFeatureError.
    """
class AES_128_CTR_Cipher(BaseCipher):
    """AES-128-CTR stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. AES-128-CTR is rejected with UnsupportedFeatureError.
    """
class BF_CFB_Cipher(BaseCipher):
    """Blowfish-CFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. Blowfish-CFB is rejected with UnsupportedFeatureError.
    """
class CAST5_CFB_Cipher(BaseCipher):
    """CAST5-CFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. CAST5-CFB is rejected with UnsupportedFeatureError.
    """
class DES_CFB_Cipher(BaseCipher):
    """DES-CFB stream cipher -- intentionally unsupported.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. DES-CFB is rejected with UnsupportedFeatureError.
    """

class PacketCipher:
    """Wraps a cipher for UDP datagram use, adding nonce/tag framing.

    Note: This class is construction-only. It does not implement functional
    encrypt/decrypt methods. All encryption and decryption is handled by the
    Rust backend via protocol-level AEAD.
    """
    @property
    def cipher(self) -> BaseCipher: ...
    @property
    def key(self) -> bytes: ...
    @property
    def name(self) -> str: ...
    def __init__(self, cipher: BaseCipher, key: bytes | str, name: str) -> None: ...
    def encrypt(self, data: bytes) -> bytes: ...
    def decrypt(self, data: bytes) -> bytes: ...

MAP: dict[str, type[BaseCipher]]

class _ApplyCipher:
    @property
    def cipher(self) -> BaseCipher: ...
    @property
    def key(self) -> bytes: ...
    @property
    def name(self) -> str: ...
    @property
    def ota(self) -> bool: ...
    @property
    def plugins(self) -> list[Any]: ...
    @property
    def datagram(self) -> Optional[PacketCipher]: ...
    def __call__(self, data: bytes) -> bytes: ...

def get_cipher(cipher_key: str) -> Tuple[Optional[str], Optional[_ApplyCipher]]: ...
