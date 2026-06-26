#![no_main]
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use libfuzzer_sys::fuzz_target;

use eggress_core::{TargetAddr, TargetHost};
use eggress_protocol_trojan::hash::password_hash;
use eggress_protocol_trojan::tcp::encode_trojan_request;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Fuzz the password hash function directly. Invariant: 56 lowercase hex
    // chars regardless of input. The hash itself must never panic and must
    // not produce anything outside [0-9a-f].
    if let Ok(password) = std::str::from_utf8(data) {
        let h = password_hash(password);
        assert_eq!(h.len(), 56);
        for c in h.chars() {
            assert!(c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
        }
    }

    // Build a TargetAddr from the fuzz bytes and encode a Trojan request.
    // Invariant: encoding must not panic. The output for valid targets is
    // 58 bytes (v4), 70 bytes (v6), or 60-314 bytes (domain). For invalid
    // targets (empty/oversized domain) the function must return Err.
    let target = build_target_from_bytes(data);
    let password = "test-password";
    let _ = encode_trojan_request(&target, password);

    // Also fuzz with empty / extreme passwords.
    let _ = encode_trojan_request(&target, "");
    let _ = encode_trojan_request(&target, "x");
    if let Ok(long_pw) = std::str::from_utf8(data) {
        let _ = encode_trojan_request(&target, long_pw);
    }
});

fn build_target_from_bytes(data: &[u8]) -> TargetAddr {
    if data.is_empty() {
        return TargetAddr {
            host: TargetHost::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            port: 80,
        };
    }
    let port = u16::from_be_bytes([data[0], data[0]]);
    match data[0] % 5 {
        0 => TargetAddr {
            host: TargetHost::Ip(IpAddr::V4(Ipv4Addr::new(
                data.get(1).copied().unwrap_or(127),
                data.get(2).copied().unwrap_or(0),
                data.get(3).copied().unwrap_or(0),
                data.get(4).copied().unwrap_or(1),
            ))),
            port,
        },
        1 => {
            let mut octets = [0u8; 16];
            for (i, slot) in octets.iter_mut().enumerate() {
                *slot = data.get(i + 1).copied().unwrap_or(0);
            }
            TargetAddr {
                host: TargetHost::Ip(IpAddr::V6(Ipv6Addr::from(octets))),
                port,
            }
        }
        2 => TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port,
        },
        3 => TargetAddr {
            host: TargetHost::Domain(String::new()),
            port,
        },
        _ => {
            // Domain from the fuzz input. May be any length 0..=255.
            if let Ok(s) = std::str::from_utf8(&data[1..]) {
                let truncated: String = s.chars().take(300).collect();
                TargetAddr {
                    host: TargetHost::Domain(truncated),
                    port,
                }
            } else {
                TargetAddr {
                    host: TargetHost::Domain("x".into()),
                    port,
                }
            }
        }
    }
}
