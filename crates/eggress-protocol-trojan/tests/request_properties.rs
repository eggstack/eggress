use proptest::prelude::*;

use eggress_protocol_trojan::hash::password_hash;

fn arb_password() -> impl Strategy<Value = String> {
    "[\\x20-\\x7e]{0,256}".prop_map(|s| s)
}

fn arb_password_pair() -> impl Strategy<Value = (String, String)> {
    (
        "[a-z]{1,128}".prop_map(|s| s),
        "[a-z]{1,128}".prop_map(|s| s),
    )
}

proptest! {
    #[test]
    fn password_hash_always_56_hex_chars(password in arb_password()) {
        let hash = password_hash(&password);
        prop_assert_eq!(hash.len(), 56);
    }
}

proptest! {
    #[test]
    fn password_hash_lowercase_hex(password in arb_password()) {
        let hash = password_hash(&password);
        prop_assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "hash must be lowercase hex, got: {:?}",
            hash
        );
    }
}

proptest! {
    #[test]
    fn password_hash_deterministic(password in arb_password()) {
        let h1 = password_hash(&password);
        let h2 = password_hash(&password);
        prop_assert_eq!(h1, h2);
    }
}

proptest! {
    #[test]
    fn password_hash_different_inputs_different_hashes(
        (a, b) in arb_password_pair().prop_filter("distinct", |(a, b)| a != b)
    ) {
        let h1 = password_hash(&a);
        let h2 = password_hash(&b);
        prop_assert_ne!(h1, h2);
    }
}

proptest! {
    #[test]
    fn password_hash_hex_only_alphanumeric(password in arb_password()) {
        let hash = password_hash(&password);
        prop_assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be hex-only, got: {:?}",
            hash
        );
    }
}

proptest! {
    #[test]
    fn password_hash_even_length(password in arb_password()) {
        let hash = password_hash(&password);
        prop_assert_eq!(hash.len() % 2, 0);
    }
}

#[test]
fn password_hash_empty_string() {
    let hash = password_hash("");
    assert_eq!(
        hash,
        "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f"
    );
}

#[test]
fn password_hash_known_password() {
    let hash = password_hash("password");
    assert_eq!(
        hash,
        "d63dc919e201d7bc4c825630d2cf25fdc93d4b2f0d46706d29038d01"
    );
}
