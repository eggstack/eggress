//! Semantic translation builder: pproxy args/URIs become renderer-neutral
//! [`TranslationIntermediates`].
//!
//! Both renderers (`toml::generate_toml`, `native::intermediates_to_config_file`)
//! consume these intermediates, so TOML stays presentation-only and internal
//! consumers never need a serialize/parse round trip.

use super::model::{
    AuthToml, HealthToml, ListenerToml, MatchToml, ReverseClientToml, ReverseServerToml, RuleToml,
    ShadowsocksToml, SsrToml, StaticContentToml, TlsToml, TransparentToml, TrojanToml, UdpToml,
    UnixToml, UpstreamGroupToml, UpstreamToml,
};
use super::rules::{
    combine_pproxy_patterns, inline_pproxy_pattern, load_pproxy_rule_file, pproxy_rule_match,
    TcpCompatRoute,
};
use crate::args::PproxyArgs;
use crate::error::CompatError;
use crate::uri::{PproxyChain, PproxyPluginSpec, PproxyUri};
use crate::warnings::TranslationOutput;

/// Typed intermediate translation result (no TOML string).
///
/// Semantic translation produces this; TOML rendering (`generate_toml`) and
/// native compilation (`intermediates_to_config_file`) are both renderers over
/// it. This keeps TOML as presentation, not mandatory internal IR.
#[derive(Debug, Clone)]
pub(crate) struct TranslationIntermediates {
    pub(crate) listeners: Vec<ListenerToml>,
    pub(crate) upstreams: Vec<UpstreamToml>,
    pub(crate) upstream_groups: Vec<UpstreamGroupToml>,
    pub(crate) rules: Vec<RuleToml>,
    pub(crate) reverse_servers: Vec<ReverseServerToml>,
    pub(crate) reverse_clients: Vec<ReverseClientToml>,
    pub(crate) pac_enabled: bool,
    pub(crate) pac_path: Option<String>,
    pub(crate) static_content: Vec<StaticContentToml>,
}

/// Translate pproxy-style local and remote URIs into Eggress TOML.
pub(crate) fn build_intermediates(
    args: &PproxyArgs,
    local_uris: &[PproxyUri],
    remote_chains: &[PproxyChain],
) -> Result<(TranslationIntermediates, TranslationOutput), CompatError> {
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

    let intermediates = TranslationIntermediates {
        listeners,
        upstreams,
        upstream_groups,
        rules,
        reverse_servers,
        reverse_clients,
        pac_enabled,
        pac_path,
        static_content,
    };
    Ok((intermediates, output))
}

/// Parse a `-ul` address value into a bind address.
///
/// Handles formats: `:1081`, `0.0.0.0:1081`, `127.0.0.1:1081`, `socks5://:1081`, plain port `1081`.
pub(crate) fn parse_udp_listen_addr(addr: &str) -> String {
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

pub(crate) fn percent_encode(s: &str) -> String {
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

pub(crate) fn validate_pproxy_plugins(plugins: &[PproxyPluginSpec]) -> Result<(), String> {
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

pub(crate) fn build_chain_config_uri(chain: &PproxyChain) -> String {
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

pub(crate) fn build_config_uri(remote: &PproxyUri) -> String {
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
