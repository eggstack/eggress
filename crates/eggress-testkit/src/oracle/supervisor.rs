use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub max_capture_lines: usize,
    pub max_line_bytes: usize,
    pub startup_timeout: Duration,
    pub scenario_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub artifact_dir: Option<PathBuf>,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            max_capture_lines: 1000,
            max_line_bytes: 4096,
            startup_timeout: Duration::from_secs(5),
            scenario_timeout: Duration::from_secs(15),
            shutdown_timeout: Duration::from_secs(3),
            artifact_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessExit {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub lifetime_ms: u64,
}

pub struct SupervisedProcess {
    child: Option<Child>,
    pid: u32,
    stdout_lines: Vec<String>,
    stderr_lines: Vec<String>,
    start_time: Instant,
    config: SupervisorConfig,
    artifact_dir: TempDir,
    killed: bool,
}

impl SupervisedProcess {
    pub fn spawn(
        config: SupervisorConfig,
        program: &str,
        args: &[&str],
    ) -> Result<Self, std::io::Error> {
        let artifact_dir = match &config.artifact_dir {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                TempDir::new_in(dir)?
            }
            None => TempDir::new()?,
        };

        let mut cmd = Command::new(program);
        cmd.args(args);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn()?;
        let pid = child.id();

        Ok(Self {
            child: Some(child),
            pid,
            stdout_lines: Vec::new(),
            stderr_lines: Vec::new(),
            start_time: Instant::now(),
            config,
            artifact_dir,
            killed: false,
        })
    }

    pub fn capture_output(&mut self) {
        if let Some(ref mut child) = self.child {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            if let Some(ref mut r) = child.stdout {
                drain_reader(
                    r,
                    &mut stdout_buf,
                    self.config.max_capture_lines,
                    self.config.max_line_bytes,
                );
            }
            if let Some(ref mut r) = child.stderr {
                drain_reader(
                    r,
                    &mut stderr_buf,
                    self.config.max_capture_lines,
                    self.config.max_line_bytes,
                );
            }
            self.stdout_lines.extend(stdout_buf);
            self.stderr_lines.extend(stderr_buf);
        }
    }

    pub fn wait(&mut self) -> ProcessExit {
        self.capture_output();
        let exit = self.child.as_mut().map(|c| c.wait());
        self.capture_output();

        let lifetime_ms = self.start_time.elapsed().as_millis() as u64;
        match exit {
            Some(Ok(status)) => ProcessExit {
                exit_code: status.code(),
                signal: None,
                lifetime_ms,
            },
            _ => ProcessExit {
                exit_code: None,
                signal: Some("wait_failed".to_string()),
                lifetime_ms,
            },
        }
    }

    pub fn kill_group(&mut self) {
        if self.killed {
            return;
        }
        self.killed = true;

        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-9", &format!("-{}", self.pid)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(not(unix))]
        {
            if let Some(ref mut child) = self.child {
                let _ = child.kill();
            }
        }

        if let Some(ref mut child) = self.child {
            let _ = child.wait();
        }
    }

    pub fn save_artifacts(&self, label: &str) -> std::io::Result<()> {
        let stdout_path = self.artifact_dir.path().join(format!("{label}.stdout.log"));
        let stderr_path = self.artifact_dir.path().join(format!("{label}.stderr.log"));

        std::fs::write(&stdout_path, self.stdout_lines.join("\n"))?;
        std::fs::write(&stderr_path, self.stderr_lines.join("\n"))?;

        Ok(())
    }

    pub fn artifact_dir(&self) -> &std::path::Path {
        self.artifact_dir.path()
    }

    pub fn stdout_lines(&self) -> &[String] {
        &self.stdout_lines
    }

    pub fn stderr_lines(&self) -> &[String] {
        &self.stderr_lines
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

fn drain_reader(
    reader: &mut impl Read,
    lines: &mut Vec<String>,
    max_lines: usize,
    max_bytes: usize,
) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]);
                for line in chunk.lines() {
                    if lines.len() < max_lines {
                        let truncated = if line.len() > max_bytes {
                            format!("{}... (truncated)", &line[..max_bytes])
                        } else {
                            line.to_string()
                        };
                        lines.push(truncated);
                    }
                }
            }
            Err(_) => break,
        }
    }
}

impl Drop for SupervisedProcess {
    fn drop(&mut self) {
        self.kill_group();
        self.capture_output();
        let _ = self.save_artifacts(&format!("pid_{}", self.pid));
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessProbe {
    TcpPort,
    StdoutPattern(String),
    FixedDelay(u64),
    FileExists(String),
}

pub async fn probe_readiness(
    addr: std::net::SocketAddr,
    probe: &ReadinessProbe,
    timeout: Duration,
) -> bool {
    match probe {
        ReadinessProbe::TcpPort => probe_tcp_port(addr, timeout).await,
        ReadinessProbe::StdoutPattern(_) => {
            tokio::time::sleep(Duration::from_millis(200)).await;
            true
        }
        ReadinessProbe::FixedDelay(ms) => {
            tokio::time::sleep(Duration::from_millis(*ms)).await;
            true
        }
        ReadinessProbe::FileExists(path) => {
            let start = Instant::now();
            loop {
                if std::path::Path::new(path).exists() {
                    return true;
                }
                if start.elapsed() >= timeout {
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn probe_tcp_port(addr: std::net::SocketAddr, timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(_) => return true,
            Err(_) => {
                if start.elapsed() >= timeout {
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervised_process_spawn_and_wait() {
        let config = SupervisorConfig::default();
        let mut proc = SupervisedProcess::spawn(config, "echo", &["hello"]).expect("spawn failed");
        let exit = proc.wait();
        assert_eq!(exit.exit_code, Some(0));
        proc.capture_output();
        assert!(proc.stdout_lines.iter().any(|l| l.contains("hello")));
    }

    #[test]
    fn supervised_process_kill_group() {
        let config = SupervisorConfig::default();
        let mut proc = SupervisedProcess::spawn(config, "sleep", &["60"]).expect("spawn failed");
        proc.kill_group();
        let exit = proc.wait();
        assert!(exit.exit_code.is_none() || exit.exit_code != Some(0));
    }

    #[test]
    fn bounded_capture_limits_output() {
        let config = SupervisorConfig {
            max_capture_lines: 3,
            ..Default::default()
        };
        let mut proc = SupervisedProcess::spawn(
            config,
            "bash",
            &["-c", "for i in $(seq 1 10); do echo \"line $i\"; done"],
        )
        .expect("spawn failed");
        let exit = proc.wait();
        assert_eq!(exit.exit_code, Some(0));
        proc.capture_output();
        assert!(proc.stdout_lines.len() <= 3);
    }

    #[test]
    fn artifact_save() {
        let config = SupervisorConfig::default();
        let mut proc = SupervisedProcess::spawn(config, "echo", &["test"]).expect("spawn failed");
        let exit = proc.wait();
        assert_eq!(exit.exit_code, Some(0));
        proc.save_artifacts("test").expect("save failed");
        let stdout_path = proc.artifact_dir().join("test.stdout.log");
        assert!(stdout_path.exists());
    }

    #[tokio::test]
    async fn probe_tcp_port_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(probe_tcp_port(addr, Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn probe_tcp_port_timeout() {
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert!(!probe_tcp_port(addr, Duration::from_millis(100)).await);
    }
}
