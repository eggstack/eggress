//! Upstream chain/group/health compilation.

use std::sync::Arc;

use eggress_routing::scheduler::SchedulerKind;
use eggress_routing::UpstreamGroupId;

use crate::error::ConfigError;
use crate::model::{ConfigFile, HealthConfigToml};
use crate::validate::validate_duration;

use super::model::*;

pub(crate) fn compile_health_config(
    health: Option<&HealthConfigToml>,
) -> Result<eggress_routing::health::HealthConfig, ConfigError> {
    let defaults = eggress_routing::health::HealthConfig::default();
    let Some(h) = health else {
        return Ok(defaults);
    };

    let interval = h
        .interval
        .as_deref()
        .map(validate_duration)
        .transpose()?
        .unwrap_or(defaults.interval);

    let timeout = h
        .timeout
        .as_deref()
        .map(validate_duration)
        .transpose()?
        .unwrap_or(defaults.timeout);

    let failures_to_unhealthy = h
        .failures_to_unhealthy
        .unwrap_or(defaults.failures_to_unhealthy);
    if failures_to_unhealthy == 0 {
        return Err(ConfigError::validation(
            "health.failures_to_unhealthy",
            "must be greater than 0",
        ));
    }

    let successes_to_healthy = h
        .successes_to_healthy
        .unwrap_or(defaults.successes_to_healthy);
    if successes_to_healthy == 0 {
        return Err(ConfigError::validation(
            "health.successes_to_healthy",
            "must be greater than 0",
        ));
    }

    let initial_state = match h.initial_state.as_deref() {
        Some("unknown") | None => defaults.initial_state,
        Some("healthy") => eggress_routing::health::HealthState::Healthy,
        Some("unhealthy") => eggress_routing::health::HealthState::Unhealthy,
        Some("disabled") => eggress_routing::health::HealthState::Disabled,
        Some(other) => {
            return Err(ConfigError::validation(
                "health.initial_state",
                &format!(
                    "unknown state '{}', must be one of: unknown, healthy, unhealthy, disabled",
                    other
                ),
            ));
        }
    };

    Ok(eggress_routing::health::HealthConfig {
        interval,
        timeout,
        failures_to_unhealthy,
        successes_to_healthy,
        initial_state,
    })
}

pub(crate) fn compile_h2_config(
    model: &crate::model::H2UpstreamConfig,
    path_prefix: &str,
) -> Result<CompiledH2Config, ConfigError> {
    let mut compiled = CompiledH2Config::default();

    if let Some(max) = model.max_concurrent_streams {
        if max == 0 {
            return Err(ConfigError::validation(
                &format!("{}.max_concurrent_streams", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.max_concurrent_streams = max;
    }

    if let Some(pool) = model.pool_size {
        if pool == 0 {
            return Err(ConfigError::validation(
                &format!("{}.pool_size", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.pool_size = pool;
    }

    if let Some(ref idle) = model.idle_timeout {
        compiled.idle_timeout = validate_duration(idle).map_err(|e| {
            ConfigError::validation(&format!("{}.idle_timeout", path_prefix), &e.to_string())
        })?;
    }

    if let Some(ref interval) = model.keepalive_interval {
        compiled.keepalive_interval = validate_duration(interval).map_err(|e| {
            ConfigError::validation(
                &format!("{}.keepalive_interval", path_prefix),
                &e.to_string(),
            )
        })?;
    }

    if let Some(ref timeout) = model.keepalive_timeout {
        compiled.keepalive_timeout = validate_duration(timeout).map_err(|e| {
            ConfigError::validation(
                &format!("{}.keepalive_timeout", path_prefix),
                &e.to_string(),
            )
        })?;
    }

    if let Some(window) = model.stream_receive_window {
        if window == 0 {
            return Err(ConfigError::validation(
                &format!("{}.stream_receive_window", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.stream_receive_window = window;
    }

    if let Some(window) = model.connection_receive_window {
        if window == 0 {
            return Err(ConfigError::validation(
                &format!("{}.connection_receive_window", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.connection_receive_window = window;
    }

    if let Some(size) = model.max_frame_size {
        if size == 0 {
            return Err(ConfigError::validation(
                &format!("{}.max_frame_size", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.max_frame_size = size;
    }

    if let Some(size) = model.max_header_list_size {
        if size == 0 {
            return Err(ConfigError::validation(
                &format!("{}.max_header_list_size", path_prefix),
                "must be greater than 0",
            ));
        }
        compiled.max_header_list_size = size;
    }

    Ok(compiled)
}

pub(crate) fn compile_upstreams(config: &ConfigFile) -> Result<Vec<UpstreamConfig>, ConfigError> {
    let upstreams = match &config.upstreams {
        Some(u) => u,
        None => return Ok(vec![]),
    };

    upstreams
        .iter()
        .map(|u| {
            eggress_routing::upstream::validate_upstream_id(&u.id)
                .map_err(|e| ConfigError::validation(&format!("upstream {}", u.id), &e))?;

            let chain = eggress_uri::parse_proxy_chain(&u.uri).map_err(|_e| {
                ConfigError::validation(&format!("upstream {}", u.id), "invalid upstream URI")
            })?;

            for (idx, hop) in chain.hops.iter().enumerate() {
                if hop.insecure
                    && (hop.protocols.contains(&eggress_uri::ProtocolSpec::Quic)
                        || hop.protocols.contains(&eggress_uri::ProtocolSpec::Http3))
                {
                    return Err(ConfigError::validation(
                        &format!("upstream {} hop {}", u.id, idx),
                        "insecure QUIC/H3 requires the insecure-quic feature; rebuild with --features insecure-quic",
                    ));
                }
            }

            warn_if_insecure_hops(u.id.as_str(), &chain);

            let health = compile_health_config(u.health.as_ref()).map_err(|e| match e {
                ConfigError::Validation { path, message } => {
                    ConfigError::validation(&format!("upstream {}.{}", u.id, path), &message)
                }
                other => other,
            })?;

            let h2 = match u.h2.as_ref() {
                Some(h2_model) => Some(
                    compile_h2_config(h2_model, &format!("upstream {}", u.id)).map_err(
                        |e| match e {
                            ConfigError::Validation { path, message } => {
                                ConfigError::validation(&path, &message)
                            }
                            other => other,
                        },
                    )?,
                ),
                None => None,
            };

            Ok(UpstreamConfig {
                id: u.id.clone(),
                chain,
                health,
                h2,
            })
        })
        .collect()
}

pub(crate) fn compile_groups(config: &ConfigFile) -> Result<Vec<UpstreamGroupConfig>, ConfigError> {
    let groups = match &config.upstream_groups {
        Some(g) => g,
        None => return Ok(vec![]),
    };

    groups
        .iter()
        .map(|g| {
            let scheduler = match g.scheduler.as_deref() {
                Some("round-robin") | None => SchedulerKind::RoundRobin,
                Some("first-available") => SchedulerKind::FirstAvailable,
                Some("random") => SchedulerKind::Random,
                Some("least-connections") => SchedulerKind::LeastConnections,
                Some(other) => {
                    return Err(ConfigError::validation(
                        &format!("group {}", g.id),
                        &format!("unknown scheduler: {}", other),
                    ))
                }
            };

            let fallback = match g.fallback.as_deref() {
                Some("reject") | None => GroupFallback::Reject,
                Some("direct") => GroupFallback::Direct,
                Some("use-unhealthy") => GroupFallback::UseUnhealthy,
                Some(other) => {
                    return Err(ConfigError::validation(
                        &format!("group {}", g.id),
                        &format!("unknown fallback: {}", other),
                    ))
                }
            };

            Ok(UpstreamGroupConfig {
                id: UpstreamGroupId(Arc::from(g.id.as_str())),
                scheduler,
                members: g.members.clone(),
                fallback,
            })
        })
        .collect()
}

/// Emit one compile-time security notice per upstream that explicitly selects
/// a certificate-verification bypass through `?insecure`.
///
/// The notice names the upstream id plus the affected hop indexes and
/// protocols only. Hop endpoints, credentials, and every other secret-bearing
/// part of the source URI are deliberately omitted. It is emitted while
/// configuration is compiled, so it fires once per load/reload — before any
/// chain is executed — rather than once per connection or packet.
fn warn_if_insecure_hops(upstream_id: &str, chain: &eggress_uri::ProxyChainSpec) {
    let Some(summary) = insecure_hop_summary(chain) else {
        return;
    };
    tracing::warn!(
        upstream = %upstream_id,
        insecure_hops = %summary,
        policy = "insecure-tls",
        "upstream explicitly disables TLS certificate verification (?insecure); \
         peer certificates for these hops are not validated"
    );
}

/// Describe the hops of `chain` that set `?insecure`, as
/// `hop <index> (<protocol>[+<protocol>])` entries joined by `, `.
///
/// Returns `None` when every hop verifies certificates. Kept separate from the
/// logging call so the redacted shape can be asserted directly in tests.
fn insecure_hop_summary(chain: &eggress_uri::ProxyChainSpec) -> Option<String> {
    let summary: Vec<String> = chain
        .hops
        .iter()
        .enumerate()
        .filter(|(_, hop)| hop.insecure)
        .map(|(idx, hop)| {
            let protocols: Vec<&str> = hop
                .protocols
                .iter()
                .map(|protocol| protocol.canonical_name())
                .collect();
            format!("hop {idx} ({})", protocols.join("+"))
        })
        .collect();
    if summary.is_empty() {
        None
    } else {
        Some(summary.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use super::*;

    fn parse_chain(uri: &str) -> eggress_uri::ProxyChainSpec {
        eggress_uri::parse_proxy_chain(uri).expect("test URI must parse")
    }

    #[test]
    fn insecure_hop_summary_is_none_for_verified_chain() {
        let chain = parse_chain("socks5://hop1:1080__http://hop2:8080");
        assert!(insecure_hop_summary(&chain).is_none());
    }

    #[test]
    fn insecure_hop_summary_names_only_hop_index_and_protocols() {
        let chain =
            parse_chain("socks5://user:topsecret@insecure.example:1080?insecure__http://hop2:8080");
        let summary = insecure_hop_summary(&chain).expect("hop 0 selects ?insecure");
        assert_eq!(summary, "hop 0 (socks5)");
        assert!(
            !summary.contains("topsecret"),
            "credentials leaked: {summary}"
        );
        assert!(
            !summary.contains("insecure.example"),
            "host leaked: {summary}"
        );
        assert!(!summary.contains("1080"), "port leaked: {summary}");
    }

    #[test]
    fn insecure_hop_summary_lists_every_insecure_hop() {
        let chain = parse_chain("socks5://a:1?insecure__http://b:2?insecure");
        let summary = insecure_hop_summary(&chain).expect("both hops select ?insecure");
        assert_eq!(summary, "hop 0 (socks5), hop 1 (http)");
    }

    /// Minimal subscriber that records every event's fields as
    /// `name=value` pairs, so a compile-time warning can be asserted without
    /// adding a logging test dependency.
    #[derive(Default)]
    struct CaptureSubscriber {
        events: Arc<Mutex<Vec<String>>>,
    }

    struct FieldCollector(Vec<String>);

    impl Visit for FieldCollector {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.push(format!("{}={value:?}", field.name()));
        }
    }

    impl Subscriber for CaptureSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut collector = FieldCollector(Vec::new());
            event.record(&mut collector);
            self.events.lock().unwrap().push(collector.0.join(" "));
        }

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }

    fn compile_and_capture(config_toml: &str) -> Vec<String> {
        let config: crate::model::ConfigFile =
            toml::from_str(config_toml).expect("test config must parse");

        let subscriber = CaptureSubscriber::default();
        let events = Arc::clone(&subscriber.events);
        tracing::subscriber::with_default(subscriber, || {
            compile_upstreams(&config).expect("upstreams must compile");
        });
        let captured = events.lock().unwrap().clone();
        captured
    }

    #[test]
    fn compile_warns_once_for_the_insecure_upstream_only() {
        let events = compile_and_capture(
            r#"
version = 1

[[upstreams]]
id = "verified"
uri = "socks5://verify.example:1080"

[[upstreams]]
id = "bypassed"
uri = "socks5://user:topsecret@insecure.example:1080?insecure"
"#,
        );

        assert_eq!(
            events.len(),
            1,
            "exactly one notice expected, got {events:?}"
        );
        let notice = &events[0];
        assert!(notice.contains("upstream=bypassed"), "{notice}");
        assert!(notice.contains("insecure_hops=hop 0 (socks5)"), "{notice}");
        assert!(notice.contains("insecure-tls"), "{notice}");
        assert!(
            !notice.contains("topsecret"),
            "credentials leaked: {notice}"
        );
        assert!(
            !notice.contains("insecure.example"),
            "host leaked: {notice}"
        );
    }

    #[test]
    fn compile_does_not_warn_when_every_hop_verifies() {
        let events = compile_and_capture(
            r#"
version = 1

[[upstreams]]
id = "verified"
uri = "socks5+tls://verify.example:1080"
"#,
        );

        assert!(events.is_empty(), "unexpected notice: {events:?}");
    }
}
