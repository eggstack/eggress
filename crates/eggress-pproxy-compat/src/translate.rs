use crate::args::PproxyArgs;
use crate::error::CompatError;
use crate::uri::PproxyUri;
use crate::warnings::TranslationOutput;

/// Translate pproxy-style arguments into Eggress TOML configuration.
pub fn translate_pproxy_args(args: &PproxyArgs) -> Result<TranslationOutput, CompatError> {
    let local_uris = args.parse_local_uris()?;
    let remote_uris = args.parse_remote_uris()?;

    // Allow empty local_uris when -ul is present (standalone UDP mode)
    let has_udp_listen = args.raw_flags.iter().any(|f| f.starts_with("udp-listen="));

    if local_uris.is_empty() && !has_udp_listen {
        return Err(CompatError::InvalidArgs {
            message: "no local listener specified (use -l or positional args)".to_string(),
        });
    }

    let mut output = translate_from_uris(&local_uris, &remote_uris, &args.raw_flags)?;

    // Merge unknown-flag warnings
    let unknown_warnings = args.unknown_flag_warnings();
    output = output.with_warnings(unknown_warnings);

    Ok(output)
}

/// Translate pproxy-style local and remote URIs into Eggress TOML.
pub fn translate_from_uris(
    local_uris: &[PproxyUri],
    remote_uris: &[PproxyUri],
    flags: &[String],
) -> Result<TranslationOutput, CompatError> {
    let mut output = TranslationOutput::new(String::new());
    let mut listeners = Vec::new();
    let mut upstreams = Vec::new();
    let mut upstream_groups = Vec::new();
    let mut rules = Vec::new();

    // Check for unsupported flags and handle supported ones
    let mut scheduler_override = None;
    let mut udp_listen_addr: Option<String> = None;
    let mut udp_remotes: Vec<String> = Vec::new();
    for flag in flags {
        if flag == "daemon" {
            output = output.with_unsupported(
                "daemon",
                "--daemon mode is not supported; use systemd or process manager",
            );
        }
        if let Some(addr) = flag.strip_prefix("udp-listen=") {
            udp_listen_addr = Some(addr.to_string());
        }
        if let Some(remote) = flag.strip_prefix("udp-remote=") {
            udp_remotes.push(remote.to_string());
        }
        if flag.starts_with("rulefile=") {
            output = output.with_unsupported(
                "rulefile",
                "--rulefile is not supported; use eggress TOML routing rules",
            );
        }
        if flag == "verbose" {
            output = output.with_warning(
                "verbose-mode",
                "pproxy -v flag detected; set RUST_LOG=debug environment variable for equivalent behavior",
            );
        }
        if let Some(scheduler_value) = flag.strip_prefix("scheduler=") {
            let mapped = match scheduler_value {
                "fa" | "first_available" => Some("first-available".to_string()),
                "rr" | "round_robin" => Some("round-robin".to_string()),
                "rc" | "random_choice" => Some("random-choice".to_string()),
                "lc" | "least_connection" => Some("least-connections".to_string()),
                _ => None,
            };
            if let Some(m) = mapped {
                scheduler_override = Some(m);
            } else {
                output = output.with_warning(
                    "scheduler",
                    format!(
                        "pproxy scheduler '{}' is not recognized; using first-available",
                        scheduler_value
                    ),
                );
            }
        }
        if flag.starts_with("alive=") {
            output = output.with_warning(
                "alive-check",
                "pproxy -a (alive check interval) is not directly mappable; configure health probes in TOML",
            );
        }
        if let Some(ssl_value) = flag.strip_prefix("ssl=") {
            output = output.with_unsupported(
                "ssl-listener",
                format!(
                    "pproxy --ssl '{}' (TLS listener) is not yet supported; configure TLS in eggress TOML",
                    ssl_value
                ),
            );
        }
        if let Some(block_value) = flag.strip_prefix("block=") {
            output = output.with_unsupported(
                "block-rules",
                format!(
                    "pproxy -b '{}' (block regex rules) is not supported; use eggress TOML routing rules",
                    block_value
                ),
            );
        }
    }

    // Process local listeners
    for (idx, local) in local_uris.iter().enumerate() {
        // Check for unsupported local protocols
        match local.scheme.as_str() {
            "ss" | "shadowsocks" => {
                // Shadowsocks listener is supported (requires explicit protocol mode)
                tracing::debug!(
                    "shadowsocks listener '{}' accepted (explicit protocol mode)",
                    local.redacted_display()
                );
            }
            "trojan" => {
                output = output.with_unsupported(
                    "trojan-listener",
                    format!(
                        "Trojan listener '{}': Trojan is upstream-only, not a local listener",
                        local.redacted_display()
                    ),
                );
                continue;
            }
            "ssh" => {
                output = output.with_unsupported(
                    "ssh-listener",
                    format!(
                        "SSH listener '{}': SSH transport is not supported",
                        local.redacted_display()
                    ),
                );
                continue;
            }
            "unix" => {
                output = output.with_unsupported(
                    "unix-listener",
                    format!(
                        "Unix socket listener '{}': Unix domain sockets are not supported",
                        local.redacted_display()
                    ),
                );
                continue;
            }
            "redir" => {
                output = output.with_unsupported(
                    "redir-listener",
                    format!(
                        "Redir listener '{}': transparent proxy redirect is not supported",
                        local.redacted_display()
                    ),
                );
                continue;
            }
            "direct" => {
                output = output.with_unsupported(
                    "direct-listener",
                    format!(
                        "Direct listener '{}': 'direct' is not a valid listener protocol",
                        local.redacted_display()
                    ),
                );
                continue;
            }
            "http" | "https" | "socks4" | "socks4a" | "socks5" => {}
            other => {
                output = output.with_unsupported(
                    "scheme",
                    format!("unknown scheme '{}' in listener URI", other),
                );
                continue;
            }
        }

        let listener_name = format!("pproxy-local-{}", idx);
        let bind = if local.host.is_empty() {
            format!("0.0.0.0:{}", local.port)
        } else {
            format!("{}:{}", local.host, local.port)
        };

        let protocols = match local.scheme.as_str() {
            "http" | "https" => vec!["http".to_string()],
            "socks4" | "socks4a" => vec!["socks4".to_string()],
            "socks5" => vec!["socks5".to_string()],
            "ss" | "shadowsocks" => vec!["shadowsocks".to_string()],
            other => {
                return Err(CompatError::InvalidArgs {
                    message: format!("unsupported scheme: {other}"),
                })
            }
        };

        let mut listener_entry = ListenerToml {
            name: listener_name.clone(),
            bind,
            protocols,
            auth: None,
            udp: None,
            shadowsocks: None,
        };

        // Handle auth on listener
        if local.scheme.as_str() == "ss" || local.scheme.as_str() == "shadowsocks" {
            // For Shadowsocks, username = method, password = password
            if let Some(ref method) = local.username {
                if let Some(ref pass) = local.password {
                    listener_entry.shadowsocks = Some(ShadowsocksToml {
                        method: method.clone(),
                        password: pass.clone(),
                    });
                    output = output.with_warning(
                        "credential-in-toml",
                        format!(
                            "Listener '{}' has plaintext credentials in generated TOML",
                            listener_name
                        ),
                    );
                }
            }
        } else if let Some(ref user) = local.username {
            if let Some(ref pass) = local.password {
                listener_entry.auth = Some(AuthToml {
                    r#type: "password".to_string(),
                    username: Some(user.clone()),
                    password: Some(pass.clone()),
                });
                output = output.with_warning(
                    "credential-in-toml",
                    format!(
                        "Listener '{}' has plaintext credentials in generated TOML",
                        listener_name
                    ),
                );
            }
        }

        listeners.push(listener_entry);

        // If no remotes and no UDP remotes, create a direct rule
        if remote_uris.is_empty() && udp_remotes.is_empty() {
            output = output.with_warning(
                "direct-mode",
                format!(
                    "Listener '{}' has no upstream; traffic will be direct",
                    listener_name
                ),
            );
        }
    }

    // If -ul is specified, add standalone UDP config to the first listener
    if let Some(ref addr) = udp_listen_addr {
        let bind = parse_udp_listen_addr(addr);
        if let Some(listener) = listeners.first_mut() {
            listener.udp = Some(UdpToml {
                mode: Some("standalone_pproxy_udp".to_string()),
                bind: Some(bind),
            });
        } else {
            // No listener created (all were unsupported schemes); add a default SOCKS5 listener
            listeners.push(ListenerToml {
                name: "pproxy-local-0".to_string(),
                bind: "0.0.0.0:1080".to_string(),
                protocols: vec!["socks5".to_string()],
                auth: None,
                udp: Some(UdpToml {
                    mode: Some("standalone_pproxy_udp".to_string()),
                    bind: Some(parse_udp_listen_addr(addr)),
                }),
                shadowsocks: None,
            });
            output = output.with_warning(
                "ul-no-listener",
                "-ul specified without a compatible -l listener; added default SOCKS5 listener on :1080",
            );
        }
    }

    // Process remote upstreams
    for (idx, remote) in remote_uris.iter().enumerate() {
        // Check for unsupported upstream protocols
        match remote.scheme.as_str() {
            "ss" | "shadowsocks" => {
                // Shadowsocks upstream is fully supported (AEAD methods only)
            }
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "trojan" | "direct" => {}
            "ssh" => {
                output = output.with_unsupported(
                    "ssh-upstream",
                    format!(
                        "SSH upstream '{}': SSH transport is not supported",
                        remote.redacted_display()
                    ),
                );
                continue;
            }
            "unix" => {
                output = output.with_unsupported(
                    "unix-upstream",
                    format!(
                        "Unix socket upstream '{}': Unix domain sockets are not supported",
                        remote.redacted_display()
                    ),
                );
                continue;
            }
            "redir" => {
                output = output.with_unsupported(
                    "redir-upstream",
                    format!(
                        "Redir upstream '{}': transparent proxy redirect is not supported as upstream",
                        remote.redacted_display()
                    ),
                );
                continue;
            }
            other => {
                output = output.with_unsupported(
                    "scheme",
                    format!("unknown scheme '{}' in upstream URI", other),
                );
                continue;
            }
        }

        let upstream_id = format!("pproxy-upstream-{}", idx);
        let _uri_str = remote.redacted_display();

        // Build the actual URI with credentials for the config (since eggress needs them)
        let config_uri = build_config_uri(remote);

        upstreams.push(UpstreamToml {
            id: upstream_id.clone(),
            uri: config_uri,
        });
    }

    // Process UDP remote upstreams
    let mut udp_upstream_ids = Vec::new();
    for (idx, remote_str) in udp_remotes.iter().enumerate() {
        let remote_uri =
            crate::uri::parse_pproxy_uri(remote_str).map_err(|e| CompatError::InvalidArgs {
                message: format!("invalid UDP remote URI '{}': {}", remote_str, e),
            })?;

        // Check for unsupported upstream protocols
        match remote_uri.scheme.as_str() {
            "ss" | "shadowsocks" => {}
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "trojan" | "direct" => {}
            other => {
                output = output.with_unsupported(
                    "scheme",
                    format!("unknown scheme '{}' in UDP upstream URI", other),
                );
                continue;
            }
        }

        let upstream_id = format!("pproxy-udp-upstream-{}", idx);
        let config_uri = build_config_uri(&remote_uri);

        upstreams.push(UpstreamToml {
            id: upstream_id.clone(),
            uri: config_uri,
        });
        udp_upstream_ids.push(upstream_id);
    }

    // Build upstream groups and rules for TCP
    if !upstreams.is_empty()
        && upstreams
            .iter()
            .any(|u| u.id.starts_with("pproxy-upstream-"))
    {
        let group_id = "pproxy-chain".to_string();
        let member_ids: Vec<String> = upstreams
            .iter()
            .filter(|u| u.id.starts_with("pproxy-upstream-"))
            .map(|u| u.id.clone())
            .collect();
        let scheduler = scheduler_override.unwrap_or_else(|| {
            if member_ids.len() > 1 {
                "round-robin".to_string()
            } else {
                "first-available".to_string()
            }
        });

        upstream_groups.push(UpstreamGroupToml {
            id: group_id.clone(),
            scheduler,
            members: member_ids,
            fallback: "reject".to_string(),
        });

        rules.push(RuleToml {
            id: "pproxy-default".to_string(),
            any: true,
            upstream_group: group_id,
            r#match: None,
        });
    }

    // Build upstream groups and rules for UDP
    if !udp_upstream_ids.is_empty() {
        let group_id = "pproxy-udp-chain".to_string();
        let scheduler = if udp_upstream_ids.len() > 1 {
            "round-robin".to_string()
        } else {
            "first-available".to_string()
        };

        upstream_groups.push(UpstreamGroupToml {
            id: group_id.clone(),
            scheduler,
            members: udp_upstream_ids,
            fallback: "reject".to_string(),
        });

        rules.push(RuleToml {
            id: "pproxy-udp-default".to_string(),
            any: false,
            upstream_group: group_id,
            r#match: Some(MatchToml {
                transport: "udp".to_string(),
            }),
        });
    }

    // Generate TOML
    let toml_str = generate_toml(&listeners, &upstreams, &upstream_groups, &rules);

    Ok(TranslationOutput::new(toml_str)
        .with_warnings(output.warnings)
        .with_unsupported_features(output.unsupported))
}

/// Parse a `-ul` address value into a bind address.
///
/// Handles formats: `:1081`, `0.0.0.0:1081`, `127.0.0.1:1081`, `socks5://:1081`, plain port `1081`.
fn parse_udp_listen_addr(addr: &str) -> String {
    // If it's a URI like socks5://:1081, extract host:port after ://
    if let Some(rest) = addr.find("://") {
        let endpoint = &addr[rest + 3..];
        if endpoint.is_empty() || endpoint == ":" {
            return "0.0.0.0:0".to_string();
        }
        if endpoint.starts_with(':') {
            return format!("0.0.0.0{}", endpoint);
        }
        return endpoint.to_string();
    }

    // Plain address formats
    if addr.is_empty() || addr == ":" {
        "0.0.0.0:0".to_string()
    } else if addr.starts_with(':') {
        format!("0.0.0.0{}", addr)
    } else if addr.contains(':') {
        addr.to_string()
    } else {
        // Just a port number
        format!("0.0.0.0:{}", addr)
    }
}

fn percent_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

fn build_config_uri(remote: &PproxyUri) -> String {
    let scheme = if remote.scheme == "https" {
        "http".to_string()
    } else if remote.scheme == "socks4a" {
        "socks4".to_string()
    } else {
        remote.scheme.clone()
    };
    let cred_str = match (&remote.username, &remote.password) {
        (Some(user), Some(pass)) => {
            format!("{}:{}@", percent_encode(user), percent_encode(pass))
        }
        (Some(user), None) => {
            format!("{}:{}", percent_encode(user), "")
        }
        (None, Some(pass)) => {
            // Password-only format (e.g., trojan://password@host:port)
            format!("{}@", percent_encode(pass))
        }
        _ => String::new(),
    };
    let tls = remote.tls || remote.scheme == "https";
    let tls_suffix = if tls { "+tls" } else { "" };
    let rule_str = match &remote.rule {
        Some(r) => format!("?rule={}", r),
        None => String::new(),
    };
    format!(
        "{}://{}{}:{}{}{}",
        scheme, cred_str, remote.host, remote.port, tls_suffix, rule_str,
    )
}

#[derive(serde::Serialize, Clone)]
struct ListenerToml {
    name: String,
    bind: String,
    protocols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<AuthToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    udp: Option<UdpToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shadowsocks: Option<ShadowsocksToml>,
}

#[derive(serde::Serialize, Clone)]
struct ShadowsocksToml {
    method: String,
    password: String,
}

#[derive(serde::Serialize, Clone)]
struct UdpToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bind: Option<String>,
}

#[derive(serde::Serialize, Clone)]
struct AuthToml {
    #[serde(rename = "type")]
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

#[derive(serde::Serialize, Clone)]
struct UpstreamToml {
    id: String,
    uri: String,
}

#[derive(serde::Serialize, Clone)]
struct UpstreamGroupToml {
    id: String,
    scheduler: String,
    members: Vec<String>,
    fallback: String,
}

#[derive(serde::Serialize, Clone)]
struct RuleToml {
    id: String,
    any: bool,
    upstream_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "match")]
    r#match: Option<MatchToml>,
}

#[derive(serde::Serialize, Clone)]
struct MatchToml {
    transport: String,
}

#[derive(serde::Serialize)]
struct ConfigToml {
    version: u32,
    listeners: Vec<ListenerToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    upstreams: Vec<UpstreamToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    upstream_groups: Vec<UpstreamGroupToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rules: Vec<RuleToml>,
}

fn generate_toml(
    listeners: &[ListenerToml],
    upstreams: &[UpstreamToml],
    upstream_groups: &[UpstreamGroupToml],
    rules: &[RuleToml],
) -> String {
    let config = ConfigToml {
        version: 1,
        listeners: listeners.to_vec(),
        upstreams: upstreams.to_vec(),
        upstream_groups: upstream_groups.to_vec(),
        rules: rules.to_vec(),
    };

    toml::to_string_pretty(&config)
        .unwrap_or_else(|_| "# failed to serialize config\nversion = 1\n".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_socks5_direct() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("socks5"));
        assert!(output.toml.contains("127.0.0.1:1080"));
        assert!(!output.has_unsupported());
    }

    #[test]
    fn test_translate_http_direct() {
        let args = PproxyArgs::parse(&["-l".into(), "http://0.0.0.0:8080".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("http"));
        assert!(output.toml.contains("0.0.0.0:8080"));
    }

    #[test]
    fn test_translate_socks5_through_http_upstream() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("pproxy-upstream-0"));
        assert!(output.toml.contains("pproxy-chain"));
        assert!(output.toml.contains("http://proxy:8080"));
    }

    #[test]
    fn test_translate_chain() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy1:8080".into(),
            "-r".into(),
            "socks5://proxy2:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("pproxy-upstream-0"));
        assert!(output.toml.contains("pproxy-upstream-1"));
        assert!(output.toml.contains("round-robin"));
    }

    #[test]
    fn test_translate_auth_credentials_redacted() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://user:secret@127.0.0.1:1080".into()])
            .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        // Auth should be present
        assert!(output.toml.contains("password"));
        // Warning about plaintext creds
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "credential-in-toml"));
    }

    #[test]
    fn test_translate_shadowsocks_listener_supported() {
        let args =
            PproxyArgs::parse(&["-l".into(), "ss://aes-256-gcm:secret@proxy:8388".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(
            !output
                .unsupported
                .iter()
                .any(|u| u.feature == "shadowsocks-listener"),
            "shadowsocks listener should be supported"
        );
    }

    #[test]
    fn test_translate_daemon_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--daemon".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
    }

    #[test]
    fn test_no_local_listener_error() {
        let args = PproxyArgs::parse(&[]).unwrap();
        let result = translate_pproxy_args(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_toml_roundtrip() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        // Should be valid TOML
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert_eq!(parsed["version"].as_integer(), Some(1));
        let listeners = parsed["listeners"].as_array().unwrap();
        assert_eq!(listeners.len(), 1);
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1);
    }

    #[test]
    fn test_verbose_flag_emits_warning() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-v".into()])
            .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "verbose-mode"));
    }

    #[test]
    fn test_scheduler_flag_maps_to_toml() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
            "-s".into(),
            "rr".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("round-robin"));
    }

    #[test]
    fn test_scheduler_flag_all_values() {
        for (input, expected) in &[
            ("fa", "first-available"),
            ("first_available", "first-available"),
            ("rr", "round-robin"),
            ("round_robin", "round-robin"),
            ("rc", "random-choice"),
            ("random_choice", "random-choice"),
            ("lc", "least-connections"),
            ("least_connection", "least-connections"),
        ] {
            let args = PproxyArgs::parse(&[
                "-l".into(),
                "socks5://127.0.0.1:1080".into(),
                "-r".into(),
                "http://proxy:8080".into(),
                "-s".into(),
                input.to_string(),
            ])
            .unwrap();
            let output = translate_pproxy_args(&args).unwrap();
            assert!(
                output.toml.contains(expected),
                "expected '{}' for scheduler input '{}', got:\n{}",
                expected,
                input,
                output.toml
            );
        }
    }

    #[test]
    fn test_alive_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-a".into(),
            "10".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "alive-check"));
    }

    #[test]
    fn test_ssl_flag_emits_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "ssl-listener"));
    }

    #[test]
    fn test_block_flag_emits_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-b".into(),
            ".*\\.example\\.com".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "block-rules"));
    }

    #[test]
    fn test_rulefile_flag_emits_unsupported_without_unknown_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--rulefile".into(),
            "rules.txt".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.unsupported.iter().any(|u| u.feature == "rulefile"));
        assert!(!output.warnings.iter().any(|w| w.category == "unknown-flag"));
    }

    #[test]
    fn test_unknown_flags_emitted_as_warnings() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--totally-unknown".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "unknown-flag" && w.message.contains("--totally-unknown")));
    }

    #[test]
    fn test_scheduler_default_first_available() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("first-available"));
    }

    #[test]
    fn test_scheduler_default_round_robin_for_multiple_remotes() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy1:8080".into(),
            "-r".into(),
            "socks5://proxy2:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("round-robin"));
    }

    #[test]
    fn test_translate_ul_generates_standalone_udp() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-ul".into(),
            ":1081".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        assert!(output.toml.contains("standalone_pproxy_udp"));
        assert!(output.toml.contains("0.0.0.0:1081"));
    }

    #[test]
    fn test_translate_ur_generates_upstream() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-ul".into(),
            ":1081".into(),
            "-ur".into(),
            "socks5://proxy:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        assert!(output.toml.contains("pproxy-udp-upstream-0"));
        assert!(output.toml.contains("pproxy-udp-chain"));
        assert!(output.toml.contains("socks5://proxy:1080"));
        assert!(output.toml.contains("transport = \"udp\""));
    }

    #[test]
    fn test_translate_ul_and_ur_together() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://tcp-proxy:8080".into(),
            "-ul".into(),
            ":1081".into(),
            "-ur".into(),
            "socks5://udp-proxy:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        // TCP upstream group
        assert!(output.toml.contains("pproxy-upstream-0"));
        assert!(output.toml.contains("pproxy-chain"));
        // UDP upstream group
        assert!(output.toml.contains("pproxy-udp-upstream-0"));
        assert!(output.toml.contains("pproxy-udp-chain"));
        // UDP listener config
        assert!(output.toml.contains("standalone_pproxy_udp"));
        // Two rules: default (any) and UDP
        assert!(output.toml.contains("pproxy-default"));
        assert!(output.toml.contains("pproxy-udp-default"));
    }

    #[test]
    fn test_ul_without_listen_adds_default_socks5() {
        let args = PproxyArgs::parse(&["-ul".into(), ":1081".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        // Should have added a default SOCKS5 listener
        assert!(output.toml.contains("pproxy-local-0"));
        assert!(output.toml.contains("socks5"));
        assert!(output.toml.contains("standalone_pproxy_udp"));
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "ul-no-listener"));
    }

    #[test]
    fn test_ul_address_formats() {
        // Test various -ul address formats
        for (input, expected_bind) in &[
            (":1081", "0.0.0.0:1081"),
            ("0.0.0.0:1081", "0.0.0.0:1081"),
            ("127.0.0.1:1081", "127.0.0.1:1081"),
            ("1081", "0.0.0.0:1081"),
            ("socks5://:1081", "0.0.0.0:1081"),
        ] {
            let args = PproxyArgs::parse(&[
                "-l".into(),
                "socks5://127.0.0.1:1080".into(),
                "-ul".into(),
                input.to_string(),
            ])
            .unwrap();
            let output = translate_pproxy_args(&args).unwrap();
            assert!(
                output.toml.contains(expected_bind),
                "expected bind '{}' for -ul input '{}', got:\n{}",
                expected_bind,
                input,
                output.toml
            );
        }
    }

    #[test]
    fn test_ul_no_tcp_direct_warning_when_ur_present() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-ul".into(),
            ":1081".into(),
            "-ur".into(),
            "socks5://proxy:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        // No direct-mode warning when UDP upstream is specified
        assert!(!output.warnings.iter().any(|w| w.category == "direct-mode"));
    }

    #[test]
    fn test_valid_toml_roundtrip_with_udp() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-ul".into(),
            ":1081".into(),
            "-ur".into(),
            "socks5://proxy:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert_eq!(parsed["version"].as_integer(), Some(1));
        let listeners = parsed["listeners"].as_array().unwrap();
        assert_eq!(listeners.len(), 1);
        let udp = &listeners[0]["udp"];
        assert_eq!(udp["mode"].as_str(), Some("standalone_pproxy_udp"));
        assert_eq!(udp["bind"].as_str(), Some("0.0.0.0:1081"));
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1);
        let groups = parsed["upstream_groups"].as_array().unwrap();
        assert!(groups
            .iter()
            .any(|g| g["id"].as_str() == Some("pproxy-udp-chain")));
        let rules = parsed["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|r| r["id"].as_str() == Some("pproxy-udp-default")));
    }
}
