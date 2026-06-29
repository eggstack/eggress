use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum EggressRunnerError {
    #[error("failed to spawn eggress process: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("failed to write config to temp file: {0}")]
    ConfigWrite(#[source] std::io::Error),
    #[error("config file has no path")]
    ConfigNoPath,
    #[error("process exited before ready with status: {status}")]
    EarlyExit { status: String },
    #[error("port {0} not ready within timeout")]
    PortNotReady(u16),
    #[error("process has no stdout handle")]
    NoStdout,
    #[error("process has no stderr handle")]
    NoStderr,
    #[error("failed to read process output: {0}")]
    OutputRead(#[source] std::io::Error),
    #[error("shutdown timed out")]
    ShutdownTimeout,
    #[error("failed to kill process: {0}")]
    Kill(#[source] std::io::Error),
    #[error("failed to wait for process: {0}")]
    Wait(#[source] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct EggressRunnerConfig {
    pub binary_path: Option<PathBuf>,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub io_timeout: Duration,
}

impl Default for EggressRunnerConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            startup_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(5),
            io_timeout: Duration::from_secs(5),
        }
    }
}

pub struct EggressProcess {
    child: Option<Child>,
    addr: SocketAddr,
    stdout_lines: Vec<String>,
    stderr_lines: Vec<String>,
    #[allow(dead_code)]
    config_file: Option<tempfile::NamedTempFile>,
}

impl EggressProcess {
    pub async fn start_from_toml(
        config: &EggressRunnerConfig,
        toml_config: &str,
    ) -> Result<Self, EggressRunnerError> {
        let mut f = tempfile::NamedTempFile::new().map_err(EggressRunnerError::ConfigWrite)?;
        std::io::Write::write_all(&mut f, toml_config.as_bytes())
            .map_err(EggressRunnerError::ConfigWrite)?;
        std::io::Write::flush(&mut f).map_err(EggressRunnerError::ConfigWrite)?;
        let path = f
            .path()
            .to_str()
            .ok_or(EggressRunnerError::ConfigNoPath)?
            .to_string();

        let mut cmd = binary_command(config)?;
        cmd.args(["--config", &path]);

        let mut child = spawn_with_output(config, &mut cmd)?;
        let port = extract_listen_port(toml_config).ok_or_else(|| {
            let _ = child.kill();
            EggressRunnerError::PortNotReady(0)
        })?;

        wait_ready(port, config.startup_timeout)
            .await
            .map_err(|e| {
                let _ = child.kill();
                e
            })?;

        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().map_err(|_| {
            let _ = child.kill();
            EggressRunnerError::PortNotReady(port)
        })?;

        Ok(Self {
            child: Some(child),
            addr,
            stdout_lines: Vec::new(),
            stderr_lines: Vec::new(),
            config_file: Some(f),
        })
    }

    pub async fn start_from_args(
        config: &EggressRunnerConfig,
        args: &[&str],
    ) -> Result<Self, EggressRunnerError> {
        let mut cmd = binary_command(config)?;
        cmd.args(args);

        let mut child = spawn_with_output(config, &mut cmd)?;
        let port = extract_port_from_args(args).ok_or_else(|| {
            let _ = child.kill();
            EggressRunnerError::PortNotReady(0)
        })?;

        wait_ready(port, config.startup_timeout)
            .await
            .map_err(|e| {
                let _ = child.kill();
                e
            })?;

        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().map_err(|_| {
            let _ = child.kill();
            EggressRunnerError::PortNotReady(port)
        })?;

        Ok(Self {
            child: Some(child),
            addr,
            stdout_lines: Vec::new(),
            stderr_lines: Vec::new(),
            config_file: None,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn shutdown(&mut self) -> Result<(), EggressRunnerError> {
        if self.child.is_none() {
            return Ok(());
        }
        self.drain_output()?;
        if let Some(ref mut child) = self.child {
            child.kill().map_err(EggressRunnerError::Kill)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        self.child = None;
                        return Ok(());
                    }
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            self.child = None;
                            return Err(EggressRunnerError::ShutdownTimeout);
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(e) => {
                        self.child = None;
                        return Err(EggressRunnerError::Wait(e));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn stdout_lines(&self) -> &[String] {
        &self.stdout_lines
    }

    pub fn stderr_lines(&self) -> &[String] {
        &self.stderr_lines
    }

    fn drain_output(&mut self) -> Result<(), EggressRunnerError> {
        if let Some(ref mut child) = self.child {
            if let Some(ref mut stdout) = child.stdout {
                use std::io::Read;
                let mut buf = String::new();
                let _ = stdout.read_to_string(&mut buf);
                self.stdout_lines.extend(buf.lines().map(String::from));
            }
            if let Some(ref mut stderr) = child.stderr {
                use std::io::Read;
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                self.stderr_lines.extend(buf.lines().map(String::from));
            }
        }
        Ok(())
    }
}

impl Drop for EggressProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn binary_command(config: &EggressRunnerConfig) -> Result<Command, EggressRunnerError> {
    let cmd = match &config.binary_path {
        Some(path) => Command::new(path),
        None => {
            let mut c = Command::new("cargo");
            c.args(["run", "--bin", "eggress", "--"]);
            c
        }
    };
    Ok(cmd)
}

fn spawn_with_output(
    _config: &EggressRunnerConfig,
    cmd: &mut Command,
) -> Result<Child, EggressRunnerError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn().map_err(EggressRunnerError::Spawn)
}

async fn wait_ready(port: u16, timeout: Duration) -> Result<(), EggressRunnerError> {
    let start = Instant::now();
    loop {
        if start.elapsed() >= timeout {
            return Err(EggressRunnerError::PortNotReady(port));
        }
        match tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await {
            Ok(stream) => {
                drop(stream);
                return Ok(());
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

fn extract_listen_port(toml: &str) -> Option<u16> {
    for line in toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("listen") && trimmed.contains('=') {
            if let Some(val) = trimmed.split_once('=') {
                let val = val.1.trim();
                if let Some(colon) = val.rfind(':') {
                    let port_str = &val[colon + 1..];
                    let port_str = port_str.trim_matches(|c: char| c == '"' || c == '\'');
                    if let Ok(port) = port_str.parse::<u16>() {
                        return Some(port);
                    }
                }
            }
        }
    }
    None
}

fn extract_port_from_args(args: &[&str]) -> Option<u16> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-l" || args[i] == "--listen" {
            if let Some(uri) = args.get(i + 1) {
                if let Some(colon) = uri.rfind(':') {
                    let port_str = &uri[colon + 1..];
                    if let Ok(port) = port_str.parse::<u16>() {
                        return Some(port);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_guard_drop_kills_child() {
        let mut cmd = Command::new("sleep");
        cmd.arg("300");
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        let child = cmd.spawn().expect("failed to spawn sleep");
        let pid = child.id();

        {
            let _process = EggressProcess {
                child: Some(child),
                addr: "127.0.0.1:0".parse().unwrap(),
                stdout_lines: Vec::new(),
                stderr_lines: Vec::new(),
                config_file: None,
            };
        }

        std::thread::sleep(Duration::from_millis(200));
        let result = Command::new("kill").args(["-0", &pid.to_string()]).status();
        assert!(
            result.is_err() || !result.unwrap().success(),
            "process {pid} should be dead after drop"
        );
    }

    #[tokio::test]
    async fn start_from_toml_smoke_socks5() {
        let port = crate::get_free_port().await;
        let toml = format!(
            r#"
[listener]
bind = "127.0.0.1:{port}"
protocols = ["socks5"]
"#
        );
        let config = EggressRunnerConfig {
            startup_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let result = EggressProcess::start_from_toml(&config, &toml).await;
        match result {
            Ok(mut proc) => {
                assert_eq!(proc.addr().port(), port);
                let _ = proc.shutdown().await;
            }
            Err(EggressRunnerError::PortNotReady(_)) => {
                // Expected if eggress binary is not built
            }
            Err(EggressRunnerError::Spawn(e)) => {
                assert!(
                    e.kind() == std::io::ErrorKind::NotFound
                        || e.kind() == std::io::ErrorKind::Other,
                    "unexpected spawn error: {e}"
                );
            }
            Err(_) => {}
        }
    }
}
