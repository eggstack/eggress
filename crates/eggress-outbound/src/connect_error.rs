//! Typed outbound TCP failure surface (private implementation, public types).
//!
//! Owns [`OutboundConnectError`], [`OutboundConnectErrorKind`],
//! [`OutboundConnectStage`], plus the private classifier adaptation used by
//! [`OutboundConnector`](crate::OutboundConnector) TCP execution. `Display`/
//! `Debug` are bounded, redacted, and safe to log: kind/stage/hop/protocol
//! facts only, never credentials or URIs.

/// Stable failure category for listener-free TCP establishment.
///
/// Protocol-neutral and general-purpose: embedding consumers match on this
/// plus [`OutboundConnectError::stage`]/[`OutboundConnectError::hop_index`]
/// to implement their own routing policy without parsing display strings.
/// The enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so new categories can be added without breaking downstream matches.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundConnectErrorKind {
    Timeout,
    Dns,
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    Authentication,
    Tls,
    Protocol,
    Policy,
    Other,
}

impl std::fmt::Display for OutboundConnectErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Timeout => "timeout",
            Self::Dns => "dns",
            Self::ConnectionRefused => "connection_refused",
            Self::NetworkUnreachable => "network_unreachable",
            Self::HostUnreachable => "host_unreachable",
            Self::Authentication => "authentication",
            Self::Tls => "tls",
            Self::Protocol => "protocol",
            Self::Policy => "policy",
            Self::Other => "other",
        };
        f.write_str(label)
    }
}

/// Route stage where a listener-free TCP establishment failed.
///
/// Distinguishes transport failures (opening TCP to a hop) from proxy
/// protocol handshake failures (the proxy accepted TCP but reported a
/// destination/authentication problem), plus the caller-supplied outer
/// deadline. Consumers combine `kind` + `stage` + `hop_index` into their own
/// policy; Eggress never retries or falls back on their behalf.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundConnectStage {
    DirectConnect,
    HopConnect,
    HopHandshake,
    Deadline,
}

impl std::fmt::Display for OutboundConnectStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::DirectConnect => "direct_connect",
            Self::HopConnect => "hop_connect",
            Self::HopHandshake => "hop_handshake",
            Self::Deadline => "deadline",
        };
        f.write_str(label)
    }
}

/// Typed failure for listener-free TCP establishment.
///
/// Returned by
/// [`OutboundConnector::connect_tcp_detailed`](crate::OutboundConnector::connect_tcp_detailed) and
/// [`OutboundConnector::connect_tcp_timeout_detailed`](crate::OutboundConnector::connect_tcp_timeout_detailed). The struct uses
/// private fields with accessors so metadata can be extended without forcing
/// exhaustive matches on internal details.
///
/// `Display` and `Debug` are bounded, redacted, and safe to log: they carry
/// only kind/stage/hop/protocol facts, never proxy credentials, passwords,
/// auth headers, full credential-bearing URIs, or configuration snippets.
/// Callers already know the target they requested; it is not echoed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundConnectError {
    kind: OutboundConnectErrorKind,
    stage: OutboundConnectStage,
    hop_index: Option<usize>,
    protocol: Option<String>,
    message: String,
}

impl OutboundConnectError {
    pub(crate) fn new(
        kind: OutboundConnectErrorKind,
        stage: OutboundConnectStage,
        hop_index: Option<usize>,
        protocol: Option<String>,
    ) -> Self {
        let message = match (&hop_index, &protocol) {
            (Some(hop), Some(proto)) => {
                format!("outbound connection failed: kind={kind} stage={stage} hop={hop} protocol={proto}")
            }
            (Some(hop), None) => {
                format!("outbound connection failed: kind={kind} stage={stage} hop={hop}")
            }
            (None, Some(proto)) => {
                format!("outbound connection failed: kind={kind} stage={stage} protocol={proto}")
            }
            (None, None) => {
                format!("outbound connection failed: kind={kind} stage={stage}")
            }
        };
        Self {
            kind,
            stage,
            hop_index,
            protocol,
            message,
        }
    }

    /// Stable failure category for routing policy.
    pub fn kind(&self) -> OutboundConnectErrorKind {
        self.kind
    }

    /// Route stage where establishment failed.
    pub fn stage(&self) -> OutboundConnectStage {
        self.stage
    }

    /// Failing hop index for chained failures, if applicable.
    pub fn hop_index(&self) -> Option<usize> {
        self.hop_index
    }

    /// Bounded protocol label for handshake failures, if applicable.
    ///
    /// Derived from the chain's protocol identifier (lowercased, e.g.
    /// `"http"`, `"socks5"`, `"tls"`); never contains credentials.
    pub fn protocol(&self) -> Option<&str> {
        self.protocol.as_deref()
    }
}

impl std::fmt::Display for OutboundConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OutboundConnectError {}

/// Internal classified failure retaining both public representations.
///
/// `error` is the detailed typed surface; `compat_message` is the exact
/// sanitized string legacy `connect_tcp()`/`connect_tcp_timeout()` return
/// inside `OutboundError::Runtime`, preserving behavioral compatibility.
pub(crate) struct ClassifiedFailure {
    pub(crate) error: OutboundConnectError,
    pub(crate) compat_message: String,
}

pub(crate) fn map_classified_kind(
    kind: crate::classify::ClassifiedKind,
) -> OutboundConnectErrorKind {
    match kind {
        crate::classify::ClassifiedKind::Timeout => OutboundConnectErrorKind::Timeout,
        crate::classify::ClassifiedKind::Dns => OutboundConnectErrorKind::Dns,
        crate::classify::ClassifiedKind::Refused => OutboundConnectErrorKind::ConnectionRefused,
        crate::classify::ClassifiedKind::NetworkUnreachable => {
            OutboundConnectErrorKind::NetworkUnreachable
        }
        crate::classify::ClassifiedKind::HostUnreachable => {
            OutboundConnectErrorKind::HostUnreachable
        }
        crate::classify::ClassifiedKind::Auth => OutboundConnectErrorKind::Authentication,
        crate::classify::ClassifiedKind::Tls => OutboundConnectErrorKind::Tls,
        crate::classify::ClassifiedKind::Protocol => OutboundConnectErrorKind::Protocol,
        crate::classify::ClassifiedKind::Policy => OutboundConnectErrorKind::Policy,
        crate::classify::ClassifiedKind::Other => OutboundConnectErrorKind::Other,
    }
}

/// Normalize a chain protocol label for the public typed surface.
///
/// The chain executor reports labels like `"Http"`, `"Socks5"`, `"tls"`, or
/// `"Http+Socks5"`. Lowercasing keeps the stable contract predictable while
/// remaining bounded; the label originates from `ProtocolSpec` debug names
/// and never carries credentials.
pub(crate) fn normalize_protocol_label(raw: &str) -> String {
    raw.to_ascii_lowercase()
}
