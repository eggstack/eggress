use eggress_config::compile::compile_config;
use eggress_config::model::ConfigFile;

#[test]
fn fuzz_smoke_toml_config() {
    let toml_inputs: &[&str] = &[
        "",
        "[[listen]]\nprotocol = \"socks5\"\naddr = \"127.0.0.1:1080\"",
        "[[listen]]\nprotocol = \"http\"\naddr = \"127.0.0.1:8080\"\n\n[[upstream]]\naddr = \"1.2.3.4:443\"",
        "invalid toml {{{{",
        "\u{0000}\u{0001}\u{ffff}",
        "[[listen]]\nprotocol = \"socks5\"\naddr = \":::invalid\"",
    ];
    for input in toml_inputs {
        let config: Result<ConfigFile, _> = toml::from_str(input);
        if let Ok(config) = config {
            let _ = compile_config(&config);
        }
    }
}
