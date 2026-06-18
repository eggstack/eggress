pub mod client;
pub mod error;
pub mod server;

pub use client::socks4_connect;
pub use error::Socks4Error;
pub use server::{read_socks4_request, write_socks4_reply, Socks4Request, Socks4Status};
