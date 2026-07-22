#!/usr/bin/env python3
"""Cipher roundtrip probe.

Validates that encrypt(decrypt(x)) == x for all supported ciphers.
Run in oracle or candidate venv to produce an observation JSON.

Usage:
    python3 strict_cipher_roundtrip_probe.py --cipher AES_256_GCM_Cipher
    python3 strict_cipher_roundtrip_probe.py --cipher ChaCha20_IETF_Cipher
"""
import argparse
import json
import os
import sys


ROUNDTRIP_VECTORS = {
    "AES_256_GCM_Cipher": {
        "key": os.urandom(32),
        "nonce": os.urandom(12),
        "plaintext": b"Roundtrip test data for AES-256-GCM",
        "aad": b"additional",
    },
    "AES_192_GCM_Cipher": {
        "key": os.urandom(24),
        "nonce": os.urandom(12),
        "plaintext": b"Roundtrip test data for AES-192-GCM",
        "aad": b"additional",
    },
    "AES_128_GCM_Cipher": {
        "key": os.urandom(16),
        "nonce": os.urandom(12),
        "plaintext": b"Roundtrip test data for AES-128-GCM",
        "aad": b"additional",
    },
    "ChaCha20_IETF_POLY1305_Cipher": {
        "key": os.urandom(32),
        "nonce": os.urandom(12),
        "plaintext": b"Roundtrip test data for ChaCha20-Poly1305",
        "aad": b"additional",
    },
    "AES_256_CFB_Cipher": {
        "key": os.urandom(32),
        "nonce": os.urandom(16),
        "plaintext": b"Roundtrip test data for AES-256-CFB",
        "aad": b"",
    },
    "ChaCha20_IETF_Cipher": {
        "key": os.urandom(32),
        "nonce": os.urandom(12),
        "plaintext": b"Roundtrip test data for ChaCha20-IETF",
        "aad": b"",
    },
    "ChaCha20_Cipher": {
        "key": os.urandom(32),
        "nonce": os.urandom(8),
        "plaintext": b"Roundtrip test data for ChaCha20",
        "aad": b"",
    },
    "AES_256_GCM_Cipher-static": {
        "key": bytes(32),
        "nonce": bytes(12),
        "plaintext": b"Static roundtrip test",
        "aad": b"",
    },
}


def probe(cipher_name: str) -> dict:
    """Run roundtrip probe for the given cipher."""
    result = {
        "cipher": cipher_name,
        "exists": False,
        "roundtrip_passed": False,
        "encrypt_error": None,
        "decrypt_error": None,
        "error": None,
    }

    try:
        from pproxy.cipher import MAP
    except ImportError as e:
        result["error"] = f"Import error: {e}"
        return result

    if cipher_name not in MAP:
        result["error"] = f"Cipher '{cipher_name}' not found in MAP"
        return result

    result["exists"] = True
    cipher_cls = MAP[cipher_name]

    vector = ROUNDTRIP_VECTORS.get(cipher_name)
    if not vector:
        # Use a generic vector
        try:
            key_len = cipher_cls.key_len if hasattr(cipher_cls, 'key_len') else 32
            nonce_len = cipher_cls.nonce_len if hasattr(cipher_cls, 'nonce_len') else 12
        except Exception:
            key_len, nonce_len = 32, 12
        vector = {
            "key": os.urandom(key_len),
            "nonce": os.urandom(nonce_len),
            "plaintext": b"Generic roundtrip test",
            "aad": b"",
        }

    try:
        cipher = cipher_cls(
            key=vector["key"],
            nonce=vector["nonce"],
        )

        ciphertext, tag = cipher.encrypt_and_digest(
            vector["plaintext"],
            vector["aad"],
        )

        result["ciphertext_len"] = len(ciphertext)

    except Exception as e:
        result["encrypt_error"] = f"{type(e).__name__}: {e}"
        return result

    try:
        decrypted = cipher.decrypt_and_verify(
            ciphertext,
            tag,
            vector["aad"],
        )

        result["roundtrip_passed"] = decrypted == vector["plaintext"]
        result["plaintext_len"] = len(vector["plaintext"])

    except Exception as e:
        result["decrypt_error"] = f"{type(e).__name__}: {e}"

    return result


def main():
    parser = argparse.ArgumentParser(description="Cipher roundtrip probe")
    parser.add_argument("--cipher", required=True, help="Cipher class name")
    args = parser.parse_args()

    result = probe(args.cipher)
    print(json.dumps(result, indent=2, default=str))


if __name__ == "__main__":
    main()
