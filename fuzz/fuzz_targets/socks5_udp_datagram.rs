#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = eggress_protocol_socks::socks5::udp_codec::decode_socks5_udp_datagram(data);
});
