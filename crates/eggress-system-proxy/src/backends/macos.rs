use crate::command_runner::CommandRunner;
use crate::inspection::SystemProxySettings;

/// List macOS network services via `networksetup -listallnetworkservices`.
pub fn list_network_services(runner: &dyn CommandRunner) -> Result<Vec<String>, String> {
    let output = runner
        .run("networksetup", &["-listallnetworkservices"])
        .map_err(|e| format!("failed to run networksetup: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .skip(1) // skip header
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// Inspect proxy settings for a macOS network service.
pub fn inspect_macos_proxy(
    runner: &dyn CommandRunner,
    service: &str,
) -> Result<SystemProxySettings, String> {
    let mut raw = std::collections::HashMap::new();

    let http_proxy = get_macos_proxy_field(runner, service, "-getwebproxy", "Server")?;
    let https_proxy = get_macos_proxy_field(runner, service, "-getsecurewebproxy", "Server")?;
    let socks_proxy = get_macos_proxy_field(runner, service, "-getsocksfirewallproxy", "Server")?;
    let no_proxy = get_macos_proxy_field(runner, service, "-getwebproxy", "BypassDomains")?;

    if let Some(ref v) = http_proxy {
        raw.insert("http_proxy".to_string(), v.clone());
    }
    if let Some(ref v) = https_proxy {
        raw.insert("https_proxy".to_string(), v.clone());
    }
    if let Some(ref v) = socks_proxy {
        raw.insert("socks_proxy".to_string(), v.clone());
    }
    if let Some(ref v) = no_proxy {
        raw.insert("no_proxy".to_string(), v.clone());
    }

    Ok(SystemProxySettings {
        source: format!("macos:networksetup:{service}"),
        http_proxy,
        https_proxy,
        socks_proxy,
        no_proxy,
        raw,
    })
}

fn get_macos_proxy_field(
    runner: &dyn CommandRunner,
    service: &str,
    flag: &str,
    field: &str,
) -> Result<Option<String>, String> {
    let output = runner
        .run("networksetup", &[flag, service])
        .map_err(|e| format!("failed to run networksetup {flag}: {e}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim();
            let value = line[pos + 1..].trim();
            if key == field && !value.is_empty() && value != "Off" {
                return Ok(Some(value.to_string()));
            }
        }
    }
    Ok(None)
}

/// Generate `networksetup` commands to apply proxy settings (dry-run).
pub fn generate_macos_apply_commands(
    service: &str,
    http_proxy: Option<&str>,
    https_proxy: Option<&str>,
    socks_proxy: Option<&str>,
    no_proxy: Option<&str>,
) -> Vec<String> {
    let mut commands = Vec::new();
    if let Some(http) = http_proxy {
        commands.push(format!("networksetup -setwebproxy {service} on"));
        commands.push(format!("networksetup -setwebproxyservers {service} {http}"));
    }
    if let Some(https) = https_proxy {
        commands.push(format!("networksetup -setsecurewebproxy {service} on"));
        commands.push(format!(
            "networksetup -setsecurewebproxyservers {service} {https}"
        ));
    }
    if let Some(socks) = socks_proxy {
        commands.push(format!("networksetup -setsocksfirewallproxy {service} on"));
        commands.push(format!(
            "networksetup -setsocksfirewallproxyserver {service} {socks}"
        ));
    }
    if let Some(no_proxy) = no_proxy {
        commands.push(format!(
            "networksetup -setwebproxybypassdomains {service} {no_proxy}"
        ));
    }
    commands
}

/// Generate `networksetup` commands to disable proxy settings (dry-run).
pub fn generate_macos_disable_commands(service: &str) -> Vec<String> {
    vec![
        format!("networksetup -setwebproxy {service} off"),
        format!("networksetup -setsecurewebproxy {service} off"),
        format!("networksetup -setsocksfirewallproxy {service} off"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_runner::MockCommandRunner;

    fn success_output(stdout: &str) -> std::process::Output {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
            }
        }
        #[cfg(not(unix))]
        {
            std::process::Output {
                status: std::process::ExitStatus::default(),
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
            }
        }
    }

    #[test]
    fn list_services_parses_output() {
        let runner = MockCommandRunner::new().add_response(
            "networksetup",
            vec!["-listallnetworkservices".to_string()],
            Ok(success_output("An asterisk (*) denotes that a network service is disabled.\n*Ethernet\n*Wi-Fi\nUSB 10/100/1000 LAN\n")),
        );

        let services = list_network_services(&runner).unwrap();
        assert_eq!(services.len(), 3);
        assert!(services.contains(&"*Wi-Fi".to_string()));
    }

    #[test]
    fn inspect_proxy_parses_web_proxy() {
        let runner = MockCommandRunner::new()
            .add_response(
                "networksetup",
                vec!["-getwebproxy".to_string(), "*Wi-Fi".to_string()],
                Ok(success_output(
                    "Enabled: Yes\nServer: proxy.example.com:8080\nBypassDomains: localhost\n",
                )),
            )
            .add_response(
                "networksetup",
                vec!["-getsecurewebproxy".to_string(), "*Wi-Fi".to_string()],
                Ok(success_output(
                    "Enabled: Yes\nServer: proxy.example.com:8443\nBypassDomains: localhost\n",
                )),
            )
            .add_response(
                "networksetup",
                vec!["-getsocksfirewallproxy".to_string(), "*Wi-Fi".to_string()],
                Ok(success_output("Enabled: No\nServer: \nPort: 0\n")),
            );

        let settings = inspect_macos_proxy(&runner, "*Wi-Fi").unwrap();
        assert_eq!(
            settings.http_proxy.as_deref(),
            Some("proxy.example.com:8080")
        );
        assert_eq!(
            settings.https_proxy.as_deref(),
            Some("proxy.example.com:8443")
        );
        assert_eq!(settings.socks_proxy, None);
        assert!(settings.source.contains("*Wi-Fi"));
    }

    #[test]
    fn generate_apply_commands() {
        let commands = generate_macos_apply_commands(
            "*Wi-Fi",
            Some("proxy:8080"),
            Some("proxy:8443"),
            Some("socks:1080"),
            Some("localhost,127.0.0.1"),
        );
        assert!(commands.iter().any(|c| c.contains("-setwebproxy")));
        assert!(commands.iter().any(|c| c.contains("-setsecurewebproxy")));
        assert!(commands
            .iter()
            .any(|c| c.contains("-setsocksfirewallproxy")));
        assert!(commands
            .iter()
            .any(|c| c.contains("-setwebproxybypassdomains")));
    }

    #[test]
    fn generate_disable_commands() {
        let commands = generate_macos_disable_commands("*Wi-Fi");
        assert_eq!(commands.len(), 3);
        assert!(commands.iter().all(|c| c.contains("off")));
    }
}
