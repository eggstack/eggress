use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub mod client;
pub mod server;

/// Maximum frame payload size (64 KiB).
pub const MAX_FRAME_SIZE: usize = 64 * 1024;

/// Control channel frame types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    /// Authenticate with the remote side.
    Auth = 0x01,
    /// Auth success response.
    AuthOk = 0x02,
    /// Auth failure response.
    AuthFail = 0x03,
    /// Open a new stream (accepted connection).
    OpenStream = 0x10,
    /// Stream opened successfully.
    StreamOpened = 0x11,
    /// Stream data payload.
    StreamData = 0x20,
    /// Stream close (both directions).
    StreamClose = 0x21,
    /// Stream reset (error, abort).
    StreamReset = 0x22,
    /// Heartbeat ping.
    Ping = 0x30,
    /// Heartbeat pong.
    Pong = 0x31,
}

impl FrameType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(Self::Auth),
            0x02 => Some(Self::AuthOk),
            0x03 => Some(Self::AuthFail),
            0x10 => Some(Self::OpenStream),
            0x11 => Some(Self::StreamOpened),
            0x20 => Some(Self::StreamData),
            0x21 => Some(Self::StreamClose),
            0x22 => Some(Self::StreamReset),
            0x30 => Some(Self::Ping),
            0x31 => Some(Self::Pong),
            _ => None,
        }
    }
}

/// A control channel frame.
///
/// Wire format: [type:u8][stream_id:u32][length:u32][payload:length bytes]
#[derive(Debug, Clone)]
pub struct Frame {
    pub frame_type: FrameType,
    pub stream_id: u32,
    pub payload: Bytes,
}

impl Frame {
    pub fn new(frame_type: FrameType, stream_id: u32, payload: Bytes) -> Self {
        Self {
            frame_type,
            stream_id,
            payload,
        }
    }

    pub fn auth(username: &str, password: &str) -> Self {
        let mut payload = BytesMut::new();
        let user_bytes = username.as_bytes();
        let pass_bytes = password.as_bytes();
        payload.put_u16(user_bytes.len() as u16);
        payload.extend_from_slice(user_bytes);
        payload.put_u16(pass_bytes.len() as u16);
        payload.extend_from_slice(pass_bytes);
        Self::new(FrameType::Auth, 0, payload.freeze())
    }

    pub fn open_stream(stream_id: u32, target_host: &str, target_port: u16) -> Self {
        let mut payload = BytesMut::new();
        let host_bytes = target_host.as_bytes();
        payload.put_u16(host_bytes.len() as u16);
        payload.extend_from_slice(host_bytes);
        payload.put_u16(target_port);
        Self::new(FrameType::OpenStream, stream_id, payload.freeze())
    }

    pub fn stream_data(stream_id: u32, data: Bytes) -> Self {
        Self::new(FrameType::StreamData, stream_id, data)
    }

    pub fn stream_close(stream_id: u32) -> Self {
        Self::new(FrameType::StreamClose, stream_id, Bytes::new())
    }

    pub fn stream_reset(stream_id: u32) -> Self {
        Self::new(FrameType::StreamReset, stream_id, Bytes::new())
    }

    pub fn ping() -> Self {
        Self::new(FrameType::Ping, 0, Bytes::new())
    }

    pub fn pong() -> Self {
        Self::new(FrameType::Pong, 0, Bytes::new())
    }

    pub fn auth_ok() -> Self {
        Self::new(FrameType::AuthOk, 0, Bytes::new())
    }

    pub fn auth_fail() -> Self {
        Self::new(FrameType::AuthFail, 0, Bytes::new())
    }

    pub fn stream_opened(stream_id: u32) -> Self {
        Self::new(FrameType::StreamOpened, stream_id, Bytes::new())
    }

    /// Encode this frame into a BytesMut buffer.
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(self.frame_type as u8);
        buf.put_u32(self.stream_id);
        buf.put_u32(self.payload.len() as u32);
        buf.extend_from_slice(&self.payload);
    }

    /// Try to decode a frame from a buffer. Returns `None` if not enough data.
    pub fn decode(buf: &mut BytesMut) -> Option<Self> {
        if buf.len() < 9 {
            return None;
        }

        let frame_type = FrameType::from_u8(buf[0])?;
        let stream_id = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
        let length = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;

        if length > MAX_FRAME_SIZE {
            return None;
        }

        if buf.len() < 9 + length {
            return None;
        }

        buf.advance(9);
        let payload = buf.split_to(length).freeze();

        Some(Self {
            frame_type,
            stream_id,
            payload,
        })
    }
}

/// Decode an Auth frame payload into (username, password).
pub fn decode_auth_payload(payload: &Bytes) -> Result<(String, String), ProtocolError> {
    if payload.len() < 2 {
        return Err(ProtocolError::MalformedFrame);
    }
    let user_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if payload.len() < 2 + user_len + 2 {
        return Err(ProtocolError::MalformedFrame);
    }
    let username = String::from_utf8_lossy(&payload[2..2 + user_len]).to_string();
    let pass_start = 2 + user_len;
    let pass_len = u16::from_be_bytes([payload[pass_start], payload[pass_start + 1]]) as usize;
    if payload.len() < pass_start + 2 + pass_len {
        return Err(ProtocolError::MalformedFrame);
    }
    let password =
        String::from_utf8_lossy(&payload[pass_start + 2..pass_start + 2 + pass_len]).to_string();
    Ok((username, password))
}

/// Decode an OpenStream frame payload into (target_host, target_port).
pub fn decode_open_stream_payload(payload: &Bytes) -> Result<(String, u16), ProtocolError> {
    if payload.len() < 2 {
        return Err(ProtocolError::MalformedFrame);
    }
    let host_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if payload.len() < 2 + host_len + 2 {
        return Err(ProtocolError::MalformedFrame);
    }
    let host = String::from_utf8_lossy(&payload[2..2 + host_len]).to_string();
    let port_start = 2 + host_len;
    let port = u16::from_be_bytes([payload[port_start], payload[port_start + 1]]);
    Ok((host, port))
}

/// Errors specific to the reverse protocol.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("malformed frame")]
    MalformedFrame,
    #[error("frame too large")]
    FrameTooLarge,
    #[error("authentication failed")]
    AuthFailed,
    #[error("authentication required")]
    AuthRequired,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// State of a reverse control channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState {
    Disconnected,
    Connecting,
    Authenticating,
    Ready,
    Draining,
    Closed,
}

/// Write a frame to a TcpStream.
pub async fn write_frame(stream: &mut TcpStream, frame: &Frame) -> Result<(), ProtocolError> {
    let mut buf = BytesMut::new();
    frame.encode(&mut buf);
    stream.write_all(&buf).await?;
    Ok(())
}

/// Read a complete frame from a TcpStream into a reusable buffer.
pub async fn read_frame(
    stream: &mut TcpStream,
    buf: &mut BytesMut,
) -> Result<Frame, ProtocolError> {
    loop {
        // Try to decode from existing buffer
        if let Some(frame) = Frame::decode(buf) {
            return Ok(frame);
        }

        // Check for oversized frame in buffer
        if buf.len() >= 9 {
            let length = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
            if length > MAX_FRAME_SIZE {
                return Err(ProtocolError::FrameTooLarge);
            }
        }

        // Read more data
        if buf.capacity() - buf.len() < 1024 {
            buf.reserve(MAX_FRAME_SIZE);
        }
        let n = stream.read_buf(buf).await.map_err(ProtocolError::Io)?;
        if n == 0 {
            return Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "connection closed",
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let frame = Frame::auth("user", "pass");
        let mut buf = BytesMut::new();
        frame.encode(&mut buf);
        let decoded = Frame::decode(&mut buf.clone()).unwrap();
        assert_eq!(decoded.frame_type, FrameType::Auth);
        assert_eq!(decoded.stream_id, 0);
        assert_eq!(decoded.payload, frame.payload);
    }

    #[test]
    fn frame_stream_data() {
        let data = Bytes::from_static(b"hello world");
        let frame = Frame::stream_data(42, data.clone());
        let mut buf = BytesMut::new();
        frame.encode(&mut buf);
        let decoded = Frame::decode(&mut buf.clone()).unwrap();
        assert_eq!(decoded.frame_type, FrameType::StreamData);
        assert_eq!(decoded.stream_id, 42);
        assert_eq!(decoded.payload, data);
    }

    #[test]
    fn decode_auth_payload_roundtrip() {
        let frame = Frame::auth("user", "pass");
        let (user, pass) = decode_auth_payload(&frame.payload).unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "pass");
    }

    #[test]
    fn decode_open_stream_payload_roundtrip() {
        let frame = Frame::open_stream(5, "example.com", 80);
        let (host, port) = decode_open_stream_payload(&frame.payload).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
    }

    #[test]
    fn frame_type_from_u8() {
        assert_eq!(FrameType::from_u8(0x01), Some(FrameType::Auth));
        assert_eq!(FrameType::from_u8(0x20), Some(FrameType::StreamData));
        assert_eq!(FrameType::from_u8(0xFF), None);
    }

    #[test]
    fn frame_empty_payload() {
        let frame = Frame::ping();
        let mut buf = BytesMut::new();
        frame.encode(&mut buf);
        let decoded = Frame::decode(&mut buf.clone()).unwrap();
        assert_eq!(decoded.frame_type, FrameType::Ping);
        assert_eq!(decoded.payload.len(), 0);
    }
}
