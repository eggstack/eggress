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
    let rebuilt = eggress_embed::EggressConfig::from_compiled(compiled, source.to_string());
    assert!(!rebuilt.source_toml().is_empty());
    let _compiled = rebuilt.into_compiled();

    let direct = eggress_embed::outbound::OutboundConnector::direct();
    assert_eq!(direct.hop_count(), 0);
    let chain = eggress_uri::parse_proxy_chain("socks5://127.0.0.1:1080").unwrap();
    let chained = eggress_embed::outbound::OutboundConnector::from_chain(chain).unwrap();
    assert_eq!(chained.hop_count(), 1);
}
