//! Listener-free outbound chain execution for eggress.
//!
//! `eggress-outbound` owns the concrete proxy-hop composition used for
//! outbound chains: protocol-specific `HopHandler` implementations, the
//! chain-executor factory with TLS composition, typed failure
//! classification, and the listener-free [`OutboundConnector`].
//!
//! This crate is the direct listener-free Rust dependency. `eggress-server`
//! consumes the hop registry, executor factory, and classifier for its
//! listener-bound sessions; `eggress-embed` re-exports this API as its
//! full-service facade (`eggress_embed::outbound::*` remains
//! source-compatible).
//!
//! Feature selection is explicit: `toml` enables TOML construction,
//! `pproxy-compat` enables pproxy-URI construction, `udp` enables
//! listener-free UDP associations, `extended`/`pproxy-legacy`/
//! `legacy-crypto`/`ssh`/`quic` enable their protocol transports. `ssh`
//! never implies `pproxy-compat` and `pproxy-compat` never implies `ssh`;
//! pproxy-style SSH is available only when both are enabled. Typed error
//! kind/stage/hop/protocol facts are diagnostic, not retry recommendations.

pub mod classify;
mod connector;
mod error;
mod executor;
mod hops;

#[cfg(not(feature = "udp"))]
pub use connector::OUTBOUND_MAX_DATAGRAM_SIZE;
pub use connector::{
    OutboundConnectError, OutboundConnectErrorKind, OutboundConnectStage, OutboundConnector,
    OutboundInfo,
};
#[cfg(feature = "udp")]
pub use connector::{UdpAssociation, OUTBOUND_MAX_DATAGRAM_SIZE};
pub use error::OutboundError;
pub use executor::{
    build_chain_executor, build_chain_executor_with_options, OutboundExecutorOptions,
};
/// Shared target conversion used by outbound handshakes and listener
/// session code (`#[doc(hidden)]` shared seam, not end-user API).
#[doc(hidden)]
pub use hops::target_to_socks_addr;
