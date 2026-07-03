use crate::command_runner::CommandRunner;
use crate::inspection::SystemProxySettings;

/// Inspect proxy settings from Windows Internet Settings registry keys.
///
/// On non-Windows platforms, returns an error.
pub fn inspect_windows_proxy(runner: &dyn CommandRunner) -> Result<SystemProxySettings, String> {
    let _ = runner;
    #[cfg(target_os = "windows")]
    {
        inspect_windows_registry(runner)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Windows Internet Settings not available on this platform".to_string())
    }
}

#[cfg(target_os = "windows")]
fn inspect_windows_registry(runner: &dyn CommandRunner) -> Result<SystemProxySettings, String> {
    let mut raw = std::collections::HashMap::new();

    // HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings
    let key = r#"Software\Microsoft\Windows\CurrentVersion\Internet Settings"#;

    let proxy_enable = read_reg_value(runner, key, "ProxyEnable")?;
    let proxy_server = read_reg_value(runner, key, "ProxyServer")?;
    let proxy_override = read_reg_value(runner, key, "ProxyOverride")?;

    raw.insert("ProxyEnable".to_string(), proxy_enable.unwrap_or_default());
    raw.insert(
        "ProxyServer".to_string(),
        proxy_server.clone().unwrap_or_default(),
    );
    raw.insert(
        "ProxyOverride".to_string(),
        proxy_override.clone().unwrap_or_default(),
    );

    let enabled = raw.get("ProxyEnable").map_or(false, |v| v == "1");

    let (http_proxy, https_proxy, socks_proxy) = if enabled {
        parse_windows_proxy_server(&proxy_server.unwrap_or_default())
    } else {
        (None, None, None)
    };

    Ok(SystemProxySettings {
        source: "windows:internet_settings".to_string(),
        http_proxy,
        https_proxy,
        socks_proxy,
        no_proxy: proxy_override,
        raw,
    })
}

#[cfg(target_os = "windows")]
fn read_reg_value(
    runner: &dyn CommandRunner,
    key: &str,
    value: &str,
) -> Result<Option<String>, String> {
    let output = runner
        .run("reg", &["query", &format!("HKCU\\{key}"), "/v", value])
        .map_err(|e| format!("failed to run reg: {e}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains(value) {
            if let Some(pos) = line.rfind("REG_SZ") {
                return Ok(Some(line[pos + 6..].trim().to_string()));
            }
        }
    }
    Ok(None)
}

#[cfg(target_os = "windows")]
fn parse_windows_proxy_server(server: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut http = None;
    let mut https = None;
    let mut socks = None;

    for part in server.split(';') {
        let part = part.trim();
        if let Some(addr) = part.strip_prefix("http=") {
            http = Some(addr.to_string());
        } else if let Some(addr) = part.strip_prefix("https=") {
            https = Some(addr.to_string());
        } else if let Some(addr) = part.strip_prefix("socks=") {
            socks = Some(addr.to_string());
        } else if !part.is_empty() {
            // Bare address applies to all protocols
            if http.is_none() {
                http = Some(part.to_string());
            }
            if https.is_none() {
                https = Some(part.to_string());
            }
        }
    }

    (http, https, socks)
}

/// Generate commands to apply Windows proxy settings (dry-run only).
pub fn generate_windows_apply_commands(
    http_proxy: Option<&str>,
    https_proxy: Option<&str>,
    socks_proxy: Option<&str>,
    no_proxy: Option<&str>,
) -> Vec<String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let mut commands = Vec::new();

    // Build ProxyServer value
    let mut parts = Vec::new();
    if let Some(http) = http_proxy {
        parts.push(format!("http={http}"));
    }
    if let Some(https) = https_proxy {
        parts.push(format!("https={https}"));
    }
    if let Some(socks) = socks_proxy {
        parts.push(format!("socks={socks}"));
    }

    if !parts.is_empty() {
        commands.push(format!(
            "reg add \"{key}\" /v ProxyServer /t REG_SZ /d \"{}\" /f",
            parts.join(";")
        ));
        commands.push(format!(
            "reg add \"{key}\" /v ProxyEnable /t REG_DWORD /d 1 /f"
        ));
    }

    if let Some(no_proxy) = no_proxy {
        commands.push(format!(
            "reg add \"{key}\" /v ProxyOverride /t REG_SZ /d \"{no_proxy}\" /f"
        ));
    }

    commands
}

/// Generate commands to disable Windows proxy settings (dry-run only).
pub fn generate_windows_disable_commands() -> Vec<String> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    vec![format!(
        "reg add \"{key}\" /v ProxyEnable /t REG_DWORD /d 0 /f"
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_server_bare_address() {
        let (http, https, socks) = parse_windows_proxy_server("proxy:8080");
        assert_eq!(http.as_deref(), Some("proxy:8080"));
        assert_eq!(https.as_deref(), Some("proxy:8080"));
        assert_eq!(socks, None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_server_explicit_protocols() {
        let (http, https, socks) =
            parse_windows_proxy_server("http=proxy:8080;https=proxy:8443;socks=proxy:1080");
        assert_eq!(http.as_deref(), Some("proxy:8080"));
        assert_eq!(https.as_deref(), Some("proxy:8443"));
        assert_eq!(socks.as_deref(), Some("proxy:1080"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_server_empty() {
        let (http, https, socks) = parse_windows_proxy_server("");
        assert_eq!(http, None);
        assert_eq!(https, None);
        assert_eq!(socks, None);
    }

    #[test]
    fn generate_apply_commands_produces_reg_commands() {
        let commands = generate_windows_apply_commands(
            Some("proxy:8080"),
            Some("proxy:8443"),
            None,
            Some("localhost"),
        );
        assert!(commands.iter().any(|c| c.contains("ProxyServer")));
        assert!(commands.iter().any(|c| c.contains("ProxyEnable")));
        assert!(commands.iter().any(|c| c.contains("ProxyOverride")));
    }

    #[test]
    fn generate_disable_commands() {
        let commands = generate_windows_disable_commands();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("ProxyEnable"));
        assert!(commands[0].contains("/d 0"));
    }
}
