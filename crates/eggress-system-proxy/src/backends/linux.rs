use crate::command_runner::CommandRunner;
use crate::inspection::SystemProxySettings;

/// Inspect GNOME proxy settings via `gsettings`.
pub fn inspect_gnome_proxy(runner: &dyn CommandRunner) -> Result<SystemProxySettings, String> {
    let mut raw = std::collections::HashMap::new();

    let mode = get_gsettings_value(runner, "org.gnome.system.proxy", "mode")?;
    raw.insert("mode".to_string(), mode.clone().unwrap_or_default());

    if mode.as_deref() == Some("none") || mode.is_none() {
        return Ok(SystemProxySettings {
            source: "linux:gnome:gsettings".to_string(),
            http_proxy: None,
            https_proxy: None,
            socks_proxy: None,
            no_proxy: None,
            raw,
        });
    }

    let http_host = get_gsettings_value(runner, "org.gnome.system.proxy.http", "host")?;
    let http_port = get_gsettings_value(runner, "org.gnome.system.proxy.http", "port")?;
    let https_host = get_gsettings_value(runner, "org.gnome.system.proxy.https", "host")?;
    let https_port = get_gsettings_value(runner, "org.gnome.system.proxy.https", "port")?;
    let socks_host = get_gsettings_value(runner, "org.gnome.system.proxy.socks", "host")?;
    let socks_port = get_gsettings_value(runner, "org.gnome.system.proxy.socks", "port")?;
    let ignore_hosts = get_gsettings_value(runner, "org.gnome.system.proxy", "ignore-hosts")?;

    let http_proxy = format_proxy_address(&http_host, &http_port);
    let https_proxy = format_proxy_address(&https_host, &https_port);
    let socks_proxy = format_proxy_address(&socks_host, &socks_port);

    if let Some(ref v) = http_proxy {
        raw.insert("http_proxy".to_string(), v.clone());
    }
    if let Some(ref v) = https_proxy {
        raw.insert("https_proxy".to_string(), v.clone());
    }
    if let Some(ref v) = socks_proxy {
        raw.insert("socks_proxy".to_string(), v.clone());
    }

    let no_proxy = ignore_hosts.map(|h| {
        h.trim_start_matches('[')
            .trim_end_matches(']')
            .replace("', '", ",")
    });

    if let Some(ref v) = no_proxy {
        raw.insert("no_proxy".to_string(), v.clone());
    }

    Ok(SystemProxySettings {
        source: "linux:gnome:gsettings".to_string(),
        http_proxy,
        https_proxy,
        socks_proxy,
        no_proxy,
        raw,
    })
}

/// Generate `gsettings` commands to apply GNOME proxy settings (dry-run).
pub fn generate_gnome_apply_commands(
    http_proxy: Option<&str>,
    https_proxy: Option<&str>,
    socks_proxy: Option<&str>,
    no_proxy: Option<&str>,
) -> Vec<String> {
    let mut commands = Vec::new();

    let has_any = http_proxy.is_some() || https_proxy.is_some() || socks_proxy.is_some();
    if has_any {
        commands.push("gsettings set org.gnome.system.proxy mode 'manual'".to_string());
    }

    if let Some(http) = http_proxy {
        if let Some((host, port)) = parse_proxy_address(http) {
            commands.push(format!(
                "gsettings set org.gnome.system.proxy.http host '{host}'"
            ));
            commands.push(format!(
                "gsettings set org.gnome.system.proxy.http port {port}"
            ));
        }
    }

    if let Some(https) = https_proxy {
        if let Some((host, port)) = parse_proxy_address(https) {
            commands.push(format!(
                "gsettings set org.gnome.system.proxy.https host '{host}'"
            ));
            commands.push(format!(
                "gsettings set org.gnome.system.proxy.https port {port}"
            ));
        }
    }

    if let Some(socks) = socks_proxy {
        if let Some((host, port)) = parse_proxy_address(socks) {
            commands.push(format!(
                "gsettings set org.gnome.system.proxy.socks host '{host}'"
            ));
            commands.push(format!(
                "gsettings set org.gnome.system.proxy.socks port {port}"
            ));
        }
    }

    if let Some(no_proxy) = no_proxy {
        let gsettings_list: Vec<String> = no_proxy
            .split(',')
            .map(|s| format!("'{}'", s.trim()))
            .collect();
        commands.push(format!(
            "gsettings set org.gnome.system.proxy ignore-hosts [{}]",
            gsettings_list.join(", ")
        ));
    }

    commands
}

/// Generate `gsettings` commands to disable GNOME proxy (dry-run).
pub fn generate_gnome_disable_commands() -> Vec<String> {
    vec!["gsettings set org.gnome.system.proxy mode 'none'".to_string()]
}

fn get_gsettings_value(
    runner: &dyn CommandRunner,
    schema: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let output = runner
        .run("gsettings", &["get", schema, key])
        .map_err(|e| format!("failed to run gsettings: {e}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() || stdout == "undefined" {
        return Ok(None);
    }

    // Strip surrounding quotes from gsettings output
    let value = if stdout.starts_with('\'') && stdout.ends_with('\'') {
        stdout[1..stdout.len() - 1].to_string()
    } else {
        stdout
    };

    Ok(Some(value))
}

fn format_proxy_address(host: &Option<String>, port: &Option<String>) -> Option<String> {
    match (host, port) {
        (Some(h), Some(p)) if !h.is_empty() => Some(format!("{h}:{p}")),
        (Some(h), _) if !h.is_empty() => Some(h.clone()),
        _ => None,
    }
}

fn parse_proxy_address(addr: &str) -> Option<(String, u16)> {
    let addr = addr
        .strip_prefix("http://")
        .or_else(|| addr.strip_prefix("https://"))
        .or_else(|| addr.strip_prefix("socks://"))
        .or_else(|| addr.strip_prefix("socks5://"))
        .unwrap_or(addr);

    if let Some(pos) = addr.rfind(':') {
        let host = addr[..pos].to_string();
        let port_str = &addr[pos + 1..];
        if let Ok(port) = port_str.parse::<u16>() {
            return Some((host, port));
        }
    }
    None
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
    fn inspect_gnome_manual_mode() {
        let runner = MockCommandRunner::new()
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy".to_string(),
                    "mode".to_string(),
                ],
                Ok(success_output("'manual'\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy.http".to_string(),
                    "host".to_string(),
                ],
                Ok(success_output("'proxy.example.com'\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy.http".to_string(),
                    "port".to_string(),
                ],
                Ok(success_output("8080\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy.https".to_string(),
                    "host".to_string(),
                ],
                Ok(success_output("'proxy.example.com'\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy.https".to_string(),
                    "port".to_string(),
                ],
                Ok(success_output("8443\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy.socks".to_string(),
                    "host".to_string(),
                ],
                Ok(success_output("''\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy.socks".to_string(),
                    "port".to_string(),
                ],
                Ok(success_output("0\n")),
            )
            .add_response(
                "gsettings",
                vec![
                    "get".to_string(),
                    "org.gnome.system.proxy".to_string(),
                    "ignore-hosts".to_string(),
                ],
                Ok(success_output("['localhost', '127.0.0.1']\n")),
            );

        let settings = inspect_gnome_proxy(&runner).unwrap();
        assert_eq!(
            settings.http_proxy.as_deref(),
            Some("proxy.example.com:8080")
        );
        assert_eq!(
            settings.https_proxy.as_deref(),
            Some("proxy.example.com:8443")
        );
        assert_eq!(settings.socks_proxy, None);
        assert!(settings.no_proxy.unwrap().contains("localhost"));
    }

    #[test]
    fn generate_gnome_apply_commands_structure() {
        let commands = generate_gnome_apply_commands(
            Some("proxy:8080"),
            Some("proxy:8443"),
            None,
            Some("localhost"),
        );
        assert!(commands.iter().any(|c| c.contains("mode 'manual'")));
        assert!(commands.iter().any(|c| c.contains("http host")));
        assert!(commands.iter().any(|c| c.contains("https host")));
        assert!(commands.iter().any(|c| c.contains("ignore-hosts")));
    }

    #[test]
    fn generate_gnome_disable_commands_works() {
        let commands = generate_gnome_disable_commands();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("mode 'none'"));
    }

    #[test]
    fn parse_proxy_address_with_scheme() {
        assert_eq!(
            parse_proxy_address("http://proxy:8080"),
            Some(("proxy".to_string(), 8080))
        );
    }

    #[test]
    fn parse_proxy_address_without_scheme() {
        assert_eq!(
            parse_proxy_address("proxy:8080"),
            Some(("proxy".to_string(), 8080))
        );
    }
}
