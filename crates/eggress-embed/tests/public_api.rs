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

#[test]
fn supporting_config_runtime_paths_compile() {
    // Maintenance Phase 4: representative downstream-shaped compile contracts
    // for supporting surfaces (compile-time/type-level use only; no I/O).
    // `eggress-config`: canonical TOML validation/compile entry point.
    let minimal = "version = 1\n[[listeners]]\nname = \"api\"\nbind = \"127.0.0.1:0\"\nprotocols = [\"socks5\"]\n";
    let compiled: eggress_config::compile::RuntimeConfig =
        eggress_config::validate_and_compile_toml(minimal).expect("minimal TOML must compile");
    assert_eq!(compiled.listeners.len(), 1);
    // `RuntimeConfig` handoff shape already used by embed consumers.
    let _listeners = &compiled.listeners;

    // `eggress-runtime`: published supervisor/state/shutdown-token signatures.
    let _start = eggress_runtime::ServiceSupervisor::start_from_config
        as fn(
            eggress_config::compile::RuntimeConfig,
            Option<String>,
        ) -> Result<eggress_runtime::ServiceSupervisor, eggress_runtime::RuntimeError>;
    let _compat_start = eggress_runtime::ServiceSupervisor::start_from_config_with_compatibility
        as fn(
            eggress_config::compile::RuntimeConfig,
            Option<String>,
            eggress_runtime::CompatibilityRuntimeHooks,
        ) -> Result<eggress_runtime::ServiceSupervisor, eggress_runtime::RuntimeError>;
    let _reload_result = eggress_runtime::ReloadResult::Applied {
        generation: 0,
        upstreams: 0,
    };
    let _classify = eggress_runtime::classify_reload_config
        as fn(
            &[eggress_config::compile::ListenerConfig],
            &eggress_config::compile::TimeoutConfig,
            Option<&eggress_config::compile::AdminConfig>,
            &eggress_config::compile::RuntimeConfig,
        ) -> Result<(), String>;
}

#[test]
fn protocol_representative_paths_compile() {
    // Maintenance Phase 4: small representative set from documented
    // supported lower-level surfaces (no I/O; owning crates retain semantic
    // tests, server/TLS/SSH/QUIC qualification stays in feature slices).
    // HTTP/SOCKS protocol types via URI parsing.
    let http_chain =
        eggress_uri::parse_proxy_chain("http://127.0.0.1:8080").expect("http chain must parse");
    assert_eq!(http_chain.hops.len(), 1);
    let socks_chain =
        eggress_uri::parse_proxy_chain("socks5://127.0.0.1:1080").expect("socks chain must parse");
    assert_eq!(socks_chain.hops.len(), 1);

    // HTTP handler surface via existing dev-dependency (no I/O).
    let _detector = std::any::type_name::<eggress_protocol_http::HttpDetector>();
    assert!(!_detector.is_empty());
    let _connect_req = std::any::type_name::<eggress_protocol_http::ConnectRequest>();
    assert!(!_connect_req.is_empty());
}
