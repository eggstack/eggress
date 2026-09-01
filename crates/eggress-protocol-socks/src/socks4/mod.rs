pub mod client;
pub mod error;
pub mod server;

#[cfg(test)]
mod test_server;

/// Maximum length for the SOCKS4 user ID field (inclusive).
pub const MAX_USER_ID_LEN: usize = 255;
/// Maximum length for a SOCKS4a domain field (inclusive, RFC 1035).
pub const MAX_DOMAIN_LEN: usize = 255;

pub use client::socks4_connect;
pub use error::Socks4Error;
pub use server::{read_socks4_request, write_socks4_reply, Socks4Request, Socks4Status};
