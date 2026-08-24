#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = eggress_uri::parse_proxy_chain(&text);
    let _ = eggress_pproxy_compat::uri::parse_pproxy_uri(&text);
});
