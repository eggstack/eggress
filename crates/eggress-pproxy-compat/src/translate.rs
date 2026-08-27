use crate::args::PproxyArgs;
use crate::error::CompatError;
use crate::uri::{PproxyChain, PproxyPluginSpec, PproxyUri};
use crate::warnings::TranslationOutput;

/// Translate pproxy-style arguments into Eggress TOML configuration.
pub fn translate_pproxy_args(args: &PproxyArgs) -> Result<TranslationOutput, CompatError> {
    let local_uris = args.parse_local_uris()?;

    // Parse remote URIs as chains (supports __ hop separator)
    let mut remote_chains = Vec::new();
    let mut chain_warnings = TranslationOutput::new(String::new());
    for raw_remote in args.remotes.iter() {
        match crate::uri::parse_pproxy_chain(raw_remote) {
            Ok(chain) => remote_chains.push(chain),
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Validate chain hops for unsupported protocols
    for chain in &remote_chains {
        let unsupported = crate::uri::validate_chain_hops(chain);
        for (idx, scheme) in unsupported {
            chain_warnings = chain_warnings.with_unsupported(
                "chain-unsupported-hop",
                format!(
                    "chain hop {} in '{}' uses unsupported scheme '{}'",
                    idx + 1,
                    chain.redacted_display(),
                    scheme
                ),
            );
        }
    }

    // Allow empty local_uris when -ul is present (standalone UDP mode)
    let has_udp_listen = args
        .known_unsupported
        .iter()
        .any(|f| f.starts_with("udp-listen="));

    if local_uris.is_empty() && !has_udp_listen {
        return Err(CompatError::InvalidArgs {
            message: "no local listener specified (use -l or positional args)".to_string(),
        });
    }

    let mut output = translate_from_uris(args, &local_uris, &remote_chains)?;

    // Merge chain validation warnings
    output = output.with_unsupported_features(chain_warnings.unsupported);

    // Merge unknown-flag diagnostics
    let unknown_warnings = args.unknown_flag_diagnostics();
    output = output.with_warnings(unknown_warnings);

    Ok(output)
}

/// Translate pproxy-style local and remote URIs into Eggress TOML.
pub fn translate_from_uris(
    args: &PproxyArgs,
    local_uris: &[PproxyUri],
    remote_chains: &[PproxyChain],
) -> Result<TranslationOutput, CompatError> {
    let mut output = TranslationOutput::new(String::new());
    let mut listeners = Vec::new();
    let mut upstreams = Vec::new();
    let mut upstream_groups = Vec::new();
    let mut rules = Vec::new();
    let mut reverse_servers = Vec::new();
    let mut reverse_clients = Vec::new();

    let mut scheduler_override = None;
    let mut udp_listen_addr: Option<String> = None;
    let mut udp_remotes: Vec<String> = Vec::new();
    let mut ssl_config: Option<TlsToml> = None;
    let mut block_rules: Vec<String> = Vec::new();
    let mut health_interval: Option<String> = None;
    let mut pac_enabled = false;
    let mut pac_path: Option<String> = None;
    let mut static_content: Vec<StaticContentToml> = Vec::new();

    // Handle typed fields first
    if args.daemon {
        #[cfg(feature = "daemon")]
        {
            output = output.with_warning(
                "daemon",
                "--daemon uses a safe Linux re-exec compatibility mode; the child owns runtime and --sys rollback",
            );
        }
        #[cfg(not(feature = "daemon"))]
        {
            output = output.with_unsupported(
                "daemon",
                "--daemon mode requires the optional daemon compatibility feature",
            );
        }
    }
    if args.system_proxy {
        output = output.with_warning(
            "system-proxy",
            "--sys applies the selected local HTTP or SOCKS5 listener and restores prior settings on shutdown",
        );
    }
    if let Some(auth) = args.auth_timeout {
        output = output.with_warning(
            "auth-timeout",
            format!(
                "--auth {}s enables pproxy-compatible source-IP authentication reuse",
                auth.as_secs()
            ),
        );
    }
    if args.verbose_level > 0 {
        output = output.with_warning(
            "verbose-mode",
            "pproxy -v flag detected; set RUST_LOG=debug environment variable for equivalent behavior",
        );
    }
    if args.debug {
        output = output.with_warning(
            "debug-mode",
            "pproxy -d detected; Eggress enables debug-level default tracing, but does not reproduce Python traceback semantics",
        );
    }
    if args.reuse_port {
        output = output.with_warning(
            "reuse-port",
            "pproxy --reuse enables SO_REUSEPORT on listener sockets (platform-dependent)",
        );
    }
    if remote_chains.iter().any(|chain| {
        chain.hops.iter().any(|hop| {
            hop.protocol_chain
                .iter()
                .any(|protocol| matches!(protocol.as_str(), "quic" | "h3"))
        })
    }) {
        output = output.with_warning(
            "quic-insecure",
            "pproxy QUIC/H3 compatibility uses an explicit insecure certificate verifier; generated upstream URIs carry insecure=true",
        );
    }

    // Process known-but-unsupported flags
    for flag in &args.known_unsupported {
        if let Some(addr) = flag.strip_prefix("udp-listen=") {
            udp_listen_addr = Some(addr.to_string());
        }
        if let Some(remote) = flag.strip_prefix("udp-remote=") {
            udp_remotes.push(remote.to_string());
        }
        if let Some(rulefile_path) = flag.strip_prefix("rulefile=") {
            let patterns = load_pproxy_rule_file(rulefile_path, &mut output)?;
            block_rules.push(combine_pproxy_patterns(&patterns));
        }
        if let Some(scheduler_value) = flag.strip_prefix("scheduler=") {
            let mapped = match scheduler_value {
                "fa" | "first_available" => Some("first-available".to_string()),
                "rr" | "round_robin" => Some("round-robin".to_string()),
                "rc" | "random_choice" => Some("random".to_string()),
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
            health_interval = Some(format!("{}s", interval));
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
            ssl_config = Some(TlsToml {
                cert,
                key,
                alpn: None,
            });
        }
        if let Some(block_value) = flag.strip_prefix("block=") {
            if block_value.starts_with('{') && block_value.ends_with('}') {
                let pattern = inline_pproxy_pattern(block_value);
                if let Err(error) = crate::regex_compat::compile_block_pattern(&pattern) {
                    return Err(CompatError::ConfigValidation {
                        message: format!("block regex is invalid: {}", error),
                    });
                }
                block_rules.push(pattern);
            } else {
                let patterns = load_pproxy_rule_file(block_value, &mut output)?;
                block_rules.push(combine_pproxy_patterns(&patterns));
            }
        }
        if let Some(value) = flag.strip_prefix("pac=") {
            pac_enabled = true;
            pac_path = Some(if value.starts_with('/') {
                value.to_string()
            } else {
                format!("/{value}")
            });
            output = output.with_warning(
                "pac-serving",
                format!("pproxy --pac {value} maps to the Eggress admin PAC path"),
            );
        }
        if let Some(value) = flag.strip_prefix("test=") {
            output = output.with_warning(
                "test-mode",
                format!("pproxy --test {value} will run an upstream request and exit"),
            );
        }
        if flag == "sys" {
            // Handled via args.system_proxy above
        }
        if flag.starts_with("log=") {
            output = output.with_warning(
                "log-file",
                "pproxy --log flag detected; eggress logs to stderr via tracing-subscriber; redirect stderr with shell redirection for file logging",
            );
        }
        if flag == "reuse" {
            // Handled via args.reuse_port below
        }
        if let Some(value) = flag.strip_prefix("get=") {
            match value.split_once(',') {
                Some((path, filename))
                    if path.starts_with('/') && !path.contains("..") && !filename.is_empty() =>
                {
                    match std::fs::read_to_string(filename) {
                        Ok(body) => static_content.push(StaticContentToml {
                            path: path.to_string(),
                            body,
                        }),
                        Err(error) => {
                            output = output.with_unsupported(
                                "get-file",
                                format!("--get file '{filename}' could not be read: {error}"),
                            )
                        }
                    }
                }
                _ => {
                    output = output.with_unsupported(
                        "get-file",
                        format!(
                            "--get value '{value}' must be PATH,FILE with an absolute safe PATH"
                        ),
                    )
                }
            }
            output = output.with_warning(
                "get-static-content",
                format!("pproxy --get {value} is served as admin static content"),
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
                control_bind: bind.clone(),
                external_bind: bind,
                auth_username: local.username.clone(),
                auth_password: local.password.clone(),
                pproxy_compat: true,
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
        let mut reject_listener = false;
        for protocol in &local.protocol_chain {
            match protocol.as_str() {
                "ss" | "shadowsocks" => {
                    // Shadowsocks listener is supported (requires explicit protocol mode)
                    tracing::debug!(
                        "shadowsocks listener '{}' accepted (explicit protocol mode)",
                        local.redacted_display()
                    );
                }
                "ssr" => {
                    if let Err(error) = validate_pproxy_plugins(&local.plugins) {
                        output = output.with_unsupported("plugin", error);
                        reject_listener = true;
                    }
                }
                "trojan" => {
                    tracing::debug!(
                        "Trojan listener '{}' accepted (TLS required)",
                        local.redacted_display()
                    );
                }
                "ssh" => {
                    output = output.with_unsupported(
                        "ssh-listener",
                        format!(
                            "SSH listener '{}': SSH transport is not supported",
                            local.redacted_display()
                        ),
                    );
                    reject_listener = true;
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
                    reject_listener = true;
                }
                "http" | "https" | "socks4" | "socks4a" | "socks5" | "echo" => {}
                "httponly" => {
                    output = output.with_unsupported(
                        "unsupported-role",
                        format!(
                            "httponly listener '{}' is unsupported: httponly is an upstream request adapter only",
                            local.redacted_display()
                        ),
                    );
                    reject_listener = true;
                }
                "raw" | "tunnel" if local.fixed_target.is_some() => {}
                "h2" | "h3" | "quic" => {}
                "ws" | "wss" if local.fixed_target.is_some() => {}
                "ws" | "wss" => {
                    output = output.with_unsupported(
                        "listener-fixed-target",
                        format!(
                            "{} listener '{}' requires a fixed target such as ws{{target}}://:port",
                            protocol,
                            local.redacted_display()
                        ),
                    );
                    reject_listener = true;
                }
                other => {
                    output = output.with_unsupported(
                        "scheme",
                        format!("unknown scheme '{}' in listener URI", other),
                    );
                    reject_listener = true;
                }
            }
        }
        if reject_listener {
            continue;
        }

        let protocols = local
            .protocol_chain
            .iter()
            .map(|protocol| {
                match protocol.as_str() {
                    "https" => "http",
                    "httponly" => "httponly",
                    "socks4a" => "socks4",
                    "ss" | "shadowsocks" => "shadowsocks",
                    "ssr" => "ssr",
                    "redir" => "http",
                    "unix" => "socks5",
                    "echo" => "echo",
                    "raw" | "tunnel" => "raw",
                    "ws" | "wss" => "websocket",
                    other => other,
                }
                .to_string()
            })
            .collect::<Vec<_>>();
        let has_non_sniffable = protocols
            .iter()
            .any(|p| matches!(p.as_str(), "shadowsocks" | "ssr" | "trojan"));
        if protocols.len() > 1 && has_non_sniffable {
            output = output.with_unsupported("mixed-listener", format!("mixed listener '{}' includes a non-sniffable protocol; use only http+socks4+socks5", local.redacted_display()));
            continue;
        }
        if let Err(error) = validate_pproxy_plugins(&local.plugins) {
            output = output.with_unsupported("plugin", error);
            reject_listener = true;
        }
        if reject_listener {
            continue;
        }

        let listener_name = format!("pproxy-local-{}", idx);
        let bind = local.bind_display();
        let is_h2_listener = protocols.len() == 1 && protocols[0] == "h2";
        let is_h3_listener = protocols.len() == 1 && protocols[0] == "h3";
        let is_quic_listener = protocols.iter().any(|protocol| protocol == "quic");

        let mut listener_entry = ListenerToml {
            name: listener_name.clone(),
            bind,
            protocols,
            reuse_port: if args.reuse_port { Some(true) } else { None },
            auth: None,
            udp: None,
            shadowsocks: None,
            ssr: None,
            trojan: None,
            transparent: None,
            unix: None,
            tls: None,
            fixed_target: local.fixed_target.clone(),
            local_bind: local.local_bind.clone(),
        };

        if local.tls || local.scheme == "wss" {
            if ssl_config.is_none() {
                output = output.with_unsupported(
                    "tls-listener-cert",
                    format!(
                        "TLS listener '{}' requires --ssl CERT,KEY",
                        local.redacted_display()
                    ),
                );
            } else {
                listener_entry.tls = ssl_config.clone();
            }
        }

        if is_h2_listener {
            if let Some(ref mut tls) = listener_entry.tls {
                tls.alpn = Some(vec!["h2".to_string()]);
            }
        }

        if is_h3_listener || is_quic_listener {
            if let Some(ref mut tls) = listener_entry.tls {
                tls.alpn = None;
            }
        }

        // Handle auth on listener
        if local.scheme.as_str() == "ss" || local.scheme.as_str() == "shadowsocks" {
            // For Shadowsocks, username = method, password = password
            if let Some(ref method) = local.username {
                // Check for legacy stream cipher methods
                if crate::uri::is_legacy_ss_method(method) {
                    #[cfg(feature = "legacy-crypto")]
                    {
                        output = output.with_warning(
                            "legacy-cipher",
                            format!(
                                "Shadowsocks listener '{}': legacy stream cipher '{}' is unauthenticated and requires the optional compatibility feature",
                                local.redacted_display(),
                                method
                            ),
                        );
                    }
                    #[cfg(not(feature = "legacy-crypto"))]
                    {
                        output = output.with_unsupported(
                            "legacy-cipher",
                            format!(
                                "Shadowsocks listener '{}': legacy stream cipher method '{}' requires the optional legacy-crypto feature; use an AEAD method otherwise",
                                local.redacted_display(),
                                method
                            ),
                        );
                    }
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
        } else if local.scheme.as_str() == "ssr" {
            listener_entry.ssr = Some(SsrToml {
                auth_prefix: local.auth_fragment.clone(),
                plugins: local
                    .plugins
                    .iter()
                    .map(|plugin| plugin.name.clone())
                    .collect(),
            });
        } else if local.scheme.as_str() == "trojan" {
            // Trojan: password-only format — password = trojan password, username unused
            // Trojan requires real TLS material. Do not emit placeholder paths that
            // make an invalid translated configuration appear runnable.
            if listener_entry.tls.is_none() {
                output = output.with_unsupported(
                    "trojan-tls-config",
                    format!(
                        "Trojan listener '{}': TLS is required; provide --ssl CERT,KEY with the pproxy compatibility command",
                        listener_name
                    ),
                );
            }
            if let Some(ref pass) = local.password {
                listener_entry.trojan = Some(TrojanToml {
                    password: pass.clone(),
                });
                output = output.with_warning(
                    "credential-in-toml",
                    format!(
                        "Listener '{}' has plaintext credentials in generated TOML",
                        listener_name
                    ),
                );
            } else {
                output = output.with_unsupported(
                    "trojan-no-password",
                    format!(
                        "Trojan listener '{}': password is required",
                        local.redacted_display()
                    ),
                );
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
        if remote_chains.is_empty() && udp_remotes.is_empty() {
            output = output.with_warning(
                "direct-mode",
                format!(
                    "Listener '{}' has no upstream; traffic will be direct",
                    listener_name
                ),
            );
        }
    }

    // Apply --ssl TLS config to all compatible listeners.
    // pproxy loads the cert chain into every ssl context (one per listener),
    // so TLS is enabled on all listeners, not just the first.
    if let Some(tls) = ssl_config {
        if !listeners.is_empty() {
            for listener in listeners.iter_mut() {
                let mut listener_tls = tls.clone();
                if listener.protocols.iter().any(|protocol| protocol == "h2") {
                    listener_tls.alpn = Some(vec!["h2".to_string()]);
                } else if listener
                    .protocols
                    .iter()
                    .any(|protocol| protocol == "websocket")
                {
                    listener_tls.alpn = Some(vec!["http/1.1".to_string()]);
                }
                listener.tls = Some(listener_tls);
            }
        } else {
            output = output.with_warning(
                "ssl-no-listener",
                "--ssl specified but no compatible TCP listener was generated; cert/key are recorded as a no-op",
            );
        }
    }

    // If -ul is specified, add standalone UDP config to the first listener
    if let Some(ref addr) = udp_listen_addr {
        let udp_uri = addr
            .contains("://")
            .then(|| crate::uri::parse_pproxy_uri(addr))
            .transpose()?;
        let bind = parse_udp_listen_addr(addr);
        if let Some(listener) = listeners.first_mut() {
            listener.udp = Some(match udp_uri {
                Some(ref uri) if uri.scheme == "echo" => UdpToml {
                    mode: Some("echo".to_string()),
                    bind: Some(bind),
                    fixed_target: None,
                },
                Some(ref uri) if matches!(uri.scheme.as_str(), "raw" | "tunnel") => {
                    let target =
                        uri.fixed_target
                            .clone()
                            .ok_or_else(|| CompatError::InvalidUri {
                                message: "UDP fixed-target listener requires a brace target"
                                    .to_string(),
                            })?;
                    UdpToml {
                        mode: Some("fixed_target".to_string()),
                        bind: Some(bind),
                        fixed_target: Some(target),
                    }
                }
                _ => UdpToml {
                    mode: Some("standalone_pproxy_udp".to_string()),
                    bind: Some(bind),
                    fixed_target: None,
                },
            });
        } else {
            // No listener created (all were unsupported schemes); add a default SOCKS5 listener
            listeners.push(ListenerToml {
                name: "pproxy-local-0".to_string(),
                bind: "0.0.0.0:1080".to_string(),
                protocols: vec!["socks5".to_string()],
                reuse_port: if args.reuse_port { Some(true) } else { None },
                auth: None,
                udp: Some(UdpToml {
                    mode: Some("standalone_pproxy_udp".to_string()),
                    bind: Some(parse_udp_listen_addr(addr)),
                    fixed_target: None,
                }),
                shadowsocks: None,
                ssr: None,
                trojan: None,
                transparent: None,
                unix: None,
                tls: None,
                fixed_target: None,
                local_bind: None,
            });
            output = output.with_warning(
                "ul-no-listener",
                "-ul specified without a compatible -l listener; added default SOCKS5 listener on :1080",
            );
        }
    }

    // Process remote upstreams (chains). Keep this separate from the native
    // group so URI declaration order and each remote's predicate survive
    // lowering.
    let mut tcp_routes: Vec<TcpCompatRoute> = Vec::new();
    for (idx, chain) in remote_chains.iter().enumerate() {
        // Backward/upstream URIs with +in modifier become one maintained
        // compatibility worker per +in occurrence. The final non-direct hop
        // supplies pproxy's raw auth field; the complete chain is retained
        // for jump-aware transport setup.
        if chain.hops.iter().any(PproxyUri::is_backward) {
            let remote = &chain.hops[0];
            let auth_hop = chain
                .hops
                .iter()
                .rev()
                .find(|hop| hop.scheme != "direct")
                .unwrap_or(remote);
            // Backward + SSL (+ssl modifier) is not supported
            if chain.hops.iter().any(|hop| hop.ssl) {
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
                server_uri: Some(build_chain_config_uri(chain)),
                auth_username: auth_hop.username.clone(),
                auth_password: auth_hop.password.clone(),
                parallel_connections: {
                    let count: u32 = chain
                        .hops
                        .iter()
                        .map(PproxyUri::backward_num)
                        .sum::<u32>()
                        .max(1);
                    (count > 1).then_some(count)
                },
                pproxy_compat: true,
            });
            // Emit credential warning if auth present
            if auth_hop.username.is_some() {
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

        // Check for unsupported upstream protocols across all hops
        let mut hop_unsupported = false;
        for hop in &chain.hops {
            if hop.local_bind.is_some() {
                let bind = hop.local_bind.as_deref().unwrap_or_default();
                if hop.scheme == "unix" {
                    output = output.with_unsupported(
                        "local-bind",
                        format!(
                            "local bind '{}' cannot be applied to Unix upstream '{}'",
                            bind,
                            hop.redacted_display()
                        ),
                    );
                    hop_unsupported = true;
                } else if bind.parse::<std::net::IpAddr>().is_err() {
                    output = output.with_unsupported(
                        "local-bind",
                        format!(
                            "local bind '{}' must be an IP address for upstream '{}'",
                            bind,
                            hop.redacted_display()
                        ),
                    );
                    hop_unsupported = true;
                }
            }
            // raw/tunnel endpoints are the native fixed-target form. The
            // compatibility parser keeps the brace-delimited target in
            // `fixed_target`; build_config_uri lowers it back to the same
            // endpoint URI consumed by the native raw handler.
            if let Err(error) = validate_pproxy_plugins(&hop.plugins) {
                output = output.with_unsupported("plugin", error);
                hop_unsupported = true;
            }
            match hop.scheme.as_str() {
                "ss" | "shadowsocks" | "ssr" => {}
                "http" | "https" | "httponly" | "socks4" | "socks4a" | "socks5" | "trojan"
                | "direct" | "h2" | "h3" | "quic" | "quic+http" | "http+quic" | "ws" | "wss"
                | "raw" | "tunnel" => {}
                "ssh" if cfg!(feature = "ssh") => {}
                "ssh" => {
                    output = output.with_unsupported(
                        "ssh-upstream",
                        format!(
                            "SSH upstream '{}': SSH transport is not supported",
                            hop.redacted_display()
                        ),
                    );
                    hop_unsupported = true;
                }
                "unix" => {}
                "redir" => {
                    output = output.with_unsupported(
                        "redir-upstream",
                        format!(
                            "Redir upstream '{}': transparent proxy redirect is not supported as upstream",
                            hop.redacted_display()
                        ),
                    );
                    hop_unsupported = true;
                }
                other => {
                    output = output.with_unsupported(
                        "scheme",
                        format!("unknown scheme '{}' in upstream URI", other),
                    );
                    hop_unsupported = true;
                }
            }
        }
        if hop_unsupported {
            continue;
        }

        // Build the upstream URI for the chain
        let config_uri = build_chain_config_uri(chain);
        let upstream_id = format!("pproxy-upstream-{}", idx);

        upstreams.push(UpstreamToml {
            id: upstream_id.clone(),
            uri: config_uri,
            health: health_interval.as_ref().map(|interval| HealthToml {
                interval: interval.clone(),
            }),
        });

        let Some(remote) = chain.hops.first() else {
            return Err(CompatError::InvalidArgs {
                message: "remote URI contains no proxy hop".to_string(),
            });
        };
        let predicate = if let Some(rule) = remote.rule.as_deref().or(remote.rule_suffix.as_deref())
        {
            Some((inline_pproxy_pattern(rule), format!("inline:{idx}")))
        } else if let Some(path) = remote.rules_file.as_deref() {
            let patterns = load_pproxy_rule_file(path, &mut output)?;
            Some((combine_pproxy_patterns(&patterns), format!("file:{path}")))
        } else {
            None
        };
        tcp_routes.push(TcpCompatRoute {
            declaration_index: idx,
            upstream_id,
            predicate,
        });
    }

    // Process UDP remote upstreams
    let mut udp_upstream_ids = Vec::new();
    let mut udp_routes: Vec<TcpCompatRoute> = Vec::new();
    for (idx, remote_str) in udp_remotes.iter().enumerate() {
        let remote_chain =
            crate::uri::parse_pproxy_chain(remote_str).map_err(|e| CompatError::InvalidArgs {
                message: format!("invalid UDP remote URI '{}': {}", remote_str, e),
            })?;
        let Some(remote_uri) = remote_chain.hops.first() else {
            return Err(CompatError::InvalidArgs {
                message: format!("invalid UDP remote URI '{remote_str}': no proxy hop"),
            });
        };

        // UDP composition is recursive, but intentionally closed over the
        // protocols with a real pproxy UDP path. Unsupported chains are
        // retained as diagnostics instead of being silently coerced to TCP.
        let unsupported = remote_chain
            .hops
            .iter()
            .find(|hop| !matches!(hop.scheme.as_str(), "socks5" | "ss" | "shadowsocks"));
        if let Some(unsupported) = unsupported {
            match unsupported.scheme.as_str() {
                "ssr" => {
                    output = output.with_unsupported(
                    "ssr-udp",
                    format!(
                        "ShadowsocksR (SSR) UDP upstream '{}': only bounded SSR TCP framing/plugins are supported; SSR UDP is not implemented",
                        remote_uri.redacted_display()
                    ),
                );
                }
                "http" | "https" => {
                    output = output.with_unsupported(
                    "udp-http-transport",
                    format!(
                        "HTTP/HTTPS UDP upstream '{}': HTTP CONNECT does not support UDP relay; use direct://, socks5://, or ss:// for UDP upstreams",
                        remote_uri.redacted_display()
                    ),
                );
                }
                "socks4" | "socks4a" => {
                    output = output.with_unsupported(
                    "udp-socks4-transport",
                    format!(
                        "SOCKS4 UDP upstream '{}': SOCKS4 does not support UDP relay; use socks5:// for UDP upstreams",
                        remote_uri.redacted_display()
                    ),
                );
                }
                "trojan" => {
                    output = output.with_unsupported(
                    "udp-trojan-transport",
                    format!(
                        "Trojan UDP upstream '{}': Trojan does not support UDP relay; use direct://, socks5://, or ss://",
                        remote_uri.redacted_display()
                    ),
                );
                }
                "h2" | "h3" | "quic" | "ws" | "wss" | "raw" | "tunnel" => {
                    output = output.with_unsupported(
                        "unsupported-role",
                        format!(
                            "{} UDP upstream '{}' is recognized but only supports TCP",
                            remote_uri.scheme,
                            remote_uri.redacted_display()
                        ),
                    );
                }
                other => {
                    output = output.with_unsupported(
                        "scheme",
                        format!("unknown scheme '{}' in UDP upstream URI", other),
                    );
                }
            }
            continue;
        }
        let upstream_id = format!("pproxy-udp-upstream-{}", idx);
        let config_uri = build_chain_config_uri(&remote_chain);

        upstreams.push(UpstreamToml {
            id: upstream_id.clone(),
            uri: config_uri,
            health: health_interval.as_ref().map(|interval| HealthToml {
                interval: interval.clone(),
            }),
        });
        udp_upstream_ids.push(upstream_id);
        let predicate = if let Some(rule) = remote_uri
            .rule
            .as_deref()
            .or(remote_uri.rule_suffix.as_deref())
        {
            Some((inline_pproxy_pattern(rule), format!("inline:{idx}")))
        } else if let Some(path) = remote_uri.rules_file.as_deref() {
            let patterns = load_pproxy_rule_file(path, &mut output)?;
            Some((combine_pproxy_patterns(&patterns), format!("file:{path}")))
        } else {
            None
        };
        udp_routes.push(TcpCompatRoute {
            declaration_index: idx,
            upstream_id: format!("pproxy-udp-upstream-{idx}"),
            predicate,
        });
    }

    // Build ordered TCP routes. Unruled remotes form one final catch-all
    // group, while ruled remotes get one-member groups so their predicates
    // cannot accidentally become global reject rules.
    if !tcp_routes.is_empty() {
        let mut unruled = Vec::new();
        for route in &tcp_routes {
            if route.predicate.is_none() {
                unruled.push(route.upstream_id.clone());
                continue;
            }
            let group_id = format!("pproxy-route-{}", route.declaration_index);
            upstream_groups.push(UpstreamGroupToml {
                id: group_id.clone(),
                scheduler: "first-available".to_string(),
                members: vec![route.upstream_id.clone()],
                fallback: "reject".to_string(),
            });
            let Some((pattern, source)) = route.predicate.as_ref() else {
                continue;
            };
            rules.push(RuleToml {
                id: format!(
                    "pproxy-route-{}-{}-pattern={}",
                    route.declaration_index, source, pattern
                ),
                any: false,
                upstream_group: group_id,
                direct: None,
                r#match: Some(pproxy_rule_match(pattern, "tcp")),
                host_regex: None,
                reject: None,
            });
        }
        if !unruled.is_empty() {
            let group_id = "pproxy-chain".to_string();
            let scheduler = scheduler_override
                .clone()
                .unwrap_or_else(|| "first-available".to_string());
            upstream_groups.push(UpstreamGroupToml {
                id: group_id.clone(),
                scheduler,
                members: unruled,
                fallback: "reject".to_string(),
            });
            rules.push(RuleToml {
                id: "pproxy-default".to_string(),
                any: true,
                upstream_group: group_id,
                direct: None,
                r#match: None,
                host_regex: None,
                reject: None,
            });
        } else {
            // pproxy falls back to DIRECT when every remote has a predicate
            // and none matches.
            rules.push(RuleToml {
                id: "pproxy-direct-fallback".to_string(),
                any: true,
                upstream_group: String::new(),
                direct: Some(true),
                r#match: None,
                host_regex: None,
                reject: None,
            });
        }
    } else if !listeners.is_empty() {
        // No upstream specified: emit a default direct rule so pproxy's
        // "no -r means direct passthrough" behavior is preserved. A warning
        // ("direct-mode") is already emitted above for each listener.
        rules.push(RuleToml {
            id: "pproxy-default".to_string(),
            any: true,
            upstream_group: String::new(),
            direct: Some(true),
            r#match: None,
            host_regex: None,
            reject: None,
        });
    }

    // Build upstream groups and rules for UDP
    if !udp_upstream_ids.is_empty() {
        let has_predicates = udp_routes.iter().any(|route| route.predicate.is_some());
        if has_predicates {
            let mut unruled = Vec::new();
            for route in &udp_routes {
                if route.predicate.is_none() {
                    unruled.push(route.upstream_id.clone());
                    continue;
                }
                let group_id = format!("pproxy-udp-route-{}", route.declaration_index);
                upstream_groups.push(UpstreamGroupToml {
                    id: group_id.clone(),
                    scheduler: "first-available".to_string(),
                    members: vec![route.upstream_id.clone()],
                    fallback: "reject".to_string(),
                });
                let Some((pattern, source)) = route.predicate.as_ref() else {
                    continue;
                };
                rules.push(RuleToml {
                    id: format!(
                        "pproxy-udp-route-{}-{}-pattern={}",
                        route.declaration_index, source, pattern
                    ),
                    any: false,
                    upstream_group: group_id,
                    direct: None,
                    r#match: Some(pproxy_rule_match(pattern, "udp")),
                    host_regex: None,
                    reject: None,
                });
            }
            if !unruled.is_empty() {
                let group_id = "pproxy-udp-chain".to_string();
                upstream_groups.push(UpstreamGroupToml {
                    id: group_id.clone(),
                    scheduler: scheduler_override
                        .clone()
                        .unwrap_or_else(|| "first-available".to_string()),
                    members: unruled,
                    fallback: "reject".to_string(),
                });
                rules.push(RuleToml {
                    id: "pproxy-udp-default".to_string(),
                    any: false,
                    upstream_group: group_id,
                    direct: None,
                    r#match: Some(MatchToml {
                        transport: Some("udp".to_string()),
                        host_regex: None,
                        destination_port_regex: None,
                        any_of: Vec::new(),
                    }),
                    host_regex: None,
                    reject: None,
                });
            }
        } else {
            let group_id = "pproxy-udp-chain".to_string();
            upstream_groups.push(UpstreamGroupToml {
                id: group_id.clone(),
                scheduler: scheduler_override
                    .clone()
                    .unwrap_or_else(|| "first-available".to_string()),
                members: udp_upstream_ids,
                fallback: "reject".to_string(),
            });
            rules.push(RuleToml {
                id: "pproxy-udp-default".to_string(),
                any: false,
                upstream_group: group_id,
                direct: None,
                r#match: Some(MatchToml {
                    transport: Some("udp".to_string()),
                    host_regex: None,
                    destination_port_regex: None,
                    any_of: Vec::new(),
                }),
                host_regex: None,
                reject: None,
            });
        }
    }

    // Prepend block rules (first-match-wins: block rules before default rules)
    if !block_rules.is_empty() {
        let mut all_rules = Vec::new();
        for (idx, pattern) in block_rules.iter().enumerate() {
            all_rules.push(RuleToml {
                id: format!("pproxy-block-{}-pattern={}", idx, pattern),
                any: false,
                upstream_group: String::new(),
                direct: None,
                r#match: None,
                host_regex: Some(pattern.clone()),
                reject: Some("blocked".to_string()),
            });
        }
        all_rules.extend(rules);
        rules = all_rules;
    }

    // Generate TOML
    let toml_str = generate_toml(TomlInput {
        listeners: &listeners,
        upstreams: &upstreams,
        upstream_groups: &upstream_groups,
        rules: &rules,
        reverse_servers: &reverse_servers,
        reverse_clients: &reverse_clients,
        pac_enabled,
        pac_path,
        static_content: &static_content,
    });

    Ok(TranslationOutput::new(toml_str)
        .with_warnings(output.warnings)
        .with_unsupported_features(output.unsupported))
}

#[derive(Debug, Clone)]
struct TcpCompatRoute {
    declaration_index: usize,
    upstream_id: String,
    predicate: Option<(String, String)>,
}

fn inline_pproxy_pattern(pattern: &str) -> String {
    let pattern = pattern
        .strip_prefix('{')
        .and_then(|p| p.strip_suffix('}'))
        .unwrap_or(pattern);
    format!("^(?:{pattern})")
}

fn combine_pproxy_patterns(patterns: &[String]) -> String {
    if patterns.len() == 1 {
        return inline_pproxy_pattern(&patterns[0]);
    }
    let joined = patterns
        .iter()
        .map(|pattern| format!("(?:{pattern})"))
        .collect::<Vec<_>>()
        .join("|");
    format!("^(?:{joined})$")
}

fn load_pproxy_rule_file(
    path: &str,
    output: &mut TranslationOutput,
) -> Result<Vec<String>, CompatError> {
    let rule_file =
        crate::regex_compat::PproxyRuleFile::load(std::path::Path::new(path)).map_err(|error| {
            CompatError::ConfigValidation {
                message: format!("failed to load pproxy rule file '{}': {}", path, error),
            }
        })?;
    for diagnostic in &rule_file.diagnostics {
        match diagnostic.severity {
            crate::regex_compat::RuleSeverity::Error => {
                return Err(CompatError::ConfigValidation {
                    message: format!(
                        "pproxy rule file '{}' is invalid: {}",
                        path, diagnostic.message
                    ),
                });
            }
            crate::regex_compat::RuleSeverity::Warning => {
                *output = output
                    .clone()
                    .with_warning("rulefile-partial", diagnostic.message.clone());
            }
            crate::regex_compat::RuleSeverity::Info => {
                *output = output
                    .clone()
                    .with_warning("rulefile-fancy-regex", diagnostic.message.clone());
            }
        }
    }
    if rule_file.entries.iter().any(|entry| entry.uses_fancy) {
        return Err(CompatError::ConfigValidation {
            message: format!(
                "pproxy rule file '{}' uses regex features unavailable in native routing",
                path
            ),
        });
    }
    Ok(rule_file
        .entries
        .iter()
        .map(|entry| entry.raw.clone())
        .collect())
}

fn pproxy_rule_match(pattern: &str, transport: &str) -> MatchToml {
    MatchToml {
        transport: None,
        host_regex: None,
        destination_port_regex: None,
        any_of: vec![
            MatchToml {
                transport: Some(transport.to_string()),
                host_regex: Some(pattern.to_string()),
                destination_port_regex: None,
                any_of: Vec::new(),
            },
            MatchToml {
                transport: Some(transport.to_string()),
                host_regex: None,
                destination_port_regex: Some(pattern.to_string()),
                any_of: Vec::new(),
            },
        ],
    }
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

fn validate_pproxy_plugins(plugins: &[PproxyPluginSpec]) -> Result<(), String> {
    for plugin in plugins {
        if !matches!(
            plugin.name.as_str(),
            "plain"
                | "origin"
                | "http_simple"
                | "tls1.2_ticket_auth"
                | "verify_simple"
                | "verify_deflate"
        ) {
            return Err(format!(
                "unknown pproxy plugin '{}'; existing plugins: plain, origin, http_simple, tls1.2_ticket_auth, verify_simple, verify_deflate",
                plugin.name
            ));
        }
        if plugin.options.is_some() {
            return Err(format!(
                "pproxy plugin '{}' options are not supported by the bounded compatibility implementation",
                plugin.name
            ));
        }
    }
    Ok(())
}

fn build_chain_config_uri(chain: &PproxyChain) -> String {
    if chain.hops.len() == 1 {
        return build_config_uri(&chain.hops[0]);
    }
    // Multi-hop chain: join hops with __ separator
    chain
        .hops
        .iter()
        .map(build_config_uri)
        .collect::<Vec<_>>()
        .join("__")
}

fn build_config_uri(remote: &PproxyUri) -> String {
    if remote.scheme == "unix" {
        return format!(
            "unix://{}",
            remote.path.as_deref().unwrap_or("/tmp/eggress.sock")
        );
    }
    let mut scheme = if remote.scheme == "https" {
        "http".to_string()
    } else if remote.scheme == "socks4a" {
        "socks4".to_string()
    } else {
        remote.scheme.clone()
    };
    // pproxy's wss and h2 schemes imply their native TLS transport. The
    // native URI grammar spells both as ws+tls and h2+tls so the shared
    // ChainExecutor applies the TLS wrapper and H2 ALPN consistently.
    if remote.tls || remote.scheme == "https" || remote.scheme == "wss" || remote.scheme == "h2" {
        if remote.scheme == "wss" {
            scheme = "ws".to_string();
        }
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
    let endpoint = remote
        .fixed_target
        .clone()
        .unwrap_or_else(|| remote.endpoint_display());
    let plugin_str = if remote.plugins.is_empty() {
        String::new()
    } else {
        format!(
            "/,{}",
            remote
                .plugins
                .iter()
                .map(|plugin| plugin.name.clone())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    // SSH uses the pproxy fragment as its login/password source. The parser
    // has already promoted that fragment into username/password credentials;
    // retaining it would duplicate the authentication material in the native
    // URI and leak a second credential-bearing suffix into diagnostics.
    let auth_str = if remote.scheme == "ssh" {
        String::new()
    } else {
        remote
            .auth_fragment
            .as_deref()
            .map(|auth| format!("#{auth}"))
            .unwrap_or_default()
    };
    let mut query = Vec::new();
    if let Some(rule) = &remote.rule {
        query.push(format!("rule={rule}"));
    }
    if remote
        .protocol_chain
        .iter()
        .any(|protocol| matches!(protocol.as_str(), "quic" | "h3"))
    {
        query.push("insecure=true".to_string());
    }
    let rule_str = if query.is_empty() {
        String::new()
    } else {
        format!("?{}", query.join("&"))
    };
    let bind = remote
        .local_bind
        .as_deref()
        .map(|v| format!("@{v}"))
        .unwrap_or_default();
    format!(
        "{}://{}{}{}{}{}{}",
        scheme, cred_str, endpoint, plugin_str, rule_str, bind, auth_str
    )
}

#[derive(serde::Serialize, Clone)]
struct TlsToml {
    cert: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alpn: Option<Vec<String>>,
}

#[derive(serde::Serialize, Clone)]
struct ListenerToml {
    name: String,
    bind: String,
    protocols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reuse_port: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<AuthToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    udp: Option<UdpToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shadowsocks: Option<ShadowsocksToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssr: Option<SsrToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trojan: Option<TrojanToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transparent: Option<TransparentToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unix: Option<UnixToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<TlsToml>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixed_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_bind: Option<String>,
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
struct SsrToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    plugins: Vec<String>,
}

#[derive(serde::Serialize, Clone)]
struct TrojanToml {
    password: String,
}

#[derive(serde::Serialize, Clone)]
struct UdpToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixed_target: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<HealthToml>,
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
    #[serde(skip_serializing_if = "String::is_empty")]
    upstream_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    direct: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destination_port_regex: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    any_of: Vec<MatchToml>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    admin: Option<AdminToml>,
}

#[derive(serde::Serialize, Clone)]
struct ReverseServerToml {
    id: String,
    control_bind: String,
    external_bind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_password: Option<String>,
    pproxy_compat: bool,
}

#[derive(serde::Serialize, Clone)]
struct ReverseClientToml {
    id: String,
    server_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_connections: Option<u32>,
    pproxy_compat: bool,
}

#[derive(serde::Serialize, Clone)]
struct HealthToml {
    interval: String,
}

#[derive(serde::Serialize, Clone)]
struct PacToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direct_fallback: Option<bool>,
}

#[derive(serde::Serialize, Clone)]
struct AdminToml {
    pac: PacToml,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    static_content: Vec<StaticContentToml>,
}

#[derive(serde::Serialize, Clone)]
struct StaticContentToml {
    path: String,
    body: String,
}

struct TomlInput<'a> {
    listeners: &'a [ListenerToml],
    upstreams: &'a [UpstreamToml],
    upstream_groups: &'a [UpstreamGroupToml],
    rules: &'a [RuleToml],
    reverse_servers: &'a [ReverseServerToml],
    reverse_clients: &'a [ReverseClientToml],
    pac_enabled: bool,
    pac_path: Option<String>,
    static_content: &'a [StaticContentToml],
}

fn generate_toml(input: TomlInput<'_>) -> String {
    let admin = if input.pac_enabled || !input.static_content.is_empty() {
        Some(AdminToml {
            pac: PacToml {
                path: input.pac_path.or_else(|| Some("/proxy.pac".to_string())),
                proxy: Some("PROXY {}".to_string()),
                direct_fallback: Some(true),
            },
            static_content: input.static_content.to_vec(),
        })
    } else {
        None
    };

    let config = ConfigToml {
        version: 1,
        listeners: input.listeners.to_vec(),
        upstreams: input.upstreams.to_vec(),
        upstream_groups: input.upstream_groups.to_vec(),
        rules: input.rules.to_vec(),
        reverse_servers: input.reverse_servers.to_vec(),
        reverse_clients: input.reverse_clients.to_vec(),
        admin,
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
        assert!(output.toml.contains("pproxy-default"));
        assert!(output.toml.contains("direct = true"));
        eprintln!("{}", output.toml);
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
        assert!(output.toml.contains("first-available"));
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

    #[cfg(feature = "ssh")]
    #[test]
    fn test_translate_ssh_fragment_credentials_to_native_uri() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "ssh://host/#login:password".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("ssh://login:password@host:22"));
        assert!(!output.toml.contains("#login:password"));
        assert!(!output
            .unsupported
            .iter()
            .any(|unsupported| unsupported.feature == "ssh-upstream"));
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
    fn test_translate_daemon_feature_state() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--daemon".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        #[cfg(feature = "daemon")]
        assert!(!output.has_unsupported());
        #[cfg(not(feature = "daemon"))]
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
    fn test_debug_flag_emits_compatible_warning() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-d".into()])
            .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.warnings.iter().any(|w| w.category == "debug-mode"));
        assert_eq!(
            crate::classify_aggregate_tier(&output.warnings, &[]),
            crate::ManifestTier::CompatibleWithWarning
        );
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
            ("rc", "random"),
            ("random_choice", "random"),
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
    fn test_ssl_flag_applies_to_all_listeners() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-l".into(),
            "http://127.0.0.1:8080".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        let listener_count = output.toml.matches("[[listeners]]").count();
        assert_eq!(
            listener_count, 2,
            "expected 2 listeners, got: {}",
            output.toml
        );
        let tls_block_count = output.toml.matches("[listeners.tls]").count();
        assert_eq!(
            tls_block_count, 2,
            "expected 2 [listeners.tls] blocks (one per listener), got: {}",
            output.toml
        );
    }

    #[test]
    fn test_block_flag_generates_reject_rule() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-b".into(),
            "{.*\\.example\\.com}".into(),
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
            "{.*\\.blocked\\.com}".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let rules = parsed["rules"].as_array().unwrap();
        let block_rule = rules
            .iter()
            .find(|r| {
                r["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("pproxy-block-0-pattern="))
            })
            .unwrap();
        assert_eq!(
            block_rule["host_regex"].as_str(),
            Some("^(?:.*\\.blocked\\.com)")
        );
        assert_eq!(block_rule["reject"].as_str(), Some("blocked"));
    }

    #[test]
    fn test_rulefile_missing_file_fails_translation() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--rulefile".into(),
            "/nonexistent/rules.txt".into(),
        ])
        .unwrap();
        let error = translate_pproxy_args(&args).unwrap_err();
        assert!(error
            .to_string()
            .contains("failed to load pproxy rule file"));
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
    fn test_translate_ssr_listener_supported() {
        let args = PproxyArgs::parse(&["-l".into(), "ssr://aes-256-ctr:secret@proxy:8388".into()])
            .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        assert!(output.toml.contains("protocols = [\"ssr\"]"));
    }

    #[test]
    fn test_translate_ssr_upstream_supported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "ssr://aes-256-ctr:secret@proxy:8388".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        assert!(output.toml.contains("ssr://"));
    }

    #[test]
    fn test_translate_legacy_cipher_feature_state() {
        let args =
            PproxyArgs::parse(&["-l".into(), "ss://aes-128-ctr:secret@proxy:8388".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        #[cfg(feature = "legacy-crypto")]
        assert!(!output.has_unsupported());
        #[cfg(not(feature = "legacy-crypto"))]
        assert!(output.has_unsupported());
        #[cfg(not(feature = "legacy-crypto"))]
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
    fn test_scheduler_default_first_available_for_multiple_remotes() {
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
        assert!(output.toml.contains("first-available"));
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
                || parsed["upstreams"].as_array().is_none_or(|a| a.is_empty())
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
        assert_eq!(servers[0]["external_bind"].as_str(), Some("0.0.0.0:8080"));
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
        assert_eq!(servers[0]["external_bind"].as_str(), Some("0.0.0.0:8080"));
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
    fn test_translate_backward_with_jump_chain() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+in://a:1__http://b:2".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let clients = parsed["reverse_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(
            clients[0]["server_uri"].as_str(),
            Some("socks5://a:1__http://b:2")
        );
        assert_eq!(clients[0]["pproxy_compat"].as_bool(), Some(true));
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
            "/proxy.pac".into(),
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
            "http://example.com".into(),
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
        assert!(!output.has_unsupported());
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
    fn test_reuse_flag_sets_reuse_port() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--reuse".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("reuse_port = true"));
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
            "/index.html,body.txt".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output
            .warnings
            .iter()
            .any(|w| w.category == "get-static-content"));
    }

    #[test]
    fn test_translate_two_hop_chain_one_upstream() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://a:1080__http://b:80".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1);
        // Chain URI should contain __ separator
        let uri = upstreams[0]["uri"].as_str().unwrap();
        assert!(uri.contains("__"), "expected __ in chain URI, got: {}", uri);
        // Verify it parses as a valid eggress chain
        assert!(uri.starts_with("socks5://"));
        assert!(uri.ends_with("http://b:80"));
        // Group should be first-available (single upstream)
        let groups = parsed["upstream_groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["scheduler"].as_str(), Some("first-available"));
    }

    #[test]
    fn test_translate_three_hop_chain() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://127.0.0.1:8080".into(),
            "-r".into(),
            "socks5://a:1080__http://b:80__socks5://c:1080".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1);
        let uri = upstreams[0]["uri"].as_str().unwrap();
        let hop_count = uri.split("__").count();
        assert_eq!(hop_count, 3, "expected 3 hops in chain URI: {}", uri);
    }

    #[test]
    fn test_translate_two_r_flags_two_upstreams() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://a:1080".into(),
            "-r".into(),
            "http://b:80".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 2);
        // Two separate upstreams, not a chain
        assert!(!upstreams[0]["uri"].as_str().unwrap().contains("__"));
        assert!(!upstreams[1]["uri"].as_str().unwrap().contains("__"));
        // Group should preserve pproxy's first-available declaration order.
        let groups = parsed["upstream_groups"].as_array().unwrap();
        assert_eq!(groups[0]["scheduler"].as_str(), Some("first-available"));
    }

    #[test]
    fn test_per_remote_rules_preserve_order_and_direct_fallback() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://a:80?rule=alpha".into(),
            "-r".into(),
            "socks5://b:1080?rule=beta".into(),
            "-r".into(),
            "http://c:80".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let groups = parsed["upstream_groups"].as_array().unwrap();
        assert_eq!(groups[0]["id"].as_str(), Some("pproxy-route-0"));
        assert_eq!(groups[1]["id"].as_str(), Some("pproxy-route-1"));
        assert_eq!(groups[2]["id"].as_str(), Some("pproxy-chain"));
        assert_eq!(groups[2]["members"][0].as_str(), Some("pproxy-upstream-2"));

        let rules = parsed["rules"].as_array().unwrap();
        assert!(rules[0]["id"]
            .as_str()
            .unwrap()
            .starts_with("pproxy-route-0-inline:0-pattern="));
        assert!(rules[1]["id"]
            .as_str()
            .unwrap()
            .starts_with("pproxy-route-1-inline:1-pattern="));
        assert_eq!(rules[2]["id"].as_str(), Some("pproxy-default"));
        assert!(rules[0]["match"]["any_of"].as_array().unwrap().len() == 2);

        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), &output.toml).unwrap();
        eggress_config::load_and_validate(file.path().to_str().unwrap()).unwrap();
    }

    #[test]
    fn test_explicit_round_robin_only_changes_unruled_group() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://a:80".into(),
            "-r".into(),
            "http://b:80".into(),
            "-s".into(),
            "rr".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("scheduler = \"round-robin\""));
    }

    #[test]
    fn test_translate_chain_with_creds_preserved() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://user:pass@a:1080__http://b:80".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let upstreams = parsed["upstreams"].as_array().unwrap();
        let uri = upstreams[0]["uri"].as_str().unwrap();
        // Credentials should be preserved in the config URI
        assert!(
            uri.contains("user:pass@"),
            "expected credentials in URI, got: {}",
            uri
        );
    }

    #[test]
    fn test_translate_chain_with_tls_hop() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5+tls://a:1080__http://b:80".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        let upstreams = parsed["upstreams"].as_array().unwrap();
        let uri = upstreams[0]["uri"].as_str().unwrap();
        assert!(
            uri.starts_with("socks5+tls://"),
            "expected TLS modifier in first hop, got: {}",
            uri
        );
    }

    #[test]
    fn test_translate_chain_ssh_hop_unsupported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://a:1080__ssh://b:22".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        #[cfg(feature = "ssh")]
        assert!(!output.has_unsupported());
        #[cfg(not(feature = "ssh"))]
        {
            assert!(output.has_unsupported());
            assert!(output
                .unsupported
                .iter()
                .any(|u| u.feature == "ssh-upstream" || u.feature == "chain-unsupported-hop"));
        }
    }

    #[test]
    fn test_translate_chain_ssr_hop_supported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://a:1080__ssr://b:8388".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(!output.has_unsupported());
        assert!(output.toml.contains("__ssr://"));
    }

    #[test]
    fn test_translate_chain_valid_toml_roundtrip() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "socks5://a:1080__http://b:80".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert_eq!(parsed["version"].as_integer(), Some(1));
        let listeners = parsed["listeners"].as_array().unwrap();
        assert_eq!(listeners.len(), 1);
        let upstreams = parsed["upstreams"].as_array().unwrap();
        assert_eq!(upstreams.len(), 1);
        let rules = parsed["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|r| r["id"].as_str() == Some("pproxy-default")));
    }

    #[test]
    fn test_alive_flag_generates_health_config() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
            "-a".into(),
            "10".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("[upstreams.health]"));
        assert!(output.toml.contains("interval = \"10s\""));
    }

    #[test]
    fn test_pac_flag_generates_admin_pac_config() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--pac".into(),
            "/proxy.pac".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("[admin.pac]"));
    }

    #[test]
    fn test_translate_trojan_listener_supported() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "trojan://my-secret@0.0.0.0:443".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(
            !output
                .unsupported
                .iter()
                .any(|u| u.feature == "trojan-listener"),
            "trojan listener should be supported now"
        );
        assert!(output.toml.contains("[listeners.trojan]"));
        assert!(output.toml.contains("password = \"my-secret\""));
        assert!(output.toml.contains("[listeners.tls]"));
    }

    #[test]
    fn test_translate_trojan_listener_toml_roundtrip() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "trojan://pass123@0.0.0.0:443".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        let parsed: toml::Value = toml::from_str(&output.toml).unwrap();
        assert_eq!(parsed["version"].as_integer(), Some(1));
        let listeners = parsed["listeners"].as_array().unwrap();
        assert_eq!(listeners.len(), 1);
        assert_eq!(
            listeners[0]["protocols"].as_array().unwrap()[0].as_str(),
            Some("trojan")
        );
        assert_eq!(listeners[0]["trojan"]["password"].as_str(), Some("pass123"));
        assert!(
            listeners[0]["tls"].is_table(),
            "configured TLS should be present for trojan"
        );
    }

    #[test]
    fn test_translate_trojan_listener_without_tls_is_unsupported() {
        let args =
            PproxyArgs::parse(&["-l".into(), "trojan://my-secret@0.0.0.0:443".into()]).unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("[listeners.trojan]"));
        assert!(output.toml.contains("password = \"my-secret\""));
        assert!(output
            .unsupported
            .iter()
            .any(|u| u.feature == "trojan-tls-config"));
        assert!(!output.toml.contains("/path/to/cert.pem"));
    }

    #[test]
    fn test_translate_trojan_upstream_still_works() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "trojan://secret@proxy.example:443".into(),
        ])
        .unwrap();
        let output = translate_pproxy_args(&args).unwrap();
        assert!(
            !output.has_unsupported(),
            "trojan upstream should remain supported: {:?}",
            output.unsupported
        );
        assert!(output.toml.contains("trojan://secret@proxy.example:443"));
    }
}
