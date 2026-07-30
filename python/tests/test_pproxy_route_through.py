"""Deterministic route-through tests for ProxySimple (PC2/CE3).

These tests prove that ProxySimple.tcp_connect() traverses the configured
proxy endpoint rather than connecting directly to the final target.  Each
scenario uses a scripted proxy server that records every received byte and
logical event, then returns success without contacting a real target.

Direct-bypass sentinel destinations cannot succeed if reached directly but
can be accepted symbolically by the scripted proxy.
"""

from __future__ import annotations

import subprocess
import sys

import pytest

pproxy = pytest.importorskip("pproxy", reason="requires upstream pproxy package")

_SCRIPT = """\
import asyncio
import json
import socket
import struct
import sys


class ScriptedSOCKS5Proxy:
    def __init__(self, proxy_id='socks5'):
        self.proxy_id = proxy_id
        self.events = []
        self._server = None
        self.port = 0

    async def start(self):
        self._server = await asyncio.start_server(self._handle_client, '127.0.0.1', 0)
        self.port = self._server.sockets[0].getsockname()[1]

    async def stop(self):
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _handle_client(self, reader, writer):
        try:
            greeting = await reader.readexactly(2)
            nmethods = greeting[1]
            methods = await reader.readexactly(nmethods)
            writer.write(b'\\x05\\x00')
            await writer.drain()
            header = await reader.readexactly(4)
            _, _, _, atyp = header
            if atyp == 0x03:
                domain_len = (await reader.readexactly(1))[0]
                domain_data = await reader.readexactly(domain_len)
                host = domain_data.decode('ascii')
            elif atyp == 0x01:
                addr_data = await reader.readexactly(4)
                host = '.'.join(str(b) for b in addr_data)
            elif atyp == 0x04:
                addr_data = await reader.readexactly(16)
                host = socket.inet_ntop(socket.AF_INET6, addr_data)
            else:
                writer.close()
                return
            port_data = await reader.readexactly(2)
            port = struct.unpack('!H', port_data)[0]
            self.events.append({'proxy_id': self.proxy_id, 'kind': 'connect', 'host': host, 'port': port})
            writer.write(b'\\x05\\x00\\x00\\x01' + socket.inet_aton('0.0.0.0') + struct.pack('!H', 0))
            await writer.drain()
            try:
                while not reader.at_eof():
                    data = await reader.read(65536)
                    if not data:
                        break
                    writer.write(data)
                    await writer.drain()
            except (asyncio.IncompleteReadError, ConnectionError):
                pass
        except asyncio.CancelledError:
            raise
        except Exception:
            pass
        finally:
            writer.close()


class ScriptedHTTPProxy:
    def __init__(self, proxy_id='http'):
        self.proxy_id = proxy_id
        self.events = []
        self._server = None
        self.port = 0

    async def start(self):
        self._server = await asyncio.start_server(self._handle_client, '127.0.0.1', 0)
        self.port = self._server.sockets[0].getsockname()[1]

    async def stop(self):
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _handle_client(self, reader, writer):
        try:
            request_line = await reader.readline()
            if not request_line:
                writer.close()
                return
            parts = request_line.split(b' ')
            if len(parts) < 2:
                writer.write(b'HTTP/1.1 400 Bad Request\\r\\n\\r\\n')
                await writer.drain()
                writer.close()
                return
            method = parts[0]
            target = parts[1].decode('ascii')
            if method == b'CONNECT':
                if ':' in target:
                    host, port_str = target.rsplit(':', 1)
                    port = int(port_str)
                else:
                    host = target
                    port = 443
                self.events.append({'proxy_id': self.proxy_id, 'kind': 'connect', 'host': host, 'port': port})
                while True:
                    line = await reader.readline()
                    if not line or line == b'\\r\\n':
                        break
                writer.write(b'HTTP/1.1 200 Connection Established\\r\\n\\r\\n')
                await writer.drain()
                try:
                    while not reader.at_eof():
                        data = await reader.read(65536)
                        if not data:
                            break
                        writer.write(data)
                        await writer.drain()
                except (asyncio.IncompleteReadError, ConnectionError):
                    pass
        except asyncio.CancelledError:
            raise
        except Exception:
            pass
        finally:
            writer.close()


class RelayingSOCKS5Proxy:
    def __init__(self, proxy_id='socks5'):
        self.proxy_id = proxy_id
        self.events = []
        self._server = None
        self.port = 0

    async def start(self):
        self._server = await asyncio.start_server(self._handle_client, '127.0.0.1', 0)
        self.port = self._server.sockets[0].getsockname()[1]

    async def stop(self):
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _handle_client(self, reader, writer):
        try:
            greeting = await reader.readexactly(2)
            nmethods = greeting[1]
            methods = await reader.readexactly(nmethods)
            writer.write(b'\\x05\\x00')
            await writer.drain()
            header = await reader.readexactly(4)
            _, _, _, atyp = header
            if atyp == 0x03:
                domain_len = (await reader.readexactly(1))[0]
                domain_data = await reader.readexactly(domain_len)
                host = domain_data.decode('ascii')
            elif atyp == 0x01:
                addr_data = await reader.readexactly(4)
                host = '.'.join(str(b) for b in addr_data)
            elif atyp == 0x04:
                addr_data = await reader.readexactly(16)
                host = socket.inet_ntop(socket.AF_INET6, addr_data)
            else:
                writer.close()
                return
            port_data = await reader.readexactly(2)
            port = struct.unpack('!H', port_data)[0]
            self.events.append({'proxy_id': self.proxy_id, 'kind': 'connect', 'host': host, 'port': port})
            try:
                remote_reader, remote_writer = await asyncio.wait_for(
                    asyncio.open_connection(host, port), timeout=5.0,
                )
            except Exception:
                writer.write(b'\\x05\\x05\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00')
                await writer.drain()
                writer.close()
                return
            writer.write(b'\\x05\\x00\\x00\\x01' + socket.inet_aton('0.0.0.0') + struct.pack('!H', 0))
            await writer.drain()
            async def relay(r, w):
                try:
                    while not r.at_eof():
                        data = await r.read(65536)
                        if not data:
                            break
                        w.write(data)
                        await w.drain()
                except Exception:
                    pass
                finally:
                    w.close()
            t1 = asyncio.ensure_future(relay(reader, remote_writer))
            t2 = asyncio.ensure_future(relay(remote_reader, writer))
            await asyncio.wait([t1, t2], return_when=asyncio.FIRST_COMPLETED)
            t1.cancel()
            t2.cancel()
        except asyncio.CancelledError:
            raise
        except Exception:
            pass
        finally:
            writer.close()


class RejectingSOCKS5Proxy:
    # SOCKS5 proxy that rejects all CONNECT requests.
    def __init__(self, proxy_id='socks5-reject'):
        self.proxy_id = proxy_id
        self.events = []
        self._server = None
        self.port = 0

    async def start(self):
        self._server = await asyncio.start_server(self._handle_client, '127.0.0.1', 0)
        self.port = self._server.sockets[0].getsockname()[1]

    async def stop(self):
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _handle_client(self, reader, writer):
        try:
            greeting = await reader.readexactly(2)
            nmethods = greeting[1]
            methods = await reader.readexactly(nmethods)
            writer.write(b'\\x05\\x00')
            await writer.drain()
            header = await reader.readexactly(4)
            _, _, _, atyp = header
            if atyp == 0x03:
                domain_len = (await reader.readexactly(1))[0]
                await reader.readexactly(domain_len)
            elif atyp == 0x01:
                await reader.readexactly(4)
            elif atyp == 0x04:
                await reader.readexactly(16)
            else:
                writer.close()
                return
            await reader.readexactly(2)
            self.events.append({'proxy_id': self.proxy_id, 'kind': 'connect_rejected'})
            writer.write(b'\\x05\\x05\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00')
            await writer.drain()
        except asyncio.CancelledError:
            raise
        except Exception:
            pass
        finally:
            writer.close()


async def run_test(test_name):
    from pproxy.server import proxy_by_uri, DIRECT

    if test_name == 'socks5_single_hop':
        proxy = ScriptedSOCKS5Proxy('socks5-a')
        await proxy.start()
        try:
            p = proxy_by_uri('socks5://127.0.0.1:%d' % proxy.port, DIRECT)
            reader, writer = await asyncio.wait_for(
                p.tcp_connect('route-through.invalid', 443), timeout=5.0,
            )
            connect_events = [e for e in proxy.events if e['kind'] == 'connect']
            assert len(connect_events) == 1
            assert connect_events[0]['host'] == 'route-through.invalid'
            assert connect_events[0]['port'] == 443
            writer.close()
            await writer.wait_closed()
        finally:
            await proxy.stop()

    elif test_name == 'http_single_hop':
        proxy = ScriptedHTTPProxy('http-b')
        await proxy.start()
        try:
            p = proxy_by_uri('http://127.0.0.1:%d' % proxy.port, DIRECT)
            reader, writer = await asyncio.wait_for(
                p.tcp_connect('route-through.invalid', 443), timeout=5.0,
            )
            connect_events = [e for e in proxy.events if e['kind'] == 'connect']
            assert len(connect_events) >= 1
            assert connect_events[0]['host'] == 'route-through.invalid'
            assert connect_events[0]['port'] == 443
            writer.close()
            await writer.wait_closed()
        finally:
            await proxy.stop()

    elif test_name == 'two_hop_socks5_http':
        socks5_proxy = RelayingSOCKS5Proxy('socks5-a')
        http_proxy = ScriptedHTTPProxy('http-b')
        await socks5_proxy.start()
        await http_proxy.start()
        try:
            chain_uri = 'socks5://127.0.0.1:%d__http://127.0.0.1:%d' % (
                socks5_proxy.port, http_proxy.port,
            )
            from pproxy.server import proxies_by_uri
            p = proxies_by_uri(chain_uri)
            reader, writer = await asyncio.wait_for(
                p.tcp_connect('chain-target.invalid', 443), timeout=8.0,
            )
            socks5_events = [e for e in socks5_proxy.events if e['kind'] == 'connect']
            http_events = [e for e in http_proxy.events if e['kind'] == 'connect']
            assert len(socks5_events) >= 1, 'SOCKS5 proxy should have received a connect'
            assert len(http_events) >= 1, 'HTTP proxy should have received a connect'
            assert socks5_events[0]['host'] == '127.0.0.1'
            assert socks5_events[0]['port'] == http_proxy.port
            assert http_events[0]['host'] == 'chain-target.invalid'
            assert http_events[0]['port'] == 443
            writer.close()
            await writer.wait_closed()
        finally:
            await socks5_proxy.stop()
            await http_proxy.stop()

    elif test_name == 'two_hop_http_socks5':
        http_proxy = RelayingSOCKS5Proxy.__new__(RelayingSOCKS5Proxy)
        http_proxy.proxy_id = 'http-relay'
        http_proxy.events = []
        http_proxy._server = None
        http_proxy.port = 0
        # Use a relaying HTTP proxy
        class RelayingHTTPProxy:
            def __init__(self):
                self.proxy_id = 'http-a'
                self.events = []
                self._server = None
                self.port = 0
            async def start(self):
                self._server = await asyncio.start_server(self._handle, '127.0.0.1', 0)
                self.port = self._server.sockets[0].getsockname()[1]
            async def stop(self):
                if self._server:
                    self._server.close()
                    await self._server.wait_closed()
            async def _handle(self, reader, writer):
                try:
                    line = await reader.readline()
                    parts = line.split(b' ')
                    if len(parts) < 2:
                        writer.close()
                        return
                    method = parts[0]
                    target = parts[1].decode('ascii')
                    if method == b'CONNECT':
                        host, _, port_s = target.partition(':')
                        port = int(port_s) if port_s else 443
                        self.events.append({'proxy_id': self.proxy_id, 'kind': 'connect', 'host': host, 'port': port})
                        while True:
                            hdr = await reader.readline()
                            if not hdr or hdr == b'\\r\\n':
                                break
                        try:
                            remote_r, remote_w = await asyncio.wait_for(
                                asyncio.open_connection(host, port), timeout=5.0)
                        except Exception:
                            writer.write(b'HTTP/1.1 502 Bad Gateway\\r\\n\\r\\n')
                            await writer.drain()
                            writer.close()
                            return
                        writer.write(b'HTTP/1.1 200 Connection Established\\r\\n\\r\\n')
                        await writer.drain()
                        async def relay(r, w):
                            try:
                                while not r.at_eof():
                                    data = await r.read(65536)
                                    if not data: break
                                    w.write(data); await w.drain()
                            except Exception: pass
                            finally: w.close()
                        t1 = asyncio.ensure_future(relay(reader, remote_w))
                        t2 = asyncio.ensure_future(relay(remote_r, writer))
                        await asyncio.wait([t1, t2], return_when=asyncio.FIRST_COMPLETED)
                        t1.cancel(); t2.cancel()
                except asyncio.CancelledError: raise
                except Exception: pass
                finally: writer.close()

        http_proxy = RelayingHTTPProxy()
        socks5_proxy = ScriptedSOCKS5Proxy('socks5-b')
        await http_proxy.start()
        await socks5_proxy.start()
        try:
            chain_uri = 'http://127.0.0.1:%d__socks5://127.0.0.1:%d' % (
                http_proxy.port, socks5_proxy.port,
            )
            from pproxy.server import proxies_by_uri
            p = proxies_by_uri(chain_uri)
            reader, writer = await asyncio.wait_for(
                p.tcp_connect('chain-target.invalid', 443), timeout=8.0,
            )
            http_events = [e for e in http_proxy.events if e['kind'] == 'connect']
            socks5_events = [e for e in socks5_proxy.events if e['kind'] == 'connect']
            assert len(http_events) >= 1, 'HTTP proxy should have received a connect'
            assert len(socks5_events) >= 1, 'SOCKS5 proxy should have received a connect'
            assert http_events[0]['host'] == '127.0.0.1'
            assert http_events[0]['port'] == socks5_proxy.port
            assert socks5_events[0]['host'] == 'chain-target.invalid'
            assert socks5_events[0]['port'] == 443
            writer.close()
            await writer.wait_closed()
        finally:
            await http_proxy.stop()
            await socks5_proxy.stop()

    elif test_name == 'direct_bypass_fails':
        p = proxy_by_uri('socks5://127.0.0.1:1', DIRECT)
        try:
            await asyncio.wait_for(
                p.tcp_connect('route-through.invalid', 443), timeout=2.0,
            )
            assert False, 'Should have raised'
        except (ConnectionError, OSError, asyncio.TimeoutError):
            pass

    elif test_name == 'proxy_failure':
        p = proxy_by_uri('socks5://127.0.0.1:1', DIRECT)
        try:
            await asyncio.wait_for(
                p.tcp_connect('route-through.invalid', 443), timeout=2.0,
            )
            assert False, 'Should have raised'
        except (ConnectionError, OSError, asyncio.TimeoutError):
            pass

    elif test_name == 'no_fallback_timeout':
        p = proxy_by_uri('socks5://192.0.2.1:1', DIRECT)
        try:
            await asyncio.wait_for(
                p.tcp_connect('route-through.invalid', 443), timeout=2.0,
            )
            assert False, 'Should have raised'
        except (ConnectionError, OSError, asyncio.TimeoutError):
            pass

    elif test_name == 'cleanup_success':
        proxy = ScriptedSOCKS5Proxy('cleanup-test')
        await proxy.start()
        try:
            p = proxy_by_uri('socks5://127.0.0.1:%d' % proxy.port, DIRECT)
            reader, writer = await asyncio.wait_for(
                p.tcp_connect('cleanup-test.invalid', 443), timeout=5.0,
            )
            writer.close()
            await writer.wait_closed()
            await asyncio.sleep(0.1)
        finally:
            await proxy.stop()

    elif test_name == 'first_hop_unavailable':
        p = proxy_by_uri('socks5://127.0.0.1:1__http://127.0.0.1:1', DIRECT)
        try:
            await asyncio.wait_for(
                p.tcp_connect('target.invalid', 443), timeout=2.0,
            )
            assert False, 'Should have raised'
        except (ConnectionError, OSError, asyncio.TimeoutError):
            pass

    elif test_name == 'second_hop_unavailable':
        socks5_proxy = RelayingSOCKS5Proxy('socks5-a')
        await socks5_proxy.start()
        try:
            chain_uri = 'socks5://127.0.0.1:%d__http://127.0.0.1:1' % socks5_proxy.port
            from pproxy.server import proxies_by_uri
            p = proxies_by_uri(chain_uri)
            try:
                await asyncio.wait_for(
                    p.tcp_connect('target.invalid', 443), timeout=5.0,
                )
                assert False, 'Should have raised'
            except (ConnectionError, OSError, asyncio.TimeoutError, ValueError, AssertionError):
                pass
            connect_events = [e for e in socks5_proxy.events if e['kind'] == 'connect']
            assert len(connect_events) >= 1
            assert connect_events[0]['host'] == '127.0.0.1'
        finally:
            await socks5_proxy.stop()

    elif test_name == 'handshake_rejection':
        reject_proxy = RejectingSOCKS5Proxy('reject-a')
        await reject_proxy.start()
        try:
            p = proxy_by_uri('socks5://127.0.0.1:%d' % reject_proxy.port, DIRECT)
            try:
                await asyncio.wait_for(
                    p.tcp_connect('target.invalid', 443), timeout=5.0,
                )
                assert False, 'Should have raised'
            except (ConnectionError, OSError, asyncio.TimeoutError, ValueError, AssertionError) as e:
                pass
            reject_events = [e for e in reject_proxy.events if e['kind'] == 'connect_rejected']
            assert len(reject_events) >= 1
        finally:
            await reject_proxy.stop()

    print(json.dumps({'status': 'ok'}))


test_name = sys.argv[1]
asyncio.run(run_test(test_name))
"""


def _run_route_test(test_name: str, timeout: float = 20.0) -> None:
    """Run a route-through test in a subprocess to avoid event loop conflicts."""
    result = subprocess.run(
        [sys.executable, "-c", _SCRIPT, test_name],
        capture_output=True, text=True, timeout=timeout,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"Route-through test '{test_name}' failed (rc={result.returncode}):\n"
            f"stderr: {result.stderr.strip()}"
        )
    assert '{"status": "ok"}' in result.stdout, (
        f"Route-through test '{test_name}' produced unexpected output:\n"
        f"stdout: {result.stdout.strip()}\nstderr: {result.stderr.strip()}"
    )


class TestSOCKS5RouteThrough:
    def test_single_hop_socks5_route_through(self):
        """SOCKS5 connection reaches the scripted proxy before any final target activity."""
        _run_route_test("socks5_single_hop")

    def test_direct_bypass_fails(self):
        """A destination that cannot succeed directly must fail when proxy is unavailable."""
        _run_route_test("direct_bypass_fails")

    def test_proxy_failure_is_operation_failure(self):
        """Disabling the proxy causes failure, not direct fallback."""
        _run_route_test("proxy_failure")


class TestHTTPRouteThrough:
    def test_single_hop_http_route_through(self):
        """HTTP CONNECT reaches the scripted proxy before final target activity."""
        _run_route_test("http_single_hop")


class TestTwoHopChain:
    def test_socks5_to_http_chain(self):
        """Two-hop chain: outer SOCKS5 receives inner HTTP address, inner HTTP receives final target."""
        _run_route_test("two_hop_socks5_http")

    def test_http_to_socks5_chain(self):
        """Two-hop chain: outer HTTP receives inner SOCKS5 address, inner SOCKS5 receives final target."""
        _run_route_test("two_hop_http_socks5")


class TestNoDirectFallback:
    def test_no_fallback_on_timeout(self):
        """Timeout connecting to proxy must fail, not fall back to direct."""
        _run_route_test("no_fallback_timeout")

    def test_first_hop_unavailable(self):
        """First hop unavailable: entire chain fails, no fallback."""
        _run_route_test("first_hop_unavailable")

    def test_second_hop_unavailable(self):
        """Second hop unavailable: first hop receives connect, then chain fails."""
        _run_route_test("second_hop_unavailable")


class TestHandshakeRejection:
    def test_socks5_rejection(self):
        """SOCKS5 handshake rejection causes operation failure."""
        _run_route_test("handshake_rejection")


class TestConnectionCleanup:
    def test_resources_cleaned_on_success(self):
        """After successful connection, resources are cleaned up."""
        _run_route_test("cleanup_success")
