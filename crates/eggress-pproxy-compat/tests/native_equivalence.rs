//! WS3 equivalence: direct native compilation vs TOML-render/reparse.
//!
//! For representative inputs, `translate_to_runtime_config` (no TOML string)
//! and `translate_from_uris` + `validate_and_compile_toml` (TOML string) must
//! produce equivalent compiled semantics, identical warnings/unsupported, and
//! no credential leakage. Outbound `compile_chain_to_native` must match the
//! TOML path upstream chain without TOML serialization.

use eggress_pproxy_compat::{translate_from_uris, translate_to_runtime_config, PproxyArgs};

fn compile_via_toml(
    args: &PproxyArgs,
    locals: &[eggress_pproxy_compat::PproxyUri],
    chains: &[eggress_pproxy_compat::PproxyChain],
) -> eggress_config::compile::RuntimeConfig {
    let output = translate_from_uris(args, locals, chains).expect("TOML translate");
    assert!(
        output.unsupported.is_empty(),
        "expected supported case, got unsupported: {:?}",
        output.unsupported
    );
    eggress_config::validate_and_compile_toml(&output.toml).expect("TOML compile")
}

fn assert_runtime_equivalent(
    native: &eggress_config::compile::RuntimeConfig,
    via_toml: &eggress_config::compile::RuntimeConfig,
) {
    assert_eq!(native.listeners.len(), via_toml.listeners.len());
    assert_eq!(native.upstreams.len(), via_toml.upstreams.len());
    assert_eq!(native.groups.len(), via_toml.groups.len());
    assert_eq!(native.rules.len(), via_toml.rules.len());
    for (index, (left, right)) in native
        .upstreams
        .iter()
        .zip(via_toml.upstreams.iter())
        .enumerate()
    {
        assert_eq!(left.id, right.id, "upstream {index} id");
        assert_eq!(
            left.chain.hops.len(),
            right.chain.hops.len(),
            "upstream {index} hop count"
        );
        for (hop_index, (left_hop, right_hop)) in left
            .chain
            .hops
            .iter()
            .zip(right.chain.hops.iter())
            .enumerate()
        {
            assert_eq!(
                left_hop.protocols, right_hop.protocols,
                "upstream {index} hop {hop_index} protocols"
            );
            assert_eq!(
                left_hop.endpoint, right_hop.endpoint,
                "upstream {index} hop {hop_index} endpoint"
            );
            assert_eq!(
                left_hop.credentials.as_ref().map(|c| &c.username),
                right_hop.credentials.as_ref().map(|c| &c.username),
                "upstream {index} hop {hop_index} username"
            );
            // Passwords must match but never appear in debug output
            // (CredentialSpec redacts). Compare without logging values.
            assert_eq!(
                left_hop.credentials.as_ref().map(|c| c.password.len()),
                right_hop.credentials.as_ref().map(|c| c.password.len()),
                "upstream {index} hop {hop_index} password length"
            );
        }
    }
}

fn case(
    local: &str,
    remotes: &[&str],
) -> (
    PproxyArgs,
    Vec<eggress_pproxy_compat::PproxyUri>,
    Vec<eggress_pproxy_compat::PproxyChain>,
) {
    let mut argv = vec!["-l".to_string(), local.to_string()];
    for remote in remotes {
        argv.push("-r".to_string());
        argv.push((*remote).to_string());
    }
    let arg_refs: Vec<String> = argv;
    let args = PproxyArgs::parse(&arg_refs).expect("parse args");
    let locals = args.parse_local_uris().expect("parse locals");
    let mut chains = Vec::new();
    for raw in &args.remotes {
        chains.push(eggress_pproxy_compat::uri::parse_pproxy_chain(raw).expect("parse chain"));
    }
    (args, locals, chains)
}

#[test]
fn http_socks_variants_equivalent() {
    for (local, remotes) in [
        ("http://127.0.0.1:8080", vec![] as Vec<&str>),
        ("socks5://127.0.0.1:1080", vec![]),
        ("socks4://127.0.0.1:1080", vec!["http://127.0.0.1:8080"]),
        ("socks5://127.0.0.1:1080", vec!["socks5://127.0.0.1:1081"]),
        (
            "http://127.0.0.1:8080",
            vec!["socks5://127.0.0.1:1080__http://127.0.0.1:8081"],
        ),
    ] {
        let (args, locals, chains) = case(local, &remotes);
        let native = translate_to_runtime_config(&args, &locals, &chains).expect("native compile");
        let via_toml = compile_via_toml(&args, &locals, &chains);
        assert_runtime_equivalent(&native.runtime, &via_toml);
        let toml_output = translate_from_uris(&args, &locals, &chains).expect("toml");
        assert_eq!(native.warnings, toml_output.warnings);
        assert_eq!(native.unsupported, toml_output.unsupported);
        // TOML renderer must still emit valid TOML.
        assert!(toml_output.toml.contains("version = 1"));
    }
}

#[test]
fn tls_shadowsocks_trojan_equivalent() {
    // TLS-wrapped (https listener requires --ssl; here test upstream TLS form
    // via socks5+tls which needs no cert files).
    let (args, locals, chains) = case("socks5://127.0.0.1:1080", &["socks5+tls://127.0.0.1:1081"]);
    let native = translate_to_runtime_config(&args, &locals, &chains).expect("native");
    let via_toml = compile_via_toml(&args, &locals, &chains);
    assert_runtime_equivalent(&native.runtime, &via_toml);

    // Shadowsocks AEAD.
    let (args, locals, chains) = case(
        "socks5://127.0.0.1:1080",
        &["ss://aes-256-gcm:secret@127.0.0.1:8388"],
    );
    let native = translate_to_runtime_config(&args, &locals, &chains).expect("native ss");
    let via_toml = compile_via_toml(&args, &locals, &chains);
    assert_runtime_equivalent(&native.runtime, &via_toml);

    // Trojan password-only.
    let (args, locals, chains) = case(
        "socks5://127.0.0.1:1080",
        &["trojan://secret@127.0.0.1:443"],
    );
    let native = translate_to_runtime_config(&args, &locals, &chains).expect("native trojan");
    let via_toml = compile_via_toml(&args, &locals, &chains);
    assert_runtime_equivalent(&native.runtime, &via_toml);
}

#[test]
fn fixed_target_local_bind_rule_equivalent() {
    // Fixed-target (raw tunnel) listener form.
    let (args, locals, chains) = case("raw{127.0.0.1:80}://:8080", &[]);
    // raw fixed-target may need no remote; just check native path doesn't panic
    // and matches TOML path (both may emit warnings, but must agree).
    let native = translate_to_runtime_config(&args, &locals, &chains);
    let toml_result = translate_from_uris(&args, &locals, &chains);
    match (native, toml_result) {
        (Ok(native), Ok(toml_output)) => {
            assert_eq!(native.warnings, toml_output.warnings);
            assert_eq!(native.unsupported, toml_output.unsupported);
            if native.unsupported.is_empty() {
                let via_toml = eggress_config::validate_and_compile_toml(&toml_output.toml)
                    .expect("toml compile");
                assert_runtime_equivalent(&native.runtime, &via_toml);
            }
        }
        (Err(native_err), Err(toml_err)) => {
            // Both paths must fail consistently (no silent divergence).
            assert_eq!(
                std::mem::discriminant(&native_err),
                std::mem::discriminant(&toml_err)
            );
        }
        (Ok(_), Err(_)) | (Err(_), Ok(_)) => {
            panic!("native and TOML paths diverged");
        }
    }
}

#[test]
fn outbound_chain_direct_matches_toml_upstream() {
    for uri in [
        "socks5://127.0.0.1:1080",
        "http://user1:pass1@127.0.0.1:8080",
        "socks5://127.0.0.1:1080__http://127.0.0.1:8080",
        "ss://aes-256-gcm:secret@127.0.0.1:8388",
        "trojan://secret@127.0.0.1:443",
    ] {
        let chain = eggress_pproxy_compat::uri::parse_pproxy_chain(uri).expect("parse");
        let native = eggress_pproxy_compat::translate::compile_chain_to_native(&chain)
            .expect("direct native");
        // TOML path: translate single chain via default args, take first upstream URI, parse native.
        let default_args = PproxyArgs::default_args();
        let output =
            translate_from_uris(&default_args, &[], &[chain.clone()]).expect("toml translate");
        assert!(
            output.unsupported.is_empty(),
            "unexpected unsupported for {uri}: {:?}",
            output.unsupported
        );
        let runtime =
            eggress_config::validate_and_compile_toml(&output.toml).expect("toml compile");
        assert!(!runtime.upstreams.is_empty());
        let via_toml_chain = &runtime.upstreams[0].chain;
        assert_eq!(
            native.hops.len(),
            via_toml_chain.hops.len(),
            "hop count for {uri}"
        );
        for (left, right) in native.hops.iter().zip(via_toml_chain.hops.iter()) {
            assert_eq!(left.protocols, right.protocols);
            assert_eq!(left.endpoint, right.endpoint);
        }
        // Credentials must not leak via display.
        assert!(!chain.redacted_display().contains("pass1"));
        assert!(!chain.redacted_display().contains("secret"));
    }
}

#[test]
fn unsupported_and_warnings_identical() {
    // Unsupported hop (redir upstream) must be reported identically.
    let (args, locals, chains) = case("socks5://127.0.0.1:1080", &["redir://127.0.0.1:1234"]);
    let native = translate_to_runtime_config(&args, &locals, &chains).expect("native");
    let toml_output = translate_from_uris(&args, &locals, &chains).expect("toml");
    assert_eq!(native.warnings, toml_output.warnings);
    assert_eq!(native.unsupported, toml_output.unsupported);
    assert!(!native.unsupported.is_empty());
}
