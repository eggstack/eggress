use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use h2::server::Connection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Notify, Semaphore};

use crate::error::HttpError;
use eggress_core::connector::is_dns_rebinding_risk;
use eggress_core::{TargetAddr, TargetHost};

// ===== H2 Protocol Metrics (atomic counters for bridging into MetricsRegistry) =====

/// Atomic counters for H2 protocol-level metrics. The `MetricsRegistry`
/// bridges these into Prometheus via `set_h2_metrics()` / `render_prometheus()`.
pub struct H2ProtocolMetrics {
    pub connections_opened: AtomicU64,
    pub connections_closed: AtomicU64,
    pub streams_opened: AtomicU64,
    pub streams_closed: AtomicU64,
    pub goaway_received: AtomicU64,
    pub handshake_failures: AtomicU64,
    pub auth_failures: AtomicU64,
    pub flow_control_stalls: AtomicU64,
    pub pool_exhausted: AtomicU64,
    pub bytes_relayed: AtomicU64,
}

impl H2ProtocolMetrics {
    pub const fn new() -> Self {
        Self {
            connections_opened: AtomicU64::new(0),
            connections_closed: AtomicU64::new(0),
            streams_opened: AtomicU64::new(0),
            streams_closed: AtomicU64::new(0),
            goaway_received: AtomicU64::new(0),
            handshake_failures: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            flow_control_stalls: AtomicU64::new(0),
            pool_exhausted: AtomicU64::new(0),
            bytes_relayed: AtomicU64::new(0),
        }
    }
}

impl Default for H2ProtocolMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Global H2 protocol metrics instance.
pub static H2_PROTOCOL_METRICS: once_cell::sync::Lazy<Arc<H2ProtocolMetrics>> =
    once_cell::sync::Lazy::new(|| Arc::new(H2ProtocolMetrics::new()));

#[derive(Debug, thiserror::Error)]
pub enum H2ConnectError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("H2 protocol error: {0}")]
    H2(String),
    #[error("HTTP error: {0}")]
    Http(#[from] HttpError),
    #[error("pool exhausted: no connections available and pool at capacity")]
    PoolExhausted,
    #[error("DNS rebinding detected: target resolved to reserved/private address {0}")]
    DnsRebinding(std::net::IpAddr),
}

impl From<h2::Error> for H2ConnectError {
    fn from(e: h2::Error) -> Self {
        H2ConnectError::H2(e.to_string())
    }
}

pub struct H2StreamWrite {
    send_stream: h2::SendStream<Bytes>,
    capacity: usize,
}

impl H2StreamWrite {
    pub fn new(send_stream: h2::SendStream<Bytes>) -> Self {
        Self {
            send_stream,
            capacity: 0,
        }
    }
}

impl tokio::io::AsyncWrite for H2StreamWrite {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if self.capacity == 0 {
            self.send_stream.reserve_capacity(buf.len());
            match self.send_stream.poll_capacity(cx) {
                Poll::Ready(Some(Ok(capacity))) => {
                    self.capacity = capacity;
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Err(std::io::Error::other("h2 stream closed")));
                }
                Poll::Pending => {
                    H2_PROTOCOL_METRICS
                        .flow_control_stalls
                        .fetch_add(1, Ordering::Relaxed);
                    return Poll::Pending;
                }
            }
        }

        let len = buf.len().min(self.capacity);
        self.send_stream
            .send_data(Bytes::copy_from_slice(&buf[..len]), false)
            .map_err(std::io::Error::other)?;
        self.capacity -= len;
        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.send_stream
            .send_data(Bytes::new(), true)
            .map_err(std::io::Error::other)?;
        Poll::Ready(Ok(()))
    }
}

pub async fn h2_connect_relay(
    mut recv_stream: h2::RecvStream,
    send_stream: h2::SendStream<Bytes>,
    target: TargetAddr,
) -> Result<(), H2ConnectError> {
    let tcp = match &target.host {
        TargetHost::Ip(_) => TcpStream::connect(target.to_string()).await?,
        TargetHost::Domain(domain) => {
            let lookup = format!("{}:{}", domain, target.port);
            let mut addrs = tokio::net::lookup_host(&lookup)
                .await
                .map_err(|e| H2ConnectError::H2(format!("DNS resolution failed: {e}")))?;
            let resolved = addrs.next().ok_or_else(|| {
                H2ConnectError::H2("DNS resolution failed: no addresses found".to_string())
            })?;
            if is_dns_rebinding_risk(&resolved.ip()) {
                return Err(H2ConnectError::DnsRebinding(resolved.ip()));
            }
            TcpStream::connect(resolved).await?
        }
    };
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    let mut h2_write = H2StreamWrite::new(send_stream);

    let h2_to_tcp = async move {
        loop {
            match recv_stream.data().await {
                Some(Ok(data)) => {
                    let len = data.len();
                    tcp_write.write_all(&data).await?;
                    H2_PROTOCOL_METRICS
                        .bytes_relayed
                        .fetch_add(len as u64, Ordering::Relaxed);
                }
                Some(Err(e)) => {
                    return Err(std::io::Error::other(e));
                }
                None => break,
            }
        }
        Ok::<(), std::io::Error>(())
    };

    let tcp_to_h2 = async {
        let mut buf = [0u8; 8192];
        loop {
            let n = tcp_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            h2_write.write_all(&buf[..n]).await?;
            H2_PROTOCOL_METRICS
                .bytes_relayed
                .fetch_add(n as u64, Ordering::Relaxed);
        }
        Ok::<(), std::io::Error>(())
    };

    let h2_task = tokio::spawn(h2_to_tcp);
    let tcp_result = tcp_to_h2.await;
    let h2_result = h2_task.await.unwrap();

    h2_result?;
    tcp_result?;
    Ok(())
}

pub async fn handle_h2_connect(
    mut connection: Connection<TcpStream, Bytes>,
) -> Result<(), H2ConnectError> {
    loop {
        match connection.accept().await {
            Some(Ok((request, mut send_response))) => {
                if *request.method() == http::Method::CONNECT {
                    let authority = request
                        .uri()
                        .authority()
                        .ok_or_else(|| H2ConnectError::H2("missing authority".into()))?;

                    let target_str = match authority.port_u16() {
                        Some(port) => format!("{}:{}", authority.host(), port),
                        None => format!("{}:443", authority.host()),
                    };

                    let target: TargetAddr = target_str
                        .parse()
                        .map_err(|e: String| H2ConnectError::H2(e))?;

                    let response = http::Response::builder().status(200).body(()).unwrap();

                    let send_stream = send_response.send_response(response, false)?;
                    let recv_stream = request.into_body();

                    tokio::spawn(async move {
                        if let Err(e) = h2_connect_relay(recv_stream, send_stream, target).await {
                            tracing::warn!("h2 connect relay error: {}", e);
                        }
                    });
                } else {
                    send_response.send_reset(h2::Reason::PROTOCOL_ERROR);
                }
            }
            Some(Err(e)) => {
                return Err(H2ConnectError::H2(e.to_string()));
            }
            None => break,
        }
    }
    Ok(())
}

pub struct H2StreamRead {
    recv: h2::RecvStream,
    buffer: Bytes,
}

impl H2StreamRead {
    pub fn new(recv: h2::RecvStream) -> Self {
        Self {
            recv,
            buffer: Bytes::new(),
        }
    }
}

impl tokio::io::AsyncRead for H2StreamRead {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        if !this.buffer.is_empty() {
            let len = this.buffer.len().min(buf.remaining());
            buf.put_slice(&this.buffer.split_to(len));
            return Poll::Ready(Ok(()));
        }

        let mut data_fut = Box::pin(this.recv.data());
        match data_fut.as_mut().poll(cx) {
            Poll::Ready(Some(Ok(data))) => {
                let len = data.len().min(buf.remaining());
                buf.put_slice(&data[..len]);
                if len < data.len() {
                    this.buffer = data.slice(len..);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(std::io::Error::other(e))),
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn h2_base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Perform an H2 CONNECT handshake as a client.
///
/// Establishes an HTTP/2 connection over the given stream, sends a CONNECT
/// request for the specified target authority, and returns the bidirectional
/// stream pair plus a connection task handle.
///
/// The caller must keep the `JoinHandle` alive (or `.abort()` it) for the
/// duration of the relay — dropping it will close the H2 connection.
pub async fn h2_connect_client<S>(
    stream: S,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
) -> Result<
    (
        h2::SendStream<Bytes>,
        h2::RecvStream,
        tokio::task::JoinHandle<Result<(), h2::Error>>,
    ),
    H2ConnectError,
>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut send_request, conn) = h2::client::handshake(stream).await?;

    let conn_handle = tokio::spawn(async move {
        conn.await?;
        Ok(())
    });

    let authority = match target.port {
        443 => target.host.to_string(),
        port => format!("{}:{}", target.host, port),
    };

    let mut builder = http::Request::builder()
        .method(http::Method::CONNECT)
        .uri(&authority)
        .header(http::header::HOST, &authority);

    if let Some((user, pass)) = auth {
        let credentials = format!("{}:{}", user, pass);
        let encoded = h2_base64_encode(credentials.as_bytes());
        builder = builder.header(
            http::header::PROXY_AUTHORIZATION,
            format!("Basic {}", encoded),
        );
    }

    let request = builder
        .body(())
        .map_err(|e| H2ConnectError::H2(e.to_string()))?;

    let (response_future, send_stream) = send_request.send_request(request, false)?;

    let response = response_future.await?;
    if response.status() != http::StatusCode::OK {
        return Err(H2ConnectError::H2(format!(
            "CONNECT rejected with status {}",
            response.status()
        )));
    }

    let recv_stream = response.into_body();
    Ok((send_stream, recv_stream, conn_handle))
}

// ===== H2 Connection Pool =====

/// Pool key identifying a unique H2 upstream connection group.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct H2PoolKey {
    pub endpoint_host: String,
    pub endpoint_port: u16,
    pub use_tls: bool,
    pub server_name: Option<String>,
    pub auth_hash: Option<u64>,
}

impl H2PoolKey {
    pub fn new(
        host: &str,
        port: u16,
        use_tls: bool,
        server_name: Option<&str>,
        auth: Option<(&str, &str)>,
    ) -> Self {
        let auth_hash = auth.map(|(u, p)| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            u.hash(&mut hasher);
            p.hash(&mut hasher);
            hasher.finish()
        });
        Self {
            endpoint_host: host.to_string(),
            endpoint_port: port,
            use_tls,
            server_name: server_name.map(|s| s.to_string()),
            auth_hash,
        }
    }
}

/// Metadata for a pooled H2 connection.
pub struct H2ConnectionEntry {
    sender: Arc<Mutex<h2::client::SendRequest<Bytes>>>,
    conn_handle: tokio::task::JoinHandle<Result<(), h2::Error>>,
    #[allow(dead_code)]
    created_at: Instant,
    last_used: Arc<Mutex<Instant>>,
    active_streams: Arc<AtomicU64>,
    retired: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl H2ConnectionEntry {
    fn is_available(&self, max_concurrent_streams: u32) -> bool {
        !self.retired.load(Ordering::Acquire)
            && self.active_streams.load(Ordering::Acquire) < max_concurrent_streams as u64
    }

    fn mark_retired(&self) {
        self.retired.store(true, Ordering::Release);
    }
}

impl Drop for H2ConnectionEntry {
    fn drop(&mut self) {
        H2_PROTOCOL_METRICS
            .connections_closed
            .fetch_add(1, Ordering::Relaxed);
        self.conn_handle.abort();
    }
}

/// Bounded H2 connection pool with idle timeout and GOAWAY-aware retirement.
pub struct H2ConnectionPool {
    entries: Mutex<Vec<Arc<H2ConnectionEntry>>>,
    semaphore: Semaphore,
    pool_size: u32,
    idle_timeout: Duration,
    max_concurrent_streams: u32,
    #[allow(dead_code)]
    created_at: Instant,
    reaper_running: AtomicBool,
}

impl H2ConnectionPool {
    pub fn new(pool_size: u32, idle_timeout: Duration, max_concurrent_streams: u32) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(Vec::new()),
            semaphore: Semaphore::new(pool_size as usize),
            pool_size,
            idle_timeout,
            max_concurrent_streams,
            created_at: Instant::now(),
            reaper_running: AtomicBool::new(false),
        })
    }

    /// Try to acquire an existing idle connection from the pool.
    fn try_acquire_entry(&self) -> Option<Arc<H2ConnectionEntry>> {
        let entries = self.entries.lock().unwrap();
        let now = Instant::now();
        for entry in entries.iter() {
            if entry.is_available(self.max_concurrent_streams)
                && now.duration_since(*entry.last_used.lock().unwrap()) < self.idle_timeout
            {
                entry.active_streams.fetch_add(1, Ordering::AcqRel);
                *entry.last_used.lock().unwrap() = now;
                return Some(Arc::clone(entry));
            }
        }
        None
    }

    /// Create a new H2 connection and add it to the pool.
    async fn create_entry<S>(
        self: &Arc<Self>,
        stream: S,
    ) -> Result<Arc<H2ConnectionEntry>, H2ConnectError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (send_request, conn) = h2::client::handshake(stream).await?;

        let conn_handle = tokio::spawn(async move {
            conn.await?;
            Ok(())
        });

        let sender = Arc::new(Mutex::new(send_request));
        let entry = Arc::new(H2ConnectionEntry {
            sender: Arc::clone(&sender),
            conn_handle,
            created_at: Instant::now(),
            last_used: Arc::new(Mutex::new(Instant::now())),
            active_streams: Arc::new(AtomicU64::new(1)),
            retired: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        });

        self.entries.lock().unwrap().push(Arc::clone(&entry));
        H2_PROTOCOL_METRICS
            .connections_opened
            .fetch_add(1, Ordering::Relaxed);
        self.maybe_start_reaper();
        Ok(entry)
    }

    fn maybe_start_reaper(self: &Arc<Self>) {
        if self
            .reaper_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(pool.idle_timeout / 2).await;
                pool.reap_idle_entries();
                if pool.entries.lock().unwrap().is_empty() {
                    pool.reaper_running.store(false, Ordering::Release);
                    return;
                }
            }
        });
    }

    fn reap_idle_entries(&self) {
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|entry| {
            if entry.retired.load(Ordering::Acquire) {
                return false;
            }
            let last_used = *entry.last_used.lock().unwrap();
            if now.duration_since(last_used) >= self.idle_timeout
                && entry.active_streams.load(Ordering::Acquire) == 0
            {
                entry.mark_retired();
                false
            } else {
                true
            }
        });
    }

    /// Release a connection back to the pool after a stream completes.
    pub fn release(&self, entry: &Arc<H2ConnectionEntry>) {
        entry.active_streams.fetch_sub(1, Ordering::AcqRel);
        *entry.last_used.lock().unwrap() = Instant::now();
        entry.notify.notify_waiters();
    }

    /// Mark a connection as retired (e.g., on GOAWAY).
    pub fn retire(&self, entry: &Arc<H2ConnectionEntry>) {
        entry.mark_retired();
    }

    /// Get pool statistics.
    pub fn stats(&self) -> H2PoolStats {
        let entries = self.entries.lock().unwrap();
        let active = entries
            .iter()
            .filter(|e| !e.retired.load(Ordering::Acquire))
            .count();
        let total_streams: u64 = entries
            .iter()
            .map(|e| e.active_streams.load(Ordering::Acquire))
            .sum();
        H2PoolStats {
            pool_size: self.pool_size,
            active_connections: active as u32,
            total_streams,
            idle_timeout_secs: self.idle_timeout.as_secs(),
        }
    }
}

/// Pool statistics snapshot.
#[derive(Debug, Clone)]
pub struct H2PoolStats {
    pub pool_size: u32,
    pub active_connections: u32,
    pub total_streams: u64,
    pub idle_timeout_secs: u64,
}

/// Global H2 connection pool registry, keyed by (endpoint_host, endpoint_port, use_tls, server_name, auth_hash).
pub struct H2PoolRegistry {
    pools: std::sync::RwLock<HashMap<H2PoolKey, Arc<H2ConnectionPool>>>,
    default_pool_size: u32,
    default_idle_timeout: Duration,
    default_max_concurrent_streams: u32,
}

impl H2PoolRegistry {
    pub fn new() -> Self {
        Self {
            pools: std::sync::RwLock::new(HashMap::new()),
            default_pool_size: 4,
            default_idle_timeout: Duration::from_secs(60),
            default_max_concurrent_streams: 100,
        }
    }

    /// Get or create a pool for the given key.
    pub fn get_or_create(&self, key: &H2PoolKey) -> Arc<H2ConnectionPool> {
        {
            let pools = self.pools.read().unwrap();
            if let Some(pool) = pools.get(key) {
                return Arc::clone(pool);
            }
        }
        let mut pools = self.pools.write().unwrap();
        pools
            .entry(key.clone())
            .or_insert_with(|| {
                H2ConnectionPool::new(
                    self.default_pool_size,
                    self.default_idle_timeout,
                    self.default_max_concurrent_streams,
                )
            })
            .clone()
    }

    /// Configure default pool settings.
    pub fn with_defaults(
        pool_size: u32,
        idle_timeout: Duration,
        max_concurrent_streams: u32,
    ) -> Self {
        Self {
            pools: std::sync::RwLock::new(HashMap::new()),
            default_pool_size: pool_size,
            default_idle_timeout: idle_timeout,
            default_max_concurrent_streams: max_concurrent_streams,
        }
    }
}

impl Default for H2PoolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global pool registry instance.
pub static H2_POOL_REGISTRY: once_cell::sync::Lazy<H2PoolRegistry> =
    once_cell::sync::Lazy::new(H2PoolRegistry::new);

/// A guard that releases an H2 connection back to the pool when dropped.
pub struct H2PoolGuard {
    entry: Arc<H2ConnectionEntry>,
    pool: Arc<H2ConnectionPool>,
}

impl Drop for H2PoolGuard {
    fn drop(&mut self) {
        H2_PROTOCOL_METRICS
            .streams_closed
            .fetch_add(1, Ordering::Relaxed);
        self.pool.release(&self.entry);
    }
}

impl H2PoolGuard {
    /// Mark this connection as retired (e.g., on GOAWAY).
    pub fn retire(&self) {
        H2_PROTOCOL_METRICS
            .goaway_received
            .fetch_add(1, Ordering::Relaxed);
        H2_PROTOCOL_METRICS
            .connections_closed
            .fetch_add(1, Ordering::Relaxed);
        self.pool.retire(&self.entry);
    }

    /// Get the sender for creating new streams on this connection.
    pub fn sender(&self) -> &Arc<Mutex<h2::client::SendRequest<Bytes>>> {
        &self.entry.sender
    }
}

/// Perform an H2 CONNECT handshake using a pooled connection.
///
/// Acquires a connection from the pool (or creates a new one), sends a CONNECT
/// request, and returns the bidirectional streams with a pool guard. When the
/// guard is dropped, the connection is released back to the pool.
pub async fn h2_connect_client_pooled<S>(
    stream: S,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
    pool_key: &H2PoolKey,
) -> Result<(h2::SendStream<Bytes>, h2::RecvStream, H2PoolGuard), H2ConnectError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let pool = H2_POOL_REGISTRY.get_or_create(pool_key);

    // Try to acquire an existing connection from the pool
    if let Some(result) = try_pooled_connection(&pool, target, auth).await {
        return result;
    }

    // No available connection — create a new one (semaphore limits total)
    let _permit = pool.semaphore.acquire().await.map_err(|_| {
        H2_PROTOCOL_METRICS
            .pool_exhausted
            .fetch_add(1, Ordering::Relaxed);
        H2ConnectError::PoolExhausted
    })?;

    let entry = pool.create_entry(stream).await?;

    let authority = match target.port {
        443 => target.host.to_string(),
        port => format!("{}:{}", target.host, port),
    };

    let mut builder = http::Request::builder()
        .method(http::Method::CONNECT)
        .uri(&authority)
        .header(http::header::HOST, &authority);

    if let Some((user, pass)) = auth {
        let credentials = format!("{}:{}", user, pass);
        let encoded = h2_base64_encode(credentials.as_bytes());
        builder = builder.header(
            http::header::PROXY_AUTHORIZATION,
            format!("Basic {}", encoded),
        );
    }

    let request = builder
        .body(())
        .map_err(|e| H2ConnectError::H2(e.to_string()))?;

    let (response_future, send_stream) = {
        let mut sender = entry.sender.lock().unwrap();
        sender.send_request(request, false)?
    };

    let response = response_future.await?;
    if response.status() != http::StatusCode::OK {
        pool.retire(&entry);
        if response.status() == http::StatusCode::PROXY_AUTHENTICATION_REQUIRED {
            H2_PROTOCOL_METRICS
                .auth_failures
                .fetch_add(1, Ordering::Relaxed);
        }
        return Err(H2ConnectError::H2(format!(
            "CONNECT rejected with status {}",
            response.status()
        )));
    }

    let recv_stream = response.into_body();
    H2_PROTOCOL_METRICS
        .streams_opened
        .fetch_add(1, Ordering::Relaxed);
    let guard = H2PoolGuard {
        entry: Arc::clone(&entry),
        pool: Arc::clone(&pool),
    };
    Ok((send_stream, recv_stream, guard))
}

/// Try to send a CONNECT request on an existing pooled connection.
/// Returns `Some(result)` if a connection was found, `None` if no connection available.
async fn try_pooled_connection(
    pool: &Arc<H2ConnectionPool>,
    target: &TargetAddr,
    auth: Option<(&str, &str)>,
) -> Option<Result<(h2::SendStream<Bytes>, h2::RecvStream, H2PoolGuard), H2ConnectError>> {
    let entry = pool.try_acquire_entry()?;

    let authority = match target.port {
        443 => target.host.to_string(),
        port => format!("{}:{}", target.host, port),
    };

    let mut builder = http::Request::builder()
        .method(http::Method::CONNECT)
        .uri(&authority)
        .header(http::header::HOST, &authority);

    if let Some((user, pass)) = auth {
        let credentials = format!("{}:{}", user, pass);
        let encoded = h2_base64_encode(credentials.as_bytes());
        builder = builder.header(
            http::header::PROXY_AUTHORIZATION,
            format!("Basic {}", encoded),
        );
    }

    let request = match builder.body(()) {
        Ok(r) => r,
        Err(e) => return Some(Err(H2ConnectError::H2(e.to_string()))),
    };

    let result = {
        let mut sender = entry.sender.lock().unwrap();
        sender.send_request(request, false)
    };

    match result {
        Ok((response_future, send_stream)) => {
            let response = match response_future.await {
                Ok(r) => r,
                Err(e) => {
                    pool.retire(&entry);
                    return Some(Err(e.into()));
                }
            };
            if response.status() != http::StatusCode::OK {
                pool.retire(&entry);
                if response.status() == http::StatusCode::PROXY_AUTHENTICATION_REQUIRED {
                    H2_PROTOCOL_METRICS
                        .auth_failures
                        .fetch_add(1, Ordering::Relaxed);
                }
                return Some(Err(H2ConnectError::H2(format!(
                    "CONNECT rejected with status {}",
                    response.status()
                ))));
            }
            H2_PROTOCOL_METRICS
                .streams_opened
                .fetch_add(1, Ordering::Relaxed);
            let recv_stream = response.into_body();
            let guard = H2PoolGuard {
                entry: Arc::clone(&entry),
                pool: Arc::clone(pool),
            };
            Some(Ok((send_stream, recv_stream, guard)))
        }
        Err(_) => {
            // GOAWAY or connection error — retire this entry, fall through to new connection
            pool.retire(&entry);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h2_connect_error_display() {
        let err = H2ConnectError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "test",
        ));
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_h2_connect_error_from_h2() {
        let err = H2ConnectError::H2("test error".into());
        assert_eq!(err.to_string(), "H2 protocol error: test error");
    }

    #[test]
    fn test_h2_connect_error_display_variants() {
        let err = H2ConnectError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken",
        ));
        assert!(err.to_string().contains("broken"));

        let err = H2ConnectError::H2("stream reset".into());
        assert!(err.to_string().contains("stream reset"));
    }

    #[test]
    fn test_h2_connect_error_from_std_io() {
        let io_err = std::io::Error::other("test io");
        let err: H2ConnectError = io_err.into();
        assert!(matches!(err, H2ConnectError::Io(_)));
    }

    #[test]
    fn test_h2_connect_error_pool_exhausted() {
        let err = H2ConnectError::PoolExhausted;
        assert!(err.to_string().contains("pool exhausted"));
    }

    #[test]
    fn test_pool_key_equality() {
        let k1 = H2PoolKey::new("127.0.0.1", 8080, false, None, None);
        let k2 = H2PoolKey::new("127.0.0.1", 8080, false, None, None);
        assert_eq!(k1, k2);

        let k3 = H2PoolKey::new("127.0.0.1", 8080, true, None, None);
        assert_ne!(k1, k3);

        let k4 = H2PoolKey::new("127.0.0.1", 8080, false, Some("sni.example.com"), None);
        assert_ne!(k1, k4);
    }

    #[test]
    fn test_pool_key_auth_hash() {
        let k1 = H2PoolKey::new("h", 1, false, None, Some(("u", "p")));
        let k2 = H2PoolKey::new("h", 1, false, None, Some(("u", "p")));
        let k3 = H2PoolKey::new("h", 1, false, None, Some(("u", "q")));
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_pool_stats() {
        let pool = H2ConnectionPool::new(4, Duration::from_secs(60), 100);
        let stats = pool.stats();
        assert_eq!(stats.pool_size, 4);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.total_streams, 0);
    }

    #[test]
    fn test_pool_registry_get_or_create() {
        let registry = H2PoolRegistry::new();
        let key = H2PoolKey::new("127.0.0.1", 8080, false, None, None);
        let p1 = registry.get_or_create(&key);
        let p2 = registry.get_or_create(&key);
        assert!(Arc::ptr_eq(&p1, &p2));

        let key2 = H2PoolKey::new("127.0.0.1", 9090, false, None, None);
        let p3 = registry.get_or_create(&key2);
        assert!(!Arc::ptr_eq(&p1, &p3));
    }

    #[tokio::test]
    async fn test_handle_h2_connect_accepts() {
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = server_listener.accept().await.unwrap();
            let conn = h2::server::handshake(stream).await.unwrap();
            handle_h2_connect(conn).await.ok();
        });

        let client_stream = TcpStream::connect(server_addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(client_stream).await.unwrap();

        let conn_handle = tokio::spawn(async move {
            conn.await.ok();
        });

        let request = http::Request::builder()
            .method(http::Method::CONNECT)
            .uri("127.0.0.1:9999")
            .body(())
            .unwrap();

        let (response_future, _send_stream) = send_request.send_request(request, true).unwrap();

        let response = tokio::time::timeout(std::time::Duration::from_secs(3), response_future)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);

        drop(send_request);
        drop(_send_stream);
        conn_handle.abort();
        server_handle.abort();
    }

    // NOTE: Connection reuse is tested at the integration level in
    // upstream_protocols.rs::h2_upstream_connection_reuse which exercises the
    // full stack through the ServiceSupervisor.
    //
    // RST_STREAM and GOAWAY fault injection tests are at the integration level
    // in upstream_protocols.rs::h2_upstream_rst_stream_recovery and
    // h2_upstream_goaway_recovery.
}
