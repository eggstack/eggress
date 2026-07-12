use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub success: bool,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub response: Vec<u8>,
    pub error: Option<String>,
    pub reply_code: Option<u16>,
    pub reply_message: Option<String>,
}

fn ok(response: Vec<u8>, bytes_sent: u64, bytes_received: u64) -> ProbeResult {
    ProbeResult {
        success: true,
        bytes_sent,
        bytes_received,
        response,
        error: None,
        reply_code: None,
        reply_message: None,
    }
}

fn err(msg: impl Into<String>) -> ProbeResult {
    ProbeResult {
        success: false,
        bytes_sent: 0,
        bytes_received: 0,
        response: Vec::new(),
        error: Some(msg.into()),
        reply_code: None,
        reply_message: None,
    }
}

fn err_with_reply(msg: impl Into<String>, code: u16) -> ProbeResult {
    ProbeResult {
        success: false,
        bytes_sent: 0,
        bytes_received: 0,
        response: Vec::new(),
        error: Some(msg.into()),
        reply_code: Some(code),
        reply_message: None,
    }
}

fn build_sock5_target(target: SocketAddr) -> Vec<u8> {
    let mut buf = vec![0x05, 0x01, 0x00, 0x01];
    match target.ip() {
        std::net::IpAddr::V4(ip) => buf.extend_from_slice(&ip.octets()),
        std::net::IpAddr::V6(ip) => {
            buf[3] = 0x04;
            buf.extend_from_slice(&ip.octets());
        }
    }
    buf.extend_from_slice(&target.port().to_be_bytes());
    buf
}

fn extract_http_status_line(resp: &[u8]) -> String {
    let text = String::from_utf8_lossy(resp);
    text.lines().next().unwrap_or("").to_string()
}

fn extract_http_body(resp: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(resp);
    if let Some(pos) = text.find("\r\n\r\n") {
        resp[pos + 4..].to_vec()
    } else {
        resp.to_vec()
    }
}

pub async fn socks5_tcp_connect(
    proxy: SocketAddr,
    target: SocketAddr,
    payload: &[u8],
) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    if let Err(e) = stream.write_all(&[0x05, 0x01, 0x00]).await {
        return err(format!("greeting write: {e}"));
    }
    let mut buf = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut buf).await {
        return err(format!("greeting read: {e}"));
    }
    if buf != [0x05, 0x00] {
        return err(format!("unexpected greeting response: {buf:02x?}"));
    }

    let req = build_sock5_target(target);
    let connect_bytes = req.len() as u64;
    if let Err(e) = stream.write_all(&req).await {
        return err(format!("connect request write: {e}"));
    }
    let mut resp = [0u8; 10];
    if let Err(e) = stream.read_exact(&mut resp).await {
        return err(format!("connect response read: {e}"));
    }
    if resp[1] != 0x00 {
        return err_with_reply(
            format!("SOCKS5 connect failed: reply code {:#04x}", resp[1]),
            resp[1] as u16,
        );
    }

    if let Err(e) = stream.write_all(payload).await {
        return err(format!("payload write: {e}"));
    }
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        return err(format!("response read: {e}"));
    }

    let bytes_sent = connect_bytes + payload.len() as u64;
    let bytes_received = response.len() as u64;
    ok(response, bytes_sent, bytes_received)
}

pub async fn socks5_tcp_connect_auth(
    proxy: SocketAddr,
    target: SocketAddr,
    payload: &[u8],
    username: &str,
    password: &str,
) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    if let Err(e) = stream.write_all(&[0x05, 0x01, 0x02]).await {
        return err(format!("greeting write: {e}"));
    }
    let mut buf = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut buf).await {
        return err(format!("greeting read: {e}"));
    }
    if buf[1] != 0x02 {
        return err("proxy did not accept username/password auth method".to_string());
    }

    let mut auth = vec![0x01];
    auth.push(username.len() as u8);
    auth.extend_from_slice(username.as_bytes());
    auth.push(password.len() as u8);
    auth.extend_from_slice(password.as_bytes());
    if let Err(e) = stream.write_all(&auth).await {
        return err(format!("auth write: {e}"));
    }
    let mut auth_resp = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut auth_resp).await {
        return err(format!("auth read: {e}"));
    }
    if auth_resp[1] != 0x00 {
        return err_with_reply(
            format!("SOCKS5 auth failed: reply code {:#04x}", auth_resp[1]),
            auth_resp[1] as u16,
        );
    }

    let req = build_sock5_target(target);
    let connect_bytes = req.len() as u64;
    if let Err(e) = stream.write_all(&req).await {
        return err(format!("connect request write: {e}"));
    }
    let mut resp = [0u8; 10];
    if let Err(e) = stream.read_exact(&mut resp).await {
        return err(format!("connect response read: {e}"));
    }
    if resp[1] != 0x00 {
        return err_with_reply(
            format!("SOCKS5 connect failed: reply code {:#04x}", resp[1]),
            resp[1] as u16,
        );
    }

    if let Err(e) = stream.write_all(payload).await {
        return err(format!("payload write: {e}"));
    }
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        return err(format!("response read: {e}"));
    }

    let bytes_sent = connect_bytes + payload.len() as u64;
    let bytes_received = response.len() as u64;
    ok(response, bytes_sent, bytes_received)
}

pub async fn socks5_connect_refused(proxy: SocketAddr, target: SocketAddr) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    if let Err(e) = stream.write_all(&[0x05, 0x01, 0x00]).await {
        return err(format!("greeting write: {e}"));
    }
    let mut buf = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut buf).await {
        return err(format!("greeting read: {e}"));
    }

    let req = build_sock5_target(target);
    if let Err(e) = stream.write_all(&req).await {
        return err(format!("connect request write: {e}"));
    }
    let mut resp = [0u8; 10];
    if let Err(e) = stream.read_exact(&mut resp).await {
        return err(format!("connect response read: {e}"));
    }

    let code = resp[1];
    if code == 0x00 {
        err("expected SOCKS5 failure but got success".to_string())
    } else {
        err_with_reply(
            format!("SOCKS5 connect refused: reply code {:#04x}", code),
            code as u16,
        )
    }
}

pub async fn socks5_auth_failure(
    proxy: SocketAddr,
    _target: SocketAddr,
    username: &str,
    password: &str,
) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    if let Err(e) = stream.write_all(&[0x05, 0x01, 0x02]).await {
        return err(format!("greeting write: {e}"));
    }
    let mut buf = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut buf).await {
        return err(format!("greeting read: {e}"));
    }
    if buf[1] != 0x02 {
        return err("proxy did not accept auth method".to_string());
    }

    let mut auth = vec![0x01];
    auth.push(username.len() as u8);
    auth.extend_from_slice(username.as_bytes());
    auth.push(password.len() as u8);
    auth.extend_from_slice(password.as_bytes());
    if let Err(e) = stream.write_all(&auth).await {
        return err(format!("auth write: {e}"));
    }
    let mut auth_resp = [0u8; 2];
    if let Err(e) = stream.read_exact(&mut auth_resp).await {
        return err(format!("auth read: {e}"));
    }

    let code = auth_resp[1];
    if code == 0x00 {
        err("expected auth failure but got success".to_string())
    } else {
        err_with_reply(
            format!("SOCKS5 auth rejected: reply code {:#04x}", code),
            code as u16,
        )
    }
}

pub async fn http_connect(proxy: SocketAddr, target: SocketAddr, payload: &[u8]) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    let connect_req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n");
    let connect_bytes = connect_req.len() as u64;
    if let Err(e) = stream.write_all(connect_req.as_bytes()).await {
        return err(format!("CONNECT write: {e}"));
    }

    let mut resp = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return err(format!("CONNECT response read: {e}")),
        };
        resp.extend_from_slice(&buf[..n]);
        if resp.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let status_line = extract_http_status_line(&resp);
    if !status_line.contains("200") {
        return err(format!("CONNECT failed: {status_line}"));
    }

    if let Err(e) = stream.write_all(payload).await {
        return err(format!("payload write: {e}"));
    }
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        return err(format!("response read: {e}"));
    }

    let bytes_sent = connect_bytes + payload.len() as u64;
    let bytes_received = response.len() as u64;
    ok(response, bytes_sent, bytes_received)
}

pub async fn http_connect_refused(proxy: SocketAddr, target: SocketAddr) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    let connect_req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n");
    if let Err(e) = stream.write_all(connect_req.as_bytes()).await {
        return err(format!("CONNECT write: {e}"));
    }

    let mut resp = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return err(format!("CONNECT response read: {e}")),
        };
        resp.extend_from_slice(&buf[..n]);
        if resp.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let status_line = extract_http_status_line(&resp);
    if status_line.contains("200") {
        err("expected CONNECT failure but got 200".to_string())
    } else {
        let code = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok());
        ProbeResult {
            success: false,
            bytes_sent: 0,
            bytes_received: resp.len() as u64,
            response: resp,
            error: Some(format!("CONNECT refused: {status_line}")),
            reply_code: code,
            reply_message: None,
        }
    }
}

pub async fn http_forward_get(proxy: SocketAddr, target: SocketAddr, path: &str) -> ProbeResult {
    http_forward_inner(proxy, target, "GET", path, &[]).await
}

pub async fn http_forward_post(
    proxy: SocketAddr,
    target: SocketAddr,
    path: &str,
    body: &[u8],
) -> ProbeResult {
    http_forward_inner(proxy, target, "POST", path, body).await
}

async fn http_forward_inner(
    proxy: SocketAddr,
    target: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    let mut request = format!("{method} http://{target}{path} HTTP/1.1\r\nHost: {target}\r\n");
    if !body.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("Connection: close\r\n\r\n");

    let header_bytes = request.len() as u64;
    if let Err(e) = stream.write_all(request.as_bytes()).await {
        return err(format!("request write: {e}"));
    }
    if !body.is_empty() {
        if let Err(e) = stream.write_all(body).await {
            return err(format!("body write: {e}"));
        }
    }

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        return err(format!("response read: {e}"));
    }

    let status_line = extract_http_status_line(&response);
    let body_bytes = extract_http_body(&response);
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok());

    ProbeResult {
        success: code.is_some_and(|c| (200..300).contains(&c)),
        bytes_sent: header_bytes + body.len() as u64,
        bytes_received: response.len() as u64,
        response: body_bytes,
        error: None,
        reply_code: code,
        reply_message: Some(status_line),
    }
}

pub async fn socks4_connect(proxy: SocketAddr, target: SocketAddr, payload: &[u8]) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    let mut req = vec![0x04, 0x01];
    req.extend_from_slice(&target.port().to_be_bytes());
    match target.ip() {
        std::net::IpAddr::V4(ip) => req.extend_from_slice(&ip.octets()),
        std::net::IpAddr::V6(ip) => {
            return err(format!("SOCKS4 does not support IPv6: {}", ip));
        }
    }
    req.push(0x00);
    let connect_bytes = req.len() as u64;
    if let Err(e) = stream.write_all(&req).await {
        return err(format!("connect request write: {e}"));
    }

    let mut resp = [0u8; 8];
    if let Err(e) = stream.read_exact(&mut resp).await {
        return err(format!("connect response read: {e}"));
    }
    if resp[1] != 0x5A {
        return err_with_reply(
            format!("SOCKS4 connect failed: reply code {:#04x}", resp[1]),
            resp[1] as u16,
        );
    }

    if let Err(e) = stream.write_all(payload).await {
        return err(format!("payload write: {e}"));
    }
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        return err(format!("response read: {e}"));
    }

    let bytes_sent = connect_bytes + payload.len() as u64;
    let bytes_received = response.len() as u64;
    ok(response, bytes_sent, bytes_received)
}

pub async fn socks4a_connect(
    proxy: SocketAddr,
    target_host: &str,
    target_port: u16,
    payload: &[u8],
) -> ProbeResult {
    let mut stream = match tokio::net::TcpStream::connect(proxy).await {
        Ok(s) => s,
        Err(e) => return err(format!("connect to proxy: {e}")),
    };

    let mut req = vec![0x04, 0x01];
    req.extend_from_slice(&target_port.to_be_bytes());
    req.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    req.push(0x00);
    req.extend_from_slice(target_host.as_bytes());
    req.push(0x00);
    let connect_bytes = req.len() as u64;
    if let Err(e) = stream.write_all(&req).await {
        return err(format!("connect request write: {e}"));
    }

    let mut resp = [0u8; 8];
    if let Err(e) = stream.read_exact(&mut resp).await {
        return err(format!("connect response read: {e}"));
    }
    if resp[1] != 0x5A {
        return err_with_reply(
            format!("SOCKS4a connect failed: reply code {:#04x}", resp[1]),
            resp[1] as u16,
        );
    }

    if let Err(e) = stream.write_all(payload).await {
        return err(format!("payload write: {e}"));
    }
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        return err(format!("response read: {e}"));
    }

    let bytes_sent = connect_bytes + payload.len() as u64;
    let bytes_received = response.len() as u64;
    ok(response, bytes_sent, bytes_received)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn socks5_tcp_connect_echo() {
        let (addr, jh) = crate::start_echo_server().await;
        let proxy = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
        let _ = (addr, proxy);
        jh.abort();
    }

    #[tokio::test]
    async fn http_forward_get_simple() {
        let proxy = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
        let target = "127.0.0.1:1".parse::<SocketAddr>().unwrap();
        let _ = (proxy, target);
    }
}
