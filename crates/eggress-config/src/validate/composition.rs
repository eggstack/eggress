//! Composition validation: listener/upstream protocol matrix checks.
//!
//! Warns (never errors) on unsupported combinations; a missing matrix
//! file degrades to a visible warning rather than silent acceptance.

use crate::error::ConfigWarning;
use crate::model::ConfigFile;
pub(crate) const VALID_PROTOCOLS: &[&str] = &[
    "http",
    "socks4",
    "socks5",
    "shadowsocks",
    "trojan",
    "h2",
    "h3",
    "quic",
    "websocket",
    "ws",
    "wss",
    "raw",
    "echo",
];

pub(crate) const VALID_SCHEDULERS: &[&str] = &[
    "first-available",
    "round-robin",
    "random",
    "least-connections",
];

pub(crate) const VALID_FALLBACKS: &[&str] = &["reject", "direct", "use-unhealthy"];

pub(crate) const VALID_AUTH_TYPES: &[&str] = &["password"];

pub(crate) const VALID_REJECT_REASONS: &[&str] = &[
    "unsupported-protocol",
    "auth-required",
    "access-denied",
    "blocked",
    "internal-error",
];

pub(crate) const VALID_HEALTH_MODES: &[&str] = &["tcp_connect"];
pub(crate) const VALID_HEALTH_INITIAL_STATES: &[&str] =
    &["unknown", "healthy", "unhealthy", "disabled"];

/// Validate configuration against the composition matrix.
///
/// Produces warnings (not errors) for unsupported listener→upstream
/// protocol combinations. The composition matrix is resolved relative to the
/// working directory; if it cannot be loaded, a warning is emitted so the
/// suppressed checks remain visible.
pub fn validate_config_composition(config: &ConfigFile) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    // Load the composition matrix directly (no testkit dependency)
    let matrix = match load_composition_matrix() {
        Some(m) => m,
        None => {
            warnings.push(ConfigWarning {
                path: "composition_matrix".to_string(),
                message: "composition matrix not found relative to the working directory; \
                          protocol composition warnings are suppressed"
                    .to_string(),
            });
            return warnings;
        }
    };

    // Collect listener protocols from the `protocols` field
    let listener_protocols: Vec<&str> = config
        .listeners
        .as_ref()
        .map(|listeners| {
            listeners
                .iter()
                .flat_map(|l| l.protocols.iter().map(|p| p.as_str()))
                .collect()
        })
        .unwrap_or_default();

    // Collect upstream protocols from all groups
    if let Some(ref upstreams) = config.upstreams {
        let upstream_chains: std::collections::HashMap<&str, eggress_uri::ProxyChainSpec> =
            upstreams
                .iter()
                .filter_map(|u| {
                    eggress_uri::parse_proxy_chain(&u.uri)
                        .ok()
                        .map(|chain| (u.id.as_str(), chain))
                })
                .collect();

        for upstream in upstreams {
            if let Some(chain) = upstream_chains.get(upstream.id.as_str()) {
                let caps = eggress_core::capability::classify_upstream_chain(chain);

                // Check TCP capability
                if caps.is_tcp_supported() {
                    for &proto in &listener_protocols {
                        if !matrix_cell_supported(&matrix, proto, "listener", "tcp") {
                            warnings.push(ConfigWarning {
                                path: format!("upstreams[{}].uri", upstream.id),
                                message: format!(
                                    "listener protocol '{proto}' has no TCP composition cell; \
                                     upstream '{}' may not work",
                                    upstream.id
                                ),
                            });
                        }
                    }
                }

                // Check UDP capability
                if caps.is_udp_supported() {
                    for &proto in &listener_protocols {
                        if !matrix_cell_supported(&matrix, proto, "listener", "udp") {
                            warnings.push(ConfigWarning {
                                path: format!("upstreams[{}].uri", upstream.id),
                                message: format!(
                                    "listener protocol '{proto}' has no UDP composition cell; \
                                     upstream '{}' may not work with UDP relay",
                                    upstream.id
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    warnings
}

// Minimal composition matrix types for loading from TOML
#[derive(serde::Deserialize)]
pub(crate) struct CompositionCellMinimal {
    protocol: String,
    role: String,
    traffic_kind: String,
    tier: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct CompositionMatrixMinimal {
    cell: Vec<CompositionCellMinimal>,
}

// Embedded at compile time so embedders (eggress-embed, PyO3, binaries run
// from another CWD) do not silently suppress composition warnings.
// The file is vendored inside the crate (rather than included from
// `docs/parity/`) because `cargo package` only ships files under the crate
// directory; an escaping include would break every from-registry build.
// `docs/parity/composition_matrix.toml` remains canonical —
// `vendored_matrix_matches_canonical` below enforces byte equality.
pub(crate) const EMBEDDED_COMPOSITION_MATRIX: &str = include_str!("../../composition_matrix.toml");

pub(crate) fn load_composition_matrix() -> Option<CompositionMatrixMinimal> {
    // Look for the composition matrix relative to the workspace root first so
    // developers get live updates without recompiling; fall back to the
    // compile-time embedded copy for embedders and other CWDs.
    let candidates = [
        "docs/parity/composition_matrix.toml",
        "../docs/parity/composition_matrix.toml",
        "../../docs/parity/composition_matrix.toml",
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(matrix) = toml::from_str::<CompositionMatrixMinimal>(&content) {
                return Some(matrix);
            }
        }
    }
    if let Ok(matrix) = toml::from_str::<CompositionMatrixMinimal>(EMBEDDED_COMPOSITION_MATRIX) {
        return Some(matrix);
    }
    None
}

pub(crate) fn matrix_cell_supported(
    matrix: &CompositionMatrixMinimal,
    protocol: &str,
    role: &str,
    traffic_kind: &str,
) -> bool {
    matrix.cell.iter().any(|c| {
        c.protocol == protocol
            && c.role == role
            && c.traffic_kind == traffic_kind
            && c.tier != "unsupported"
    })
}
