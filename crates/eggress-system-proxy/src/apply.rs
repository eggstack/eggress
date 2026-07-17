use std::fmt;
use std::path::PathBuf;

use crate::command_runner::CommandRunner;
use crate::inspection::SystemProxySettings;

/// A structured system command. Programs and arguments are kept separate so
/// that callers can pass them directly to `Command::new(program).args(args)`
/// without relying on shell-style splitting, which would mishandle names
/// containing spaces (e.g. Windows registry keys, macOS network services).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
}

impl Command {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {}", arg)?;
        }
        Ok(())
    }
}

/// Plan for applying system proxy settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyPlan {
    /// Platform target.
    pub platform: String,
    /// Network service or scope (e.g., "*Wi-Fi" on macOS).
    pub service: Option<String>,
    /// HTTP proxy to set.
    pub http_proxy: Option<String>,
    /// HTTPS proxy to set.
    pub https_proxy: Option<String>,
    /// SOCKS proxy to set.
    pub socks_proxy: Option<String>,
    /// No-proxy/bypass list.
    pub no_proxy: Option<String>,
    /// Commands that would be executed.
    pub commands: Vec<Command>,
    /// Previous settings captured for rollback.
    pub previous_settings: Option<SystemProxySettings>,
}

/// Rollback state saved before applying proxy changes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RollbackState {
    /// Timestamp of when the rollback was created.
    pub timestamp: String,
    /// Platform target.
    pub platform: String,
    /// Network service or scope.
    pub service: Option<String>,
    /// Previous HTTP proxy.
    pub http_proxy: Option<String>,
    /// Previous HTTPS proxy.
    pub https_proxy: Option<String>,
    /// Previous SOCKS proxy.
    pub socks_proxy: Option<String>,
    /// Previous no-proxy/bypass list.
    pub no_proxy: Option<String>,
}

impl RollbackState {
    /// Save rollback state to a JSON file.
    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize rollback state: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("failed to write rollback file: {e}"))
    }

    /// Load rollback state from a JSON file.
    pub fn load(path: &PathBuf) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read rollback file: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("failed to parse rollback file: {e}"))
    }
}

/// Create an apply plan without executing it (dry-run).
pub fn plan_apply(
    platform: &str,
    service: Option<&str>,
    http_proxy: Option<&str>,
    https_proxy: Option<&str>,
    socks_proxy: Option<&str>,
    no_proxy: Option<&str>,
    current_settings: Option<&SystemProxySettings>,
) -> ApplyPlan {
    let commands = match platform {
        "macos" => {
            let svc = service.unwrap_or("*Wi-Fi");
            crate::backends::macos::generate_macos_apply_commands(
                svc,
                http_proxy,
                https_proxy,
                socks_proxy,
                no_proxy,
            )
        }
        "windows" => crate::backends::windows::generate_windows_apply_commands(
            http_proxy,
            https_proxy,
            socks_proxy,
            no_proxy,
        ),
        "linux" => crate::backends::linux::generate_gnome_apply_commands(
            http_proxy,
            https_proxy,
            socks_proxy,
            no_proxy,
        ),
        _ => Vec::new(),
    };

    ApplyPlan {
        platform: platform.to_string(),
        service: service.map(|s| s.to_string()),
        http_proxy: http_proxy.map(|s| s.to_string()),
        https_proxy: https_proxy.map(|s| s.to_string()),
        socks_proxy: socks_proxy.map(|s| s.to_string()),
        no_proxy: no_proxy.map(|s| s.to_string()),
        commands,
        previous_settings: current_settings.cloned(),
    }
}

/// Create a rollback state from current settings.
pub fn create_rollback(
    platform: &str,
    service: Option<&str>,
    settings: &SystemProxySettings,
) -> RollbackState {
    RollbackState {
        timestamp: chrono_timestamp(),
        platform: platform.to_string(),
        service: service.map(|s| s.to_string()),
        http_proxy: settings.http_proxy.clone(),
        https_proxy: settings.https_proxy.clone(),
        socks_proxy: settings.socks_proxy.clone(),
        no_proxy: settings.no_proxy.clone(),
    }
}

/// Execute an apply plan using the provided command runner.
pub fn execute_apply(plan: &ApplyPlan, runner: &dyn CommandRunner) -> Result<Vec<Command>, String> {
    let mut executed = Vec::new();
    for cmd in &plan.commands {
        let arg_refs: Vec<&str> = cmd.args.iter().map(String::as_str).collect();
        runner
            .run(&cmd.program, &arg_refs)
            .map_err(|e| format!("failed to execute '{cmd}': {e}"))?;
        executed.push(cmd.clone());
    }
    Ok(executed)
}

/// Generate revert commands from rollback state.
pub fn generate_revert_commands(rollback: &RollbackState) -> Vec<Command> {
    match rollback.platform.as_str() {
        "macos" => {
            let svc = rollback.service.as_deref().unwrap_or("*Wi-Fi");
            let mut commands = crate::backends::macos::generate_macos_disable_commands(svc);
            if let Some(ref http) = rollback.http_proxy {
                if !http.is_empty() {
                    commands.push(Command::new(
                        "networksetup",
                        vec!["-setwebproxy".into(), svc.into(), "on".into()],
                    ));
                    commands.push(Command::new(
                        "networksetup",
                        vec!["-setwebproxyservers".into(), svc.into(), http.clone()],
                    ));
                }
            }
            if let Some(ref https) = rollback.https_proxy {
                if !https.is_empty() {
                    commands.push(Command::new(
                        "networksetup",
                        vec!["-setsecurewebproxy".into(), svc.into(), "on".into()],
                    ));
                    commands.push(Command::new(
                        "networksetup",
                        vec![
                            "-setsecurewebproxyservers".into(),
                            svc.into(),
                            https.clone(),
                        ],
                    ));
                }
            }
            if let Some(ref socks) = rollback.socks_proxy {
                if !socks.is_empty() {
                    commands.push(Command::new(
                        "networksetup",
                        vec!["-setsocksfirewallproxy".into(), svc.into(), "on".into()],
                    ));
                    commands.push(Command::new(
                        "networksetup",
                        vec![
                            "-setsocksfirewallproxyserver".into(),
                            svc.into(),
                            socks.clone(),
                        ],
                    ));
                }
            }
            commands
        }
        "windows" => {
            let mut commands = crate::backends::windows::generate_windows_disable_commands();
            let mut parts = Vec::new();
            if let Some(ref http) = rollback.http_proxy {
                parts.push(format!("http={http}"));
            }
            if let Some(ref https) = rollback.https_proxy {
                parts.push(format!("https={https}"));
            }
            if let Some(ref socks) = rollback.socks_proxy {
                parts.push(format!("socks={socks}"));
            }
            if !parts.is_empty() {
                let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
                let proxy_value = parts.join(";");
                commands.push(Command::new(
                    "reg",
                    vec![
                        "add".into(),
                        key.into(),
                        "/v".into(),
                        "ProxyServer".into(),
                        "/t".into(),
                        "REG_SZ".into(),
                        "/d".into(),
                        proxy_value,
                        "/f".into(),
                    ],
                ));
                commands.push(Command::new(
                    "reg",
                    vec![
                        "add".into(),
                        key.into(),
                        "/v".into(),
                        "ProxyEnable".into(),
                        "/t".into(),
                        "REG_DWORD".into(),
                        "/d".into(),
                        "1".into(),
                        "/f".into(),
                    ],
                ));
            }
            commands
        }
        "linux" => {
            let mut commands = crate::backends::linux::generate_gnome_disable_commands();
            let has_any = rollback.http_proxy.is_some()
                || rollback.https_proxy.is_some()
                || rollback.socks_proxy.is_some();
            if has_any {
                commands.extend(crate::backends::linux::generate_gnome_apply_commands(
                    rollback.http_proxy.as_deref(),
                    rollback.https_proxy.as_deref(),
                    rollback.socks_proxy.as_deref(),
                    rollback.no_proxy.as_deref(),
                ));
            }
            commands
        }
        _ => Vec::new(),
    }
}

fn chrono_timestamp() -> String {
    // Use simple timestamp without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::command_runner::MockCommandRunner;
    use std::collections::HashMap;

    #[test]
    fn plan_apply_macos_produces_commands() {
        let plan = plan_apply(
            "macos",
            Some("*Wi-Fi"),
            Some("proxy:8080"),
            Some("proxy:8443"),
            None,
            None,
            None,
        );
        assert_eq!(plan.platform, "macos");
        assert!(plan
            .commands
            .iter()
            .any(|c| c.to_string().contains("networksetup")));
    }

    #[test]
    fn plan_apply_windows_produces_commands() {
        let plan = plan_apply("windows", None, Some("proxy:8080"), None, None, None, None);
        assert_eq!(plan.platform, "windows");
        assert!(plan
            .commands
            .iter()
            .any(|c| c.to_string().contains("reg add")));
    }

    #[test]
    fn plan_apply_linux_produces_commands() {
        let plan = plan_apply("linux", None, Some("proxy:8080"), None, None, None, None);
        assert_eq!(plan.platform, "linux");
        assert!(plan
            .commands
            .iter()
            .any(|c| c.to_string().contains("gsettings")));
    }

    #[cfg(unix)]
    #[test]
    fn execute_apply_preserves_spaces_in_args() {
        use std::os::unix::process::ExitStatusExt;
        let runner = MockCommandRunner::new().add_always(
            "networksetup",
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        );

        let plan = ApplyPlan {
            platform: "macos".to_string(),
            service: Some("USB 10/100/1000 LAN".to_string()),
            http_proxy: Some("proxy:8080".to_string()),
            https_proxy: None,
            socks_proxy: None,
            no_proxy: None,
            commands: vec![Command::new(
                "networksetup",
                vec![
                    "-setwebproxy".into(),
                    "USB 10/100/1000 LAN".into(),
                    "on".into(),
                ],
            )],
            previous_settings: None,
        };

        let _ = execute_apply(&plan, &runner).unwrap();
        let calls = runner.calls();
        assert_eq!(calls[0].0, "networksetup");
        assert_eq!(
            calls[0].1,
            vec!["-setwebproxy", "USB 10/100/1000 LAN", "on"]
        );
    }

    #[test]
    fn rollback_state_save_and_load() {
        let state = RollbackState {
            timestamp: "12345".to_string(),
            platform: "macos".to_string(),
            service: Some("*Wi-Fi".to_string()),
            http_proxy: Some("old-proxy:8080".to_string()),
            https_proxy: None,
            socks_proxy: None,
            no_proxy: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollback.json");
        state.save(&path).unwrap();

        let loaded = RollbackState::load(&path).unwrap();
        assert_eq!(loaded.platform, "macos");
        assert_eq!(loaded.http_proxy.as_deref(), Some("old-proxy:8080"));
    }

    #[test]
    fn create_rollback_from_settings() {
        let settings = SystemProxySettings {
            source: "test".to_string(),
            http_proxy: Some("http://proxy:8080".to_string()),
            https_proxy: None,
            socks_proxy: None,
            no_proxy: None,
            raw: HashMap::new(),
        };
        let rollback = create_rollback("macos", Some("*Wi-Fi"), &settings);
        assert_eq!(rollback.http_proxy.as_deref(), Some("http://proxy:8080"));
        assert_eq!(rollback.platform, "macos");
    }

    #[cfg(unix)]
    #[test]
    fn execute_apply_runs_commands() {
        use std::os::unix::process::ExitStatusExt;
        let runner = MockCommandRunner::new().add_always(
            "networksetup",
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        );

        let plan = ApplyPlan {
            platform: "macos".to_string(),
            service: Some("*Wi-Fi".to_string()),
            http_proxy: Some("proxy:8080".to_string()),
            https_proxy: None,
            socks_proxy: None,
            no_proxy: None,
            commands: vec![Command::new(
                "networksetup",
                vec!["-setwebproxy".into(), "*Wi-Fi".into(), "on".into()],
            )],
            previous_settings: None,
        };

        let executed = execute_apply(&plan, &runner).unwrap();
        assert_eq!(executed.len(), 1);
        let calls = runner.calls();
        assert_eq!(calls[0].0, "networksetup");
        assert_eq!(calls[0].1, vec!["-setwebproxy", "*Wi-Fi", "on"]);
    }

    #[test]
    fn revert_commands_macos() {
        let rollback = RollbackState {
            timestamp: "12345".to_string(),
            platform: "macos".to_string(),
            service: Some("*Wi-Fi".to_string()),
            http_proxy: Some("proxy:8080".to_string()),
            https_proxy: None,
            socks_proxy: None,
            no_proxy: None,
        };
        let commands = generate_revert_commands(&rollback);
        assert!(commands.iter().any(|c| c.to_string().contains("off")));
        assert!(commands
            .iter()
            .any(|c| c.to_string().contains("setwebproxy")));
    }
}
