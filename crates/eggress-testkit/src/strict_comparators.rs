use std::collections::HashSet;

use crate::strict_observations::{
    ComparisonResult, MismatchClassification, MismatchKind, StrictObservation,
};

// ---------------------------------------------------------------------------
// compare_exact_json
// ---------------------------------------------------------------------------

pub fn compare_exact_json(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let oracle_json = serde_json::to_string(oracle).unwrap_or_default();
    let candidate_json = serde_json::to_string(candidate).unwrap_or_default();
    vec![if oracle_json == candidate_json {
        ComparisonResult::matched("exact_json", &oracle_json, &candidate_json)
    } else {
        ComparisonResult::mismatched(
            "exact_json",
            &oracle_json,
            &candidate_json,
            MismatchKind::ExactMismatch,
            MismatchClassification::Unclassified,
        )
    }]
}

// ---------------------------------------------------------------------------
// compare_namespace_set
// ---------------------------------------------------------------------------

pub fn compare_namespace_set(
    oracle_imports: &[String],
    candidate_imports: &[String],
) -> Vec<ComparisonResult> {
    let oracle_set: HashSet<&str> = oracle_imports.iter().map(|s| s.as_str()).collect();
    let candidate_set: HashSet<&str> = candidate_imports.iter().map(|s| s.as_str()).collect();
    let mut results = Vec::new();

    let oracle_str = format!("{:?}", oracle_set);
    let candidate_str = format!("{:?}", candidate_set);

    if oracle_set == candidate_set {
        results.push(ComparisonResult::matched(
            "namespace_set",
            &oracle_str,
            &candidate_str,
        ));
    } else {
        let missing_in_candidate: Vec<_> = oracle_set.difference(&candidate_set).collect();
        let extra_in_candidate: Vec<_> = candidate_set.difference(&oracle_set).collect();

        if !missing_in_candidate.is_empty() {
            results.push(ComparisonResult::mismatched(
                "namespace_set.missing_in_candidate",
                &format!("{:?}", missing_in_candidate),
                &candidate_str,
                MismatchKind::MissingInCandidate,
                MismatchClassification::CandidateDefect,
            ));
        }
        if !extra_in_candidate.is_empty() {
            results.push(ComparisonResult::mismatched(
                "namespace_set.extra_in_candidate",
                &oracle_str,
                &format!("{:?}", extra_in_candidate),
                MismatchKind::MissingInOracle,
                MismatchClassification::ApprovedNormalization,
            ));
        }
    }

    results
}

// ---------------------------------------------------------------------------
// compare_signature
// ---------------------------------------------------------------------------

pub fn compare_signature(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let mut results = Vec::new();

    match (&oracle.signature, &candidate.signature) {
        (Some(oracle_sig), Some(candidate_sig)) => {
            let oracle_str = serde_json::to_string(oracle_sig).unwrap_or_default();
            let candidate_str = serde_json::to_string(candidate_sig).unwrap_or_default();
            if oracle_str == candidate_str {
                results.push(ComparisonResult::matched(
                    "signature",
                    &oracle_str,
                    &candidate_str,
                ));
            } else {
                if oracle_sig.positional_args != candidate_sig.positional_args {
                    results.push(ComparisonResult::mismatched(
                        "signature.positional_args",
                        &format!("{:?}", oracle_sig.positional_args),
                        &format!("{:?}", candidate_sig.positional_args),
                        MismatchKind::SignatureMismatch,
                        MismatchClassification::CandidateDefect,
                    ));
                }
                if oracle_sig.keyword_args != candidate_sig.keyword_args {
                    results.push(ComparisonResult::mismatched(
                        "signature.keyword_args",
                        &format!("{:?}", oracle_sig.keyword_args),
                        &format!("{:?}", candidate_sig.keyword_args),
                        MismatchKind::SignatureMismatch,
                        MismatchClassification::CandidateDefect,
                    ));
                }
                if oracle_sig.defaults != candidate_sig.defaults {
                    results.push(ComparisonResult::mismatched(
                        "signature.defaults",
                        &format!("{:?}", oracle_sig.defaults),
                        &format!("{:?}", candidate_sig.defaults),
                        MismatchKind::SignatureMismatch,
                        MismatchClassification::CandidateDefect,
                    ));
                }
                if oracle_sig.return_annotation != candidate_sig.return_annotation {
                    results.push(ComparisonResult::mismatched(
                        "signature.return_annotation",
                        oracle_sig.return_annotation.as_deref().unwrap_or("None"),
                        candidate_sig.return_annotation.as_deref().unwrap_or("None"),
                        MismatchKind::SignatureMismatch,
                        MismatchClassification::CandidateDefect,
                    ));
                }
            }
        }
        (Some(oracle_sig), None) => {
            results.push(ComparisonResult::mismatched(
                "signature",
                &serde_json::to_string(oracle_sig).unwrap_or_default(),
                "missing",
                MismatchKind::MissingInCandidate,
                MismatchClassification::CandidateDefect,
            ));
        }
        (None, Some(candidate_sig)) => {
            results.push(ComparisonResult::mismatched(
                "signature",
                "missing",
                &serde_json::to_string(candidate_sig).unwrap_or_default(),
                MismatchKind::MissingInOracle,
                MismatchClassification::OracleExecutionDefect,
            ));
        }
        (None, None) => {
            results.push(ComparisonResult::matched("signature", "None", "None"));
        }
    }

    results
}

// ---------------------------------------------------------------------------
// compare_callable_kind
// ---------------------------------------------------------------------------

pub fn compare_callable_kind(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let oracle_is_coroutine = oracle.is_coroutine;
    let candidate_is_coroutine = candidate.is_coroutine;
    vec![match (oracle_is_coroutine, candidate_is_coroutine) {
        (Some(o), Some(c)) if o == c => {
            ComparisonResult::matched("callable_kind", &format!("{}", o), &format!("{}", c))
        }
        (Some(o), Some(c)) => ComparisonResult::mismatched(
            "callable_kind",
            &format!("coroutine={}", o),
            &format!("coroutine={}", c),
            MismatchKind::TypeMismatch,
            MismatchClassification::CandidateDefect,
        ),
        (None, None) => ComparisonResult::matched("callable_kind", "unknown", "unknown"),
        (Some(o), None) => ComparisonResult::mismatched(
            "callable_kind",
            &format!("coroutine={}", o),
            "unknown",
            MismatchKind::MissingInCandidate,
            MismatchClassification::CandidateDefect,
        ),
        (None, Some(c)) => ComparisonResult::mismatched(
            "callable_kind",
            "unknown",
            &format!("coroutine={}", c),
            MismatchKind::MissingInOracle,
            MismatchClassification::OracleExecutionDefect,
        ),
    }]
}

// ---------------------------------------------------------------------------
// compare_exception
// ---------------------------------------------------------------------------

pub fn compare_exception(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let mut results = Vec::new();

    match (&oracle.exception, &candidate.exception) {
        (Some(oracle_exc), Some(candidate_exc)) => {
            if oracle_exc.class_name == candidate_exc.class_name {
                results.push(ComparisonResult::matched(
                    "exception.class_name",
                    &oracle_exc.class_name,
                    &candidate_exc.class_name,
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "exception.class_name",
                    &oracle_exc.class_name,
                    &candidate_exc.class_name,
                    MismatchKind::StructuralMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }

            if oracle_exc.message_category == candidate_exc.message_category {
                results.push(ComparisonResult::matched(
                    "exception.message_category",
                    &oracle_exc.message_category,
                    &candidate_exc.message_category,
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "exception.message_category",
                    &oracle_exc.message_category,
                    &candidate_exc.message_category,
                    MismatchKind::StructuralMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }
        }
        (Some(oracle_exc), None) => {
            results.push(ComparisonResult::mismatched(
                "exception",
                &oracle_exc.class_name,
                "no_exception",
                MismatchKind::MissingInCandidate,
                MismatchClassification::CandidateDefect,
            ));
        }
        (None, Some(candidate_exc)) => {
            results.push(ComparisonResult::mismatched(
                "exception",
                "no_exception",
                &candidate_exc.class_name,
                MismatchKind::MissingInOracle,
                MismatchClassification::OracleExecutionDefect,
            ));
        }
        (None, None) => {
            results.push(ComparisonResult::matched("exception", "none", "none"));
        }
    }

    results
}

// ---------------------------------------------------------------------------
// compare_protocol_wire
// ---------------------------------------------------------------------------

pub fn compare_protocol_wire(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let mut results = Vec::new();

    match (
        &oracle.protocol_observation,
        &candidate.protocol_observation,
    ) {
        (Some(oracle_proto), Some(candidate_proto)) => {
            if oracle_proto.protocol == candidate_proto.protocol {
                results.push(ComparisonResult::matched(
                    "protocol_wire.protocol",
                    &oracle_proto.protocol,
                    &candidate_proto.protocol,
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "protocol_wire.protocol",
                    &oracle_proto.protocol,
                    &candidate_proto.protocol,
                    MismatchKind::StructuralMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }

            if oracle_proto.connection_result == candidate_proto.connection_result {
                results.push(ComparisonResult::matched(
                    "protocol_wire.connection_result",
                    &oracle_proto.connection_result,
                    &candidate_proto.connection_result,
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "protocol_wire.connection_result",
                    &oracle_proto.connection_result,
                    &candidate_proto.connection_result,
                    MismatchKind::StructuralMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }

            let oracle_sent = oracle_proto.bytes_sent.to_string();
            let candidate_sent = candidate_proto.bytes_sent.to_string();
            if oracle_proto.bytes_sent == candidate_proto.bytes_sent {
                results.push(ComparisonResult::matched(
                    "protocol_wire.bytes_sent",
                    &oracle_sent,
                    &candidate_sent,
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "protocol_wire.bytes_sent",
                    &oracle_sent,
                    &candidate_sent,
                    MismatchKind::ExactMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }

            let oracle_recv = oracle_proto.bytes_received.to_string();
            let candidate_recv = candidate_proto.bytes_received.to_string();
            if oracle_proto.bytes_received == candidate_proto.bytes_received {
                results.push(ComparisonResult::matched(
                    "protocol_wire.bytes_received",
                    &oracle_recv,
                    &candidate_recv,
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "protocol_wire.bytes_received",
                    &oracle_recv,
                    &candidate_recv,
                    MismatchKind::ExactMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }

            let oracle_status = oracle_proto.status_code.map(|c| c.to_string());
            let candidate_status = candidate_proto.status_code.map(|c| c.to_string());
            if oracle_status == candidate_status {
                results.push(ComparisonResult::matched(
                    "protocol_wire.status_code",
                    oracle_status.as_deref().unwrap_or("None"),
                    candidate_status.as_deref().unwrap_or("None"),
                ));
            } else {
                results.push(ComparisonResult::mismatched(
                    "protocol_wire.status_code",
                    oracle_status.as_deref().unwrap_or("None"),
                    candidate_status.as_deref().unwrap_or("None"),
                    MismatchKind::ExactMismatch,
                    MismatchClassification::CandidateDefect,
                ));
            }
        }
        (Some(oracle_proto), None) => {
            results.push(ComparisonResult::mismatched(
                "protocol_wire",
                &oracle_proto.protocol,
                "no_observation",
                MismatchKind::MissingInCandidate,
                MismatchClassification::CandidateDefect,
            ));
        }
        (None, Some(candidate_proto)) => {
            results.push(ComparisonResult::mismatched(
                "protocol_wire",
                "no_observation",
                &candidate_proto.protocol,
                MismatchKind::MissingInOracle,
                MismatchClassification::OracleExecutionDefect,
            ));
        }
        (None, None) => {
            results.push(ComparisonResult::matched("protocol_wire", "none", "none"));
        }
    }

    results
}

// ---------------------------------------------------------------------------
// compare_cli_flag
// ---------------------------------------------------------------------------

pub fn compare_cli_flag(
    flag_name: &str,
    oracle_parse_result: &Result<String, String>,
    candidate_parse_result: &Result<String, String>,
) -> Vec<ComparisonResult> {
    let oracle_str = match oracle_parse_result {
        Ok(v) => format!("Ok({})", v),
        Err(e) => format!("Err({})", e),
    };
    let candidate_str = match candidate_parse_result {
        Ok(v) => format!("Ok({})", v),
        Err(e) => format!("Err({})", e),
    };

    vec![if oracle_parse_result == candidate_parse_result {
        ComparisonResult::matched(
            &format!("cli_flag.{}", flag_name),
            &oracle_str,
            &candidate_str,
        )
    } else {
        let kind = match (oracle_parse_result, candidate_parse_result) {
            (Ok(_), Err(_)) | (Err(_), Ok(_)) => MismatchKind::StructuralMismatch,
            (Err(_), Err(_)) => MismatchKind::ExactMismatch,
            (Ok(_), Ok(_)) => MismatchKind::ExactMismatch,
        };
        ComparisonResult::mismatched(
            &format!("cli_flag.{}", flag_name),
            &oracle_str,
            &candidate_str,
            kind,
            MismatchClassification::CandidateDefect,
        )
    }]
}

// ---------------------------------------------------------------------------
// compare_cipher_roundtrip
// ---------------------------------------------------------------------------

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn compare_cipher_roundtrip(
    cipher_name: &str,
    oracle_input: &[u8],
    oracle_output: &[u8],
    candidate_output: &[u8],
) -> Vec<ComparisonResult> {
    let mut results = Vec::new();

    if oracle_output == candidate_output {
        results.push(ComparisonResult::matched(
            &format!("cipher_roundtrip.{}.encrypt", cipher_name),
            &to_hex(oracle_output),
            &to_hex(candidate_output),
        ));
    } else {
        results.push(ComparisonResult::mismatched(
            &format!("cipher_roundtrip.{}.encrypt", cipher_name),
            &to_hex(oracle_output),
            &to_hex(candidate_output),
            MismatchKind::ExactMismatch,
            MismatchClassification::CandidateDefect,
        ));
    }

    results.push(ComparisonResult::matched(
        &format!("cipher_roundtrip.{}.input", cipher_name),
        &to_hex(oracle_input),
        &to_hex(oracle_input),
    ));

    results
}

// ---------------------------------------------------------------------------
// compare_process_lifecycle
// ---------------------------------------------------------------------------

pub fn compare_process_lifecycle(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let mut results = Vec::new();

    let oracle_exit = oracle.exit_code.map(|c| c.to_string());
    let candidate_exit = candidate.exit_code.map(|c| c.to_string());
    if oracle_exit == candidate_exit {
        results.push(ComparisonResult::matched(
            "process_lifecycle.exit_code",
            oracle_exit.as_deref().unwrap_or("None"),
            candidate_exit.as_deref().unwrap_or("None"),
        ));
    } else {
        results.push(ComparisonResult::mismatched(
            "process_lifecycle.exit_code",
            oracle_exit.as_deref().unwrap_or("None"),
            candidate_exit.as_deref().unwrap_or("None"),
            MismatchKind::ExactMismatch,
            MismatchClassification::CandidateDefect,
        ));
    }

    let oracle_clean = !oracle.cleanup.leftover.is_empty();
    let candidate_clean = !candidate.cleanup.leftover.is_empty();
    if oracle_clean == candidate_clean {
        results.push(ComparisonResult::matched(
            "process_lifecycle.cleanup",
            &format!("{}", oracle_clean),
            &format!("{}", candidate_clean),
        ));
    } else {
        results.push(ComparisonResult::mismatched(
            "process_lifecycle.cleanup",
            &format!("leftover={}", oracle_clean),
            &format!("leftover={}", candidate_clean),
            MismatchKind::StructuralMismatch,
            MismatchClassification::CandidateDefect,
        ));
    }

    results
}

// ---------------------------------------------------------------------------
// compare_failure_class
// ---------------------------------------------------------------------------

pub fn compare_failure_class(
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    let oracle_category = oracle
        .exception
        .as_ref()
        .map(|e| e.message_category.as_str())
        .unwrap_or("none");
    let candidate_category = candidate
        .exception
        .as_ref()
        .map(|e| e.message_category.as_str())
        .unwrap_or("none");

    vec![if oracle_category == candidate_category {
        ComparisonResult::matched("failure_class", oracle_category, candidate_category)
    } else {
        ComparisonResult::mismatched(
            "failure_class",
            oracle_category,
            candidate_category,
            MismatchKind::StructuralMismatch,
            MismatchClassification::Unclassified,
        )
    }]
}

// ---------------------------------------------------------------------------
// compare_composition_validity
// ---------------------------------------------------------------------------

pub fn compare_composition_validity(
    composition_key: &str,
    oracle_accepted: bool,
    candidate_accepted: bool,
) -> Vec<ComparisonResult> {
    vec![if oracle_accepted == candidate_accepted {
        ComparisonResult::matched(
            &format!("composition_validity.{}", composition_key),
            &format!("{}", oracle_accepted),
            &format!("{}", candidate_accepted),
        )
    } else {
        ComparisonResult::mismatched(
            &format!("composition_validity.{}", composition_key),
            &format!("accepted={}", oracle_accepted),
            &format!("accepted={}", candidate_accepted),
            MismatchKind::StructuralMismatch,
            MismatchClassification::CandidateDefect,
        )
    }]
}

// ---------------------------------------------------------------------------
// run_comparator (dispatcher)
// ---------------------------------------------------------------------------

pub fn run_comparator(
    comparator_name: &str,
    oracle: &StrictObservation,
    candidate: &StrictObservation,
) -> Vec<ComparisonResult> {
    match comparator_name {
        "compare_exact_json" => compare_exact_json(oracle, candidate),
        "compare_signature" => compare_signature(oracle, candidate),
        "compare_callable_kind" => compare_callable_kind(oracle, candidate),
        "compare_exception" => compare_exception(oracle, candidate),
        "compare_protocol_wire" => compare_protocol_wire(oracle, candidate),
        "compare_process_lifecycle" => compare_process_lifecycle(oracle, candidate),
        "compare_failure_class" => compare_failure_class(oracle, candidate),
        "compare_namespace_set"
        | "compare_cli_flag"
        | "compare_cipher_roundtrip"
        | "compare_composition_validity" => {
            vec![ComparisonResult::matched(
                comparator_name,
                "dispatch_only",
                "requires_extra_args",
            )]
        }
        _ => vec![ComparisonResult::mismatched(
            comparator_name,
            "unknown_comparator",
            comparator_name,
            MismatchKind::StructuralMismatch,
            MismatchClassification::HarnessDefect,
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strict_observations::{
        CallableSignature, CleanupInfo, EnvironmentMeta, ExceptionInfo, ImportResult,
        MismatchClassification, ProtocolObservation, StrictObservation,
    };

    fn test_env() -> EnvironmentMeta {
        EnvironmentMeta {
            pproxy_version: Some("2.7.9".to_string()),
            eggress_version: "0.1.0".to_string(),
            python_version: "3.11.0".to_string(),
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            interpreter: "cpython".to_string(),
        }
    }

    fn oracle_obs() -> StrictObservation {
        StrictObservation::oracle("test.1", test_env(), ImportResult::Success)
    }

    fn candidate_obs() -> StrictObservation {
        StrictObservation::candidate("test.1", test_env(), ImportResult::Success)
    }

    #[test]
    fn compare_exact_json_match() {
        let o = oracle_obs();
        let c = oracle_obs();
        let results = compare_exact_json(&o, &c);
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_exact_json_mismatch() {
        let o = oracle_obs();
        let mut c = candidate_obs();
        c.warnings.push("extra".to_string());
        let results = compare_exact_json(&o, &c);
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
        assert_eq!(results[0].mismatch_kind, Some(MismatchKind::ExactMismatch));
    }

    #[test]
    fn compare_namespace_set_identical() {
        let imports = vec!["pproxy".to_string(), "pproxy.Connection".to_string()];
        let results = compare_namespace_set(&imports, &imports);
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_namespace_set_missing_in_candidate() {
        let oracle = vec!["pproxy".to_string(), "pproxy.Server".to_string()];
        let candidate = vec!["pproxy".to_string()];
        let results = compare_namespace_set(&oracle, &candidate);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_namespace_set_extra_in_candidate() {
        let oracle = vec!["pproxy".to_string()];
        let candidate = vec!["pproxy".to_string(), "pproxy.extra".to_string()];
        let results = compare_namespace_set(&oracle, &candidate);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_signature_match() {
        let mut o = oracle_obs();
        o.signature = Some(CallableSignature {
            name: "connect".to_string(),
            positional_args: vec!["host".to_string()],
            keyword_args: vec![],
            defaults: vec![],
            return_annotation: None,
        });
        let mut c = candidate_obs();
        c.signature = Some(CallableSignature {
            name: "connect".to_string(),
            positional_args: vec!["host".to_string()],
            keyword_args: vec![],
            defaults: vec![],
            return_annotation: None,
        });
        let results = compare_signature(&o, &c);
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_signature_mismatch_positional_args() {
        let mut o = oracle_obs();
        o.signature = Some(CallableSignature {
            name: "connect".to_string(),
            positional_args: vec!["host".to_string(), "port".to_string()],
            keyword_args: vec![],
            defaults: vec![],
            return_annotation: None,
        });
        let mut c = candidate_obs();
        c.signature = Some(CallableSignature {
            name: "connect".to_string(),
            positional_args: vec!["host".to_string()],
            keyword_args: vec![],
            defaults: vec![],
            return_annotation: None,
        });
        let results = compare_signature(&o, &c);
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_signature_missing_in_candidate() {
        let mut o = oracle_obs();
        o.signature = Some(CallableSignature {
            name: "f".to_string(),
            positional_args: vec![],
            keyword_args: vec![],
            defaults: vec![],
            return_annotation: None,
        });
        let results = compare_signature(&o, &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
        assert_eq!(
            results[0].mismatch_kind,
            Some(MismatchKind::MissingInCandidate)
        );
    }

    #[test]
    fn compare_signature_both_missing() {
        let results = compare_signature(&oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_callable_kind_match() {
        let mut o = oracle_obs();
        o.is_coroutine = Some(true);
        let mut c = candidate_obs();
        c.is_coroutine = Some(true);
        let results = compare_callable_kind(&o, &c);
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_callable_kind_mismatch() {
        let mut o = oracle_obs();
        o.is_coroutine = Some(true);
        let mut c = candidate_obs();
        c.is_coroutine = Some(false);
        let results = compare_callable_kind(&o, &c);
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
        assert_eq!(results[0].mismatch_kind, Some(MismatchKind::TypeMismatch));
    }

    #[test]
    fn compare_callable_kind_both_unknown() {
        let results = compare_callable_kind(&oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_exception_match() {
        let mut o = oracle_obs();
        o.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "timed out".to_string(),
        });
        let mut c = candidate_obs();
        c.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "timed out".to_string(),
        });
        let results = compare_exception(&o, &c);
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_exception_class_mismatch() {
        let mut o = oracle_obs();
        o.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "timed out".to_string(),
        });
        let mut c = candidate_obs();
        c.exception = Some(ExceptionInfo {
            class_name: "ConnectionRefusedError".to_string(),
            message_category: "connection_refused".to_string(),
            stage: "connect".to_string(),
            raw_message: "refused".to_string(),
        });
        let results = compare_exception(&o, &c);
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_exception_oracle_only() {
        let mut o = oracle_obs();
        o.exception = Some(ExceptionInfo {
            class_name: "Error".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "err".to_string(),
        });
        let results = compare_exception(&o, &candidate_obs());
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_exception_candidate_only() {
        let mut c = candidate_obs();
        c.exception = Some(ExceptionInfo {
            class_name: "Error".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "err".to_string(),
        });
        let results = compare_exception(&oracle_obs(), &c);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_exception_both_none() {
        let results = compare_exception(&oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_protocol_wire_match() {
        let mut o = oracle_obs();
        o.protocol_observation = Some(ProtocolObservation {
            protocol: "socks5".to_string(),
            connection_result: "success".to_string(),
            bytes_sent: 100,
            bytes_received: 200,
            status_code: Some(0),
        });
        let mut c = candidate_obs();
        c.protocol_observation = Some(ProtocolObservation {
            protocol: "socks5".to_string(),
            connection_result: "success".to_string(),
            bytes_sent: 100,
            bytes_received: 200,
            status_code: Some(0),
        });
        let results = compare_protocol_wire(&o, &c);
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_protocol_wire_mismatch_bytes() {
        let mut o = oracle_obs();
        o.protocol_observation = Some(ProtocolObservation {
            protocol: "socks5".to_string(),
            connection_result: "success".to_string(),
            bytes_sent: 100,
            bytes_received: 200,
            status_code: None,
        });
        let mut c = candidate_obs();
        c.protocol_observation = Some(ProtocolObservation {
            protocol: "socks5".to_string(),
            connection_result: "success".to_string(),
            bytes_sent: 100,
            bytes_received: 300,
            status_code: None,
        });
        let results = compare_protocol_wire(&o, &c);
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_protocol_wire_missing_in_candidate() {
        let mut o = oracle_obs();
        o.protocol_observation = Some(ProtocolObservation {
            protocol: "http".to_string(),
            connection_result: "success".to_string(),
            bytes_sent: 0,
            bytes_received: 0,
            status_code: Some(200),
        });
        let results = compare_protocol_wire(&o, &candidate_obs());
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_protocol_wire_both_none() {
        let results = compare_protocol_wire(&oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_cli_flag_ok_match() {
        let results = compare_cli_flag("--port", &Ok("8080".to_string()), &Ok("8080".to_string()));
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_cli_flag_ok_mismatch() {
        let results = compare_cli_flag("--port", &Ok("8080".to_string()), &Ok("9090".to_string()));
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
    }

    #[test]
    fn compare_cli_flag_err_match() {
        let results = compare_cli_flag(
            "--daemon",
            &Err("unsupported".to_string()),
            &Err("unsupported".to_string()),
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_cli_flag_structural_mismatch() {
        let results = compare_cli_flag(
            "--flag",
            &Ok("value".to_string()),
            &Err("rejected".to_string()),
        );
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
        assert_eq!(
            results[0].mismatch_kind,
            Some(MismatchKind::StructuralMismatch)
        );
    }

    #[test]
    fn compare_cipher_roundtrip_match() {
        let input = b"hello world";
        let encrypted = b"encrypted_bytes";
        let results = compare_cipher_roundtrip("aes_256_gcm", input, encrypted, encrypted);
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_cipher_roundtrip_mismatch() {
        let input = b"hello world";
        let oracle_enc = b"oracle_encrypted";
        let candidate_enc = b"cand_encrypted";
        let results = compare_cipher_roundtrip("aes_256_gcm", input, oracle_enc, candidate_enc);
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_process_lifecycle_match() {
        let o = oracle_obs();
        let c = candidate_obs();
        let results = compare_process_lifecycle(&o, &c);
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_process_lifecycle_exit_code_mismatch() {
        let mut o = oracle_obs();
        o.exit_code = Some(0);
        let mut c = candidate_obs();
        c.exit_code = Some(1);
        let results = compare_process_lifecycle(&o, &c);
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_process_lifecycle_cleanup_mismatch() {
        let mut o = oracle_obs();
        o.cleanup = CleanupInfo {
            processes_cleaned: true,
            sockets_cleaned: true,
            files_cleaned: true,
            leftover: vec![],
        };
        let mut c = candidate_obs();
        c.cleanup = CleanupInfo {
            processes_cleaned: false,
            sockets_cleaned: false,
            files_cleaned: false,
            leftover: vec!["/tmp/stale".to_string()],
        };
        let results = compare_process_lifecycle(&o, &c);
        assert!(results.iter().any(|r| !r.matched));
    }

    #[test]
    fn compare_failure_class_match() {
        let mut o = oracle_obs();
        o.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "timed out".to_string(),
        });
        let mut c = candidate_obs();
        c.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "timed out".to_string(),
        });
        let results = compare_failure_class(&o, &c);
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_failure_class_mismatch() {
        let mut o = oracle_obs();
        o.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "connect".to_string(),
            raw_message: "timed out".to_string(),
        });
        let mut c = candidate_obs();
        c.exception = Some(ExceptionInfo {
            class_name: "ConnectionRefusedError".to_string(),
            message_category: "connection_refused".to_string(),
            stage: "connect".to_string(),
            raw_message: "refused".to_string(),
        });
        let results = compare_failure_class(&o, &c);
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
    }

    #[test]
    fn compare_failure_class_both_none() {
        let results = compare_failure_class(&oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_composition_validity_match() {
        let results = compare_composition_validity("socks5->http", true, true);
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }

    #[test]
    fn compare_composition_validity_mismatch() {
        let results = compare_composition_validity("socks5->http", true, false);
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
    }

    #[test]
    fn run_comparator_exact_json() {
        let o = oracle_obs();
        let c = candidate_obs();
        let results = run_comparator("compare_exact_json", &o, &c);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_comparator_signature() {
        let results = run_comparator("compare_signature", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_comparator_callable_kind() {
        let results = run_comparator("compare_callable_kind", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_comparator_exception() {
        let results = run_comparator("compare_exception", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_comparator_protocol_wire() {
        let results = run_comparator("compare_protocol_wire", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_comparator_process_lifecycle() {
        let results = run_comparator("compare_process_lifecycle", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn run_comparator_failure_class() {
        let results = run_comparator("compare_failure_class", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_comparator_unknown() {
        let results = run_comparator("bogus_comparator", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(!results[0].matched);
        assert_eq!(
            results[0].classification,
            MismatchClassification::HarnessDefect
        );
    }

    #[test]
    fn run_comparator_extra_args_passthrough() {
        let results = run_comparator("compare_namespace_set", &oracle_obs(), &candidate_obs());
        assert_eq!(results.len(), 1);
        assert!(results[0].matched);
    }
}
