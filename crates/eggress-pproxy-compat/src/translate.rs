use crate::args::PproxyArgs;
use crate::error::CompatError;
use crate::uri::PproxyUri;
use crate::warnings::TranslationOutput;

/// Translate pproxy-style arguments into Eggress TOML configuration.
pub fn translate_pproxy_args(args: &PproxyArgs) -> Result<TranslationOutput, CompatError> {
    let mut pre_filter_output = TranslationOutput::new(String::new());

    // Pre-filter: detect unsupported jump chain (__ syntax) in raw remote URIs
    let mut filtered_remotes = Vec::new();
    for raw_remote in args.remotes.iter() {
        if raw_remote.contains("__") {
            pre_filter_output = pre_filter_output.with_unsupported(
                "backward-jump-chain",
                format!(
                    "Jump chain syntax '{}' is not supported; each URI must be a separate -r argument",
                    raw_remote
                ),
            );
        } else {
            filtered_remotes.push(raw_remote.clone());
        }
    }

    let local_uris = args.parse_local_uris()?;
    let remote_uris: Vec<PproxyUri> = filtered_remotes
        .iter()
        .map(|s| crate::uri::parse_pproxy_uri(s))
        .collect::<Result<Vec<_>, _>>()?;

    // Allow empty local_uris when -ul is present (standalone UDP mode)
    let has_udp_listen = args.raw_flags.iter().any(|f| f.starts_with("udp-listen="));

    if local_uris.is_empty() && !has_udp_listen {
        return Err(CompatError::InvalidArgs {
            message: "no local listener specified (use -l or positional args)".to_string(),
        });
    }

    let mut output = translate_from_uris(&local_uris, &remote_uris, &args.raw_flags)?;

    // Merge pre-filter unsupported features (e.g., backward-jump-chain)
    output = output.with_unsupported_features(pre_filter_output.unsupported);

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
    let mut reverse_servers = Vec::new();
    let mut reverse_clients = Vec::new();

    // Check for unsupported flags and handle supported ones
    let mut scheduler_override = None;
    let mut udp_listen_addr: Option<String> = None;
    let mut udp_remotes: Vec<String> = Vec::new();
    let mut ssl_config: Option<TlsToml> = None;
    let mut block_rules: Vec<String> = Vec::new();
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
        if let Some(rulefile_path) = flag.strip_prefix("rulefile=") {
            match std::fs::read_to_string(rulefile_path) {
                Ok(content) => {
                    for (line_num, line) in content.lines().enumerate() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((pattern, action)) = line.split_once("->") {
                            let pattern = pattern.trim().to_string();
                            let action = action.trim();
                            if action == "reject" || action == "block" {
                                block_rules.push(pattern);
                            } else {
                                output = output.with_warning(
                                    "rulefile-partial",
                                    format!(
                                        "rulefile line {}: complex rule '{}' -> '{}' cannot be auto-translated; use eggress TOML [[rules]] with structured matchers",
                                        line_num + 1, pattern.trim(), action
                                    ),
                                );
                            }
                        } else {
                            output = output.with_warning(
                                "rulefile-parse",
                                format!(
                                    "rulefile line {}: unrecognized format '{}'; expected 'pattern -> action'",
                                    line_num + 1, line
                                ),
                            );
                        }
                    }
                }
                Err(e) => {
                    output = output.with_warning(
                        "rulefile-read",
                        format!(
                            "failed to read rulefile '{}': {}; configure rules in eggress TOML instead",
                            rulefile_path, e
                        ),
                    );
                }
            }
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
        if let Some(interval) = flag.strip_prefix("alive=") {
            output = output.with_warning(
                "alive-check",
                format!(
                    "pproxy -a {} (alive check interval) maps to eggress health probes; configure 'health.interval' on each [[upstreams]] entry (e.g., interval = \"{}s\")",
                    interval, interval
                ),
            );
        }
        if let Some(ssl_value) = flag.strip_prefix("ssl=") {
            let parts: Vec<&str> = ssl_value.splitn(2, ',').collect();
            let cert = parts[0].to_string();
            let key = if parts.len() > 1 {
                Some(parts[1].to_string())
            } else {
                None
            };
            ssl_config = Some(TlsToml { cert, key });
        }
        if let Some(block_value) = flag.strip_prefix("block=") {
            block_rules.push(block_value.to_string());
        }
        if flag == "pac" {
            output = output.with_warning(
                "pac-serving",
                "pproxy --pac flag detected; configure PAC serving in eggress TOML admin.pac block",
            );
        }
        if flag == "test" {
            output = output.with_warning(
                "test-mode",
                "pproxy --test flag detected; use 'eggress upstream test -c <config>' to test upstream connectivity",
            );
        }
        if flag == "sys" {
            output = output.with_warning(
                "system-proxy",
                "pproxy --sys flag detected; use 'eggress system-proxy inspect' to view system proxy settings",
            );
        }
        if flag.starts_with("log=") {
            output = output.with_warning(
                "log-file",
                "pproxy --log flag detected; eggress logs to stderr via tracing-subscriber; redirect stderr with shell redirection for file logging",
            );
        }
        if flag == "reuse" {
            output = output.with_warning(
                "reuse-connection",
                "pproxy --reuse (connection pooling) is not implemented by design; eggress uses one upstream connection per session (intentional non-parity)",
            );
        }
        if flag == "get" {
            output = output.with_warning(
                "get-url",
                "pproxy --get flag detected; use 'curl --proxy <proxy-uri> <url>' instead",
            );
        }
    }

    // Process local listeners
    for (idx, local) in local_uris.iter().enumerate() {
        // Reverse-mode listeners (bind/listen/backward/rebind) → reverse_servers
        if local.is_reverse_listener() {
            let bind = local.bind_display();
            let server_id = format!("pproxy-reverse-server-{}", idx);
            reverse_servers.push(ReverseServerToml {
                id: server_id,
                control_bind: bind,
                auth_username: local.username.clone(),
                auth_password: local.password.clone(),
            });
            // Emit credential warning if auth present
            if local.username.is_some() {
                output = output.with_warning(
                    "credential-in-toml",
                    format!(
                        "Reverse server 'pproxy-reverse-server-{}' has plaintext credentials in generated TOML",
                        idx
                    ),
                );
            }
            continue;
        }

        // Check for unsupported local protocols
        match local.scheme.as_str() {
            "ss" | "shadowsocks" => {
                // Shadowsocks listener is supported (requires explicit protocol mode)
                tracing::debug!(
                    "shadowsocks listener '{}' accepted (explicit protocol mode)",
                    local.redacted_display()
                );
            }
            "ssr" => {
                output = output.with_unsupported(
                    "ssr-listener",
                    format!(
                        "ShadowsocksR (SSR) listener '{}': SSR protocol, obfs, and legacy features are not supported",
                        local.redacted_display()
                    ),
                );
                continue;
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
                // Translate unix:// listener to TOML with unix socket config
                tracing::debug!(
                    "unix socket listener '{}' accepted (unix socket mode)",
                    local.redacted_display()
                );
            }
            "redir" => {
                // Translate redir:// listener to TOML with transparent proxy config
                tracing::debug!(
                    "redir listener '{}' accepted (transparent proxy mode)",
                    local.redacted_display()
                );
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
        let bind = local.bind_display();

        let protocols = match local.scheme.as_str() {
            "http" | "https" => vec!["http".to_string()],
            "socks4" | "socks4a" => vec!["socks4".to_string()],
            "socks5" => vec!["socks5".to_string()],
            "ss" | "shadowsocks" => vec!["shadowsocks".to_string()],
            "redir" => vec!["http".to_string()],
            "unix" => vec!["socks5".to_string()],
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
            transparent: None,
            unix: None,
            tls: None,
        };

        // Handle auth on listener
        if local.scheme.as_str() == "ss" || local.scheme.as_str() == "shadowsocks" {
            // For Shadowsocks, username = method, password = password
            if let Some(ref method) = local.username {
                // Check for legacy stream cipher methods
                if crate::uri::is_legacy_ss_method(method) {
                    output = output.with_unsupported(
                        "legacy-cipher",
                        format!(
                            "Shadowsocks listener '{}': legacy stream cipher method '{}' is not supported; use an AEAD method (aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305)",
                            local.redacted_display(),
                            method
                        ),
                    );
                }
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

        // Add transparent proxy config for redir://
        if local.scheme == "redir" {
            listener_entry.transparent = Some(TransparentToml {
                enabled: true,
                protocol: "redir".to_string(),
            });
        }

        // Add unix socket config for unix://
        if local.scheme == "unix" {
            let path = local
                .path
                .clone()
                .unwrap_or_else(|| "/tmp/eggress.sock".to_string());
            listener_entry.unix = Some(UnixToml {
                path,
                unlink_existing: false,
            });
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

    // Apply --ssl TLS config to the first listener
    if let Some(tls) = ssl_config {
        if let Some(listener) = listeners.first_mut() {
            listener.tls = Some(tls);
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
                transparent: None,
                unix: None,
                tls: None,
            });
            output = output.with_warning(
                "ul-no-listener",
                "-ul specified without a compatible -l listener; added default SOCKS5 listener on :1080",
            );
        }
    }

    // Process remote upstreams
    for (idx, remote) in remote_uris.iter().enumerate() {
        // Backward/upstream URIs with +in modifier → reverse_clients
        if remote.is_backward() {
            // Backward + SSL (+ssl modifier) is not supported
            if remote.ssl {
                output = output.with_unsupported(
                    "backward-tls",
                    format!(
                        "Backward upstream '{}': TLS on backward connections is not supported",
                        remote.redacted_display()
                    ),
                );
            }
            let server_addr = remote.endpoint_display();
            let client_id = format!("pproxy-reverse-client-{}", idx);
            reverse_clients.push(ReverseClientToml {
                id: client_id,
                server_addr,
                auth_username: remote.username.clone(),
                auth_password: remote.password.clone(),
                parallel_connections: if remote.backward_num() > 1 {
                    Some(remote.backward_num())
                } else {
                    None
                },
            });
            // Emit credential warning if auth present
            if remote.username.is_some() {
                output = output.with_warning(
                    "credential-in-toml",
                    format!(
                        "Reverse client 'pproxy-reverse-client-{}' has plaintext credentials in generated TOML",
                        idx
                    ),
                );
            }
            continue;
        }

        // Check for unsupported upstream protocols
        match remote.scheme.as_str() {
            "ss" | "shadowsocks" => {
                // Shadowsocks upstream is fully supported (AEAD methods only)
            }
            "ssr" => {
                output = output.with_unsupported(
                    "ssr-upstream",
                    format!(
                        "ShadowsocksR (SSR) upstream '{}': SSR protocol, obfs, and legacy features are not supported",
                        remote.redacted_display()
                    ),
                );
                continue;
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
            "ssr" => {
                output = output.with_unsupported(
                    "ssr-upstream",
                    format!(
                        "ShadowsocksR (SSR) UDP upstream '{}': SSR protocol, obfs, and legacy features are not supported",
                        remote_uri.redacted_display()
                    ),
                );
                continue;
            }
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
            host_regex: None,
            reject: None,
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
            host_regex: None,
            reject: None,
        });
    }

    // Prepend block rules (first-match-wins: block rules before default rules)
    if !block_rules.is_empty() {
        let mut all_rules = Vec::new();
        for (idx, pattern) in block_rules.iter().enumerate() {
            all_rules.push(RuleToml {
                id: format!("pproxy-block-{}", idx),
                any: false,
                upstream_group: String::new(),
                r#match: None,
                host_regex: Some(pattern.clone()),
                reject: Some("blocked by pproxy -b rule".to_string()),
            });
        }
        all_rules.extend(rules);
        rules = all_rules;
    }

    // Generate TOML
    let toml_str = generate_toml(
        &listeners,
        &upstreams,
        &upstream_groups,
        &rules,
        &reverse_servers,
        &reverse_clients,
    );

    Ok(TranslationOutput::new(toml_str)
        .with_warnings(output.warnings)
        .with_unsupported_features(output.unsupported))
}

/// Parse a `-ul` address value into a bind address.
///
/// Handles formats: `:1081`, `0.0.0.0:1081`, `127.0.0.1:1081`, `socks5://:1081`, plain port `1081`.
fn parse_udp_listen_addr(addr: &str) -> String {
    // If it's a URI like socks5://:1081, extract host:port after ://
    if addr.contains("://") {
        return crate::uri::parse_pproxy_uri(addr)
            .map(|uri| uri.bind_display())
            .unwrap_or_else(|_| "0.0.0.0:0".to_string());
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
    let mut scheme = if remote.scheme == "https" {
        "http".to_string()
    } else if remote.scheme == "socks4a" {
        "socks4".to_string()
    } else {
        remote.scheme.clone()
    };
    if remote.tls || remote.scheme == "https" {
        scheme.push_str("+tls");
    }
    let cred_str = match (&remote.username, &remote.password) {
        (Some(user), Some(pass)) if user.is_empty() => {
            format!("{}@", percent_encode(pass))
        }
        (Some(user), Some(pass)) => {
            format!("{}:{}@", percent_encode(user), percent_encode(pass))
        }
        (Some(user), None) => {
            format!("{}@", percent_encode(user))
        }
        (None, Some(pass)) => {
            // Password-only format (e.g., trojan://password@host:port)
            format!("{}@", percent_encode(pass))
        }
        _ => String::new(),
    };
    let rule_str = match &remote.rule {
        Some(r) => format!("?rule={}", r),
        None => String::new(),
    };
    format!(
        "{}://{}{}{}",
        scheme,
        cred_str,
        remote.endpoint_display(),
        rule_str,
    )
}

#[derive(serde::Serialize, Clone)]
struct TlsToml {
    cert: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    transparent: Option<TransparentToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unix: Option<UnixToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<TlsToml>,
}

#[derive(serde::Serialize, Clone)]
struct TransparentToml {
    enabled: bool,
    protocol: String,
}

#[derive(serde::Serialize, Clone)]
struct UnixToml {
    path: String,
    unlink_existing: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    host_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reject: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reverse_servers: Vec<ReverseServerToml>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reverse_clients: Vec<ReverseClientToml>,
}

#[derive(serde::Serialize, Clone)]
struct ReverseServerToml {
    id: String,
    control_bind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_password: Option<String>,
}

#[derive(serde::Serialize, Clone)]
struct ReverseClientToml {
    id: String,
    server_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_connections: Option<u32>,
}

fn generate_toml(
    listeners: &[ListenerToml],
    upstreams: &[UpstreamToml],
    upstream_groups: &[UpstreamGroupToml],
    rules: &[RuleToml],
    reverse_servers: &[ReverseServerToml],
    reverse_clients: &[ReverseClientToml],
) -> String {
    let config = ConfigToml {
        version: 1,
        listeners: listeners.to_vec(),
        upstreams: upstreams.to_vec(),
        upstream_groups: upstream_groups.to_vec(),
        rules: rules.to_vec(),
        reverse_servers: reverse_servers.to_vec(),
        reverse_clients: reverse_clients.to_vec(),
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
    fn test_translate_explicit_tls_upstream_uses_scheme_suffix() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+tls://proxy:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("socks5+tls://proxy:1080"));
        assert!(!output.toml.contains("proxy:1080+tls"));
    }

    #[test]
    fn test_translate_ipv6_upstream_brackets_host() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://[::1]:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("socks5://[::1]:1080"));
    }

    #[test]
    fn test_translate_trojan_password_only_upstream() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "trojan://secret@proxy:443".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("trojan://secret@proxy:443"));
        assert!(!output.toml.contains("trojan://:secret@proxy:443"));
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
    fn test_ssl_flag_generates_tls_config() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("cert.pem"));
        assert!(output.toml.contains("key.pem"));
        assert!(!output
            .unsupported
            .iter()
            .any(|u| u.feature == "ssl-listener"));
    }

    #[test]
    fn test_ssl_cert_only_generates_tls_config() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
            "cert.pem".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("cert.pem"));
        assert!(!output.has_unsupported());
    }

    #[test]
    fn test_block_flag_generates_reject_rule() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-b".into(),
            ".*\\.example\\.com".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("pproxy-block-0"));
        assert!(output.toml.contains("reject"));
        assert!(output.toml.contains(".*\\.example\\.com"));
        assert!(!output.has_unsupported());
    }

    #[test]
    fn test_block_flag_toml_roundtrip() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-b".into(),
            ".*\\.blocked\\.com".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let rules = parsed["rules"].as_array().unwrap();
        let block_rule = rules
            .iter()
            .find(|r| r["id"].as_str() == Some("pproxy-block-0"))
            .unwrap();
        assert_eq!(
            block_rule["host_regex"].as_str(),
            Some(".*\\.blocked\\.com")
        );
        assert_eq!(
            block_rule["reject"].as_str(),
            Some("blocked by pproxy -b rule")
        );
    }

    #[test]
    fn test_rulefile_missing_file_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--rulefile".into(),
            "/nonexistent/rules.txt".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "rulefile-read"));
    }

    #[test]
    fn test_rulefile_generates_block_rules() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("eggress_test_rulefile");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rules.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            "# comment\n.*\\.blocked\\.com -> reject\nother\\.com -> http://proxy:8080"
        )
        .unwrap();

        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--rulefile".into(),
            path.to_str().unwrap().into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("pproxy-block-0"));
        assert!(output.toml.contains(".*\\.blocked\\.com"));
        // Complex rule should emit a warning
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "rulefile-partial"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_translate_ssr_listener_unsupported() {
        let args = PproxyArgs::parse(&["-l".into(), "ssr://aes-256-ctr:secret@proxy:8388".into()])
            .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "ssr-listener"));
    }

    #[test]
    fn test_translate_ssr_upstream_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "ssr://aes-256-ctr:secret@proxy:8388".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "ssr-upstream"));
    }

    #[test]
    fn test_translate_legacy_cipher_listener_unsupported() {
        let args =
            PproxyArgs::parse(&["-l".into(), "ss://aes-128-ctr:secret@proxy:8388".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "legacy-cipher"));
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
            ("socks5://[::1]:1081", "[::1]:1081"),
            ("socks5://user:pass@[::1]:1081?ignored=true", "[::1]:1081"),
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

    #[test]
    fn test_translate_socks5_backward_emits_reverse_client() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in://user:pass@acceptor:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let clients = parsed["reverse_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["server_addr"].as_str(), Some("acceptor:1080"));
        assert_eq!(clients[0]["auth_username"].as_str(), Some("user"));
        assert_eq!(clients[0]["auth_password"].as_str(), Some("pass"));
        // Should NOT appear in regular upstreams
        assert!(
            parsed.get("upstreams").is_none()
                || parsed["upstreams"]
                    .as_array()
                    .map_or(true, |a| a.is_empty())
        );
    }

    #[test]
    fn test_translate_bind_listener_emits_reverse_server() {
        let args = PproxyArgs::parse(&["-l".into(), "bind://0.0.0.0:8080".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let servers = parsed["reverse_servers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["control_bind"].as_str(), Some("0.0.0.0:8080"));
        // Should NOT appear in regular listeners
        let listeners = parsed["listeners"].as_array().unwrap();
        assert!(listeners.is_empty());
    }

    #[test]
    fn test_translate_backward_listener_emits_reverse_server() {
        let args = PproxyArgs::parse(&["-l".into(), "backward://0.0.0.0:8080".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let servers = parsed["reverse_servers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["control_bind"].as_str(), Some("0.0.0.0:8080"));
    }

    #[test]
    fn test_translate_backward_with_parallel_connections() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in+in://acceptor:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let clients = parsed["reverse_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["parallel_connections"].as_integer(), Some(2));
    }

    #[test]
    fn test_translate_backward_with_jump_chain_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in://a:1__http://b:2".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(
            output
                .unsupported
                .iter()
                .any(|u| u.feature == "backward-jump-chain"),
            "expected backward-jump-chain unsupported, got: {:?}",
            output.unsupported
        );
        // The invalid URI should be filtered out — no reverse_clients generated
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert!(
            parsed.get("reverse_clients").is_none()
                || parsed["reverse_clients"]
                    .as_array()
                    .map_or(true, |a| a.is_empty())
        );
    }

    #[test]
    fn test_translate_backward_tls_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in+ssl://acceptor:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.has_unsupported());
        assert!(
            output
                .unsupported
                .iter()
                .any(|u| u.feature == "backward-tls"),
            "expected backward-tls unsupported, got: {:?}",
            output.unsupported
        );
    }

    #[test]
    fn test_translate_reverse_server_with_auth() {
        let args =
            PproxyArgs::parse(&["-l".into(), "bind://user:pass@0.0.0.0:8080".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let servers = parsed["reverse_servers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0]["auth_username"].as_str(), Some("user"));
        assert_eq!(servers[0]["auth_password"].as_str(), Some("pass"));
        // Credential warning emitted
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "credential-in-toml"));
    }

    #[test]
    fn test_translate_backward_no_parallel_when_single() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in://acceptor:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let clients = parsed["reverse_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        // parallel_connections should not be present for single +in
        assert!(clients[0].get("parallel_connections").is_none());
    }

    #[test]
    fn test_translate_backward_toml_parses() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in+in://user:pass@acceptor:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        // Verify TOML is valid
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert_eq!(parsed["version"].as_integer(), Some(1));
        // Verify structure matches eggress ConfigFile expectations
        let clients = parsed["reverse_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["id"].as_str(), Some("pproxy-reverse-client-0"));
        assert_eq!(clients[0]["server_addr"].as_str(), Some("acceptor:1080"));
        assert_eq!(clients[0]["auth_username"].as_str(), Some("user"));
        assert_eq!(clients[0]["auth_password"].as_str(), Some("pass"));
        assert_eq!(clients[0]["parallel_connections"].as_integer(), Some(2));
    }

    #[test]
    fn test_pac_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--pac".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "pac-serving"));
    }

    #[test]
    fn test_test_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--test".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "test-mode"));
    }

    #[test]
    fn test_sys_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--sys".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "system-proxy"));
    }

    #[test]
    fn test_log_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--log".into(),
            "access.log".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "log-file"));
    }

    #[test]
    fn test_reuse_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--reuse".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "reuse-connection"));
    }

    #[test]
    fn test_alive_flag_includes_interval_in_message() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-a".into(),
            "15".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let alive_warn = output
            .warnings
            .iter()
            .find(|w| w.category == "alive-check")
            .unwrap();
        assert!(alive_warn.message.contains("15"));
    }

    #[test]
    fn test_get_flag_emits_warning() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--get".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "get-url"));
    }
}
