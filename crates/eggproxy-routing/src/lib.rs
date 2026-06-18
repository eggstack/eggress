use eggproxy_core::{RouteAction, TargetAddr, UpstreamId};
use eggproxy_uri::ProxyChainSpec;

/// Errors that can occur during routing.
#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("no route found for target")]
    NoRoute,
    #[error("invalid upstream configuration")]
    InvalidUpstream,
    #[error("upstream not available")]
    UpstreamUnavailable,
}

/// Configuration for an upstream proxy.
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// Unique identifier for this upstream.
    pub id: UpstreamId,
    /// Chain specification for this upstream.
    pub chain: ProxyChainSpec,
}

/// Router that resolves target addresses to route actions.
#[derive(Debug)]
pub struct Router {
    #[allow(dead_code)]
    upstreams: Vec<UpstreamConfig>,
}

impl Router {
    /// Create a new router with the given upstream configurations.
    pub fn new(upstreams: Vec<UpstreamConfig>) -> Self {
        Self { upstreams }
    }

    /// Resolve a target address to a route action.
    ///
    /// This is a placeholder implementation that always returns Direct.
    pub fn resolve(&self, _target: &TargetAddr) -> Result<RouteAction, RouteError> {
        Ok(RouteAction::Direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggproxy_core::{TargetAddr, TargetHost};

    #[test]
    fn test_router_resolve_direct() {
        let router = Router::new(vec![]);
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".to_string()),
            port: 80,
        };
        let action = router.resolve(&target).unwrap();
        assert!(matches!(action, RouteAction::Direct));
    }
}
