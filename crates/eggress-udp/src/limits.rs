use std::time::Duration;

#[derive(Debug, Clone)]
pub struct UdpLimits {
    pub max_associations_global: usize,
    pub max_associations_per_listener: usize,
    pub max_targets_per_association: usize,
    pub max_datagram_size: usize,
    pub idle_timeout: Duration,
    pub client_pin: bool,
    pub target_idle_timeout: Duration,
}

impl Default for UdpLimits {
    fn default() -> Self {
        Self {
            max_associations_global: 1024,
            max_associations_per_listener: 256,
            max_targets_per_association: 64,
            max_datagram_size: 65535,
            idle_timeout: Duration::from_secs(60),
            client_pin: true,
            target_idle_timeout: Duration::from_secs(30),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits() {
        let limits = UdpLimits::default();
        assert_eq!(limits.max_associations_global, 1024);
        assert_eq!(limits.max_associations_per_listener, 256);
        assert_eq!(limits.max_targets_per_association, 64);
        assert_eq!(limits.max_datagram_size, 65535);
        assert_eq!(limits.idle_timeout, Duration::from_secs(60));
        assert!(limits.client_pin);
        assert_eq!(limits.target_idle_timeout, Duration::from_secs(30));
    }

    #[test]
    fn clone_preserves_values() {
        let limits = UdpLimits::default();
        let cloned = limits.clone();
        assert_eq!(cloned.max_associations_global, 1024);
        assert_eq!(cloned.idle_timeout, Duration::from_secs(60));
    }
}
