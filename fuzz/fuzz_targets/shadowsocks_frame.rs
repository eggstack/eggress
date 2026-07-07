#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Exercise the Shadowsocks address parser over arbitrary bytes.
    // Invariants:
    //   - no panic;
    //   - no unbounded allocation;
    //   - parser returns structured error or valid decoded value;
    //   - no infinite loop (bounded by input length).
    let _ = eggress_protocol_shadowsocks::address::decode_address(data);
});
