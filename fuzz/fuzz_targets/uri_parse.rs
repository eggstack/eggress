#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = eggress_uri::parse_proxy_chain(data);
});
