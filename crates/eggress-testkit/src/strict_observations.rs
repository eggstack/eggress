use serde::{Deserialize, Serialize};

/// Schema version for strict observations
pub const STRICT_OBSERVATION_SCHEMA_VERSION: u32 = 1;

/// Environment metadata captured with each observation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentMeta {
    pub pproxy_version: Option<String>,
    pub eggress_version: String,
    pub python_version: String,
    pub os: String,
    pub arch: String,
    pub interpreter: String,
}

/// A strict observation emitted by either oracle or candidate runner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrictObservation {
    pub schema_version: u32,
    pub scenario_id: String,
    pub runner: RunnerKind,
    pub environment: EnvironmentMeta,
    pub import_result: ImportResult,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub signature: Option<CallableSignature>,
    pub is_coroutine: Option<bool>,
    pub return_shape: Option<String>,
    pub attributes: Vec<String>,
    pub exception: Option<ExceptionInfo>,
    pub protocol_observation: Option<ProtocolObservation>,
    pub cleanup: CleanupInfo,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerKind {
    Oracle,
    Candidate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportResult {
    Success,
    ModuleNotFound,
    ImportError,
    SyntaxError,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallableSignature {
    pub name: String,
    pub positional_args: Vec<String>,
    pub keyword_args: Vec<String>,
    pub defaults: Vec<Option<String>>,
    pub return_annotation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionInfo {
    pub class_name: String,
    pub message_category: String,
    pub stage: String,
    pub raw_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolObservation {
    pub protocol: String,
    pub connection_result: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupInfo {
    pub processes_cleaned: bool,
    pub sockets_cleaned: bool,
    pub files_cleaned: bool,
    pub leftover: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub field: String,
    pub oracle_value: String,
    pub candidate_value: String,
    pub matched: bool,
    pub mismatch_kind: Option<MismatchKind>,
    pub classification: MismatchClassification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MismatchKind {
    ExactMismatch,
    StructuralMismatch,
    MissingInCandidate,
    MissingInOracle,
    TypeMismatch,
    SignatureMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MismatchClassification {
    CandidateDefect,
    OracleExecutionDefect,
    HarnessDefect,
    KnownUpstreamDefect,
    PlatformConstraint,
    ManifestDefect,
    ApprovedNormalization,
    Unclassified,
}

impl ComparisonResult {
    pub fn matched(field: &str, oracle_value: &str, candidate_value: &str) -> Self {
        Self {
            field: field.to_string(),
            oracle_value: oracle_value.to_string(),
            candidate_value: candidate_value.to_string(),
            matched: true,
            mismatch_kind: None,
            classification: MismatchClassification::Unclassified,
        }
    }

    pub fn mismatched(
        field: &str,
        oracle_value: &str,
        candidate_value: &str,
        kind: MismatchKind,
        classification: MismatchClassification,
    ) -> Self {
        Self {
            field: field.to_string(),
            oracle_value: oracle_value.to_string(),
            candidate_value: candidate_value.to_string(),
            matched: false,
            mismatch_kind: Some(kind),
            classification,
        }
    }
}

impl StrictObservation {
    pub fn oracle(
        scenario_id: &str,
        environment: EnvironmentMeta,
        import_result: ImportResult,
    ) -> Self {
        Self {
            schema_version: STRICT_OBSERVATION_SCHEMA_VERSION,
            scenario_id: scenario_id.to_string(),
            runner: RunnerKind::Oracle,
            environment,
            import_result,
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: None,
            duration_ms: 0,
            signature: None,
            is_coroutine: None,
            return_shape: None,
            attributes: Vec::new(),
            exception: None,
            protocol_observation: None,
            cleanup: CleanupInfo {
                processes_cleaned: false,
                sockets_cleaned: false,
                files_cleaned: false,
                leftover: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    pub fn candidate(
        scenario_id: &str,
        environment: EnvironmentMeta,
        import_result: ImportResult,
    ) -> Self {
        Self {
            schema_version: STRICT_OBSERVATION_SCHEMA_VERSION,
            scenario_id: scenario_id.to_string(),
            runner: RunnerKind::Candidate,
            environment,
            import_result,
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: None,
            duration_ms: 0,
            signature: None,
            is_coroutine: None,
            return_shape: None,
            attributes: Vec::new(),
            exception: None,
            protocol_observation: None,
            cleanup: CleanupInfo {
                processes_cleaned: false,
                sockets_cleaned: false,
                files_cleaned: false,
                leftover: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn strict_observation_json_roundtrip() {
        let obs = StrictObservation::oracle("test.1", test_env(), ImportResult::Success);
        let json = serde_json::to_string(&obs).unwrap();
        let parsed: StrictObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.scenario_id, "test.1");
        assert!(matches!(parsed.runner, RunnerKind::Oracle));
        assert!(matches!(parsed.import_result, ImportResult::Success));
    }

    #[test]
    fn candidate_runner_serde() {
        let obs = StrictObservation::candidate("test.2", test_env(), ImportResult::ModuleNotFound);
        let json = serde_json::to_string(&obs).unwrap();
        assert!(json.contains("\"candidate\""));
        let parsed: StrictObservation = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.runner, RunnerKind::Candidate));
        assert!(matches!(parsed.import_result, ImportResult::ModuleNotFound));
    }

    #[test]
    fn comparison_result_matched() {
        let r = ComparisonResult::matched("field", "a", "a");
        assert!(r.matched);
        assert!(r.mismatch_kind.is_none());
    }

    #[test]
    fn comparison_result_mismatched() {
        let r = ComparisonResult::mismatched(
            "field",
            "oracle_val",
            "cand_val",
            MismatchKind::ExactMismatch,
            MismatchClassification::CandidateDefect,
        );
        assert!(!r.matched);
        assert_eq!(r.mismatch_kind, Some(MismatchKind::ExactMismatch));
        assert_eq!(r.classification, MismatchClassification::CandidateDefect);
    }

    #[test]
    fn environment_meta_json_roundtrip() {
        let env = test_env();
        let json = serde_json::to_string(&env).unwrap();
        let parsed: EnvironmentMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pproxy_version, Some("2.7.9".to_string()));
        assert_eq!(parsed.arch, "aarch64");
    }

    #[test]
    fn cleanup_info_defaults() {
        let cleanup = CleanupInfo {
            processes_cleaned: false,
            sockets_cleaned: false,
            files_cleaned: false,
            leftover: vec!["/tmp/stale".to_string()],
        };
        let json = serde_json::to_string(&cleanup).unwrap();
        let parsed: CleanupInfo = serde_json::from_str(&json).unwrap();
        assert!(!parsed.processes_cleaned);
        assert_eq!(parsed.leftover.len(), 1);
    }

    #[test]
    fn exception_info_roundtrip() {
        let exc = ExceptionInfo {
            class_name: "ConnectionRefusedError".to_string(),
            message_category: "connection_refused".to_string(),
            stage: "connect".to_string(),
            raw_message: "[Errno 61] Connection refused".to_string(),
        };
        let json = serde_json::to_string(&exc).unwrap();
        let parsed: ExceptionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.class_name, "ConnectionRefusedError");
        assert_eq!(parsed.message_category, "connection_refused");
    }

    #[test]
    fn callable_signature_roundtrip() {
        let sig = CallableSignature {
            name: "connect".to_string(),
            positional_args: vec!["host".to_string(), "port".to_string()],
            keyword_args: vec!["timeout".to_string()],
            defaults: vec![None, None, Some("30".to_string())],
            return_annotation: Some("Connection".to_string()),
        };
        let json = serde_json::to_string(&sig).unwrap();
        let parsed: CallableSignature = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.positional_args.len(), 2);
        assert_eq!(parsed.defaults[2], Some("30".to_string()));
    }

    #[test]
    fn protocol_observation_roundtrip() {
        let proto = ProtocolObservation {
            protocol: "socks5".to_string(),
            connection_result: "success".to_string(),
            bytes_sent: 1024,
            bytes_received: 2048,
            status_code: Some(0),
        };
        let json = serde_json::to_string(&proto).unwrap();
        let parsed: ProtocolObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bytes_sent, 1024);
        assert_eq!(parsed.status_code, Some(0));
    }

    #[test]
    fn mismatch_kind_serde() {
        let kinds = vec![
            MismatchKind::ExactMismatch,
            MismatchKind::StructuralMismatch,
            MismatchKind::MissingInCandidate,
            MismatchKind::MissingInOracle,
            MismatchKind::TypeMismatch,
            MismatchKind::SignatureMismatch,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: MismatchKind = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }

    #[test]
    fn mismatch_classification_serde() {
        let classes = vec![
            MismatchClassification::CandidateDefect,
            MismatchClassification::OracleExecutionDefect,
            MismatchClassification::HarnessDefect,
            MismatchClassification::KnownUpstreamDefect,
            MismatchClassification::PlatformConstraint,
            MismatchClassification::ManifestDefect,
            MismatchClassification::ApprovedNormalization,
            MismatchClassification::Unclassified,
        ];
        for cls in classes {
            let json = serde_json::to_string(&cls).unwrap();
            let parsed: MismatchClassification = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }

    #[test]
    fn full_observation_with_signature_and_exception() {
        let mut obs = StrictObservation::oracle("full.test", test_env(), ImportResult::Success);
        obs.signature = Some(CallableSignature {
            name: "proxy".to_string(),
            positional_args: vec![],
            keyword_args: vec!["port".to_string()],
            defaults: vec![Some("8080".to_string())],
            return_annotation: None,
        });
        obs.is_coroutine = Some(false);
        obs.return_shape = Some("Connection".to_string());
        obs.attributes = vec!["public".to_string(), "async".to_string()];
        obs.exception = Some(ExceptionInfo {
            class_name: "TimeoutError".to_string(),
            message_category: "timeout".to_string(),
            stage: "handshake".to_string(),
            raw_message: "timed out".to_string(),
        });
        obs.protocol_observation = Some(ProtocolObservation {
            protocol: "http".to_string(),
            connection_result: "timeout".to_string(),
            bytes_sent: 0,
            bytes_received: 0,
            status_code: None,
        });
        obs.cleanup = CleanupInfo {
            processes_cleaned: true,
            sockets_cleaned: true,
            files_cleaned: false,
            leftover: vec!["/tmp/x".to_string()],
        };
        obs.warnings = vec!["deprecated_api".to_string()];
        obs.stdout = vec!["info: started".to_string()];
        obs.stderr = vec!["warn: slow".to_string()];
        obs.exit_code = Some(0);
        obs.duration_ms = 150;

        let json = serde_json::to_string(&obs).unwrap();
        let parsed: StrictObservation = serde_json::from_str(&json).unwrap();
        assert!(parsed.signature.is_some());
        assert_eq!(parsed.signature.unwrap().name, "proxy");
        assert!(parsed.exception.is_some());
        assert_eq!(parsed.exception.unwrap().message_category, "timeout");
        assert!(parsed.protocol_observation.is_some());
        assert_eq!(parsed.warnings.len(), 1);
        assert_eq!(parsed.duration_ms, 150);
    }
}
