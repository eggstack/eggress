use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::net::TcpStream;

#[derive(Debug, Error)]
pub enum PproxyOracleError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("process failed: {0}")]
    ProcessFailed(String),

    #[error("startup timeout after {0:?}")]
    StartupTimeout(Duration),

    #[error("not ready: {0}")]
    NotReady(String),

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },

    #[error("version detection failed: {0}")]
    VersionDetectionFailed(String),
}

#[derive(Debug, Clone)]
pub struct OracleConfig {
    pub python_binary: String,
    pub pproxy_version: String,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub io_timeout: Duration,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            python_binary: "python3".to_string(),
            pproxy_version: "2.7.9".to_string(),
            startup_timeout: Duration::from_secs(15),
            shutdown_timeout: Duration::from_secs(5),
            io_timeout: Duration::from_secs(3),
        }
    }
}

pub struct PproxyProcess {
    child: Option<Child>,
    bound_addr: SocketAddr,
    stdout_buf: Arc<Mutex<Vec<u8>>>,
    stderr_buf: Arc<Mutex<Vec<u8>>>,
    #[allow(dead_code)]
    work_dir: Option<tempfile::TempDir>,
}

impl PproxyProcess {
    pub async fn start(config: &OracleConfig, args: &[String]) -> Result<Self, PproxyOracleError> {
        let work_dir = tempfile::TempDir::new().map_err(PproxyOracleError::Io)?;

        let stdout_buf = Arc::new(Mutex::new(Vec::new()));
        let stderr_buf = Arc::new(Mutex::new(Vec::new()));

        let stdout_clone = Arc::clone(&stdout_buf);
        let stderr_clone = Arc::clone(&stderr_buf);

        let mut child = Command::new(&config.python_binary)
            .arg("-m")
            .arg("pproxy")
            .args(args)
            .current_dir(work_dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(PproxyOracleError::Io)?;

        let child_stdout = child.stdout.take().expect("stdout piped");
        let child_stderr = child.stderr.take().expect("stderr piped");

        std::thread::spawn(move || {
            let mut reader = child_stderr;
            let mut tmp = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut guard) = stderr_clone.lock() {
                            guard.extend_from_slice(&tmp[..n]);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        std::thread::spawn(move || {
            let mut reader = child_stdout;
            let mut tmp = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut guard) = stdout_clone.lock() {
                            guard.extend_from_slice(&tmp[..n]);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let bound_addr = if let Some(addr) = parse_listen_addr_from_args(args) {
            addr
        } else {
            wait_for_output_ready(&stderr_buf, &stdout_buf, config.startup_timeout).await?
        };

        let proc = Self {
            child: Some(child),
            bound_addr,
            stdout_buf,
            stderr_buf,
            work_dir: Some(work_dir),
        };

        proc.wait_ready(config).await?;

        Ok(proc)
    }

    pub async fn wait_ready(&self, config: &OracleConfig) -> Result<(), PproxyOracleError> {
        let start = Instant::now();
        let interval = Duration::from_millis(100);

        loop {
            match TcpStream::connect(self.bound_addr).await {
                Ok(_) => return Ok(()),
                Err(_) if start.elapsed() < config.startup_timeout => {
                    tokio::time::sleep(interval).await;
                }
                Err(e) => {
                    return Err(PproxyOracleError::NotReady(format!(
                        "tcp connect to {} failed: {}",
                        self.bound_addr, e
                    )));
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
    }

    pub fn stdout(&self) -> Vec<u8> {
        self.stdout_buf
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn stderr(&self) -> Vec<u8> {
        self.stderr_buf
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn bound_addr(&self) -> SocketAddr {
        self.bound_addr
    }

    pub fn redacted_stderr(&self) -> String {
        let raw = self.stderr();
        redact_credentials(&raw)
    }
}

impl Drop for PproxyProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn redact_credentials(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    redact_uri_credentials(&text)
}

/// Scrub proxy credentials from free-form oracle transcript text.
///
/// Delegates to the canonical tolerant redactor
/// ([`eggress_uri::redact_proxy_uri`]) so every credential-bearing scheme is
/// covered without maintaining a parallel scheme whitelist here. The match is
/// deliberately generic: any `scheme://...@...` userinfo is stripped while
/// credential-free text passes through unchanged.
fn redact_uri_credentials(text: &str) -> String {
    // Fast path: skip allocation-free scan when no URI-like content exists.
    if !text.contains("://") && !text.contains('@') {
        return text.to_string();
    }
    // The canonical redactor handles one URI-like string; transcripts may
    // embed several across whitespace, so apply it per whitespace-separated
    // token to avoid one malformed span suppressing the rest.
    // (`split_inclusive` keeps each delimiter attached to its token, so
    // concatenation reconstructs the input exactly.)
    let mut out = String::with_capacity(text.len());
    for token in text.split_inclusive(char::is_whitespace) {
        out.push_str(&eggress_uri::redact_proxy_uri(token));
    }
    out
}

async fn wait_for_output_ready(
    stderr_buf: &Arc<Mutex<Vec<u8>>>,
    stdout_buf: &Arc<Mutex<Vec<u8>>>,
    timeout: Duration,
) -> Result<SocketAddr, PproxyOracleError> {
    let start = Instant::now();
    let interval = Duration::from_millis(100);

    loop {
        {
            let guard = stderr_buf.lock().map_err(|e| {
                PproxyOracleError::ProcessFailed(format!("stderr lock poisoned: {}", e))
            })?;
            let text = String::from_utf8_lossy(&guard);
            if let Some(addr) = parse_bound_addr(&text) {
                return Ok(addr);
            }
        }
        {
            let guard = stdout_buf.lock().map_err(|e| {
                PproxyOracleError::ProcessFailed(format!("stdout lock poisoned: {}", e))
            })?;
            let text = String::from_utf8_lossy(&guard);
            if let Some(addr) = parse_bound_addr(&text) {
                return Ok(addr);
            }
        }

        if start.elapsed() >= timeout {
            let stderr_text = stderr_buf
                .lock()
                .map(|g| String::from_utf8_lossy(&g).trim().to_string())
                .unwrap_or_default();
            let stdout_text = stdout_buf
                .lock()
                .map(|g| String::from_utf8_lossy(&g).trim().to_string())
                .unwrap_or_default();
            return Err(PproxyOracleError::ProcessFailed(format!(
                "startup timeout after {:?}, stderr: [{}], stdout: [{}]",
                timeout, stderr_text, stdout_text
            )));
        }

        tokio::time::sleep(interval).await;
    }
}

fn parse_listen_addr_from_args(args: &[String]) -> Option<SocketAddr> {
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "-l" || args[i] == "--listen") && i + 1 < args.len() {
            let uri = &args[i + 1];
            let host_port = if let Some(at_pos) = uri.rfind('@') {
                &uri[at_pos + 1..]
            } else {
                uri.as_str()
            };
            let stripped = host_port
                .strip_prefix("socks5://")
                .or_else(|| host_port.strip_prefix("http://"))
                .or_else(|| host_port.strip_prefix("socks4://"))
                .or_else(|| host_port.strip_prefix("socks4a://"))
                .or_else(|| host_port.strip_prefix("ss://"))
                .or_else(|| host_port.strip_prefix("trojan://"))
                .or_else(|| host_port.strip_prefix("direct://"))
                .unwrap_or(host_port);
            if let Some(colon_pos) = stripped.rfind(':') {
                let port_str = &stripped[colon_pos + 1..];
                if let Ok(port) = port_str.parse::<u16>() {
                    if port > 0 {
                        let host_str = &stripped[..colon_pos];
                        let host = if host_str.is_empty() || host_str == "127.0.0.1" {
                            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                        } else if host_str == "0.0.0.0" {
                            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
                        } else {
                            host_str.parse().ok()?
                        };
                        return Some(SocketAddr::new(host, port));
                    }
                }
            }
            return None;
        }
        i += 1;
    }
    None
}

fn parse_bound_addr(text: &str) -> Option<SocketAddr> {
    // (needle in oracle output, resulting IP). IPv6 literals are bracketed
    // exactly as they appear in pproxy's startup banner.
    const PATTERNS: [(&str, std::net::IpAddr); 4] = [
        (
            "127.0.0.1:",
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ),
        (
            "0.0.0.0:",
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        ),
        (
            "[::1]:",
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ),
        (
            "[::]:",
            std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
        ),
    ];

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        for (needle, ip) in PATTERNS {
            if let Some(idx) = line.find(needle) {
                let port_str = &line[idx + needle.len()..];
                let port_str = port_str
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>();
                if let Ok(port) = port_str.parse::<u16>() {
                    if port > 0 {
                        return Some(SocketAddr::new(ip, port));
                    }
                }
            }
        }
    }

    None
}

pub async fn verify_pproxy_version(config: &OracleConfig) -> Result<String, PproxyOracleError> {
    let output = Command::new(&config.python_binary)
        .args([
            "-c",
            "import pproxy; print(getattr(pproxy, '__version__', 'unknown'))",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(PproxyOracleError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PproxyOracleError::VersionDetectionFailed(
            stderr.trim().to_string(),
        ));
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(version)
}

pub async fn assert_pproxy_version(config: &OracleConfig) -> Result<(), PproxyOracleError> {
    let actual = verify_pproxy_version(config).await?;
    if actual != config.pproxy_version && actual != "unknown" {
        return Err(PproxyOracleError::VersionMismatch {
            expected: config.pproxy_version.clone(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn require_pproxy() -> bool {
        std::process::Command::new("python3")
            .args(["-c", "import pproxy"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    #[ignore]
    async fn test_process_guard_drop_kills_child() {
        if !require_pproxy() {
            eprintln!("pproxy not available, skipping");
            return;
        }

        let config = OracleConfig::default();
        let port = crate::get_free_port().await;
        let listen = format!("socks5://127.0.0.1:{}", port);
        let args = vec![
            "-l".to_string(),
            listen,
            "-r".to_string(),
            "direct".to_string(),
        ];

        let proc = PproxyProcess::start(&config, &args).await.unwrap();
        let addr = proc.bound_addr();
        assert_eq!(addr.port(), port, "should parse port from args");

        drop(proc);

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = TcpStream::connect(addr).await;
        assert!(result.is_err(), "process should be dead after drop");
    }

    #[tokio::test]
    #[ignore]
    async fn test_readiness_probe() {
        if !require_pproxy() {
            eprintln!("pproxy not available, skipping");
            return;
        }

        let config = OracleConfig::default();
        let port = crate::get_free_port().await;
        let listen = format!("socks5://127.0.0.1:{}", port);
        let args = vec![
            "-l".to_string(),
            listen,
            "-r".to_string(),
            "direct".to_string(),
        ];

        let proc = PproxyProcess::start(&config, &args).await.unwrap();

        let result = TcpStream::connect(proc.bound_addr()).await;
        assert!(result.is_ok(), "process should be ready after start");

        drop(proc);
    }

    #[test]
    fn test_log_redaction() {
        let input = b"socks5://user:secret123@127.0.0.1:1080\nhttp://admin:pw0rd@0.0.0.0:8080\n";
        let redacted = redact_credentials(input);

        assert!(!redacted.contains("secret123"));
        assert!(!redacted.contains("pw0rd"));
        assert!(!redacted.contains("user:"), "username must be redacted");
        assert!(!redacted.contains("admin:"), "username must be redacted");
        assert!(redacted.contains("****@127.0.0.1:1080"));
        assert!(redacted.contains("****@0.0.0.0:8080"));
    }

    #[test]
    fn test_log_redaction_no_credentials() {
        let input = b"socks5://127.0.0.1:1080\nlistening on port 8080\n";
        let redacted = redact_credentials(input);

        assert_eq!(redacted, String::from_utf8_lossy(input));
    }

    #[test]
    fn test_parse_bound_addr() {
        assert_eq!(
            parse_bound_addr("Listen: socks5://127.0.0.1:9090"),
            Some(SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                9090
            ))
        );

        assert_eq!(
            parse_bound_addr("Listen: http://0.0.0.0:8080"),
            Some(SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                8080
            ))
        );

        assert_eq!(parse_bound_addr("no address here"), None);
        assert_eq!(parse_bound_addr(""), None);

        assert_eq!(
            parse_bound_addr("Listen: socks5://[::1]:9090"),
            Some(SocketAddr::new(
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
                9090
            ))
        );

        assert_eq!(
            parse_bound_addr("Listen: http://[::]:8080"),
            Some(SocketAddr::new(
                std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
                8080
            ))
        );
    }

    #[test]
    fn test_redact_uri_credentials() {
        assert_eq!(
            redact_uri_credentials("socks5://user:pass@host:1080"),
            "socks5://****@host:1080"
        );

        assert_eq!(
            redact_uri_credentials("http://admin:secret@proxy:8080"),
            "http://****@proxy:8080"
        );

        assert_eq!(
            redact_uri_credentials("socks5://127.0.0.1:1080"),
            "socks5://127.0.0.1:1080"
        );

        // Schemes outside the old hardcoded whitelist are covered by the
        // canonical redactor.
        assert_eq!(
            redact_uri_credentials("ssh://deploy:s3cret@build.internal:22"),
            "ssh://****@build.internal:22"
        );
        assert_eq!(
            redact_uri_credentials("h3://user:pass@[2001:db8::1]:443"),
            "h3://****@[2001:db8::1]:443"
        );

        // Multi-token transcripts redact each credentialed token.
        assert_eq!(
            redact_uri_credentials("dial socks5://a:b@h1:1080 then http://c:d@h2:8080 ok"),
            "dial socks5://****@h1:1080 then http://****@h2:8080 ok"
        );
    }

    #[tokio::test]
    async fn test_version_detection() {
        if !require_pproxy() {
            eprintln!("pproxy not available, skipping");
            return;
        }

        let config = OracleConfig::default();
        let result = verify_pproxy_version(&config).await;
        assert!(
            result.is_ok(),
            "version detection should succeed: {:?}",
            result.err()
        );
    }
}
