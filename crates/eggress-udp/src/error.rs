use thiserror::Error;

#[derive(Debug, Error)]
pub enum UdpError {
    #[error("association limit exceeded")]
    AssociationLimitExceeded,
    #[error("per-listener association limit exceeded")]
    ListenerAssociationLimitExceeded,
    #[error("target flow limit exceeded")]
    TargetFlowLimitExceeded,
    #[error("datagram too large: {0} > {1}")]
    DatagramTooLarge(usize, usize),
    #[error("codec error: {0}")]
    Codec(#[from] eggress_protocol_socks::socks5::udp_codec::UdpCodecError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("association closed")]
    AssociationClosed,
    #[error("client address mismatch")]
    ClientAddressMismatch,
    #[error("unresolved target")]
    UnresolvedTarget,
    #[error("multicast target not allowed")]
    MulticastTarget,
    #[error("broadcast target not allowed")]
    BroadcastTarget,
    #[error("unspecified target not allowed")]
    UnspecifiedTarget,
    #[error("port zero not allowed")]
    PortZero,
    #[error("{0}")]
    Other(String),
}
