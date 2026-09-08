//! Reload classification: which config changes are hot-swappable.
//!
//! The running accept loops capture listener behavior (protocols, auth, TLS,
//! Shadowsocks/Trojan material, connection limits, fixed targets, UDP
//! settings, transparent/unix topology) at startup from prepared listener
//! state and never re-read it from the snapshot. Classification therefore
//! rejects any material listener change so a reload never publishes a
//! snapshot generation the data plane is known not to use.

/// Result of a reload attempt.
#[derive(Debug)]
pub enum ReloadResult {
    /// Reload was applied successfully.
    Applied { generation: u64, upstreams: usize },
    /// Reload was rejected due to unsupported changes.
    Rejected { reason: String },
    /// Reload failed due to a compile or build error.
    Failed { error: String },
}

/// What is and isn't reloaded on SIGHUP:
///
/// **Reloaded (hot-swap, no downtime):**
/// - Upstream chains and health config (with Arc reuse for unchanged upstreams)
/// - Upstream groups, schedulers, and fallback policies
/// - Routing rules and default action
/// - Admin PAC and static content configuration
///
/// **NOT reloaded (requires full restart):**
/// - Listener socket bindings (bound before readiness)
/// - Listener socket options (`reuse_port`)
/// - Listener protocol lists, auth material, TLS material, Shadowsocks/Trojan
///   config, `connection_limit`, `fixed_target`, `local_bind`, and all UDP
///   listener settings: the running accept loops and per-connection tasks
///   clone these values from startup-prepared listener state and never
///   re-read them from the snapshot.
/// - Transparent/unix listener configuration
/// - Process-level settings (log format, log level, shutdown grace)
/// - Timeout configuration
/// - Admin bind address
///
/// **UDP-specific reload semantics:**
/// - UDP limits apply to new associations only; existing associations keep their limits.
/// - UDP bind changes are restart-required.
/// - UDP advertise address changes are restart-required if socket bind changes.
/// - Route changes apply immediately to future UDP packets.
///
/// Classify whether a reload is supported given old and new listener
/// configs. Returns `Ok(())` if the reload is safe, or `Err(reason)`
/// if it should be rejected.
pub(crate) fn classify_listeners(
    old_listeners: &[eggress_config::compile::ListenerConfig],
    new_listeners: &[eggress_config::compile::ListenerConfig],
) -> Result<(), String> {
    if old_listeners.len() != new_listeners.len() {
        return Err(format!(
            "listener count changed ({} -> {}); restart required",
            old_listeners.len(),
            new_listeners.len()
        ));
    }

    for (old, new) in old_listeners.iter().zip(new_listeners.iter()) {
        if old.name != new.name {
            return Err(format!(
                "listener name changed ('{}' -> '{}'); restart required",
                old.name, new.name
            ));
        }
        if old.bind != new.bind {
            return Err(format!(
                "listener bind address changed for '{}': '{}' -> '{}'; restart required",
                old.name, old.bind, new.bind
            ));
        }
        match (&old.udp, &new.udp) {
            (Some(old_udp), Some(new_udp)) => {
                // All UDP listener settings are captured into startup-prepared
                // listener/relay state (`PreparedListener.udp`,
                // `RuntimeUdpService.udp_config`, standalone relay sockets).
                // Per-association relay tasks clone that startup state, so any
                // material UDP change requires a restart.
                // Socket topology group.
                if old_udp.bind != new_udp.bind
                    || old_udp.enabled != new_udp.enabled
                    || old_udp.mode != new_udp.mode
                    || old_udp.upstream_udp_bind != new_udp.upstream_udp_bind
                {
                    return Err(format!(
                        "UDP listener configuration changed for '{}'; restart required",
                        old.name
                    ));
                }
                // Association limit group.
                if old_udp.max_associations != new_udp.max_associations
                    || old_udp.max_associations_global != new_udp.max_associations_global
                    || old_udp.max_targets_per_association != new_udp.max_targets_per_association
                    || old_udp.max_datagram_size != new_udp.max_datagram_size
                {
                    return Err(format!(
                        "UDP listener limits changed for '{}'; restart required",
                        old.name
                    ));
                }
                // Timeout/behavior group.
                if old_udp.idle_timeout != new_udp.idle_timeout
                    || old_udp.target_idle_timeout != new_udp.target_idle_timeout
                    || old_udp.upstream_connect_timeout != new_udp.upstream_connect_timeout
                    || old_udp.client_pin != new_udp.client_pin
                    || old_udp.allow_private_egress != new_udp.allow_private_egress
                    || old_udp.advertise != new_udp.advertise
                    || old_udp.fixed_target != new_udp.fixed_target
                {
                    return Err(format!(
                        "UDP listener settings changed for '{}'; restart required",
                        old.name
                    ));
                }
            }
            (None, Some(new_udp)) => {
                return Err(format!(
                    "UDP configuration added for '{}': '{}'; restart required",
                    new.name, new_udp.bind
                ));
            }
            (Some(_old_udp), None) => {
                return Err(format!(
                    "UDP configuration removed for '{}'; restart required",
                    old.name
                ));
            }
            (None, None) => {}
        }

        match (&old.transparent, &new.transparent) {
            (Some(old_t), Some(new_t)) => {
                // Transparent accept loops capture their configuration at
                // startup; protocol selection included.
                if old_t.enabled != new_t.enabled || old_t.protocol != new_t.protocol {
                    return Err(format!(
                        "transparent config changed for '{}': enabled {} -> {}; restart required",
                        old.name, old_t.enabled, new_t.enabled
                    ));
                }
            }
            (None, Some(new_t)) => {
                if new_t.enabled {
                    return Err(format!(
                        "transparent proxy enabled for '{}'; restart required",
                        new.name
                    ));
                }
            }
            (Some(old_t), None) if old_t.enabled => {
                return Err(format!(
                    "transparent proxy configuration removed for '{}'; restart required",
                    old.name
                ));
            }
            (Some(_old_t), None) => {}
            (None, None) => {}
        }

        match (&old.unix, &new.unix) {
            (Some(old_u), Some(new_u)) => {
                // Unix socket setup (bind, ownership, mode) happens once at
                // startup; any change requires a restart.
                if old_u.path != new_u.path
                    || old_u.unlink_existing != new_u.unlink_existing
                    || old_u.mode != new_u.mode
                {
                    return Err(format!(
                        "unix socket path changed for '{}': '{}' -> '{}'; restart required",
                        old.name,
                        old_u.path.display(),
                        new_u.path.display()
                    ));
                }
            }
            (None, Some(_new_u)) => {
                return Err(format!(
                    "unix socket added for '{}'; restart required",
                    new.name
                ));
            }
            (Some(_old_u), None) => {
                return Err(format!(
                    "unix socket removed for '{}'; restart required",
                    old.name
                ));
            }
            (None, None) => {}
        }

        // Startup-captured listener behavior: the running accept loops clone
        // these values from `PreparedListener` at startup and never re-read
        // them from the snapshot, so any material change requires a restart.
        // Comparison groups are explicit (not whole-struct equality) so a
        // future field addition gets a deliberate classification review.
        // Socket-option group.
        if old.reuse_port != new.reuse_port {
            return Err(format!(
                "listener socket options changed for '{}'; restart required",
                old.name
            ));
        }
        // Protocol dispatch group.
        if old.protocols != new.protocols {
            return Err(format!(
                "listener protocols changed for '{}'; restart required",
                old.name
            ));
        }
        // Auth material group: compare presence and non-secret fields plus
        // resolved secret presence. Values themselves are never logged.
        match (&old.auth, &new.auth) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return Err(format!(
                    "listener auth presence changed for '{}'; restart required",
                    old.name
                ));
            }
            (Some(old_a), Some(new_a)) => {
                if old_a.auth_type != new_a.auth_type
                    || old_a.username != new_a.username
                    || old_a.password != new_a.password
                    || old_a.password_env != new_a.password_env
                {
                    return Err(format!(
                        "listener auth material changed for '{}'; restart required",
                        old.name
                    ));
                }
            }
        }
        // TLS material group: certificate/key/ALPN feed the per-connection
        // TLS acceptor built from startup state.
        match (&old.tls, &new.tls) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return Err(format!(
                    "listener TLS presence changed for '{}'; restart required",
                    old.name
                ));
            }
            (Some(old_t), Some(new_t)) => {
                if old_t.cert_pem != new_t.cert_pem
                    || old_t.key_pem != new_t.key_pem
                    || old_t.alpn != new_t.alpn
                {
                    return Err(format!(
                        "listener TLS material changed for '{}'; restart required",
                        old.name
                    ));
                }
            }
        }
        // Shadowsocks/Trojan group: cloned into per-connection inbound config.
        match (&old.shadowsocks, &new.shadowsocks) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return Err(format!(
                    "listener shadowsocks presence changed for '{}'; restart required",
                    old.name
                ));
            }
            (Some(old_s), Some(new_s)) => {
                if old_s.method != new_s.method
                    || old_s.password != new_s.password
                    || old_s.auth_prefix != new_s.auth_prefix
                    || old_s.plugins != new_s.plugins
                {
                    return Err(format!(
                        "listener shadowsocks configuration changed for '{}'; restart required",
                        old.name
                    ));
                }
            }
        }
        match (&old.trojan, &new.trojan) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return Err(format!(
                    "listener trojan presence changed for '{}'; restart required",
                    old.name
                ));
            }
            (Some(old_t), Some(new_t)) => {
                if old_t.password != new_t.password || old_t.fallback != new_t.fallback {
                    return Err(format!(
                        "listener trojan configuration changed for '{}'; restart required",
                        old.name
                    ));
                }
            }
        }
        // Connection-behavior group.
        if old.connection_limit != new.connection_limit
            || old.fixed_target != new.fixed_target
            || old.local_bind != new.local_bind
        {
            return Err(format!(
                "listener connection_limit/fixed_target/local_bind changed for '{}'; restart required",
                old.name
            ));
        }
    }

    Ok(())
}

pub fn classify_reload_config(
    old_listeners: &[eggress_config::compile::ListenerConfig],
    old_timeouts: &eggress_config::compile::TimeoutConfig,
    old_admin: Option<&eggress_config::compile::AdminConfig>,
    new_config: &eggress_config::compile::RuntimeConfig,
) -> Result<(), String> {
    classify_listeners(old_listeners, &new_config.listeners)?;
    if old_timeouts != &new_config.timeouts {
        return Err("timeout configuration changed; restart required".to_string());
    }

    let old_admin_endpoint = old_admin.map(|admin| (admin.enabled, admin.bind.as_str()));
    let new_admin_endpoint = new_config
        .admin
        .as_ref()
        .map(|admin| (admin.enabled, admin.bind.as_str()));
    if old_admin_endpoint != new_admin_endpoint {
        return Err("admin endpoint bind configuration changed; restart required".to_string());
    }

    Ok(())
}
