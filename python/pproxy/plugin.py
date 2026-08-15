"""The six bounded SSR plugins shipped by pproxy 2.7.9.

These classes intentionally mirror the upstream callback shape.  They are
compatibility obfuscators, not a general plugin registry and not a security
boundary; native Eggress networking remains Rust-owned.
"""

from __future__ import annotations

import binascii
import datetime
import os
import random
import time
import zlib


def _pack(data: bytes, width: int = 2) -> bytes:
    return len(data).to_bytes(width, "big") + data


class BasePlugin:
    async def init_client_data(self, reader, writer, cipher):
        return None

    async def init_server_data(self, reader, writer, cipher, raddr):
        return None

    def add_cipher(self, cipher):
        return None

    @classmethod
    def name(cls) -> str:
        return cls.__name__.replace("_Plugin", "").replace("__", ".").lower()


class Plain_Plugin(BasePlugin):
    pass


class Origin_Plugin(BasePlugin):
    pass


class Http_Simple_Plugin(BasePlugin):
    async def init_client_data(self, reader, writer, cipher):
        request = await reader.readuntil(b"\r\n\r\n")
        if not request.startswith(b"GET "):
            raise ValueError("invalid http_simple request")
        writer.write(
            b"HTTP/1.1 200 OK\r\nConnection: keep-alive\r\n"
            b"Content-Encoding: gzip\r\nContent-Type: text/html\r\nDate: "
            + datetime.datetime.now(datetime.timezone.utc).strftime("%a, %d %b %Y %H:%M:%S GMT").encode()
            + b"\r\nServer: nginx\r\nVary: Accept-Encoding\r\n\r\n"
        )

    async def init_server_data(self, reader, writer, cipher, raddr):
        writer.write(
            f"GET / HTTP/1.1\r\nHost: {raddr}\r\nUser-Agent: curl\r\n"
            "Accept-Encoding: gzip, deflate\r\nConnection: keep-alive\r\n\r\n".encode()
        )
        await reader.readuntil(b"\r\n\r\n")


class Tls1__2_Ticket_Auth_Plugin(BasePlugin):
    """Bounded TLS-looking preface; this is not real TLS."""

    CACHE: list[bytes] = []

    async def init_client_data(self, reader, writer, cipher):
        hello = await reader.readexactly(5)
        if hello[:3] != b"\x16\x03\x01":
            raise ValueError("invalid tls1.2_ticket_auth hello")
        body = await reader.readexactly(int.from_bytes(hello[3:], "big"))
        if len(body) > 16 * 1024:
            raise ValueError("tls1.2_ticket_auth hello too large")
        writer.write(b"\x16\x03\x03\x00\x01\x02")

    async def init_server_data(self, reader, writer, cipher, raddr):
        writer.write(b"\x16\x03\x01\x00\x20\x01\x00\x1c" + bytes(29))
        response = await reader.readexactly(5)
        # The client response is a one-byte bounded record.
        length = int.from_bytes(response[3:], "big")
        await reader.readexactly(length)


class Verify_Simple_Plugin(BasePlugin):
    def add_cipher(self, cipher):
        buffer = bytearray()

        def decrypt(data):
            buffer.extend(data)
            result = bytearray()
            while len(buffer) >= 2:
                length = int.from_bytes(buffer[:2], "big")
                if length < 7 or len(buffer) < length:
                    break
                expected = int.from_bytes(buffer[length - 4:length], "little")
                actual = (~binascii.crc32(buffer[:length - 4])) & 0xFFFFFFFF
                if expected != actual:
                    raise ValueError("verify_simple CRC mismatch")
                padding = buffer[2]
                if padding == 0 or 2 + padding > length - 4:
                    raise ValueError("invalid verify_simple padding")
                result.extend(buffer[2 + padding:length - 4])
                del buffer[:length]
            return bytes(result)

        def encrypt(data):
            result = bytearray()
            for start in range(0, len(data), 8100):
                chunk = data[start:start + 8100]
                padding = os.urandom(os.urandom(1)[0] % 16)
                body = bytes([len(padding) + 1]) + padding + chunk
                frame = (len(body) + 6).to_bytes(2, "big") + body
                result.extend(frame + ((~binascii.crc32(frame)) & 0xFFFFFFFF).to_bytes(4, "little"))
            return bytes(result)

        cipher.pdecrypt = decrypt
        cipher.pencrypt = encrypt


class Verify_Deflate_Plugin(BasePlugin):
    def add_cipher(self, cipher):
        buffer = bytearray()

        def decrypt(data):
            buffer.extend(data)
            result = bytearray()
            while len(buffer) >= 2:
                length = int.from_bytes(buffer[:2], "big")
                if length < 3 or len(buffer) < length:
                    break
                result.extend(zlib.decompress(b"\x78\x9c" + buffer[2:length]))
                del buffer[:length]
            return bytes(result)

        def encrypt(data):
            result = bytearray()
            for start in range(0, len(data), 32700):
                compressed = zlib.compress(data[start:start + 32700])
                body = compressed[2:]
                result.extend(len(body + b"xx").to_bytes(2, "big"))
                result.extend(body)
            return bytes(result)

        cipher.pdecrypt = decrypt
        cipher.pencrypt = encrypt


PLUGIN = {
    cls.name(): cls
    for cls in (
        Plain_Plugin,
        Origin_Plugin,
        Http_Simple_Plugin,
        Tls1__2_Ticket_Auth_Plugin,
        Verify_Simple_Plugin,
        Verify_Deflate_Plugin,
    )
}


def get_plugin(plugin_name):
    plugin = PLUGIN.get(plugin_name)
    if plugin is None:
        return f"existing plugins: {sorted(PLUGIN)}", None
    return None, plugin()


__all__ = ["BasePlugin", *PLUGIN, "PLUGIN", "get_plugin"]
