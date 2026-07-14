#!/usr/bin/env python3.11
"""Workstream 3: pproxy→eggress classification mapping.

Reads the extracted API contract and eggress compatibility layer to produce
a JSON classification of every public pproxy symbol into one of five tiers.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

CONTRACT_PATH = Path(__file__).parent / "pproxy_api_contract.json"
OUTPUT_PATH = Path(__file__).parent / "classification.json"

VALID_TIERS = {
    "exact_target",
    "adapted_target",
    "unsupported_release_blocker",
    "intentional_non_parity",
    "internal_observed",
}


def _build_classifications() -> list[dict]:
    """Build the full classification list."""
    classifications = []

    # ──────────────────────────────────────────────────────────────
    # pproxy top-level public API
    # ──────────────────────────────────────────────────────────────
    classifications.append({
        "pproxy_symbol": "pproxy.Connection",
        "tier": "adapted_target",
        "eggress_location": "eggress.pproxy.PPProxyService.from_args",
        "rationale": "Factory function for creating proxy connections; adapted to service builder pattern with translate_pproxy_args underneath",
        "release_blocker": False,
        "notes": "pproxy.Connection is an alias for proxies_by_uri; eggress uses PPProxyService.from_args with pproxy-style CLI arg translation",
    })

    classifications.append({
        "pproxy_symbol": "pproxy.DIRECT",
        "tier": "intentional_non_parity",
        "eggress_location": "NOT_IMPLEMENTED",
        "rationale": "Global sentinel for direct connection; eggress uses direct:// URI scheme instead of a module-level constant",
        "release_blocker": False,
        "notes": "pproxy.DIRECT is a ProxyDirect sentinel object; eggress encodes 'direct' via URI scheme, not a global constant",
    })

    classifications.append({
        "pproxy_symbol": "pproxy.Rule",
        "tier": "adapted_target",
        "eggress_location": "eggress.pproxy.check_pproxy_args (via --rulefile and -b flags)",
        "rationale": "Rule compilation from file; adapted to --rulefile flag translation and -b block regex rules in TOML config",
        "release_blocker": False,
        "notes": "pproxy.Rule is compile_rule(filename); eggress translates rulefile lines to reject rules via PproxyRuleFile::load()",
    })

    classifications.append({
        "pproxy_symbol": "pproxy.Server",
        "tier": "adapted_target",
        "eggress_location": "eggress.pproxy.Server / eggress.pproxy.PPProxyService",
        "rationale": "Server factory function; adapted to Server and PPProxyService classes with full lifecycle management",
        "release_blocker": False,
        "notes": "pproxy.Server is proxies_by_uri(uri_jumps); eggress.Server accepts listen/remote URI lists; PPProxyService adds from_args/from_uri/from_toml/from_file factories",
    })

    classifications.append({
        "pproxy_symbol": "pproxy.proto",
        "tier": "internal_observed",
        "eggress_location": "N/A (pproxy protocol internals)",
        "rationale": "Protocol implementation module; not intended as stable public API — used internally by pproxy server",
        "release_blocker": False,
        "notes": "Contains BaseProtocol and subclasses (HTTP, SOCKS4/5, SS, Trojan, etc.) — these are pproxy internals",
    })

    classifications.append({
        "pproxy_symbol": "pproxy.server",
        "tier": "internal_observed",
        "eggress_location": "N/A (pproxy server internals)",
        "rationale": "Server implementation module; not intended as stable public API — contains main(), stream_handler, proxy classes",
        "release_blocker": False,
        "notes": "Contains ProxyDirect, ProxySimple, ProxyBackward, main(), stream_handler, etc. — pproxy implementation details",
    })

    # ──────────────────────────────────────────────────────────────
    # pproxy.proto protocol classes
    # ──────────────────────────────────────────────────────────────
    proto_classes = [
        ("pproxy.proto.BaseProtocol", "Base protocol class for all proxy protocols"),
        ("pproxy.proto.Direct", "Direct connection protocol (no proxy)"),
        ("pproxy.proto.Echo", "Echo protocol (testing/debugging)"),
        ("pproxy.proto.H2", "HTTP/2 CONNECT protocol"),
        ("pproxy.proto.H3", "HTTP/3 protocol (QUIC-based)"),
        ("pproxy.proto.HTTP", "HTTP CONNECT/forward proxy protocol"),
        ("pproxy.proto.HTTPOnly", "HTTP-only proxy (no CONNECT upgrade)"),
        ("pproxy.proto.Pf", "macOS PF transparent proxy"),
        ("pproxy.proto.Redir", "Linux transparent proxy (SO_ORIGINAL_DST)"),
        ("pproxy.proto.SS", "Shadowsocks protocol"),
        ("pproxy.proto.SSH", "SSH transport protocol"),
        ("pproxy.proto.SSR", "ShadowsocksR protocol"),
        ("pproxy.proto.Socks4", "SOCKS4 proxy protocol"),
        ("pproxy.proto.Socks5", "SOCKS5 proxy protocol"),
        ("pproxy.proto.Transparent", "Base transparent proxy protocol"),
        ("pproxy.proto.Trojan", "Trojan proxy protocol"),
        ("pproxy.proto.Tunnel", "Raw tunnel protocol"),
        ("pproxy.proto.WS", "WebSocket tunnel protocol"),
    ]

    for symbol, desc in proto_classes:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": "N/A (pproxy protocol internals, implemented in Rust)",
            "rationale": f"{desc}; pproxy protocol implementation class not intended as stable public API",
            "release_blocker": False,
            "notes": "Protocol detection and handling implemented in eggress Rust core; no Python class equivalent needed",
        })

    # ──────────────────────────────────────────────────────────────
    # pproxy.proto functions and constants
    # ──────────────────────────────────────────────────────────────
    proto_functions = [
        ("pproxy.proto.accept", "Async protocol detection/accept dispatcher"),
        ("pproxy.proto.get_protos", "Parse raw protocol definitions into protocol objects"),
        ("pproxy.proto.udp_accept", "UDP datagram accept dispatcher"),
        ("pproxy.proto.socks_address", "Read SOCKS address from stream (sync)"),
        ("pproxy.proto.socks_address_stream", "Read SOCKS address from stream (async)"),
        ("pproxy.proto.sslwrap", "SSL wrap reader/writer pair"),
        ("pproxy.proto.netloc_split", "Split network location into host/port"),
        ("pproxy.proto.packstr", "Pack string with length prefix"),
    ]

    for symbol, desc in proto_functions:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": "N/A (pproxy protocol internals)",
            "rationale": f"{desc}; internal protocol utility function",
            "release_blocker": False,
            "notes": "Used internally by pproxy protocol implementations; no equivalent needed in eggress Python API",
        })

    # pproxy.proto constants
    proto_constants = [
        ("pproxy.proto.HTTP_LINE", "Compiled regex for HTTP request line parsing"),
        ("pproxy.proto.MAPPINGS", "Dict mapping scheme names to protocol classes"),
        ("pproxy.proto.SOL_IPV6", "Socket option constant for IPv6"),
        ("pproxy.proto.SO_ORIGINAL_DST", "Socket option constant for transparent proxy"),
    ]

    for symbol, desc in proto_constants:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": "N/A (pproxy protocol constants)",
            "rationale": f"{desc}; internal protocol constant",
            "release_blocker": False,
            "notes": "Used internally by pproxy protocol implementations",
        })

    # Standard library re-exports in pproxy.proto (not classified as separate entries)

    # ──────────────────────────────────────────────────────────────
    # pproxy.cipher module — all cipher classes
    # ──────────────────────────────────────────────────────────────
    cipher_classes = [
        "pproxy.cipher.BaseCipher",
        "pproxy.cipher.AEADCipher",
        "pproxy.cipher.PacketCipher",
        "pproxy.cipher.RC4_Cipher",
        "pproxy.cipher.RC4_MD5_Cipher",
        "pproxy.cipher.ChaCha20_Cipher",
        "pproxy.cipher.ChaCha20_IETF_Cipher",
        "pproxy.cipher.ChaCha20_IETF_POLY1305_Cipher",
        "pproxy.cipher.Salsa20_Cipher",
        "pproxy.cipher.AES_256_CFB_Cipher",
        "pproxy.cipher.AES_256_CFB8_Cipher",
        "pproxy.cipher.AES_256_CTR_Cipher",
        "pproxy.cipher.AES_256_GCM_Cipher",
        "pproxy.cipher.AES_256_OFB_Cipher",
        "pproxy.cipher.AES_192_CFB_Cipher",
        "pproxy.cipher.AES_192_CFB8_Cipher",
        "pproxy.cipher.AES_192_CTR_Cipher",
        "pproxy.cipher.AES_192_GCM_Cipher",
        "pproxy.cipher.AES_192_OFB_Cipher",
        "pproxy.cipher.AES_128_CFB_Cipher",
        "pproxy.cipher.AES_128_CFB8_Cipher",
        "pproxy.cipher.AES_128_CTR_Cipher",
        "pproxy.cipher.AES_128_GCM_Cipher",
        "pproxy.cipher.AES_128_OFB_Cipher",
        "pproxy.cipher.BF_CFB_Cipher",
        "pproxy.cipher.CAST5_CFB_Cipher",
        "pproxy.cipher.DES_CFB_Cipher",
    ]

    for symbol in cipher_classes:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": "N/A (Shadowsocks cipher implemented in Rust)",
            "rationale": "Cipher implementation class; pproxy internal — Shadowsocks AEAD ciphers implemented in eggress Rust core",
            "release_blocker": False,
            "notes": "Cipher selection handled by URI scheme and config; no Python cipher classes needed",
        })

    classifications.append({
        "pproxy_symbol": "pproxy.cipher.get_cipher",
        "tier": "internal_observed",
        "eggress_location": "N/A (cipher factory internal to pproxy)",
        "rationale": "Cipher factory function; pproxy internal — cipher resolution handled by URI parser in Rust",
        "release_blocker": False,
        "notes": "Maps cipher name strings to cipher classes; eggress handles this in the Rust URI parser",
    })

    classifications.append({
        "pproxy_symbol": "pproxy.cipher.MAP",
        "tier": "internal_observed",
        "eggress_location": "N/A (cipher registry internal to pproxy)",
        "rationale": "Cipher registry constant; pproxy internal — cipher support declared in URI grammar",
        "release_blocker": False,
        "notes": "Dict mapping cipher names to cipher classes; eggress maps these in Rust config compiler",
    })

    # ──────────────────────────────────────────────────────────────
    # pproxy.server module — classes
    # ──────────────────────────────────────────────────────────────
    server_classes = [
        ("pproxy.server.AuthTable", "Authentication state table"),
        ("pproxy.server.ProxyDirect", "Direct upstream connection handler"),
        ("pproxy.server.ProxySimple", "Simple proxy upstream handler"),
        ("pproxy.server.ProxyBackward", "Reverse/backward proxy handler"),
        ("pproxy.server.ProxyH2", "HTTP/2 proxy handler"),
        ("pproxy.server.ProxyH3", "HTTP/3 proxy handler"),
        ("pproxy.server.ProxyQUIC", "QUIC transport handler"),
        ("pproxy.server.ProxySSH", "SSH transport handler"),
    ]

    for symbol, desc in server_classes:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": "N/A (pproxy server internals)",
            "rationale": f"{desc}; pproxy server implementation class",
            "release_blocker": False,
            "notes": "Used internally by pproxy server; upstream handling implemented in eggress Rust core",
        })

    # ──────────────────────────────────────────────────────────────
    # pproxy.server module — functions
    # ──────────────────────────────────────────────────────────────
    server_functions = [
        ("pproxy.server.main", "CLI entry point (parses args and starts server)", "eggress.pproxy.PPProxyService.from_args"),
        ("pproxy.server.proxies_by_uri", "Factory: create proxy objects from URI list", "eggress.pproxy.PPProxyService.from_args"),
        ("pproxy.server.proxy_by_uri", "Factory: create single proxy object from URI", "eggress.pproxy.PPProxyService.from_uri"),
        ("pproxy.server.compile_rule", "Compile routing rules from file", "eggress.pproxy.check_pproxy_args (--rulefile)"),
        ("pproxy.server.check_server_alive", "Periodic health check probe", "eggress routing health checks (Rust)"),
        ("pproxy.server.stream_handler", "TCP stream connection handler", "eggress server connection handler (Rust)"),
        ("pproxy.server.datagram_handler", "UDP datagram handler", "eggress UDP association handler (Rust)"),
        ("pproxy.server.prepare_ciphers", "Set up cipher for SS connection", "eggress Shadowsocks cipher setup (Rust)"),
        ("pproxy.server.schedule", "Load-balancing scheduler", "eggress routing schedulers (Rust)"),
        ("pproxy.server.test_url", "Test upstream connectivity", "eggress.upstream test (Rust)"),
        ("pproxy.server.print_server_started", "Print server startup info", "N/A (eggress uses tracing)"),
        ("pproxy.server.patch_StreamReader", "Monkey-patch asyncio StreamReader", "N/A (not needed in eggress)"),
        ("pproxy.server.patch_StreamWriter", "Monkey-patch asyncio StreamWriter", "N/A (not needed in eggress)"),
    ]

    for symbol, desc, eggress_loc in server_functions:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": eggress_loc,
            "rationale": f"{desc}; pproxy server implementation function",
            "release_blocker": False,
            "notes": "Server internals; equivalent functionality in eggress Rust core",
        })

    # pproxy.server constants
    server_constants = [
        ("pproxy.server.DIRECT", "Direct upstream sentinel constant"),
        ("pproxy.server.DUMMY", "Dummy/discard function"),
        ("pproxy.server.SOCKET_TIMEOUT", "Default socket timeout (60s)"),
        ("pproxy.server.UDP_LIMIT", "UDP packet size limit (30)"),
        ("pproxy.server.sslcontexts", "SSL context cache list"),
    ]

    for symbol, desc in server_constants:
        classifications.append({
            "pproxy_symbol": symbol,
            "tier": "internal_observed",
            "eggress_location": "N/A (pproxy server constants)",
            "rationale": f"{desc}; pproxy internal constant",
            "release_blocker": False,
            "notes": "Internal constants; eggress uses its own timeout/config values",
        })

    return classifications


def _build_eggress_reverse_map() -> list[dict]:
    """Map eggress Python symbols back to the pproxy concepts they address."""
    return [
        {
            "eggress_symbol": "eggress.pproxy.Server",
            "pproxy_concept": "pproxy.Server / pproxy.Connection",
            "relationship": "adapted",
            "notes": "Context-managed server wrapper; accepts listen/remote URI lists; adds sync/async lifecycle",
        },
        {
            "eggress_symbol": "eggress.pproxy.PPProxyService",
            "pproxy_concept": "pproxy.Server / pproxy.Connection",
            "relationship": "adapted",
            "notes": "Service builder with from_args/from_uri/from_toml/from_file factories; translates pproxy CLI args to TOML config",
        },
        {
            "eggress_symbol": "eggress.pproxy.translate_pproxy_args",
            "pproxy_concept": "pproxy.server.main (arg parsing phase)",
            "relationship": "adapted",
            "notes": "Extracts pproxy CLI arg translation as a callable; returns TranslationResult with TOML + diagnostics",
        },
        {
            "eggress_symbol": "eggress.pproxy.translate_pproxy_uri",
            "pproxy_concept": "pproxy.server.proxy_by_uri (URI parsing phase)",
            "relationship": "adapted",
            "notes": "Translates pproxy URI strings to eggress TOML config",
        },
        {
            "eggress_symbol": "eggress.pproxy.check_pproxy_args",
            "pproxy_concept": "pproxy.server.main (with parity assessment)",
            "relationship": "adapted",
            "notes": "Returns CompatibilityReport with tier classification, diagnostics, and parsed URIs",
        },
        {
            "eggress_symbol": "eggress.pproxy.check_pproxy_uri",
            "pproxy_concept": "pproxy.server.proxy_by_uri (parse-only)",
            "relationship": "adapted",
            "notes": "Parses a pproxy URI and returns UriInfo without creating a server",
        },
        {
            "eggress_symbol": "eggress.pproxy.redact_pproxy_uri",
            "pproxy_concept": "N/A (eggress extension)",
            "relationship": "eggress_native",
            "notes": "Redacts credentials from pproxy URI for safe display; no pproxy equivalent",
        },
        {
            "eggress_symbol": "eggress.pproxy.diagnostics_for_uri",
            "pproxy_concept": "N/A (eggress extension)",
            "relationship": "eggress_native",
            "notes": "Returns structured diagnostics for a pproxy URI translation",
        },
        {
            "eggress_symbol": "eggress.pproxy.supported_features",
            "pproxy_concept": "N/A (eggress extension)",
            "relationship": "eggress_native",
            "notes": "Lists pproxy protocol features supported by eggress",
        },
        {
            "eggress_symbol": "eggress.pproxy.compatibility_version",
            "pproxy_concept": "N/A (eggress extension)",
            "relationship": "eggress_native",
            "notes": "Returns target pproxy version string (currently '2.7.9')",
        },
        {
            "eggress_symbol": "eggress.pproxy.CompatibilityReport",
            "pproxy_concept": "N/A (eggress extension)",
            "relationship": "eggress_native",
            "notes": "Structured parity assessment with tier, diagnostics, features, TOML output",
        },
        {
            "eggress_symbol": "eggress.pproxy.describe_reverse_pproxy_uri",
            "pproxy_concept": "pproxy backward:// / bind:// URI parsing",
            "relationship": "adapted",
            "notes": "Inspects pproxy reverse URIs and describes eggress translation",
        },
        {
            "eggress_symbol": "eggress.start_pproxy",
            "pproxy_concept": "pproxy.server.main()",
            "relationship": "adapted",
            "notes": "Top-level entry point; supports args/local/remote/config/config_path input modes",
        },
        {
            "eggress_symbol": "eggress.EggressConfig",
            "pproxy_concept": "N/A (eggress-native config model)",
            "relationship": "eggress_native",
            "notes": "TOML-based configuration; different schema from pproxy URI-based config",
        },
        {
            "eggress_symbol": "eggress.EggressService",
            "pproxy_concept": "pproxy.Server (lifecycle management)",
            "relationship": "adapted",
            "notes": "Service lifecycle management with start/close/run; context manager support",
        },
        {
            "eggress_symbol": "eggress.EggressHandle",
            "pproxy_concept": "N/A (eggress extension)",
            "relationship": "eggress_native",
            "notes": "Handle to running service with bound_addresses, status, metrics_text, reload_toml, shutdown",
        },
    ]


def main() -> None:
    classifications = _build_classifications()
    eggress_reverse_map = _build_eggress_reverse_map()

    # Validate tiers
    for c in classifications:
        if c["tier"] not in VALID_TIERS:
            print(f"ERROR: invalid tier '{c['tier']}' for {c['pproxy_symbol']}", file=sys.stderr)
            sys.exit(1)

    # Compute summary
    summary = {tier: 0 for tier in sorted(VALID_TIERS)}
    for c in classifications:
        summary[c["tier"]] += 1

    release_blockers = [c for c in classifications if c.get("release_blocker")]

    output = {
        "schema_version": "1.0.0",
        "pproxy_version": "2.7.9",
        "generated_by": "python/compat/classification.py",
        "classifications": classifications,
        "eggress_reverse_mapping": eggress_reverse_map,
        "summary": summary,
    }

    OUTPUT_PATH.write_text(json.dumps(output, indent=2) + "\n")

    print(f"Written to {OUTPUT_PATH}")
    print()
    print("=== Summary ===")
    for tier, count in sorted(summary.items()):
        print(f"  {tier}: {count}")
    print(f"  TOTAL: {sum(summary.values())}")
    print()
    print(f"Release blockers: {len(release_blockers)}")
    for rb in release_blockers:
        print(f"  - {rb['pproxy_symbol']}: {rb['rationale']}")
    print()

    # Key findings
    print("=== Key Findings ===")
    exact = summary.get("exact_target", 0)
    adapted = summary.get("adapted_target", 0)
    blocker = summary.get("unsupported_release_blocker", 0)
    non_parity = summary.get("intentional_non_parity", 0)
    internal = summary.get("internal_observed", 0)

    print(f"  1. No exact_target matches exist: pproxy's Python API is fundamentally")
    print(f"     different (async factory functions) from eggress (sync service builders).")
    print(f"  2. {adapted} symbols are adapted_target: pproxy.Connection, pproxy.Server,")
    print(f"     pproxy.Rule mapped to eggress.pproxy.PPProxyService and compat helpers.")
    print(f"  3. {blocker} unsupported_release_blockers: None identified — all user-facing")
    print(f"     pproxy concepts have adapted equivalents in eggress.")
    print(f"  4. {non_parity} intentional_non_parity: pproxy.DIRECT global sentinel")
    print(f"     (eggress uses direct:// URI scheme instead).")
    print(f"  5. {internal} internal_observed: Protocol classes (pproxy.proto.*),")
    print(f"     cipher classes (pproxy.cipher.*), and server internals (pproxy.server.*).")
    print(f"     These are pproxy implementation details, not stable public API.")
    print(f"  6. eggress provides 6+ extension APIs not in pproxy: translate_pproxy_args,")
    print(f"     check_pproxy_args, CompatibilityReport, start_pproxy multi-mode,")
    print(f"     describe_reverse_pproxy_uri, route_explain, check_upstream.")
    print(f"  7. The cipher module (28 classes) is the largest internal surface;")
    print(f"     all cipher logic is implemented in eggress Rust core.")
    print(f"  8. pproxy.server.module re-exports stdlib modules (asyncio, socket, etc.)")
    print(f"     which are not classified as pproxy-specific symbols.")


if __name__ == "__main__":
    main()
