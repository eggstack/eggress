use eggress_core::{TargetAddr, TargetHost};
use eggress_protocol_trojan::hash::password_hash;
use eggress_protocol_trojan::tcp::{encode_trojan_request, trojan_accept};

#[test]
fn fuzz_smoke_trojan_request_encode() {
    let passwords: &[&str] = &[
        "",
        "test",
        "very-long-password-that-exceeds-normal-length-expectations",
    ];
    let targets: Vec<TargetAddr> = vec![
        TargetAddr {
            host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
            port: 80,
        },
        TargetAddr {
            host: TargetHost::Ip("::1".parse().unwrap()),
            port: 443,
        },
        TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 8080,
        },
        TargetAddr {
            host: TargetHost::Domain(String::new()),
            port: 80,
        },
        TargetAddr {
            host: TargetHost::Domain("a".repeat(255)),
            port: 65535,
        },
    ];
    for target in &targets {
        for pw in passwords {
            let _ = encode_trojan_request(target, pw);
        }
    }
}

#[test]
fn fuzz_smoke_trojan_password_hash() {
    let inputs: &[&str] = &[
        "",
        "a",
        "test-password",
        "\u{0000}\u{0001}\u{ffff}",
        &"x".repeat(10000),
    ];
    for input in inputs {
        let h = password_hash(input);
        assert_eq!(h.len(), 56);
        for c in h.chars() {
            assert!(c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
        }
    }
}

#[test]
fn fuzz_smoke_trojan_accept() {
    let password = "fuzz-test-password";
    let payloads: Vec<Vec<u8>> = vec![
        vec![],
        vec![0x00],
        vec![0x54, 0x52, 0x4f, 0x4a, 0x41, 0x4e], // "TROJAN" prefix
        "TROJAN\r\n".as_bytes().to_vec(),
    ];
    for payload in &payloads {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let stream: eggress_core::BoxStream = Box::new(std::io::Cursor::new(payload.clone()));
            let _ = trojan_accept(stream, password).await;
        });
    }
}
