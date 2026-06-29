use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio_rustls::TlsAcceptor;

pub struct TcpEchoServer {
    addr: SocketAddr,
    connection_count: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl TcpEchoServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let connection_count = Arc::new(AtomicU64::new(0));
        let cc = connection_count.clone();

        let handle = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                cc.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });

        Self {
            addr,
            connection_count,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn connection_count(&self) -> &AtomicU64 {
        &self.connection_count
    }
}

impl Drop for TcpEchoServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct UdpEchoServer {
    addr: SocketAddr,
    packet_count: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl UdpEchoServer {
    pub async fn start() -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let packet_count = Arc::new(AtomicU64::new(0));
        let pc = packet_count.clone();

        let handle = tokio::spawn(async move {
            let mut buf = [0u8; 65535];
            loop {
                let (n, peer) = match socket.recv_from(&mut buf).await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                pc.fetch_add(1, Ordering::Relaxed);
                let _ = socket.send_to(&buf[..n], peer).await;
            }
        });

        Self {
            addr,
            packet_count,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn packet_count(&self) -> &AtomicU64 {
        &self.packet_count
    }
}

impl Drop for UdpEchoServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct HttpOriginServer {
    addr: SocketAddr,
    request_count: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl HttpOriginServer {
    pub async fn start() -> Self {
        Self::start_with_body(b"hello from origin").await
    }

    pub async fn start_with_body(body: &[u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let request_count = Arc::new(AtomicU64::new(0));
        let rc = request_count.clone();
        let body = body.to_vec();

        let handle = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                rc.fetch_add(1, Ordering::Relaxed);
                let body = body.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => return,
                            Ok(n) => {
                                request.extend_from_slice(&buf[..n]);
                                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => return,
                        }
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n\
                         Content-Length: {}\r\n\
                         Connection: close\r\n\
                         \r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        Self {
            addr,
            request_count,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn request_count(&self) -> &AtomicU64 {
        &self.request_count
    }
}

impl Drop for HttpOriginServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct HttpConnectUpstream {
    addr: SocketAddr,
    connection_count: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl HttpConnectUpstream {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let connection_count = Arc::new(AtomicU64::new(0));
        let cc = connection_count.clone();

        let handle = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                cc.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => return,
                            Ok(n) => {
                                request.extend_from_slice(&buf[..n]);
                                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => return,
                        }
                    }

                    let request_str = String::from_utf8_lossy(&request);
                    let first_line = request_str.lines().next().unwrap_or("");
                    let parts: Vec<&str> = first_line.split_whitespace().collect();

                    if parts.len() < 2 || parts[0] != "CONNECT" {
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                        return;
                    }

                    let target_addr = parts[1].to_string();

                    let target = match tokio::net::TcpStream::connect(&target_addr).await {
                        Ok(t) => t,
                        Err(_) => {
                            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                            return;
                        }
                    };

                    if stream
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .await
                        .is_err()
                    {
                        return;
                    }

                    let (mut cr, mut cw) = stream.into_split();
                    let (mut tr, mut tw) = target.into_split();

                    let c2t = tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut cr, &mut tw).await;
                        let _ = tw.shutdown().await;
                    });
                    let t2c = tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut tr, &mut cw).await;
                        let _ = cw.shutdown().await;
                    });
                    let _ = tokio::join!(c2t, t2c);
                });
            }
        });

        Self {
            addr,
            connection_count,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn connection_count(&self) -> &AtomicU64 {
        &self.connection_count
    }
}

impl Drop for HttpConnectUpstream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct Socks5Upstream {
    addr: SocketAddr,
    connection_count: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl Socks5Upstream {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let connection_count = Arc::new(AtomicU64::new(0));
        let cc = connection_count.clone();

        let handle =
            tokio::spawn(async move {
                loop {
                    let (mut stream, _) = match listener.accept().await {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    cc.fetch_add(1, Ordering::Relaxed);
                    tokio::spawn(async move {
                        let mut header = [0u8; 2];
                        if stream.read_exact(&mut header).await.is_err() {
                            return;
                        }
                        let nmethods = header[1] as usize;
                        let mut methods = vec![0u8; nmethods];
                        if stream.read_exact(&mut methods).await.is_err() {
                            return;
                        }
                        if stream.write_all(&[0x05, 0x00]).await.is_err() {
                            return;
                        }

                        let mut req = [0u8; 4];
                        if stream.read_exact(&mut req).await.is_err() {
                            return;
                        }
                        let atyp = req[3];
                        let target_addr =
                            match atyp {
                                0x01 => {
                                    let mut addr = [0u8; 4];
                                    if stream.read_exact(&mut addr).await.is_err() {
                                        return;
                                    }
                                    let port = stream.read_u16().await.unwrap_or(0);
                                    format!(
                                        "{}.{}.{}.{}:{}",
                                        addr[0], addr[1], addr[2], addr[3], port
                                    )
                                }
                                0x03 => {
                                    let len = stream.read_u8().await.unwrap_or(0) as usize;
                                    let mut domain = vec![0u8; len];
                                    if stream.read_exact(&mut domain).await.is_err() {
                                        return;
                                    }
                                    let port = stream.read_u16().await.unwrap_or(0);
                                    let domain = String::from_utf8_lossy(&domain);
                                    format!("{}:{}", domain, port)
                                }
                                0x04 => {
                                    let mut addr = [0u8; 16];
                                    if stream.read_exact(&mut addr).await.is_err() {
                                        return;
                                    }
                                    let port = stream.read_u16().await.unwrap_or(0);
                                    format!(
                                "[{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}]:{}",
                                addr[0], addr[1], addr[2], addr[3], addr[4], addr[5], addr[6],
                                addr[7], port
                            )
                                }
                                _ => return,
                            };

                        let target = match tokio::net::TcpStream::connect(&target_addr).await {
                            Ok(t) => t,
                            Err(_) => {
                                let _ = stream
                                    .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                                    .await;
                                return;
                            }
                        };

                        if stream
                            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await
                            .is_err()
                        {
                            return;
                        }

                        let (mut cr, mut cw) = stream.into_split();
                        let (mut tr, mut tw) = target.into_split();

                        let c2t = tokio::spawn(async move {
                            let _ = tokio::io::copy(&mut cr, &mut tw).await;
                            let _ = tw.shutdown().await;
                        });
                        let t2c = tokio::spawn(async move {
                            let _ = tokio::io::copy(&mut tr, &mut cw).await;
                            let _ = cw.shutdown().await;
                        });
                        let _ = tokio::join!(c2t, t2c);
                    });
                }
            });

        Self {
            addr,
            connection_count,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn connection_count(&self) -> &AtomicU64 {
        &self.connection_count
    }
}

impl Drop for Socks5Upstream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct Socks4Upstream {
    addr: SocketAddr,
    connection_count: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl Socks4Upstream {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let connection_count = Arc::new(AtomicU64::new(0));
        let cc = connection_count.clone();

        let handle = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                cc.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    let mut header = [0u8; 8];
                    if stream.read_exact(&mut header).await.is_err() {
                        return;
                    }
                    let version = header[0];
                    let cmd = header[1];
                    if version != 0x04 || cmd != 0x01 {
                        return;
                    }
                    let port = u16::from_be_bytes([header[2], header[3]]);
                    let ip = [header[4], header[5], header[6], header[7]];

                    let mut userid = Vec::new();
                    loop {
                        let mut byte = [0u8; 1];
                        if stream.read_exact(&mut byte).await.is_err() {
                            return;
                        }
                        if byte[0] == 0 {
                            break;
                        }
                        userid.push(byte[0]);
                    }
                    let _ = userid;

                    let target_addr = format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port);

                    let target = match tokio::net::TcpStream::connect(&target_addr).await {
                        Ok(t) => t,
                        Err(_) => {
                            let _ = stream.write_all(&[0x00, 0x5B, 0, 0, 0, 0, 0, 0]).await;
                            return;
                        }
                    };

                    if stream
                        .write_all(&[0x00, 0x5A, 0, 0, 0, 0, 0, 0])
                        .await
                        .is_err()
                    {
                        return;
                    }

                    let (mut cr, mut cw) = stream.into_split();
                    let (mut tr, mut tw) = target.into_split();

                    let c2t = tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut cr, &mut tw).await;
                        let _ = tw.shutdown().await;
                    });
                    let t2c = tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut tr, &mut cw).await;
                        let _ = cw.shutdown().await;
                    });
                    let _ = tokio::join!(c2t, t2c);
                });
            }
        });

        Self {
            addr,
            connection_count,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn connection_count(&self) -> &AtomicU64 {
        &self.connection_count
    }
}

impl Drop for Socks4Upstream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct TlsEchoServer {
    addr: SocketAddr,
    cert_der: rustls::pki_types::CertificateDer<'static>,
    handle: tokio::task::JoinHandle<()>,
}

impl TlsEchoServer {
    pub async fn start() -> Self {
        let cert_params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = cert_params.self_signed(&key_pair).unwrap();
        let cert_der = cert.der().clone();
        let key_der = key_pair.serialize_der();

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_der.clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(key_der).into(),
            )
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            loop {
                let (tcp_stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let mut stream = match acceptor.accept(tcp_stream).await {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut buf = [0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });

        Self {
            addr,
            cert_der,
            handle,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn root_store(&self) -> rustls::RootCertStore {
        let mut store = rustls::RootCertStore::empty();
        store.add(self.cert_der.clone()).unwrap();
        store
    }
}

impl Drop for TlsEchoServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct RefusedServer;

impl RefusedServer {
    pub fn addr() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }
}
