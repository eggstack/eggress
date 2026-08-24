//! pproxy 2.7.9 SSR framing and stream adapter.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

use crate::address::{decode_address, encode_address};
use crate::compat::plugin::{self, PluginDecoder, PproxyPlugin};
use crate::error::ShadowsocksError;
use eggress_core::{BoxStream, TargetAddr};

/// A bounded SSR compatibility configuration.
#[derive(Debug, Clone, Default)]
pub struct SsrConfig {
    pub auth_prefix: Option<Vec<u8>>,
    pub plugins: Vec<PproxyPlugin>,
}

/// Client-side SSR handshake. The returned stream applies ordered plugin
/// framing to the SSR address and subsequent application bytes.
pub async fn ssr_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    config: &SsrConfig,
) -> Result<BoxStream, ShadowsocksError> {
    run_client_prefaces(&mut stream, target, &config.plugins).await?;
    let mut stream = PproxyStream::new(stream, config.plugins.clone());
    let mut header = config.auth_prefix.clone().unwrap_or_default();
    header.extend_from_slice(&encode_address(target)?);
    stream.write_all(&header).await?;
    stream.flush().await?;
    Ok(Box::new(stream))
}

/// Server-side SSR handshake. The returned stream preserves all bytes after
/// the destination address as application payload.
pub async fn ssr_accept(
    mut stream: BoxStream,
    config: &SsrConfig,
) -> Result<(BoxStream, TargetAddr), ShadowsocksError> {
    run_server_prefaces(&mut stream, &config.plugins).await?;
    let mut stream = PproxyStream::new(stream, config.plugins.clone());
    if let Some(prefix) = &config.auth_prefix {
        let mut received = vec![0u8; prefix.len()];
        stream.read_exact(&mut received).await?;
        if received != *prefix {
            return Err(ShadowsocksError::DecryptionFailed(
                "SSR auth prefix mismatch".into(),
            ));
        }
    }
    let mut header = vec![0u8; 1];
    let mut read_from = 1;
    stream.read_exact(&mut header).await?;
    match header[0] {
        1 => header.resize(7, 0),
        3 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            header.push(len[0]);
            header.resize(4 + len[0] as usize, 0);
            read_from = 2;
        }
        4 => header.resize(19, 0),
        17 | 19 | 20 => {
            return Err(ShadowsocksError::Other(
                "SSR OTA framing is not supported".into(),
            ))
        }
        other => {
            return Err(ShadowsocksError::InvalidAddress(format!(
                "unknown SSR address type {other:#04x}"
            )))
        }
    }
    if header.len() > read_from {
        stream.read_exact(&mut header[read_from..]).await?;
    }
    let (target, _) = decode_address(&header)?;
    Ok((Box::new(stream), target))
}

async fn run_client_prefaces(
    stream: &mut BoxStream,
    target: &TargetAddr,
    plugins: &[PproxyPlugin],
) -> Result<(), ShadowsocksError> {
    for plugin_kind in plugins {
        match plugin_kind {
            PproxyPlugin::HttpSimple => {
                plugin::http_simple_client_preface(stream, &target.host.to_string()).await?
            }
            PproxyPlugin::Tls12TicketAuth => {
                stream.write_all(&[0x16, 0x03, 0x01, 0x00, 0x20]).await?;
                stream.write_all(&[0x01, 0x00, 0x1c]).await?;
                stream.write_all(&[0u8; 29]).await?;
                stream.flush().await?;
                let mut response = [0u8; 5];
                stream.read_exact(&mut response).await?;
                if response[..3] != [0x16, 0x03, 0x03] {
                    return Err(ShadowsocksError::Other(
                        "invalid tls1.2_ticket_auth response".into(),
                    ));
                }
                let len = u16::from_be_bytes([response[3], response[4]]) as usize;
                if len > 16 * 1024 {
                    return Err(ShadowsocksError::FrameTooLarge);
                }
                let mut body = vec![0u8; len];
                stream.read_exact(&mut body).await?;
            }
            PproxyPlugin::Plain
            | PproxyPlugin::Origin
            | PproxyPlugin::VerifySimple
            | PproxyPlugin::VerifyDeflate => {}
        }
    }
    Ok(())
}

async fn run_server_prefaces(
    stream: &mut BoxStream,
    plugins: &[PproxyPlugin],
) -> Result<(), ShadowsocksError> {
    for plugin_kind in plugins {
        match plugin_kind {
            PproxyPlugin::HttpSimple => plugin::http_simple_server_preface(stream).await?,
            PproxyPlugin::Tls12TicketAuth => {
                let mut hello = [0u8; 5];
                stream.read_exact(&mut hello).await?;
                if hello[..3] != [0x16, 0x03, 0x01] {
                    return Err(ShadowsocksError::Other(
                        "invalid tls1.2_ticket_auth hello".into(),
                    ));
                }
                let len = u16::from_be_bytes([hello[3], hello[4]]) as usize;
                if len > 16 * 1024 {
                    return Err(ShadowsocksError::FrameTooLarge);
                }
                let mut body = vec![0u8; len];
                stream.read_exact(&mut body).await?;
                stream.write_all(&[0x16, 0x03, 0x03, 0, 1, 2]).await?;
                stream.flush().await?;
            }
            PproxyPlugin::Plain
            | PproxyPlugin::Origin
            | PproxyPlugin::VerifySimple
            | PproxyPlugin::VerifyDeflate => {}
        }
    }
    Ok(())
}

/// Async stream adapter for incremental plugin framing.
pub struct PproxyStream {
    inner: BoxStream,
    plugins: Vec<PproxyPlugin>,
    decoder: PluginDecoder,
    read_buf: [u8; 8192],
    write_buf: Vec<u8>,
    write_pos: usize,
}

impl PproxyStream {
    pub fn new(inner: BoxStream, plugins: Vec<PproxyPlugin>) -> Self {
        Self {
            decoder: PluginDecoder::new(&plugins),
            inner,
            plugins,
            read_buf: [0; 8192],
            write_buf: Vec::new(),
            write_pos: 0,
        }
    }

    fn write_pending(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        while this.write_pos < this.write_buf.len() {
            let pos = this.write_pos;
            match Pin::new(&mut this.inner).poll_write(cx, &this.write_buf[pos..]) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(plugin::io_invalid(
                        "plugin stream write returned zero",
                    )))
                }
                Poll::Ready(Ok(n)) => this.write_pos += n,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        this.write_buf.clear();
        this.write_pos = 0;
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for PproxyStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        let mut plain = [0u8; 8192];
        let limit = output.remaining().min(plain.len());
        let count = this.decoder.take_plain(&mut plain[..limit]);
        if count > 0 {
            output.put_slice(&plain[..count]);
            return Poll::Ready(Ok(()));
        }
        let mut read = ReadBuf::new(&mut this.read_buf);
        match Pin::new(&mut this.inner).poll_read(cx, &mut read) {
            Poll::Ready(Ok(())) => {
                let filled = read.filled().len();
                if filled == 0 {
                    if this.decoder.has_partial_frame() {
                        return Poll::Ready(Err(plugin::io_invalid("truncated plugin frame")));
                    }
                    return Poll::Ready(Ok(()));
                }
                if let Err(error) = this.decoder.feed(&this.plugins, &this.read_buf[..filled]) {
                    return Poll::Ready(Err(plugin::io_invalid(error.to_string())));
                }
                let limit = output.remaining().min(plain.len());
                let count = this.decoder.take_plain(&mut plain[..limit]);
                if count > 0 {
                    output.put_slice(&plain[..count]);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for PproxyStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if !self.write_buf.is_empty() {
            match self.as_mut().write_pending(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        let encoded = match plugin::encode_payload(&self.plugins, data) {
            Ok(encoded) => encoded,
            Err(error) => return Poll::Ready(Err(plugin::io_invalid(error.to_string()))),
        };
        self.write_buf = encoded;
        self.write_pos = 0;
        match self.as_mut().write_pending(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(data.len())),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            // The encoded bytes are buffered but not yet accepted by the
            // inner stream; report Pending so the caller is woken to flush.
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.as_mut().write_pending(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.get_mut().inner).poll_flush(cx),
            other => other,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.get_mut().inner).poll_shutdown(cx),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::{TargetAddr, TargetHost};
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn ssr_roundtrip_preserves_target_and_payload() {
        let (left, right) = tokio::io::duplex(4096);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 443,
        };
        let config = SsrConfig {
            auth_prefix: Some(b"user".to_vec()),
            plugins: vec![PproxyPlugin::VerifySimple],
        };
        let server_config = config.clone();
        let server_target = target.clone();
        let server = tokio::spawn(async move {
            let (mut stream, received) = ssr_accept(Box::new(right), &server_config).await.unwrap();
            assert_eq!(received, server_target);
            let mut payload = Vec::new();
            stream.read_to_end(&mut payload).await.unwrap();
            payload
        });
        let mut client = ssr_connect(Box::new(left), &target, &config).await.unwrap();
        client.write_all(b"hello SSR").await.unwrap();
        client.shutdown().await.unwrap();
        assert_eq!(server.await.unwrap(), b"hello SSR");
    }

    #[test]
    fn address_types_are_exactly_socks_tags() {
        for (host, tag) in [
            (TargetHost::Ip("127.0.0.1".parse().unwrap()), 1),
            (TargetHost::Domain("example.com".into()), 3),
            (TargetHost::Ip("::1".parse().unwrap()), 4),
        ] {
            let bytes = crate::address::encode_address(&TargetAddr { host, port: 80 }).unwrap();
            assert_eq!(bytes[0], tag);
        }
    }
}
