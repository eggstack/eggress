//! Representative source-compatibility contracts for the published embed API.

#[test]
fn embed_config_handoff_and_outbound_facade_paths_compile() {
    let source = r#"
version = 1
[[listeners]]
name = "api"
bind = "127.0.0.1:0"
protocols = ["socks5"]
"#;
    let config = eggress_embed::EggressConfig::from_toml_str(source).unwrap();
    let compiled = config.compiled().clone();
    // Representative `eggress-config::RuntimeConfig` path through the
    // maintained embed handoff.
    let compiled: eggress_config::compile::RuntimeConfig = compiled;
    assert!(compiled.listeners.len() == 1);
    let rebuilt = eggress_embed::EggressConfig::from_compiled(compiled, source.to_string());
    assert!(!rebuilt.source_toml().is_empty());
    let _compiled = rebuilt.into_compiled();

    let direct = eggress_embed::outbound::OutboundConnector::direct();
    assert_eq!(direct.hop_count(), 0);
    let chain = eggress_uri::parse_proxy_chain("socks5://127.0.0.1:1080").unwrap();
    let chained = eggress_embed::outbound::OutboundConnector::from_chain(chain).unwrap();
    assert_eq!(chained.hop_count(), 1);
}

#[test]
fn outbound_authority_and_compat_reexport_paths_compile() {
    // Single implementation authority.
    let direct = eggress_outbound::OutboundConnector::direct();
    assert_eq!(direct.upstream_count(), 0);
    assert_eq!(direct.hop_count(), 0);

    let chain = eggress_uri::parse_proxy_chain("socks5://127.0.0.1:1080").unwrap();
    let native = eggress_outbound::OutboundConnector::from_chain(chain).unwrap();
    assert_eq!(native.upstream_count(), 1);
    assert_eq!(native.hop_count(), 1);

    // Full-service facade re-export stays source-compatible.
    let facade = eggress_embed::outbound::OutboundConnector::direct();
    assert_eq!(facade.upstream_count(), 0);
    assert_eq!(facade.hop_count(), 0);
}

#[test]
fn relay_routing_core_representative_paths_compile() {
    // `eggress-relay::{relay, RelayOptions, RelayReport}` type paths.
    let options = eggress_relay::RelayOptions::default();
    assert!(options.buffer_size.get() > 0);
    let report = eggress_relay::RelayReport {
        bytes_upstream: 0,
        bytes_downstream: 0,
        termination: eggress_relay::RelayTermination::ClientClosed,
    };
    assert_eq!(report.bytes_upstream, 0);
    // Proves the maintained async entry point resolves without executing I/O.
    let _relay = eggress_relay::relay::<tokio::io::DuplexStream, tokio::io::DuplexStream>;

    // Representative `eggress-routing` construction already documented as supported.
    let router = eggress_routing::Router::new(vec![], eggress_routing::RouteActionSpec::Direct);
    let _ = router;

    // Representative `eggress-core` types already documented as supported.
    let id = eggress_core::UpstreamId::new("api");
    assert_eq!(id.to_string(), "api");
    let target = eggress_core::TargetAddr {
        host: eggress_core::TargetHost::Domain("example.com".to_string()),
        port: 443,
    };
    assert_eq!(target.port, 443);
}
