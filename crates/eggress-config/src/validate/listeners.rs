//! Listener validation: bindings, protocols, auth, TLS, and UDP blocks.

use std::collections::HashSet;

use super::composition::{VALID_AUTH_TYPES, VALID_PROTOCOLS};
use super::core::parse_duration;
use crate::error::ConfigError;
pub(crate) fn validate_listeners(
    listeners: &[crate::model::ListenerConfig],
    errors: &mut Vec<ConfigError>,
) {
    let mut names = HashSet::new();

    for (i, listener) in listeners.iter().enumerate() {
        let path = format!("listeners[{}]", i);

        if !names.insert(&listener.name) {
            errors.push(ConfigError::validation(
                &path,
                &format!("duplicate listener name: {}", listener.name),
            ));
        }

        if listener.protocols.is_empty() {
            errors.push(ConfigError::validation(
                &path,
                "protocols must not be empty",
            ));
        }

        for protocol in &listener.protocols {
            if !VALID_PROTOCOLS.contains(&protocol.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown protocol: {}", protocol),
                ));
            }
        }

        if let Some(ref auth) = listener.auth {
            if !VALID_AUTH_TYPES.contains(&auth.auth_type.as_str()) {
                errors.push(ConfigError::validation(
                    &path,
                    &format!("unknown auth type: {}", auth.auth_type),
                ));
            }
            if auth.username.as_deref().unwrap_or("").is_empty() {
                errors.push(ConfigError::validation(
                    &path,
                    "auth requires a non-empty username",
                ));
            }
            if auth.password.is_none() && auth.password_env.is_none() {
                errors.push(ConfigError::validation(
                    &path,
                    "auth requires at least one of password or password_env",
                ));
            }
            if auth.password.as_deref() == Some("") {
                errors.push(ConfigError::validation(
                    &path,
                    "auth password must not be empty",
                ));
            }
        }

        if listener.connection_limit == Some(0) {
            errors.push(ConfigError::validation(
                &path,
                "connection_limit must be greater than 0",
            ));
        }

        if let Some(ref udp) = listener.udp {
            validate_listener_udp(udp, &path, errors);
        }

        // Trojan requires TLS — the TLS layer is part of the protocol
        if listener.protocols.contains(&"trojan".to_string()) && listener.tls.is_none() {
            errors.push(ConfigError::validation(
                &path,
                "trojan protocol requires TLS configuration ([listeners.tls])",
            ));
        }

        // Trojan requires a [listeners.trojan] section with a password
        if listener.protocols.contains(&"trojan".to_string()) && listener.trojan.is_none() {
            errors.push(ConfigError::validation(
                &path,
                "trojan protocol requires [listeners.trojan] section with password",
            ));
        }

        // Trojan password must not be empty if provided
        if let Some(ref trojan) = listener.trojan {
            if trojan.password.is_empty() {
                errors.push(ConfigError::validation(
                    &format!("{}.trojan.password", path),
                    "trojan password must not be empty",
                ));
            }
            // Validate fallback address format if provided
            if let Some(ref fallback) = trojan.fallback {
                if fallback.parse::<eggress_core::TargetAddr>().is_err() {
                    errors.push(ConfigError::validation(
                        &format!("{}.trojan.fallback", path),
                        &format!(
                            "invalid fallback address format: '{fallback}' (expected host:port)"
                        ),
                    ));
                }
            }
        }
    }
}

pub(crate) fn validate_listener_udp(
    udp: &crate::model::ListenerUdpConfig,
    parent_path: &str,
    errors: &mut Vec<ConfigError>,
) {
    let udp_path = format!("{}.udp", parent_path);

    if let Some(ref bind) = udp.bind {
        if bind.parse::<std::net::SocketAddr>().is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.bind", udp_path),
                &format!("invalid socket address: {}", bind),
            ));
        }
    }

    if let Some(ref advertise) = udp.advertise {
        if advertise.parse::<std::net::IpAddr>().is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.advertise", udp_path),
                &format!("invalid IP address: {}", advertise),
            ));
        }
    }

    if let Some(ref idle_timeout) = udp.idle_timeout {
        if parse_duration(idle_timeout).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.idle_timeout", udp_path),
                &format!("invalid duration: {}", idle_timeout),
            ));
        }
    }

    if let Some(ref target_idle_timeout) = udp.target_idle_timeout {
        if parse_duration(target_idle_timeout).is_err() {
            errors.push(ConfigError::validation(
                &format!("{}.target_idle_timeout", udp_path),
                &format!("invalid duration: {}", target_idle_timeout),
            ));
        }
    }

    if let Some(max_associations) = udp.max_associations {
        if max_associations == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.max_associations", udp_path),
                "must be greater than 0",
            ));
        }
    }

    if let Some(max_targets) = udp.max_targets_per_association {
        if max_targets == 0 {
            errors.push(ConfigError::validation(
                &format!("{}.max_targets_per_association", udp_path),
                "must be greater than 0",
            ));
        }
    }

    if let Some(max_datagram_size) = udp.max_datagram_size {
        if !(257..=65535).contains(&max_datagram_size) {
            errors.push(ConfigError::validation(
                &format!("{}.max_datagram_size", udp_path),
                &format!("must be between 257 and 65535, got {}", max_datagram_size),
            ));
        }
    }
}

/// Check if a socket address string binds to loopback.
///
/// Returns `true` for `127.x.x.x`, `::1`, and IPv4-mapped loopback addresses,
/// including bare IPs without a port and `localhost` hostnames.
/// Returns `false` for `0.0.0.0`, `::`, and other non-loopback addresses.
pub(crate) fn is_loopback_bind(addr: &str) -> bool {
    // Canonical `SocketAddr` form (host:port, including `[::1]:8080`).
    if let Ok(socket) = addr.parse::<std::net::SocketAddr>() {
        return match socket.ip() {
            std::net::IpAddr::V4(v4) => v4.is_loopback(),
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
            }
        };
    }
    // Bare IP without port (e.g. `127.0.0.1`, `::1`, `::ffff:127.0.0.1`).
    if let Ok(ip) = addr.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback(),
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
            }
        };
    }
    // Hostname forms: `localhost` or `localhost:8080`, plus bracketed IPv6
    // without port. Only `localhost` is treated as loopback at the hostname
    // layer; other names are conservative non-loopback.
    let host_part = if addr.starts_with('[') {
        if let Some(end) = addr.find(']') {
            let inside = &addr[1..end];
            // Validate that trailing part is either empty or `:port`.
            let after = &addr[end + 1..];
            if after.is_empty() || (after.starts_with(':') && after[1..].parse::<u16>().is_ok()) {
                inside
            } else {
                addr
            }
        } else {
            addr
        }
    } else if let Some(colon) = addr.rfind(':') {
        let host = &addr[..colon];
        let port_part = &addr[colon + 1..];
        if port_part.parse::<u16>().is_ok() && !host.is_empty() {
            host
        } else {
            addr
        }
    } else {
        addr
    };
    if host_part.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = host_part.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback(),
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
            }
        };
    }
    false
}
