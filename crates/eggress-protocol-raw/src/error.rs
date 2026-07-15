#[derive(Debug, thiserror::Error)]
pub enum RawTunnelError {
    #[error("no target configured for raw tunnel")]
    NoTarget,
    #[error("target connect failed: {0}")]
    TargetConnect(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("DNS rebinding detected: target resolved to reserved/private address {0}")]
    DnsRebinding(std::net::IpAddr),
}
