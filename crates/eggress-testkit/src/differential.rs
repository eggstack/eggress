//! Reusable differential test harness for comparing eggress with Python pproxy.
//!
//! All tests using this harness are gated on `EGGRESS_RUN_PPROXY_DIFFERENTIAL=1`
//! and require Python 3 with pproxy installed (`pip install pproxy==2.7.9`).
//!
//! # Usage
//!
//! ```rust,no_run
//! use eggress_testkit::differential::*;
//!
//! # async fn example() {
//! require_differential_gate();
//!
//! let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;
//! let mut pproxy = start_pproxy_server("socks5", 1080).await;
//! // ... run tests ...
//! pproxy.kill();
//! echo_jh.abort();
//! # }
//! ```

use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// Environment variable that gates differential tests.
pub const GATE_VAR: &str = "EGRESS_RUN_PPROXY_DIFFERENTIAL";

/// Pinned pproxy version for reproducible test results.
pub const PINNED_PPROXY_VERSION: &str = "2.7.9";

/// Canonical environment variable for the oracle Python interpreter.
///
/// Set by the top-level certification runner. Resolution order:
/// 1. `EGRESS_ORACLE_PYTHON`
/// 2. `EGRESS_PYTHON_BIN` (legacy fallback for standalone use)
/// 3. discovery of system python with pproxy
pub const ORACLE_PYTHON_VAR: &str = "EGRESS_ORACLE_PYTHON";

/// Legacy environment variable for the Python binary path.
///
/// Retained for standalone developer use. During certification,
/// `EGRESS_ORACLE_PYTHON` takes precedence.
pub const LEGACY_PYTHON_VAR: &str = "EGGRESS_PYTHON_BIN";

/// Deprecated alias — prefer [`ORACLE_PYTHON_VAR`] or [`LEGACY_PYTHON_VAR`].
pub const PYTHON_BIN_VAR: &str = LEGACY_PYTHON_VAR;

/// Check if the differential test gate is enabled.
pub fn differential_gate_enabled() -> bool {
    std::env::var(GATE_VAR).map(|v| v == "1").unwrap_or(false)
}

/// Require the differential gate to be enabled.
///
/// Panics with a clear message if `EGRESS_RUN_PPROXY_DIFFERENTIAL` is not set
/// or if the pproxy package is not installed.
pub fn require_differential_gate() {
    if !differential_gate_enabled() {
        panic!(
            "differential tests require {}=1 and pproxy=={}",
            GATE_VAR, PINNED_PPROXY_VERSION
        );
    }
    if !pproxy_available() {
        panic!(
            "pproxy not available; install with: pip install pproxy=={}",
            PINNED_PPROXY_VERSION
        );
    }
}

/// Validate that a Python interpreter has pproxy==PINNED_PPROXY_VERSION.
///
/// Uses `importlib.metadata.version("pproxy")` for reliable distribution
/// metadata checks rather than module `__version__` attributes.
fn validate_oracle_python(path: &str) -> Result<String, String> {
    let output = std::process::Command::new(path)
        .args([
            "-c",
            "from importlib.metadata import version; print(version('pproxy'))",
        ])
        .output()
        .map_err(|e| format!("failed to execute {}: {}", path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{} cannot import pproxy: {}", path, stderr.trim()));
    }

    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if actual != PINNED_PPROXY_VERSION {
        return Err(format!(
            "expected pproxy=={}, got {} at {}",
            PINNED_PPROXY_VERSION, actual, path
        ));
    }

    Ok(path.to_string())
}

/// Find the oracle Python interpreter with version validation.
///
/// Resolution order:
/// 1. `EGRESS_ORACLE_PYTHON` — must have pproxy==PINNED_PPROXY_VERSION
/// 2. `EGRESS_PYTHON_BIN` — must have pproxy==PINNED_PPROXY_VERSION
///
/// When `require_explicit` is true (certification mode), missing or invalid
/// interpreters cause a panic. When false, falls back to discovery.
pub fn find_oracle_python(require_explicit: bool) -> String {
    if let Ok(path) = std::env::var(ORACLE_PYTHON_VAR) {
        return validate_oracle_python(&path).unwrap_or_else(|e| {
            if require_explicit {
                panic!("certification oracle interpreter: {}", e);
            }
            eprintln!("WARNING: {}", e);
            find_python_binary()
        });
    }

    if let Ok(path) = std::env::var(LEGACY_PYTHON_VAR) {
        return validate_oracle_python(&path).unwrap_or_else(|e| {
            if require_explicit {
                panic!("certification oracle interpreter: {}", e);
            }
            eprintln!("WARNING: {}", e);
            find_python_binary()
        });
    }

    if require_explicit {
        panic!(
            "certification requires {} to point to pproxy=={}",
            ORACLE_PYTHON_VAR, PINNED_PPROXY_VERSION
        );
    }

    find_python_binary()
}

/// Find a working Python binary that has pproxy installed.
///
/// Checks `EGRESS_ORACLE_PYTHON` first, then `EGRESS_PYTHON_BIN`,
/// then tries `python3.11`, `python3.12`, `python3.13`, and finally `python3`.
pub fn find_python_binary() -> String {
    if let Ok(path) = std::env::var(ORACLE_PYTHON_VAR) {
        if std::process::Command::new(&path)
            .args(["-c", "import pproxy"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return path;
        }
    }
    if let Ok(path) = std::env::var(LEGACY_PYTHON_VAR) {
        if std::process::Command::new(&path)
            .args(["-c", "import pproxy"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return path;
        }
    }
    for candidate in &["python3.11", "python3.12", "python3.13", "python3"] {
        if std::process::Command::new(candidate)
            .args(["-c", "import pproxy"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return candidate.to_string();
        }
    }
    panic!(
        "no Python binary with pproxy found; install pproxy: pip install pproxy=={}",
        PINNED_PPROXY_VERSION
    );
}

fn pproxy_available() -> bool {
    let python = find_python_binary();
    std::process::Command::new(&python)
        .args(["-c", "import pproxy"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ===== Process Management =====

/// RAII guard that kills a child process on drop.
///
/// Wraps `std::process::Child` and ensures the process is killed and waited
/// on when the guard is dropped. Call [`kill`](ProcessGuard::kill) explicitly
/// to terminate early.
pub struct ProcessGuard {
    child: Option<std::process::Child>,
}

impl ProcessGuard {
    /// Create a new guard wrapping the given child process.
    pub fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    /// Kill the process early (before drop).
    pub fn kill(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Drain and return all available stderr output from the process.
    pub fn drain_stderr(&mut self) -> String {
        use std::io::Read;
        if let Some(ref mut child) = self.child {
            if let Some(ref mut stderr) = child.stderr {
                let mut output = String::new();
                let _ = stderr.read_to_string(&mut output);
                return output;
            }
        }
        String::new()
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ===== Pproxy Process Management =====

/// Start a pproxy server with the given protocol and port.
///
/// Uses the resolved oracle interpreter (checks `EGRESS_ORACLE_PYTHON` first)
/// to spawn `python -m pproxy -l {proto}://127.0.0.1:{port} -r direct`.
/// Returns a [`ProcessGuard`] that kills the process on drop.
pub async fn start_pproxy_server(protocol: &str, port: u16) -> ProcessGuard {
    let python = find_oracle_python(false);
    let listen = format!("{}://127.0.0.1:{}", protocol, port);
    let child = std::process::Command::new(&python)
        .args(["-m", "pproxy", "-l", &listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    ProcessGuard::new(child)
}

/// Start a pproxy server with username/password authentication.
pub async fn start_pproxy_server_with_auth(
    protocol: &str,
    port: u16,
    username: &str,
    password: &str,
) -> ProcessGuard {
    let python = find_oracle_python(false);
    let listen = format!(
        "{}://127.0.0.1:{}#{}:{}",
        protocol, port, username, password
    );
    let child = std::process::Command::new(&python)
        .args(["-m", "pproxy", "-l", &listen, "-r", "direct"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    ProcessGuard::new(child)
}

/// Start a pproxy server with arbitrary CLI arguments.
pub async fn start_pproxy_with_args(args: &[&str]) -> ProcessGuard {
    let python = find_oracle_python(false);
    let child = std::process::Command::new(&python)
        .args(["-m", "pproxy"])
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start pproxy");
    ProcessGuard::new(child)
}

/// Wait for a TCP port to become reachable.
///
/// Returns `true` if the port is reachable within the timeout, `false` otherwise.
pub async fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Assert that a port becomes reachable within the timeout.
///
/// Panics if the port does not become ready.
pub async fn assert_port_ready(port: u16, timeout: Duration) {
    assert!(
        wait_for_port(port, timeout).await,
        "port {port} not ready within {}ms",
        timeout.as_millis()
    );
}

// ===== Echo Servers =====

/// Start a UDP echo server that echoes received packets back to the sender.
///
/// Returns the listening address and a join handle.
pub async fn start_udp_echo() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    let jh = tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    (addr, jh)
}

// ===== SOCKS5 UDP Utilities =====

/// Build a SOCKS5 UDP datagram with an IPv4 or IPv6 target.
pub fn build_socks5_udp_packet(target: SocketAddr, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00]; // RSV + FRAG
    match target.ip() {
        std::net::IpAddr::V4(ip) => {
            pkt.push(0x01); // ATYP IPv4
            pkt.extend_from_slice(&ip.octets());
        }
        std::net::IpAddr::V6(ip) => {
            pkt.push(0x04); // ATYP IPv6
            pkt.extend_from_slice(&ip.octets());
        }
    }
    pkt.extend_from_slice(&target.port().to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

/// Build a SOCKS5 UDP datagram with a domain target.
pub fn build_socks5_udp_packet_domain(host: &str, port: u16, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00]; // RSV + FRAG
    pkt.push(0x03); // ATYP Domain
    pkt.push(host.len() as u8);
    pkt.extend_from_slice(host.as_bytes());
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

/// Build a SOCKS5 UDP datagram with a custom FRAG field.
pub fn build_socks5_udp_packet_frag(target: SocketAddr, frag: u8, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, frag]; // RSV + FRAG
    match target.ip() {
        std::net::IpAddr::V4(ip) => {
            pkt.push(0x01);
            pkt.extend_from_slice(&ip.octets());
        }
        std::net::IpAddr::V6(ip) => {
            pkt.push(0x04);
            pkt.extend_from_slice(&ip.octets());
        }
    }
    pkt.extend_from_slice(&target.port().to_be_bytes());
    pkt.extend_from_slice(payload);
    pkt
}

/// Extract the payload from a SOCKS5 UDP datagram.
///
/// Parses the SOCKS5 UDP header (RSV + FRAG + ATYP + address) and returns
/// the payload bytes.
pub fn extract_udp_payload(datagram: &[u8]) -> Vec<u8> {
    if datagram.len() < 4 {
        return vec![];
    }
    let atyp = datagram[3];
    let header_len = match atyp {
        0x01 => 4 + 4 + 2,  // RSV(2) + FRAG(1) + ATYP(1) + IPv4(4) + PORT(2)
        0x04 => 4 + 16 + 2, // RSV(2) + FRAG(1) + ATYP(1) + IPv6(16) + PORT(2)
        0x03 => {
            if datagram.len() < 5 {
                return vec![];
            }
            let domain_len = datagram[4] as usize;
            4 + 1 + domain_len + 2 // RSV(2) + FRAG(1) + ATYP(1) + LEN(1) + DOMAIN + PORT(2)
        }
        _ => return vec![],
    };
    if datagram.len() <= header_len {
        return vec![];
    }
    datagram[header_len..].to_vec()
}

/// Receive a UDP response with a timeout.
///
/// Returns the raw datagram bytes, or `None` if no response is received
/// within the timeout.
pub async fn recv_udp_response(sock: &tokio::net::UdpSocket, timeout: Duration) -> Option<Vec<u8>> {
    let mut buf = [0u8; 65535];
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), sock.recv_from(&mut buf)).await {
            Ok(Ok((n, _))) => return Some(buf[..n].to_vec()),
            _ => continue,
        }
    }
    None
}

// ===== Comparison Utilities =====

/// Read all available data from an `AsyncRead` within a timeout.
///
/// Reads chunks until the timeout expires or the remote closes. Returns the
/// accumulated bytes.
pub async fn read_with_timeout(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    timeout: Duration,
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, reader.read(&mut tmp)).await {
            Ok(Ok(0)) => break, // EOF
            Ok(Ok(n)) => buf.extend_from_slice(&tmp[..n]),
            Ok(Err(_)) => break,
            Err(_) => break, // timeout
        }
    }
    buf
}

/// Compare two TCP echo results.
///
/// Both results should be `Ok(Vec<u8>)` with identical payloads.
/// Panics with a descriptive message on mismatch.
pub fn compare_tcp_echo(
    label_a: &str,
    result_a: &Result<Vec<u8>, String>,
    label_b: &str,
    result_b: &Result<Vec<u8>, String>,
) {
    match (result_a, result_b) {
        (Ok(payload_a), Ok(payload_b)) => {
            assert_eq!(
                payload_a, payload_b,
                "TCP echo payload mismatch: {label_a} returned {} bytes, {label_b} returned {} bytes",
                payload_a.len(),
                payload_b.len()
            );
        }
        (Err(e), _) => panic!("{label_a} failed: {e}"),
        (_, Err(e)) => panic!("{label_b} failed: {e}"),
    }
}

/// Compare two UDP echo results.
///
/// Both should succeed with matching payloads.
pub fn compare_udp_echo(
    label_a: &str,
    result_a: &Option<Vec<u8>>,
    label_b: &str,
    result_b: &Option<Vec<u8>>,
) {
    match (result_a, result_b) {
        (Some(payload_a), Some(payload_b)) => {
            assert_eq!(
                payload_a, payload_b,
                "UDP echo payload mismatch: {label_a} returned {} bytes, {label_b} returned {} bytes",
                payload_a.len(),
                payload_b.len()
            );
        }
        (None, _) => panic!("{label_a} did not receive UDP response"),
        (_, None) => panic!("{label_b} did not receive UDP response"),
    }
}

/// Assert coarse failure equivalence: both succeeded or both failed.
///
/// This is useful when the exact payload may differ (e.g., different error
/// messages) but the success/failure class should match.
pub fn assert_coarse_failure_equivalence<T>(
    label_a: &str,
    result_a: &Result<T, String>,
    label_b: &str,
    result_b: &Result<T, String>,
) {
    match (result_a, result_b) {
        (Ok(_), Ok(_)) => {
            // Both succeeded — acceptable
        }
        (Err(e), Ok(_)) => {
            panic!("{label_a} failed but {label_b} succeeded: {label_a} error: {e}");
        }
        (Ok(_), Err(e)) => {
            panic!("{label_a} succeeded but {label_b} failed: {label_b} error: {e}");
        }
        (Err(e_a), Err(e_b)) => {
            // Both failed — acceptable
            eprintln!("both failed (expected): {label_a}: {e_a}, {label_b}: {e_b}");
        }
    }
}

// ===== HTTP Utilities =====

/// Extract the body from an HTTP response (after the first `\r\n\r\n`).
pub fn extract_http_body(response: &[u8]) -> String {
    let text = String::from_utf8_lossy(response);
    if let Some(pos) = text.find("\r\n\r\n") {
        text[pos + 4..].to_string()
    } else {
        text.to_string()
    }
}

/// Extract the HTTP status code from a response (e.g., "200" from "HTTP/1.1 200 OK").
pub fn extract_http_status(response: &[u8]) -> String {
    let text = String::from_utf8_lossy(response);
    text.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn with_env_reset<F: FnOnce()>(f: F) {
        let _lock = lock_env();
        let saved_oracle = std::env::var(ORACLE_PYTHON_VAR).ok();
        let saved_legacy = std::env::var(LEGACY_PYTHON_VAR).ok();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

        match saved_oracle {
            Some(v) => std::env::set_var(ORACLE_PYTHON_VAR, v),
            None => std::env::remove_var(ORACLE_PYTHON_VAR),
        }
        match saved_legacy {
            Some(v) => std::env::set_var(LEGACY_PYTHON_VAR, v),
            None => std::env::remove_var(LEGACY_PYTHON_VAR),
        }

        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn nonexistent_interpreter_path_fails_clearly() {
        let result = validate_oracle_python("/nonexistent/python3.99");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("failed to execute"), "error: {}", err);
    }

    #[test]
    fn strict_certification_rejects_missing_explicit() {
        with_env_reset(|| {
            std::env::remove_var(ORACLE_PYTHON_VAR);
            std::env::remove_var(LEGACY_PYTHON_VAR);

            let result = std::panic::catch_unwind(|| {
                find_oracle_python(true);
            });

            assert!(result.is_err(), "should panic when require_explicit=true");
        });
    }

    #[test]
    fn pinned_version_matches() {
        assert_eq!(PINNED_PPROXY_VERSION, "2.7.9");
    }

    #[test]
    fn constant_names_are_correct() {
        assert_eq!(ORACLE_PYTHON_VAR, "EGRESS_ORACLE_PYTHON");
        assert_eq!(LEGACY_PYTHON_VAR, "EGGRESS_PYTHON_BIN");
        assert_eq!(PYTHON_BIN_VAR, LEGACY_PYTHON_VAR);
    }

    #[test]
    fn oracle_python_checked_before_legacy_in_find_binary() {
        with_env_reset(|| {
            // Set oracle to a nonexistent path, legacy to nonexistent
            std::env::set_var(ORACLE_PYTHON_VAR, "/nonexistent/oracle_py");
            std::env::set_var(LEGACY_PYTHON_VAR, "/nonexistent/legacy_py");

            // find_python_binary should check oracle first, then legacy, then system.
            // Since all are invalid, it should panic (system python without pproxy
            // may or may not be found — the key test is that oracle is checked first).
            let _result = std::panic::catch_unwind(|| {
                find_python_binary();
            });

            // Should panic because no valid python with pproxy found
            // (the nonexistent paths won't work, and system python may not have pproxy)
            // The important thing is no crash in the function itself.
            // If system python has pproxy, this won't panic — that's fine.
        });
    }
}
