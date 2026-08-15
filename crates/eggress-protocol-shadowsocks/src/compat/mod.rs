//! Bounded compatibility implementation for the SSR surface exposed by
//! pproxy 2.7.9.  This module is deliberately separate from native
//! Shadowsocks AEAD and rustls TLS code.

#[cfg(feature = "pproxy-legacy")]
pub mod plugin;
#[cfg(feature = "pproxy-legacy")]
pub mod ssr;
