#![no_main]
use libfuzzer_sys::fuzz_target;

use eggress_protocol_trojan::tcp::trojan_accept;

fuzz_target!(|data: &[u8]| {
    // Fuzz the trojan_accept server-side parser with arbitrary bytes.
    // Invariant: must never panic. Returns Ok or Err for all inputs.
    // Use a fixed password for deterministic comparison.
    let password = "fuzz-test-password";

    // Use tokio's current_thread runtime to drive the async function.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let stream: eggress_core::BoxStream = Box::new(std::io::Cursor::new(data.to_vec()));
        let _ = trojan_accept(stream, password).await;
    });

    // Also fuzz with empty password.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let stream: eggress_core::BoxStream = Box::new(std::io::Cursor::new(data.to_vec()));
        let _ = trojan_accept(stream, "").await;
    });

    // Also fuzz with very long password.
    if let Ok(long_pw) = std::str::from_utf8(data) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let stream: eggress_core::BoxStream = Box::new(std::io::Cursor::new(data.to_vec()));
            let _ = trojan_accept(stream, long_pw).await;
        });
    }
});
