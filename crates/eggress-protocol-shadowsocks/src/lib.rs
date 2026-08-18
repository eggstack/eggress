pub mod address;
pub mod aead;
pub mod compat;
pub mod error;
#[cfg(feature = "legacy-crypto")]
pub mod legacy;
pub mod method;
pub mod metrics;
pub mod nonce;
pub mod server;
pub mod tcp;
pub mod tcp_stream;
pub mod udp;

pub use error::ShadowsocksError;
pub use method::CipherMethod;
pub use metrics::ShadowsocksMetrics;
pub use tcp::{shadowsocks_accept, shadowsocks_connect};
