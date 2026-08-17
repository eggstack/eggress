"""pproxy server/proxy factories backed by Eggress protocol adapters."""

from __future__ import annotations

import argparse
import asyncio
import random
import re
import signal
import sys
import threading
import urllib.parse

from eggress._pproxy_proxy import (
    AuthTable, DIRECT, ProxyBackward, ProxyDirect, ProxyH2, ProxyH3,
    ProxyQUIC, ProxySSH, ProxySimple,
)
from eggress.cipher import get_cipher
from pproxy.plugin import get_plugin
from eggress.protocol import BaseProtocol, accept, get_protos, netloc_split, udp_accept
from eggress._eggress import (
    init_pproxy_logging,
    pproxy_runtime_options,
    run_pproxy_test,
    validate_pproxy_args,
)

SOCKET_TIMEOUT = 60
UDP_LIMIT = 30


def DUMMY(value):
    """Identity helper matching pproxy 2.7.9's ``pproxy.server.DUMMY``."""
    return value


def compile_rule(filename):
    if filename.startswith("{") and filename.endswith("}"):
        return re.compile(filename[1:-1]).match
    with open(filename, encoding="utf-8") as handle:
        patterns = [
            line.strip()
            for line in handle
            if line.strip() and not line.startswith("#")
        ]
    if not patterns:
        return re.compile(r"(?!)").match
    return re.compile("(:?" + "|".join(patterns) + ")$").match


def schedule(rserver, salgorithm, host_name, port):
    candidates = [p for p in rserver if p.alive and p.match_rule(host_name, port)]
    if salgorithm == "fa":
        return candidates[0] if candidates else None
    if salgorithm == "rr":
        if not candidates:
            return None
        selected = candidates[0]
        rserver.remove(selected)
        rserver.append(selected)
        return selected
    if salgorithm == "rc":
        return random.choice(candidates) if candidates else None
    if salgorithm == "lc":
        return min(candidates, key=lambda p: p.connections, default=None)
    raise ValueError(f"Unknown scheduling algorithm: {salgorithm}")


def _proxy_by_uri(uri, jump):
    scheme, _, rest = uri.partition("://")
    if not _:
        raise argparse.ArgumentTypeError(f"invalid URI: {uri!r}")
    url = __import__("urllib.parse", fromlist=["urlparse"]).urlparse("s://" + rest)
    raw = [part.lower() for part in scheme.split("+")]
    err, protos = get_protos(raw)
    if err:
        raise argparse.ArgumentTypeError(err)
    path, _, _plugins = url.path.partition(",")
    plugin_names = [name for name in _plugins.split(",") if name] if _plugins else []
    path, _, lbind = path.partition("@")
    cipher, _, location = url.netloc.rpartition("@")
    host, port = (
        netloc_split(location, default_port=22 if "ssh" in raw else 8080)
        if location
        else (None, None)
    )
    auth = url.fragment.encode() if url.fragment else None
    users = [line for line in auth.split(b"\n")] if auth else None
    cipher_apply = None
    if cipher:
        error, cipher_apply = get_cipher(cipher)
        if error:
            raise argparse.ArgumentTypeError(error)
        for plugin_name in plugin_names:
            error, plugin = get_plugin(plugin_name)
            if error:
                raise argparse.ArgumentTypeError(error)
            cipher_apply.plugins.append(plugin)
    elif plugin_names:
        raise argparse.ArgumentTypeError(
            "pproxy plugins require a cipher/protocol context"
        )
    if "direct" in [p.name for p in protos]:
        return ProxyDirect(lbind=lbind or None)
    params = dict(
        jump=jump,
        protos=protos,
        cipher=cipher_apply,
        users=users,
        rule=url.query or None,
        bind=location or path,
        host_name=host,
        port=port,
        unix=not location,
        lbind=lbind or None,
    )
    if "h2" in raw:
        return ProxyH2(**params)
    if "ssh" in raw:
        return ProxySSH(**params)
    if "quic" in raw:
        return ProxyQUIC(**params)
    if "h3" in [p.name for p in protos]:
        return ProxyH3(**params)
    proxy = ProxySimple(**params)
    if "in" in raw:
        proxy = ProxyBackward(proxy, raw.count("in"), **params)
    return proxy


def proxy_by_uri(uri, jump=None):
    return _proxy_by_uri(uri, jump if jump is not None else DIRECT)


def proxies_by_uri(uri_jumps):
    jump = DIRECT
    for uri in reversed(uri_jumps.split("__")):
        jump = _proxy_by_uri(uri, jump)
    return jump


Connection = proxies_by_uri
"""pproxy-shaped URI factory.  Builds a proxy chain from ``__``-separated
URIs.  This is the upstream pproxy contract preserved for migration
compatibility; it is NOT the native ``eggress.pproxy.Server`` lifecycle
class."""

Server = proxies_by_uri
"""pproxy-shaped URI factory (alias for ``Connection``).
Preserved for upstream pproxy migration compatibility; it is NOT
the native ``eggress.pproxy.Server`` lifecycle class.  To manage a
Rust-backed service lifecycle, use ``eggress.pproxy.Server`` instead."""

Rule = compile_rule


async def check_server_alive(interval, rserver, verbose):
    """Probe configured proxy endpoints until the task is cancelled."""
    while True:
        await asyncio.sleep(interval)
        for remote in rserver:
            if isinstance(remote, ProxyDirect):
                continue
            try:
                _, writer = await remote.open_connection(
                    None, None, None, None, timeout=3
                )
            except asyncio.CancelledError:
                return
            except Exception:
                if getattr(remote, "_alive", True):
                    remote._alive = 0
                    verbose(
                        f"{getattr(getattr(remote, 'rproto', None), 'name', 'proxy')} "
                        f"{remote.bind} -> OFFLINE"
                    )
            else:
                remote._alive = 1
                verbose(
                    f"{getattr(getattr(remote, 'rproto', None), 'name', 'proxy')} "
                    f"{remote.bind} -> ONLINE"
                )
                writer.close()
                wait_closed = getattr(writer, "wait_closed", None)
                if wait_closed is not None:
                    await wait_closed()


async def prepare_ciphers(cipher, reader, writer, bind=None, server_side=True):
    if cipher is None:
        return None, None
    cipher.pdecrypt = cipher.pdecrypt2 = cipher.pencrypt = cipher.pencrypt2 = DUMMY
    for plugin in cipher.plugins:
        if server_side:
            await plugin.init_server_data(reader, writer, cipher, bind)
        else:
            await plugin.init_client_data(reader, writer, cipher)
        plugin.add_cipher(cipher)
    return cipher(
        reader,
        writer,
        cipher.pdecrypt,
        cipher.pdecrypt2,
        cipher.pencrypt,
        cipher.pencrypt2,
    )


async def datagram_handler(writer, data, addr, protos, urserver, block, cipher, salgorithm,
                           verbose=lambda *args: None, **kwargs):
    try:
        if cipher is not None and cipher.datagram is not None:
            data = cipher.datagram.decrypt(data)
        lproto, _user, host, port, payload = udp_accept(protos, data, **kwargs)
        if block is not None and block(host):
            raise ValueError(f"BLOCK {host}")
        roption = schedule(urserver, salgorithm, host, port) or DIRECT
        prepared = roption.udp_prepare_connection(host, port, payload)

        def reply(response):
            packed = lproto.udp_pack(host, port, response)
            if cipher is not None and cipher.datagram is not None:
                packed = cipher.datagram.encrypt(packed)
            writer.sendto(packed, addr)

        await roption.udp_open_connection(host, port, prepared, addr, reply)
    except Exception as exc:
        verbose(str(exc) or "Unsupported protocol")


async def stream_handler(reader, writer, unix, lbind, protos, rserver, cipher, sslserver,
                         debug=0, authtime=2592000, block=None, salgorithm="fa",
                         verbose=lambda *args: None,
                         modstat=lambda *args: (lambda *_: DUMMY), **kwargs):
    remote_writer = None
    try:
        reader_cipher, _ = await prepare_ciphers(
            cipher, reader, writer, server_side=False
        )
        lproto, user, host, port, _ = await accept(
            protos, reader=reader, writer=writer, reader_cipher=reader_cipher, **kwargs
        )
        if block is not None and block(host):
            raise ValueError(f"BLOCK {host}")
        roption = schedule(rserver, salgorithm, host, port) or DIRECT
        reader_remote, remote_writer = await roption.open_connection(
            host, port, None, lbind, timeout=SOCKET_TIMEOUT
        )
        reader_remote, remote_writer = await roption.prepare_connection(
            reader_remote, remote_writer, host, port
        )
        rproto = getattr(roption, "rproto", None) or BaseProtocol("")
        stats = modstat(user, "unknown", host)
        await asyncio.gather(
            lproto.channel(reader, remote_writer, stats(0), stats(1)),
            rproto.channel(reader_remote, writer, stats(2), stats(3)),
        )
    except asyncio.CancelledError:
        raise
    except Exception as exc:
        if debug:
            raise
        verbose(str(exc) or "Unsupported protocol")
    finally:
        for stream in (writer, remote_writer):
            if stream is not None:
                stream.close()
                wait_closed = getattr(stream, "wait_closed", None)
                if wait_closed is not None:
                    try:
                        await wait_closed()
                    except Exception:
                        pass


def print_server_started(*args, **kwargs):
    """Format and return a startup message.

    Matches pproxy oracle: formats a startup message with listener
    addresses.  Returns the formatted string rather than printing to stdout,
    allowing callers to control output.
    """
    parts = []
    for arg in args:
        if isinstance(arg, str):
            parts.append(arg)
        elif hasattr(arg, "bind"):
            parts.append(str(arg.bind))
    if kwargs:
        for k, v in kwargs.items():
            if k in ("host", "port", "bind", "verbose"):
                parts.append(f"{k}={v}")
    return " ".join(parts) if parts else None


async def test_url(url, rserver):
    """Issue a bounded HTTP request per configured remote proxy."""
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme not in ("http", "https"):
        raise ValueError(f"Unknown scheme {parsed.scheme}")
    host = parsed.hostname
    if not host:
        raise ValueError(f"missing host in URL: {url!r}")
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    target = parsed.path or "/"
    if parsed.query:
        target += "?" + parsed.query
    request = (
        f"GET {target} HTTP/1.1\r\nHost: {host}\r\n"
        f"User-Agent: pproxy-{__import__('eggress').__version__}\r\n"
        "Accept: */*\r\nConnection: close\r\n\r\n"
    ).encode()
    for remote in rserver:
        reader, writer = await remote.tcp_connect(host, port)
        try:
            writer.write(request)
            await writer.drain()
            await reader.read(1)
        finally:
            writer.close()
            wait_closed = getattr(writer, "wait_closed", None)
            if wait_closed is not None:
                await wait_closed()

sslcontexts = []
compile_rule.__module__ = __name__


def main(args=None):
    """Run the compatibility service through the Python/native adapter."""
    argv = list(sys.argv[1:] if args is None else args)
    try:
        validate_pproxy_args(argv)
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 2

    if "--version" in argv:
        from eggress import __version__

        print(f"eggress-pproxy-compat {__version__}")
        return 0
    if "-h" in argv or "--help" in argv:
        print("pproxy compatibility binary (eggress-pproxy-compat)")
        print("USAGE: pproxy [OPTIONS]")
        print("  -l URI, -r URI, -ul URI, -ur URI   repeatable listener/remote options")
        print("  -b PATTERN  -a SECONDS  -s {fa,rr,rc,lc}")
        print("  -d, -v       repeatable debug/verbose actions (-vv adds traffic stats)")
        print("  --ssl FILE  --pac PATH  --get PATH,FILE  --auth SECONDS")
        print("  --sys  --reuse  --daemon  --test URL  --version  -h/--help")
        print("Positional URIs, --log, --rulefile, and long listener aliases are rejected.")
        return 0

    if "--test" in argv:
        index = argv.index("--test")
        target = argv[index + 1] if index + 1 < len(argv) else ""
        try:
            return int(run_pproxy_test(argv, target))
        except Exception as exc:
            print(str(exc), file=sys.stderr)
            return 2

    from eggress.pproxy import PPProxyService, translate_pproxy_args

    service_args = argv or ["-l", "http+socks4+socks5://:8080"]
    options = pproxy_runtime_options(service_args)
    init_pproxy_logging(options["default_log_level"])
    translation = translate_pproxy_args(service_args)
    if not translation.ok:
        for feature in translation.unsupported:
            print(f"pproxy: unsupported: {feature.message}", file=sys.stderr)
        return 5

    try:
        service = PPProxyService.from_args(service_args)
    except Exception as exc:
        print(f"pproxy: startup error: {exc}", file=sys.stderr)
        return 1
    handle = service.start()
    stopped = threading.Event()

    def stop(_signum, _frame):
        stopped.set()
        handle.shutdown()

    previous = {}
    for name in ("SIGINT", "SIGTERM"):
        signal_name = getattr(signal, name, None)
        if signal_name is not None:
            previous[signal_name] = signal.signal(signal_name, stop)
    try:
        stopped.wait()
    except KeyboardInterrupt:
        stop(signal.SIGINT, None)
    finally:
        for signal_name, old_handler in previous.items():
            signal.signal(signal_name, old_handler)
    return 0

__all__ = ["AuthTable", "DIRECT", "DUMMY", "ProxyBackward", "ProxyDirect", "ProxyH2",
           "ProxyH3", "ProxyQUIC", "ProxySSH", "ProxySimple", "Rule", "Connection",
           "Server", "compile_rule", "proxy_by_uri", "proxies_by_uri", "schedule",
           "check_server_alive", "datagram_handler", "main", "prepare_ciphers",
           "print_server_started", "sslcontexts", "stream_handler", "test_url"]
