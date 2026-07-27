#!/usr/bin/env python3
"""Probe pproxy 2.7.9 oracle chain topology and wire event order.

Runs against the real pproxy==2.7.9 installation (Python 3.12).
Records object topology and wire events for single-hop and two-hop URIs.

Usage:
    python3.12 scripts/probe_pproxy_chain_topology.py
"""

import asyncio
import hashlib
import json
import os
import socket
import sys
import time
from pathlib import Path

# ---------------------------------------------------------------------------
# Oracle imports (must be real pproxy==2.7.9)
# ---------------------------------------------------------------------------

try:
    import pproxy
    from pproxy import server as pproxy_server
except ImportError:
    print("ERROR: pproxy not importable; requires pproxy==2.7.9", file=sys.stderr)
    sys.exit(1)

ORACLE_VERSION = getattr(pproxy, "__version__", "unknown")
ORACLE_FILE = getattr(pproxy, "__file__", "unknown")
INTERPRETER = sys.executable

# ---------------------------------------------------------------------------
# Scripted proxy fixtures
# ---------------------------------------------------------------------------


class ScriptedSOCKS5Proxy:
    """SOCKS5 proxy that records events and optionally relays to a target."""

    def __init__(self, relay: bool = False):
        self.server = None
        self.host = "127.0.0.1"
        self.port = 0
        self.events: list[dict] = []
        self.relay = relay

    async def start(self):
        self.server = await asyncio.start_server(
            self._handle, self.host, 0, reuse_port=True
        )
        sock = self.server.sockets[0]
        self.port = sock.getsockname()[1]

    async def stop(self):
        if self.server:
            self.server.close()
            await self.server.wait_closed()

    async def _handle(self, reader, writer):
        try:
            # Greeting
            header = await reader.readexactly(2)
            nmethods = header[1]
            await reader.readexactly(nmethods)
            # No-auth response
            writer.write(b"\x05\x00")
            await writer.drain()
            # Connect request
            req = await reader.readexactly(4)
            atyp = req[3]
            if atyp == 0x01:  # IPv4
                addr_raw = await reader.readexactly(4)
                addr = socket.inet_ntoa(addr_raw)
            elif atyp == 0x03:  # Domain
                dom_len = (await reader.readexactly(1))[0]
                addr = (await reader.readexactly(dom_len)).decode()
            elif atyp == 0x04:  # IPv6
                addr_raw = await reader.readexactly(16)
                addr = socket.inet_ntop(socket.AF_INET6, addr_raw)
            else:
                writer.close()
                return
            port_bytes = await reader.readexactly(2)
            port = int.from_bytes(port_bytes, "big")
            self.events.append({"host": addr, "port": port})
            if self.relay:
                # Actually connect to the target
                try:
                    remote_reader, remote_writer = await asyncio.wait_for(
                        asyncio.open_connection(addr, port), timeout=3
                    )
                except Exception:
                    writer.write(b"\x05\x05\x00\x01" + b"\x00" * 6)
                    await writer.drain()
                    return
                # Success reply
                writer.write(b"\x05\x00\x00\x01" + socket.inet_aton("0.0.0.0") + b"\x00\x00")
                await writer.drain()
                # Relay data bidirectionally
                await _relay(reader, writer, remote_reader, remote_writer)
            else:
                # Success reply only (no relay)
                writer.write(b"\x05\x00\x00\x01" + socket.inet_aton("0.0.0.0") + b"\x00\x00")
                await writer.drain()
                while True:
                    data = await reader.read(4096)
                    if not data:
                        break
                    writer.write(data)
                    await writer.drain()
        except (asyncio.IncompleteReadError, ConnectionResetError, BrokenPipeError):
            pass
        finally:
            try:
                writer.close()
                await writer.wait_closed()
            except Exception:
                pass


class ScriptedHTTPProxy:
    """HTTP CONNECT proxy that records events and optionally relays to a target."""

    def __init__(self, relay: bool = False):
        self.server = None
        self.host = "127.0.0.1"
        self.port = 0
        self.events: list[dict] = []
        self.relay = relay

    async def start(self):
        self.server = await asyncio.start_server(
            self._handle, self.host, 0, reuse_port=True
        )
        sock = self.server.sockets[0]
        self.port = sock.getsockname()[1]

    async def stop(self):
        if self.server:
            self.server.close()
            await self.server.wait_closed()

    async def _handle(self, reader, writer):
        try:
            line = await reader.readline()
            parts = line.decode().strip().split()
            if len(parts) < 3:
                writer.close()
                return
            method, target, _ = parts[0], parts[1], parts[2]
            if method.upper() == "CONNECT":
                host, _, port = target.partition(":")
                port = int(port) if port else 443
                self.events.append({"host": host, "port": port})
                while True:
                    hdr = await reader.readline()
                    if hdr in (b"\r\n", b"\n", b""):
                        break
                if self.relay:
                    try:
                        remote_reader, remote_writer = await asyncio.wait_for(
                            asyncio.open_connection(host, port), timeout=3
                        )
                    except Exception:
                        writer.write(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
                        await writer.drain()
                        writer.close()
                        return
                    writer.write(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    await writer.drain()
                    await _relay(reader, writer, remote_reader, remote_writer)
                else:
                    writer.write(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    await writer.drain()
                    while True:
                        data = await reader.read(4096)
                        if not data:
                            break
                        writer.write(data)
                        await writer.drain()
            else:
                writer.write(b"HTTP/1.1 501 Not Implemented\r\n\r\n")
                await writer.drain()
                writer.close()
        except (asyncio.IncompleteReadError, ConnectionResetError, BrokenPipeError):
            pass
        finally:
            try:
                writer.close()
                await writer.wait_closed()
            except Exception:
                pass


async def _relay(reader_a, writer_a, reader_b, writer_b):
    """Bidirectional relay between two pairs."""
    async def _pipe(r, w):
        try:
            while True:
                data = await r.read(4096)
                if not data:
                    break
                w.write(data)
                await w.drain()
        except (asyncio.IncompleteReadError, ConnectionResetError, BrokenPipeError):
            pass
        finally:
            try:
                w.close()
            except Exception:
                pass

    task_a = asyncio.ensure_future(_pipe(reader_a, writer_b))
    task_b = asyncio.ensure_future(_pipe(reader_b, writer_a))
    await asyncio.gather(task_a, task_b, return_exceptions=True)


# ---------------------------------------------------------------------------
# Topology probe
# ---------------------------------------------------------------------------

URIS = [
    "http://127.0.0.1:18080",
    "socks5://127.0.0.1:11080",
    "socks5://127.0.0.1:11080__http://127.0.0.1:18080",
    "http://127.0.0.1:18080__socks5://127.0.0.1:11080",
]


def probe_topology(uri: str) -> dict:
    """Probe the oracle chain object topology for a URI."""
    proxy = pproxy.server.proxies_by_uri(uri)
    obs: dict = {
        "uri": uri,
        "type": type(proxy).__name__,
        "host_name": getattr(proxy, "host_name", None),
        "port": getattr(proxy, "port", None),
        "direct": proxy.direct,
        "bind": getattr(proxy, "bind", None),
        "protos": [p.name for p in getattr(proxy, "protos", [])],
    }
    jump = getattr(proxy, "jump", None)
    if jump is not None:
        obs["jump"] = {
            "type": type(jump).__name__,
            "host_name": getattr(jump, "host_name", None),
            "port": getattr(jump, "port", None),
            "direct": jump.direct,
            "protos": [p.name for p in getattr(jump, "protos", [])] if hasattr(jump, "protos") else [],
        }
        jump_jump = getattr(jump, "jump", None)
        if jump_jump is not None:
            obs["jump"]["jump"] = {
                "type": type(jump_jump).__name__,
                "direct": jump_jump.direct,
            }
    # destination() behavior
    test_host, test_port = "example.com", 443
    obs["destination"] = proxy.destination(test_host, test_port)
    if jump is not None:
        obs["jump_destination"] = jump.destination(test_host, test_port)
        jump_jump = getattr(jump, "jump", None)
        if jump_jump is not None:
            obs["jump_jump_destination"] = jump_jump.destination(test_host, test_port)
    return obs


# ---------------------------------------------------------------------------
# Wire event probe
# ---------------------------------------------------------------------------


async def probe_wire_events(
    uri_template: str,
    outer_proxy,
    inner_proxy,
    outer_port: int,
    inner_port: int,
    outer_type: str,
    inner_type: str,
) -> dict:
    """Probe wire events for a URI using scripted proxy fixtures."""
    # Replace placeholder ports: 11080 → SOCKS5 port, 18080 → HTTP port
    socks5_port = outer_port if outer_type == "SOCKS5" else inner_port
    http_port = outer_port if outer_type == "HTTP" else inner_port
    actual_uri = uri_template.replace("11080", str(socks5_port)).replace("18080", str(http_port))

    proxy = pproxy.server.proxies_by_uri(actual_uri)
    error_info = None
    try:
        reader, writer = await asyncio.wait_for(
            proxy.tcp_connect("chain-target.invalid", 443),
            timeout=8,
        )
        writer.write(b"hello")
        await writer.drain()
        try:
            await asyncio.wait_for(reader.read(4096), timeout=2)
        except (asyncio.TimeoutError, Exception):
            pass
        writer.close()
        await writer.wait_closed()
    except Exception as e:
        import traceback
        error_info = {"error": str(e), "error_type": type(e).__name__, "traceback": traceback.format_exc()}

    events: list[dict] = []
    for i, ev in enumerate(outer_proxy.events):
        events.append({"sequence": i + 1, "proxy": "outer", **ev})
    offset = len(outer_proxy.events)
    for i, ev in enumerate(inner_proxy.events):
        events.append({"sequence": offset + i + 1, "proxy": "inner", **ev})
    result = {
        "actual_uri": actual_uri,
        "events": events,
        "outer_event_count": len(outer_proxy.events),
        "inner_event_count": len(inner_proxy.events),
    }
    if error_info:
        result.update(error_info)
    return result


async def run_wire_probes() -> list[dict]:
    """Run wire event probes for two-hop URIs.

    The outer proxy uses relay mode (it connects to the inner proxy).
    The inner proxy uses non-relay mode (accepts handshake, echoes data)
    to avoid timeout on unreachable targets.
    """
    results = []
    two_hop_uris = [
        ("socks5://127.0.0.1:11080__http://127.0.0.1:18080", "SOCKS5", "HTTP"),
        ("http://127.0.0.1:18080__socks5://127.0.0.1:11080", "HTTP", "SOCKS5"),
    ]
    for uri, outer_type, inner_type in two_hop_uris:
        # Outer proxy relays to inner; inner accepts handshake only (no relay to target)
        outer = (
            ScriptedSOCKS5Proxy(relay=True) if outer_type == "SOCKS5"
            else ScriptedHTTPProxy(relay=True)
        )
        inner = (
            ScriptedSOCKS5Proxy(relay=False) if inner_type == "SOCKS5"
            else ScriptedHTTPProxy(relay=False)
        )
        await outer.start()
        await inner.start()
        try:
            wire = await probe_wire_events(uri, outer, inner, outer.port, inner.port, outer_type, inner_type)
        finally:
            await outer.stop()
            await inner.stop()
        results.append({"uri": uri, "outer_type": outer_type, "inner_type": inner_type, **wire})
    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


async def main():
    print(f"Oracle: pproxy=={ORACLE_VERSION}")
    print(f"File: {ORACLE_FILE}")
    print(f"Interpreter: {INTERPRETER}")
    print()

    # Hash the oracle artifact
    try:
        oracle_hash = hashlib.sha256(Path(ORACLE_FILE).read_bytes()).hexdigest()
    except Exception:
        oracle_hash = "unavailable"

    # Topology observations
    topology_obs = []
    for uri in URIS:
        obs = probe_topology(uri)
        topology_obs.append(obs)
        print(f"URI: {uri}")
        print(f"  type={obs['type']} host={obs['host_name']}:{obs['port']} direct={obs['direct']}")
        print(f"  protos={obs['protos']}")
        if "jump" in obs:
            j = obs["jump"]
            print(f"  jump: type={j['type']} host={j['host_name']}:{j['port']} direct={j['direct']} protos={j['protos']}")
            if "jump" in j:
                jj = j["jump"]
                print(f"  jump.jump: type={jj['type']} direct={jj['direct']}")
        print(f"  destination('example.com', 443) = {obs['destination']}")
        if "jump_destination" in obs:
            print(f"  jump.destination('example.com', 443) = {obs['jump_destination']}")
        if "jump_jump_destination" in obs:
            print(f"  jump.jump.destination('example.com', 443) = {obs['jump_jump_destination']}")
        print()

    # Wire event observations
    print("Running wire event probes...")
    wire_obs = await run_wire_probes()
    for w in wire_obs:
        print(f"URI: {w['uri']}")
        if "error" in w:
            print(f"  ERROR: {w['error']}")
        else:
            print(f"  events: {w['events']}")
        print()

    # Build final observation
    observation = {
        "oracle_version": ORACLE_VERSION,
        "oracle_file": ORACLE_FILE,
        "oracle_hash": oracle_hash,
        "interpreter": INTERPRETER,
        "timestamp": time.time(),
        "topology": topology_obs,
        "wire_events": wire_obs,
    }

    # Write observation
    out_dir = Path("compat/pproxy-2.7.9/observations")
    out_dir.mkdir(parents=True, exist_ok=True)
    out_file = out_dir / "chain_topology.json"
    out_file.write_text(json.dumps(observation, indent=2))
    print(f"Observation written to {out_file}")


if __name__ == "__main__":
    asyncio.run(main())
