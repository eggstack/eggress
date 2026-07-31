"""Focused Phase 4 tests for the installed top-level pproxy namespace."""

from __future__ import annotations

import asyncio
import socket
import threading

import pproxy


def test_public_namespace_and_aliases():
    from pproxy import cipher, proto, server

    assert pproxy.Connection is server.proxies_by_uri
    assert pproxy.Server is server.proxies_by_uri
    assert pproxy.Rule is server.compile_rule
    assert pproxy.DIRECT is server.DIRECT
    assert proto.Socks5 and cipher.AES_256_GCM_Cipher


def test_direct_tcp_connect_reader_writer_contract():
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    address = listener.getsockname()

    def echo():
        conn, _ = listener.accept()
        with conn:
            payload = conn.recv(64)
            conn.sendall(payload)
        listener.close()

    thread = threading.Thread(target=echo, daemon=True)
    thread.start()

    async def exercise():
        reader, writer = await pproxy.Connection("direct://").tcp_connect(*address)
        writer.write(b"phase4")
        await writer.drain()
        assert await reader.readexactly(6) == b"phase4"
        writer.close()
        await writer.wait_closed()

    asyncio.run(exercise())
    thread.join(timeout=2)


def test_direct_udp_callback_contract():
    async def exercise():
        received = asyncio.get_running_loop().create_future()

        class Echo(asyncio.DatagramProtocol):
            def datagram_received(self, data, addr):
                self.transport.sendto(data, addr)

            def connection_made(self, transport):
                self.transport = transport

        transport, _ = await asyncio.get_running_loop().create_datagram_endpoint(
            Echo, local_addr=("127.0.0.1", 0)
        )
        target = transport.get_extra_info("sockname")
        proxy = pproxy.Connection("direct://")

        def callback(data):
            if not received.done():
                received.set_result(data)

        await proxy.udp_sendto(target[0], target[1], b"udp", callback)
        # The callback is scheduled by the datagram protocol.
        assert await asyncio.wait_for(received, 2) == b"udp"
        transport.close()

    asyncio.run(exercise())

