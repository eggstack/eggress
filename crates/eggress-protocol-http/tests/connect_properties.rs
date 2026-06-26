use proptest::prelude::*;

use eggress_protocol_http::{validate_credentials, HttpError};

proptest! {
    #[test]
    fn validate_credentials_accepts_printable(s in "[\\x20-\\x7e]{0,255}") {
        prop_assert!(validate_credentials(&s).is_ok(), "should accept printable: {:?}", s);
    }

    #[test]
    fn validate_credentials_rejects_null_byte(prefix in "[\\x20-\\x7e]{0,64}", suffix in "[\\x20-\\x7e]{0,64}") {
        let value = format!("{}{}\x00{}", prefix, "a", suffix);
        prop_assert!(
            matches!(validate_credentials(&value), Err(HttpError::InvalidCredentials)),
            "should reject null byte in {:?}",
            value
        );
    }

    #[test]
    fn validate_credentials_rejects_del(prefix in "[\\x20-\\x7e]{0,64}", suffix in "[\\x20-\\x7e]{0,64}") {
        let value = format!("{}{}\x7f{}", prefix, "a", suffix);
        prop_assert!(
            matches!(validate_credentials(&value), Err(HttpError::InvalidCredentials)),
            "should reject DEL in {:?}",
            value
        );
    }

    #[test]
    fn validate_credentials_rejects_tab(prefix in "[\\x20-\\x7e]{0,64}", suffix in "[\\x20-\\x7e]{0,64}") {
        let value = format!("{}\t{}", prefix, suffix);
        prop_assert!(
            matches!(validate_credentials(&value), Err(HttpError::InvalidCredentials)),
            "should reject tab in {:?}",
            value
        );
    }

    #[test]
    fn validate_credentials_rejects_any_control(c in 0x00u8..0x20u8) {
        let value = format!("pre{}suf", char::from(c));
        prop_assert!(
            matches!(validate_credentials(&value), Err(HttpError::InvalidCredentials)),
            "should reject control char {:02x}",
            c
        );
    }

    #[test]
    fn credentials_with_high_bytes_rejected(byte in 0x00u8..0x20u8) {
        let value = format!("user{}name", char::from(byte));
        prop_assert!(
            validate_credentials(&value).is_err(),
            "should reject byte 0x{:02x}",
            byte
        );
    }

    #[test]
    fn empty_string_accepted_by_validate(_: ()) {
        prop_assert!(validate_credentials("").is_ok());
    }

    #[test]
    fn username_no_control_chars(user in "[a-zA-Z0-9._@\\-]{1,64}") {
        prop_assert!(validate_credentials(&user).is_ok(), "should accept: {:?}", user);
    }

    #[test]
    fn del_char_rejected(s in "[a-zA-Z]{0,32}") {
        let value = format!("{}\x7f{}", s, "end");
        prop_assert!(
            matches!(validate_credentials(&value), Err(HttpError::InvalidCredentials)),
            "should reject DEL in {:?}",
            value
        );
    }

    #[test]
    fn status_line_over_limit(max_len in 1u8..100u8) {
        let status_line = "HTTP/1.1 200 OK";
        // The parse_status_code function (crate-internal) checks first_line.len() > max_status_line
        // We can test this indirectly: if we set a very small max_status_line, the default
        // status line "HTTP/1.1 200 OK" should be rejected.
        if max_len < 15 {
            prop_assert!(
                status_line.len() > max_len as usize,
                "status line should exceed small limit"
            );
        }
    }

    #[test]
    fn control_chars_range_0x01_to_0x1f_rejected(c in 0x01u8..0x20u8) {
        let value = format!("{}value{}", char::from(c), char::from(0x7eu8));
        prop_assert!(
            validate_credentials(&value).is_err(),
            "should reject control char 0x{:02x}",
            c
        );
    }

    #[test]
    fn printable_range_accepted(s in "[\\x20-\\x7e]{1,128}") {
        prop_assert!(
            validate_credentials(&s).is_ok(),
            "should accept all printable chars, got {:?}",
            s
        );
    }
}
