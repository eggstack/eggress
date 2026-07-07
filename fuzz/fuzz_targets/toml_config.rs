#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the TOML config parser and compiler.
    // Invariants:
    //   - no panic;
    //   - no unbounded allocation;
    //   - parser returns structured error or valid config;
    //   - no infinite loop (bounded by input length).
    //
    // We treat the fuzz input as a TOML string and attempt to:
    //   1. Parse it into a ConfigFile (toml::from_str)
    //   2. Compile it (compile_config)
    // Both steps must not panic.

    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    let config: Result<eggress_config::model::ConfigFile, _> = toml::from_str(text);
    if let Ok(config) = config {
        let _ = eggress_config::compile::compile_config(&config);
    }
});
