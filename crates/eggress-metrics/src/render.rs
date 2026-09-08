//! Prometheus exposition: bridge promotion plus text encoding.
//!
//! [`MetricsRegistry::render_prometheus`] first promotes every bridged
//! subsystem (UDP, Shadowsocks, H2, transparent) and then encodes the
//! registry. Promotion is idempotent with respect to stored totals: a render
//! with no new subsystem activity changes nothing.

use prometheus_client::encoding::text::encode;

use super::registry::MetricsRegistry;

impl MetricsRegistry {
    /// Promote all bridges and encode the registry to Prometheus text format.
    pub fn render_prometheus(&self) -> String {
        self.sync_udp_bridges();
        #[cfg(feature = "extended")]
        self.sync_shadowsocks_bridges();
        self.sync_h2();
        self.sync_transparent();

        let mut buf = String::new();
        encode(&mut buf, &self.registry).expect("String write is infallible");
        buf
    }
}
