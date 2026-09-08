//! Prometheus label sets and label hygiene.
//!
//! Label cardinality is bounded by construction: routes by rule/action/outcome,
//! upstreams by upstream/group id, decode errors by kind, H2 streams by a
//! fixed upstream id and outcome. [`bounded_route_label`] keeps free-form
//! route labels within [`MAX_ROUTE_LABEL_LENGTH`] and neutralizes control
//! characters so untrusted rule names cannot pollute exposition.

use prometheus_client::encoding::EncodeLabelSet;

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct RouteLabels {
    pub rule: String,
    pub action: String,
    pub outcome: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UpstreamLabels {
    pub upstream_id: String,
    pub group_id: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct DecodeErrorLabels {
    pub kind: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UpstreamOpenLabels {
    pub protocol: String,
    pub outcome: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UpstreamFailureLabels {
    pub protocol: String,
    pub reason: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UnsupportedTransportLabels {
    pub protocol: String,
    pub transport: String,
    pub reason: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct H2ConnectionLabels {
    pub upstream_id: String,
}

#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct H2StreamLabels {
    pub upstream_id: String,
    pub outcome: String,
}

pub(crate) const MAX_ROUTE_LABEL_LENGTH: usize = 128;

pub(crate) fn bounded_route_label(value: &str) -> String {
    let mut bounded = String::with_capacity(value.len().min(MAX_ROUTE_LABEL_LENGTH));
    for character in value.chars() {
        let character = if character.is_control() {
            '_'
        } else {
            character
        };
        if bounded.len() + character.len_utf8() > MAX_ROUTE_LABEL_LENGTH - 3 {
            bounded.push_str("...");
            debug_assert!(bounded.len() <= MAX_ROUTE_LABEL_LENGTH);
            return bounded;
        }
        bounded.push(character);
    }
    debug_assert!(bounded.len() <= MAX_ROUTE_LABEL_LENGTH);
    bounded
}
