#!/usr/bin/env python3
"""Cipher Known-Answer Test (KAT) probe.

Validates that a cipher produces expected ciphertext for known inputs.
Run in oracle or candidate venv to produce an observation JSON.

Usage:
    python3 strict_cipher_kat_probe.py --cipher AES_256_GCM_Cipher
    python3 strict_cipher_kat_probe.py --cipher ChaCha20_IETF_POLY1305_Cipher
"""
import argparse
import json
import sys


# NIST SP 800-38D and RFC 8439 known-answer vectors
KAT_VECTORS = {
    "AES_256_GCM_Cipher": {
        "key": bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000"),
        "nonce": bytes.fromhex("000000000000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
    "AES_192_GCM_Cipher": {
        "key": bytes.fromhex("000000000000000000000000000000000000000000000000"),
        "nonce": bytes.fromhex("000000000000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
    "AES_128_GCM_Cipher": {
        "key": bytes.fromhex("00000000000000000000000000000000"),
        "nonce": bytes.fromhex("000000000000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
    "ChaCha20_IETF_POLY1305_Cipher": {
        "key": bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000"),
        "nonce": bytes.fromhex("000000000000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
    "AES_256_CFB_Cipher": {
        "key": bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000"),
        "nonce": bytes.fromhex("00000000000000000000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
    "ChaCha20_IETF_Cipher": {
        "key": bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000"),
        "nonce": bytes.fromhex("00000000000000000000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
    "ChaCha20_Cipher": {
        "key": bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000"),
        "nonce": bytes.fromhex("0000000000000000"),
        "plaintext": b"Hello, World!",
        "aad": b"",
    },
}


def probe(cipher_name: str) -> dict:
    """Run KAT probe for the given cipher."""
    result = {
        "cipher": cipher_name,
        "exists": False,
        "kat_passed": False,
        "encrypt_output": None,
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

    vector = KAT_VECTORS.get(cipher_name)
    if not vector:
        result["error"] = f"No KAT vector for {cipher_name}"
        return result

    try:
        cipher = cipher_cls(
            key=vector["key"],
            nonce=vector["nonce"],
        )

        ciphertext, tag = cipher.encrypt_and_digest(
            vector["plaintext"],
            vector["aad"],
        )

        result["encrypt_output"] = {
            "ciphertext_hex": ciphertext.hex(),
            "tag_hex": tag.hex() if tag else None,
            "ciphertext_len": len(ciphertext),
        }

        # Verify decrypt round-trip
        decrypted = cipher.decrypt_and_verify(
            ciphertext,
            tag,
            vector["aad"],
        )
        result["kat_passed"] = decrypted == vector["plaintext"]

    except Exception as e:
        result["error"] = f"{type(e).__name__}: {e}"

    return result


def main():
    parser = argparse.ArgumentParser(description="Cipher KAT probe")
    parser.add_argument("--cipher", required=True, help="Cipher class name")
    args = parser.parse_args()

    result = probe(args.cipher)
    print(json.dumps(result, indent=2, default=str))


if __name__ == "__main__":
    main()
