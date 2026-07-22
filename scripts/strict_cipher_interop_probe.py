#!/usr/bin/env python3
"""Bidirectional cipher interop probe.

Tests that oracle-encrypted data can be decrypted by candidate and vice versa.
This is the strongest form of cipher evidence — proves wire compatibility.

Usage:
    python3 strict_cipher_interop_probe.py --cipher AES_256_GCM_Cipher --peer-venv /path/to/other/venv
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile


def get_peer_python(peer_venv: str) -> str:
    """Get the python executable from a peer venv."""
    return os.path.join(peer_venv, "bin", "python")


def run_peer_probe(peer_python: str, cipher_name: str, key_hex: str, nonce_hex: str, plaintext_hex: str) -> dict:
    """Run an encrypt probe in the peer venv and return the ciphertext."""
    probe_code = f"""
import json
import sys
sys.path.insert(0, "{os.path.dirname(os.path.abspath(__file__))}")

from pproxy.cipher import MAP

cipher_name = "{cipher_name}"
key = bytes.fromhex("{key_hex}")
nonce = bytes.fromhex("{nonce_hex}")
plaintext = bytes.fromhex("{plaintext_hex}")

if cipher_name not in MAP:
    print(json.dumps({{"error": f"Cipher {{cipher_name}} not found in MAP"}}))
    sys.exit(1)

cipher = MAP[cipher_name](key=key, nonce=nonce)
ciphertext, tag = cipher.encrypt_and_digest(plaintext, b"")
result = {{
    "ciphertext_hex": ciphertext.hex(),
    "tag_hex": tag.hex() if tag else None,
}}
print(json.dumps(result))
"""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
        f.write(probe_code)
        f.flush()
        try:
            result = subprocess.run(
                [peer_python, f.name],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0 and result.stdout.strip():
                return json.loads(result.stdout)
            else:
                return {"error": f"Peer probe failed: {result.stderr.strip() or 'no output'}"}
        except subprocess.TimeoutExpired:
            return {"error": "Peer probe timed out"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON from peer: {e}"}
        finally:
            os.unlink(f.name)


def run_peer_decrypt(peer_python: str, cipher_name: str, key_hex: str, nonce_hex: str, ciphertext_hex: str, tag_hex: str) -> dict:
    """Run a decrypt probe in the peer venv and return the plaintext."""
    probe_code = f"""
import json
import sys

from pproxy.cipher import MAP

cipher_name = "{cipher_name}"
key = bytes.fromhex("{key_hex}")
nonce = bytes.fromhex("{nonce_hex}")
ciphertext = bytes.fromhex("{ciphertext_hex}")
tag = bytes.fromhex("{tag_hex}") if "{tag_hex}" else None

if cipher_name not in MAP:
    print(json.dumps({{"error": f"Cipher {{cipher_name}} not found in MAP"}}))
    sys.exit(1)

cipher = MAP[cipher_name](key=key, nonce=nonce)
try:
    plaintext = cipher.decrypt_and_verify(ciphertext, tag, b"")
    result = {{"plaintext_hex": plaintext.hex(), "success": True}}
except Exception as e:
    result = {{"error": f"{{type(e).__name__}}: {{e}}", "success": False}}
print(json.dumps(result))
"""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
        f.write(probe_code)
        f.flush()
        try:
            result = subprocess.run(
                [peer_python, f.name],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0 and result.stdout.strip():
                return json.loads(result.stdout)
            else:
                return {"error": f"Peer decrypt failed: {result.stderr.strip() or 'no output'}"}
        except subprocess.TimeoutExpired:
            return {"error": "Peer decrypt timed out"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON from peer decrypt: {e}"}
        finally:
            os.unlink(f.name)


def probe(cipher_name: str, peer_venv: str, direction: str = "both") -> dict:
    """Run bidirectional interop probe."""
    result = {
        "cipher": cipher_name,
        "exists": False,
        "oracle_to_candidate": None,
        "candidate_to_oracle": None,
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

    # Generate consistent test vectors
    try:
        key_len = cipher_cls.key_len if hasattr(cipher_cls, 'key_len') else 32
        nonce_len = cipher_cls.nonce_len if hasattr(cipher_cls, 'nonce_len') else 12
    except Exception:
        key_len, nonce_len = 32, 12

    key = os.urandom(key_len)
    nonce = os.urandom(nonce_len)
    plaintext = f"Bidirectional interop test for {cipher_name}".encode()

    # Test 1: This venv encrypts, peer decrypts
    if direction in ("both", "this_to_peer"):
        try:
            cipher = cipher_cls(key=key, nonce=nonce)
            ciphertext, tag = cipher.encrypt_and_digest(plaintext, b"")

            peer_result = run_peer_decrypt(
                get_peer_python(peer_venv),
                cipher_name,
                key.hex(),
                nonce.hex(),
                ciphertext.hex(),
                tag.hex() if tag else "",
            )

            if peer_result.get("error"):
                result["oracle_to_candidate"] = {"passed": False, "error": peer_result["error"]}
            else:
                decrypted = bytes.fromhex(peer_result["plaintext_hex"])
                result["oracle_to_candidate"] = {
                    "passed": decrypted == plaintext,
                    "plaintext_match": decrypted == plaintext,
                }
        except Exception as e:
            result["oracle_to_candidate"] = {"passed": False, "error": f"{type(e).__name__}: {e}"}

    # Test 2: Peer encrypts, this venv decrypts
    if direction in ("both", "peer_to_this"):
        try:
            peer_enc_result = run_peer_probe(
                get_peer_python(peer_venv),
                cipher_name,
                key.hex(),
                nonce.hex(),
                plaintext.hex(),
            )

            if peer_enc_result.get("error"):
                result["candidate_to_oracle"] = {"passed": False, "error": peer_enc_result["error"]}
            else:
                ciphertext = bytes.fromhex(peer_enc_result["ciphertext_hex"])
                tag = bytes.fromhex(peer_enc_result["tag_hex"]) if peer_enc_result.get("tag_hex") else None

                cipher = cipher_cls(key=key, nonce=nonce)
                decrypted = cipher.decrypt_and_verify(ciphertext, tag, b"")
                result["candidate_to_oracle"] = {
                    "passed": decrypted == plaintext,
                    "plaintext_match": decrypted == plaintext,
                }
        except Exception as e:
            result["candidate_to_oracle"] = {"passed": False, "error": f"{type(e).__name__}: {e}"}

    return result


def main():
    parser = argparse.ArgumentParser(description="Bidirectional cipher interop probe")
    parser.add_argument("--cipher", required=True, help="Cipher class name")
    parser.add_argument("--peer-venv", required=True, help="Path to peer venv")
    parser.add_argument("--direction", default="both", choices=["both", "this_to_peer", "peer_to_this"])
    args = parser.parse_args()

    result = probe(args.cipher, args.peer_venv, args.direction)
    print(json.dumps(result, indent=2, default=str))


if __name__ == "__main__":
    main()
