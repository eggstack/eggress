"""Server/factory compatibility surface for supported pproxy programs."""

from __future__ import annotations

import argparse
import asyncio
import base64
import functools
import random
import re
import socket
import time
import urllib

try:
    from eggress import start_pproxy
except ImportError:
    start_pproxy = None  # type: ignore[assignment]

try:
    from eggress.pproxy import PPProxyService, Server
except ImportError:
    PPProxyService = Server = None  # type: ignore[assignment,misc]

try:
    from eggress._pproxy_proxy import (
        AuthTable,
        ProxyBackward,
        ProxyDirect,
        ProxyH2,
        ProxyH3,
        ProxyQUIC,
        ProxySSH,
        ProxySimple,
        DIRECT as _DIRECT_INSTANCE,
    )
except ImportError:
    pass

SOCKET_TIMEOUT = 60
UDP_LIMIT = 30
DUMMY = lambda s: s
sslcontexts = []


def proxy_by_uri(uri: str, jump=None):
    """Create a proxy object from a pproxy-style URI.

    In pproxy 2.7.9, this returns a ProxySimple (or ProxyDirect for direct://)
    with the chain topology preserved.
    """
    from eggress.protocol import MAPPINGS

    if not uri:
        raise TypeError("proxy_by_uri() missing required argument: 'uri'")

    # Parse the URI scheme to determine the protocol class
    scheme = uri.split("://")[0].lower() if "://" in uri else ""
    proto_cls = MAPPINGS.get(scheme)

    if scheme == "direct" or proto_cls is None:
        obj = ProxyDirect()
        if jump is not None:
            obj._jump = jump
        return obj
    else:
        try:
            from eggress.pproxy import check_pproxy_uri
            info = check_pproxy_uri(uri)
            host = info.host if info.ok else None
            port = info.port if info.ok else None
        except ImportError:
            # Fallback: parse host:port from URI
            host, port = None, None
            try:
                import urllib.parse
                parsed = urllib.parse.urlparse(uri)
                host = parsed.hostname
                port = parsed.port
            except Exception:
                pass
        # Build bind string from host:port (matching pproxy oracle)
        bind_str = None
        if host and port:
            bind_str = f"{host}:{port}"
        elif host:
            bind_str = host
        return ProxySimple(
            jump=jump if jump is not None else uri,
            protos=(proto_cls,),
            bind=bind_str,
            host_name=host,
            port=port,
        )


def proxies_by_uri(uri_jumps):
    jump = DIRECT
    for uri in reversed(uri_jumps.split('__')):
        jump = proxy_by_uri(uri, jump)
    return jump


def compile_rule(filename):
    if filename.startswith("{") and filename.endswith("}"):
        return re.compile(filename[1:-1]).match
    with open(filename) as f:
        return re.compile('(:?' + ''.join('|'.join(i.strip() for i in f if i.strip() and not i.startswith('#'))) + ')$').match


async def check_server_alive(interval, rserver, verbose=DUMMY):
    while True:
        await asyncio.sleep(interval)
        for remote in rserver:
            if type(remote) is ProxyDirect:
                continue
            try:
                _, writer = await remote.open_connection(None, None, None, None, timeout=3)
            except asyncio.CancelledError:
                return
            except Exception:
                if remote.alive:
                    verbose(f'{getattr(remote.rproto, "name", "?")} {getattr(remote, "bind", "?")} -> OFFLINE')
                    remote.alive = False
                continue
            if not remote.alive:
                verbose(f'{getattr(remote.rproto, "name", "?")} {getattr(remote, "bind", "?")} -> ONLINE')
                remote.alive = True
            try:
                if isinstance(remote, ProxyBackward):
                    writer.write(b'\x00')
                writer.close()
            except Exception:
                pass


async def prepare_ciphers(cipher, reader, writer, bind=None, server_side=True):
    if cipher:
        cipher.pdecrypt = cipher.pdecrypt2 = cipher.pencrypt = cipher.pencrypt2 = DUMMY
        for plugin in cipher.plugins:
            if server_side:
                await plugin.init_server_data(reader, writer, cipher, bind)
            else:
                await plugin.init_client_data(reader, writer, cipher)
            plugin.add_cipher(cipher)
        return cipher(reader, writer, cipher.pdecrypt, cipher.pdecrypt2, cipher.pencrypt, cipher.pencrypt2)
    else:
        return None, None


def schedule(rserver, salgorithm, host_name, port):
    filter_cond = lambda o: o.alive and o.match_rule(host_name, port)
    if salgorithm == 'fa':
        return next(filter(filter_cond, rserver), None)
    elif salgorithm == 'rr':
        for i, roption in enumerate(rserver):
            if filter_cond(roption):
                rserver.append(rserver.pop(i))
                return roption
    elif salgorithm == 'rc':
        filters = [i for i in rserver if filter_cond(i)]
        return random.choice(filters) if filters else None
    elif salgorithm == 'lc':
        return min(filter(filter_cond, rserver), default=None, key=lambda i: i.connections)
    else:
        raise Exception('Unknown scheduling algorithm')


def main(*args, **kwargs):
    return start_pproxy(*args, **kwargs)


def _unsupported_handler(name: str):
    def handler(*args, **kwargs):
        raise NotImplementedError(
            f"pproxy.server.{name} is not part of the certified live path"
        )
    return handler


async def stream_handler(
    reader,
    writer,
    unix,
    lbind,
    protos,
    rserver,
    cipher,
    sslserver,
    debug=0,
    authtime=86400*30,
    block=None,
    salgorithm='fa',
    verbose=DUMMY,
    modstat=lambda u, r, h: lambda i: DUMMY,
    **kwargs,
):
    try:
        reader, writer = proto.sslwrap(reader, writer, sslserver, True, None, verbose)
        if unix:
            remote_ip, remote_text = 'local', 'unix_local'
        else:
            peername = writer.get_extra_info('peername')
            remote_ip, remote_port, *_ = peername if peername else ('unknow_remote_ip', 'unknow_remote_port')
            remote_text = f'{remote_ip}:{remote_port}'
        reader_cipher, _ = await prepare_ciphers(cipher, reader, writer, server_side=False)
        lproto, user, host_name, port, client_connected = await proto.accept(
            protos, reader=reader, writer=writer,
            authtable=AuthTable(remote_ip, authtime),
            reader_cipher=reader_cipher,
            sock=writer.get_extra_info('socket'),
            **kwargs,
        )
        if host_name == 'echo':
            asyncio.ensure_future(lproto.channel(reader, writer, DUMMY, DUMMY))
        elif host_name == 'empty':
            asyncio.ensure_future(lproto.channel(reader, writer, None, DUMMY))
        elif block and block(host_name):
            raise Exception('BLOCK ' + host_name)
        else:
            roption = schedule(rserver, salgorithm, host_name, port) or DIRECT
            verbose(f'{lproto.name} {remote_text}{roption.logtext(host_name, port)}')
            try:
                reader_remote, writer_remote = await roption.open_connection(host_name, port, None, lbind)
            except asyncio.TimeoutError:
                raise Exception(f'Connection timeout {roption.bind}')
            try:
                reader_remote, writer_remote = await roption.prepare_connection(reader_remote, writer_remote, host_name, port)
                use_http = (await client_connected(writer_remote)) if client_connected else None
            except Exception:
                writer_remote.close()
                raise Exception('Unknown remote protocol')
            m = modstat(user, remote_ip, host_name)
            lchannel = lproto.http_channel if use_http else lproto.channel
            asyncio.ensure_future(lproto.channel(reader_remote, writer, m(2 + roption.direct), m(4 + roption.direct)))
            asyncio.ensure_future(lchannel(reader, writer_remote, m(roption.direct), roption.connection_change))
    except Exception as ex:
        if not isinstance(ex, asyncio.TimeoutError) and not str(ex).startswith('Connection closed'):
            verbose(f'{str(ex) or "Unsupported protocol"} from {remote_ip}')
        try:
            writer.close()
        except Exception:
            pass
        if debug:
            raise


async def datagram_handler(
    writer,
    data,
    addr,
    protos,
    urserver,
    block,
    cipher,
    salgorithm,
    verbose=DUMMY,
    **kwargs,
):
    try:
        remote_ip, remote_port, *_ = addr
        remote_text = f'{remote_ip}:{remote_port}'
        data = cipher.datagram.decrypt(data) if cipher else data
        lproto, user, host_name, port, data = proto.udp_accept(
            protos, data, sock=writer.get_extra_info('socket'), **kwargs,
        )
        if host_name == 'echo':
            writer.sendto(data, addr)
        elif host_name == 'empty':
            pass
        elif block and block(host_name):
            raise Exception('BLOCK ' + host_name)
        else:
            roption = schedule(urserver, salgorithm, host_name, port) or DIRECT
            verbose(f'UDP {lproto.name} {remote_text}{roption.logtext(host_name, port)}')
            data = roption.udp_prepare_connection(host_name, port, data)

            def reply(rdata):
                rdata = lproto.udp_pack(host_name, port, rdata)
                writer.sendto(cipher.datagram.encrypt(rdata) if cipher else rdata, addr)

            await roption.udp_open_connection(host_name, port, data, addr, reply)
    except Exception as ex:
        if not str(ex).startswith('Connection closed'):
            verbose(f'{str(ex) or "Unsupported protocol"} from {remote_ip}')


def patch_StreamReader(reader):
    """Patch a StreamReader for compatibility. No-op, returns reader unchanged."""
    return reader


def patch_StreamWriter(writer):
    """Patch a StreamWriter for compatibility. No-op, returns writer unchanged."""
    return writer


def print_server_started(*args, **kwargs):
    """Print a server startup message. No-op for compatibility."""
    return None


def test_url(url, proxy=None, timeout=5.0):
    """Test if a URL is reachable through the proxy.

    Returns a dict with 'ok' (bool), 'code' (HTTP status or None),
    and 'error' (str or None).
    """
    import urllib.request
    import urllib.error
    if not url:
        return {"ok": False, "code": None, "error": "empty url"}
    try:
        req = urllib.request.Request(url, method="GET")
        if proxy is not None:
            proxy_uri = getattr(proxy, "jump", None) or getattr(proxy, "_jump", None)
            if proxy_uri:
                req.set_proxy(proxy_uri.split("://")[-1].split("/")[0], "http")
        resp = urllib.request.urlopen(req, timeout=timeout)
        code = resp.getcode()
        resp.close()
        return {"ok": True, "code": code, "error": None}
    except urllib.error.HTTPError as e:
        return {"ok": False, "code": e.code, "error": str(e)}
    except Exception as e:
        return {"ok": False, "code": None, "error": str(e)}

DIRECT = ProxyDirect()

__all__ = [
    "AuthTable", "DIRECT", "DUMMY", "PPProxyService", "ProxyBackward",
    "ProxyDirect", "ProxyH2", "ProxyH3", "ProxyQUIC", "ProxySSH",
    "ProxySimple", "Server", "SOCKET_TIMEOUT", "UDP_LIMIT", "compile_rule",
    "proxies_by_uri", "proxy_by_uri", "main", "sslcontexts",
    "schedule", "check_server_alive", "prepare_ciphers",
    "stream_handler", "datagram_handler",
]
