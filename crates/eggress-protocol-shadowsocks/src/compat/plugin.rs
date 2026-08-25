//! The closed set of pproxy 2.7.9 SSR plugins.

use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::ShadowsocksError;

const MAX_FRAME: usize = 32_700;
const VERIFY_MAX_PAYLOAD: usize = 8_100;
const TLS_MAX_RECORD: usize = 16 * 1024;
/// Upper bound on the decompressed size of one verify_deflate frame. Legit
/// peers frame payloads of at most ~64 KiB before compression; anything
/// beyond this bound is a decompression bomb from a malicious peer.
const MAX_DECOMPRESSED_FRAME: usize = 256 * 1024;

/// The six plugin names exported by pproxy 2.7.9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PproxyPlugin {
    Plain,
    Origin,
    HttpSimple,
    Tls12TicketAuth,
    VerifySimple,
    VerifyDeflate,
}

impl PproxyPlugin {
    pub const SUPPORTED: [&'static str; 6] = [
        "plain",
        "origin",
        "http_simple",
        "tls1.2_ticket_auth",
        "verify_simple",
        "verify_deflate",
    ];

    pub fn parse(name: &str) -> Result<Self, ShadowsocksError> {
        match name {
            "plain" => Ok(Self::Plain),
            "origin" => Ok(Self::Origin),
            "http_simple" => Ok(Self::HttpSimple),
            "tls1.2_ticket_auth" => Ok(Self::Tls12TicketAuth),
            "verify_simple" => Ok(Self::VerifySimple),
            "verify_deflate" => Ok(Self::VerifyDeflate),
            other => Err(ShadowsocksError::Other(format!(
                "unknown pproxy plugin '{}'; existing plugins: {}",
                other,
                Self::SUPPORTED.join(", ")
            ))),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Origin => "origin",
            Self::HttpSimple => "http_simple",
            Self::Tls12TicketAuth => "tls1.2_ticket_auth",
            Self::VerifySimple => "verify_simple",
            Self::VerifyDeflate => "verify_deflate",
        }
    }

    fn frames_data(self) -> bool {
        matches!(
            self,
            Self::Tls12TicketAuth | Self::VerifySimple | Self::VerifyDeflate
        )
    }
}

/// Parse and validate an ordered plugin list without allowing dynamic names.
pub fn parse_plugins(names: &[String]) -> Result<Vec<PproxyPlugin>, ShadowsocksError> {
    names.iter().map(|name| PproxyPlugin::parse(name)).collect()
}

/// Encode one or more payload bytes through plugins in source order.
pub fn encode_payload(
    plugins: &[PproxyPlugin],
    payload: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    let mut data = payload.to_vec();
    for plugin in plugins.iter().copied().filter(|p| p.frames_data()) {
        data = match plugin {
            PproxyPlugin::Tls12TicketAuth => encode_tls_records(&data),
            PproxyPlugin::VerifySimple => encode_verify_simple(&data),
            PproxyPlugin::VerifyDeflate => encode_verify_deflate(&data)?,
            PproxyPlugin::Plain | PproxyPlugin::Origin | PproxyPlugin::HttpSimple => data,
        };
    }
    Ok(data)
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn encode_verify_simple(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for chunk in payload.chunks(VERIFY_MAX_PAYLOAD) {
        let padding_len = (rand::random::<u8>() % 16) as usize;
        let mut frame = Vec::with_capacity(2 + 1 + padding_len + chunk.len() + 4);
        frame.push((padding_len + 1) as u8);
        frame.extend(std::iter::repeat_n(0, padding_len));
        frame.extend_from_slice(chunk);
        let length = frame.len() + 6;
        frame.splice(0..0, (length as u16).to_be_bytes());
        let crc = !crc32(&frame);
        frame.extend_from_slice(&crc.to_le_bytes());
        output.extend_from_slice(&frame);
    }
    output
}

fn encode_verify_deflate(payload: &[u8]) -> Result<Vec<u8>, ShadowsocksError> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;

    let mut output = Vec::new();
    for chunk in payload.chunks(MAX_FRAME) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(chunk).map_err(ShadowsocksError::Io)?;
        let compressed = encoder.finish().map_err(ShadowsocksError::Io)?;
        let body = compressed.get(2..).ok_or_else(|| {
            ShadowsocksError::Other("zlib encoder returned a truncated stream".into())
        })?;
        let length = body.len() + 2;
        if length > u16::MAX as usize {
            return Err(ShadowsocksError::FrameTooLarge);
        }
        output.extend_from_slice(&(length as u16).to_be_bytes());
        output.extend_from_slice(body);
    }
    Ok(output)
}

fn encode_tls_records(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for chunk in payload.chunks(TLS_MAX_RECORD) {
        output.extend_from_slice(&[0x17, 0x03, 0x03]);
        output.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
        output.extend_from_slice(chunk);
    }
    output
}

/// Incremental decode state for the framing plugins.
#[derive(Debug, Default)]
pub struct PluginDecoder {
    stages: Vec<Vec<u8>>,
    plain: Vec<u8>,
}

impl PluginDecoder {
    pub fn new(plugins: &[PproxyPlugin]) -> Self {
        Self {
            stages: vec![Vec::new(); plugins.iter().filter(|p| p.frames_data()).count() + 1],
            plain: Vec::new(),
        }
    }

    pub fn feed(&mut self, plugins: &[PproxyPlugin], input: &[u8]) -> Result<(), ShadowsocksError> {
        let framed: Vec<PproxyPlugin> = plugins
            .iter()
            .copied()
            .filter(|p| p.frames_data())
            .collect();
        self.stages[0].extend_from_slice(input);
        for (idx, plugin) in framed.iter().rev().enumerate() {
            loop {
                let (decoded, consumed) = decode_one(*plugin, &self.stages[idx])?;
                if consumed == 0 {
                    break;
                }
                self.stages[idx].drain(..consumed);
                self.stages[idx + 1].extend_from_slice(&decoded);
            }
        }
        if let Some(last) = self.stages.last_mut() {
            self.plain.append(last);
        }
        Ok(())
    }

    pub fn take_plain(&mut self, output: &mut [u8]) -> usize {
        let count = output.len().min(self.plain.len());
        output[..count].copy_from_slice(&self.plain[..count]);
        self.plain.drain(..count);
        count
    }

    pub fn has_partial_frame(&self) -> bool {
        self.stages.iter().any(|stage| !stage.is_empty())
    }
}

fn decode_one(plugin: PproxyPlugin, input: &[u8]) -> Result<(Vec<u8>, usize), ShadowsocksError> {
    match plugin {
        PproxyPlugin::VerifySimple => {
            if input.len() < 2 {
                return Ok((Vec::new(), 0));
            }
            let length = u16::from_be_bytes([input[0], input[1]]) as usize;
            if !(7..=MAX_FRAME + 64).contains(&length) {
                return Err(ShadowsocksError::Other(
                    "invalid verify_simple frame length".into(),
                ));
            }
            if input.len() < length {
                return Ok((Vec::new(), 0));
            }
            let expected = u32::from_le_bytes(input[length - 4..length].try_into().unwrap());
            let actual = !crc32(&input[..length - 4]);
            if expected != actual {
                return Err(ShadowsocksError::DecryptionFailed(
                    "verify_simple CRC mismatch".into(),
                ));
            }
            let padding_len = input[2] as usize;
            if padding_len == 0 || 2 + padding_len > length - 4 {
                return Err(ShadowsocksError::Other(
                    "invalid verify_simple padding".into(),
                ));
            }
            Ok((input[2 + padding_len..length - 4].to_vec(), length))
        }
        PproxyPlugin::VerifyDeflate => {
            if input.len() < 2 {
                return Ok((Vec::new(), 0));
            }
            let length = u16::from_be_bytes([input[0], input[1]]) as usize;
            if !(3..=u16::MAX as usize).contains(&length) {
                return Err(ShadowsocksError::Other(
                    "invalid verify_deflate frame length".into(),
                ));
            }
            if input.len() < length {
                return Ok((Vec::new(), 0));
            }
            let mut zlib = vec![0x78, 0x9c];
            zlib.extend_from_slice(&input[2..length]);
            let mut decoder = std::io::Read::take(
                flate2::read::ZlibDecoder::new(zlib.as_slice()),
                MAX_DECOMPRESSED_FRAME as u64 + 1,
            );
            let mut output = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut output).map_err(ShadowsocksError::Io)?;
            if output.len() > MAX_DECOMPRESSED_FRAME {
                return Err(ShadowsocksError::Other(
                    "verify_deflate frame exceeds maximum decompressed size".into(),
                ));
            }
            Ok((output, length))
        }
        PproxyPlugin::Tls12TicketAuth => {
            if input.len() < 5 {
                return Ok((Vec::new(), 0));
            }
            let length = u16::from_be_bytes([input[3], input[4]]) as usize;
            if length > TLS_MAX_RECORD || input.len() < 5 + length {
                return if input.len() < 5 + length {
                    Ok((Vec::new(), 0))
                } else {
                    Err(ShadowsocksError::FrameTooLarge)
                };
            }
            let content_type = input[0];
            let output = if content_type == 0x17 {
                input[5..5 + length].to_vec()
            } else {
                Vec::new()
            };
            Ok((output, 5 + length))
        }
        PproxyPlugin::Plain | PproxyPlugin::Origin | PproxyPlugin::HttpSimple => {
            Ok((input.to_vec(), input.len()))
        }
    }
}

/// Send the pproxy HTTP-simple client preface (the upstream/server side).
pub async fn http_simple_client_preface(
    stream: &mut eggress_core::BoxStream,
    host: &str,
) -> Result<(), ShadowsocksError> {
    stream
        .write_all(format!("GET / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: curl\r\nAccept-Encoding: gzip, deflate\r\nConnection: keep-alive\r\n\r\n").as_bytes())
        .await?;
    stream.flush().await?;
    read_headers(stream).await.map(|_| ())
}

/// Receive the pproxy HTTP-simple client preface and answer it.
pub async fn http_simple_server_preface(
    stream: &mut eggress_core::BoxStream,
) -> Result<(), ShadowsocksError> {
    let headers = read_headers(stream).await?;
    if !headers.starts_with(b"GET ") {
        return Err(ShadowsocksError::Other(
            "invalid http_simple request".into(),
        ));
    }
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nConnection: keep-alive\r\nContent-Encoding: gzip\r\nContent-Type: text/html\r\n\r\n")
        .await?;
    stream.flush().await?;
    Ok(())
}

async fn read_headers(stream: &mut eggress_core::BoxStream) -> Result<Vec<u8>, ShadowsocksError> {
    let mut data = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    while data.len() <= 16 * 1024 {
        stream.read_exact(&mut byte).await?;
        data.push(byte[0]);
        if data.ends_with(b"\r\n\r\n") {
            return Ok(data);
        }
    }
    Err(ShadowsocksError::Other(
        "plugin headers exceed 16 KiB".into(),
    ))
}

pub fn io_invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pproxy_names_are_closed_and_stable() {
        for name in PproxyPlugin::SUPPORTED {
            assert_eq!(PproxyPlugin::parse(name).unwrap().name(), name);
        }
        let error = PproxyPlugin::parse("external").unwrap_err().to_string();
        assert!(error.contains("verify_deflate"));
    }

    #[test]
    fn verify_simple_roundtrips_when_fragmented() {
        let plugins = [PproxyPlugin::VerifySimple];
        let encoded = encode_payload(&plugins, b"fragmented payload").unwrap();
        let mut decoder = PluginDecoder::new(&plugins);
        for byte in encoded {
            decoder.feed(&plugins, &[byte]).unwrap();
        }
        let mut output = [0u8; 64];
        let count = decoder.take_plain(&mut output);
        assert_eq!(&output[..count], b"fragmented payload");
    }

    #[test]
    fn verify_simple_rejects_bad_crc() {
        let plugins = [PproxyPlugin::VerifySimple];
        let mut encoded = encode_payload(&plugins, b"payload").unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 1;
        let mut decoder = PluginDecoder::new(&plugins);
        assert!(decoder.feed(&plugins, &encoded).is_err());
    }

    #[cfg(feature = "pproxy-legacy")]
    #[test]
    fn ordered_plugins_roundtrip_through_reverse_decoder() {
        let plugins = [PproxyPlugin::VerifySimple, PproxyPlugin::VerifyDeflate];
        let encoded = encode_payload(&plugins, b"ordered plugin payload").unwrap();
        let mut decoder = PluginDecoder::new(&plugins);
        for chunk in encoded.chunks(3) {
            decoder.feed(&plugins, chunk).unwrap();
        }
        let mut output = [0u8; 64];
        let count = decoder.take_plain(&mut output);
        assert_eq!(&output[..count], b"ordered plugin payload");
    }

    #[cfg(feature = "pproxy-legacy")]
    #[test]
    fn verify_deflate_roundtrips() {
        let plugins = [PproxyPlugin::VerifyDeflate];
        let encoded = encode_payload(&plugins, b"compressed payload").unwrap();
        let mut decoder = PluginDecoder::new(&plugins);
        decoder.feed(&plugins, &encoded).unwrap();
        let mut output = [0u8; 64];
        let count = decoder.take_plain(&mut output);
        assert_eq!(&output[..count], b"compressed payload");
    }

    #[test]
    fn verify_deflate_rejects_decompression_bomb() {
        use std::io::Write;

        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(9));
        encoder
            .write_all(&vec![0u8; MAX_DECOMPRESSED_FRAME * 4])
            .unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() <= u16::MAX as usize);

        let mut frame = (compressed.len() as u16).to_be_bytes().to_vec();
        frame.extend_from_slice(&compressed[2..]);
        let error = decode_one(PproxyPlugin::VerifyDeflate, &frame).unwrap_err();
        assert!(error.to_string().contains("maximum decompressed size"));
    }
}
