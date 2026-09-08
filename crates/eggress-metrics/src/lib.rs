//! Prometheus metrics for eggress: one registry, explicit per-family ownership.
//!
//! # Ownership model (Model A — subsystem counters canonical)
//!
//! Each metric family has exactly one canonical stored value:
//!
//! | Family group | Canonical owner | Registry role |
//! |---|---|---|
//! | Session/connections/bytes/auth | `MetricsRegistry` counters/gauges, fed by `SessionMetrics` events | canonical store |
//! | Route decisions, upstream open/failure, unsupported transport | Prometheus `Family` objects in this registry, fed by per-event calls | canonical store (subsystems own no parallel labeled totals) |
//! | Reload/generation/platform/transparent/unix | `MetricsRegistry` counters/gauges, fed by [`RuntimeMetrics`] events or the transparent bridge | canonical store |
//! | UDP relay/standalone | [`eggress_udp::metrics::UdpMetrics`] atomics (hot-path, lock-free) | exposition mirror via saturating-delta promotion at render time |
//! | Shadowsocks (feature `extended`) | `ShadowsocksMetrics` atomics | exposition mirror via delta promotion |
//! | H2 (except labeled streams) | `H2_PROTOCOL_METRICS` global atomics | exposition mirror via delta promotion; active gauges derived live as opened-minus-closed |
//! | H2 streams (`{upstream_id, outcome}`) | Prometheus `Family` in this registry | canonical store (protocol layer owns no per-label state) |
//! | Transparent accepted/dst-failed | Supervisor-state atomics, bridged via `set_transparent_counters` | exposition mirror via delta promotion |
//!
//! Delta promotion exists only because `prometheus_client::Counter` is
//! increment-only: at render time each mirror advances by
//! `current.saturating_sub(previous)`. It is the transport from the canonical
//! subsystem value to exposition, not a second stored truth. Gauges tracking
//! current state (`*_active`, `config_generation`, health) are set directly.
//! Repeated renders with no new subsystem activity change nothing
//! (idempotent totals, no double-count).
//!
//! # Interfaces
//!
//! - `eggress_server::SessionMetrics`: session, route, upstream, auth events
//!   only. The server never sees reload, platform, or exposition concerns.
//! - [`RuntimeMetrics`]: reload, generation, platform, transparent, unix, UDP
//!   association, and exposition for the supervisor, reload paths, and embed
//!   API.
//!
//! # Modules
//!
//! Split from the former single-file `lib.rs` by metric domain; no metric
//! names, labels, or recording semantics changed in the move.

mod h2;
mod labels;
mod registry;
mod render;
mod runtime;
mod session;
#[cfg(feature = "extended")]
mod shadowsocks;
#[cfg(test)]
mod tests;
mod udp;

pub use h2::H2MetricsSnapshot;
pub use labels::{
    DecodeErrorLabels, H2ConnectionLabels, H2StreamLabels, RouteLabels, UnsupportedTransportLabels,
    UpstreamFailureLabels, UpstreamLabels, UpstreamOpenLabels,
};
pub use registry::MetricsRegistry;
pub use runtime::RuntimeMetrics;
