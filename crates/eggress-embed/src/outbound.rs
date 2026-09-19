//! Listener-free outbound connector (compatibility facade).
//!
//! The implementation authority lives in `eggress-outbound`:
//! [`OutboundConnector`] executes compiled native chains directly without
//! starting a listener service. This module re-exports that API so existing
//! `use eggress_embed::outbound::{OutboundConnector, OutboundConnectErrorKind}`
//! downstream code continues to work unchanged.
//!
//! `eggress-embed` remains the full-service facade (`EggressService` /
//! `EggressHandle`); `eggress-outbound` is the direct listener-free Rust
//! dependency. Feature selection is explicit and forwarded: `pproxy-compat`
//! enables `from_pproxy_uri`, `ssh` enables native/TOML SSH upstreams
//! (pproxy-style SSH requires both), and outbound UDP stays available.

pub use eggress_outbound::*;
