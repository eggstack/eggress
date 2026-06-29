use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceLevel {
    Unimplemented,
    ImplementedSynthetic,
    ImplementedDifferential,
    ImplementedInterop,
    Compatible,
    IntentionalNonParity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonCategory {
    PayloadEquality,
    StatusCodeEquality,
    CloseBehaviorEquality,
    ErrorClassEquality,
    ExitCodeEquality,
    StdoutPatternEquality,
    StderrPatternEquality,
    NegativeCaseEquivalence,
    AllowedDivergence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseResult {
    Pass,
    Fail { details: String },
    Skip { reason: String },
}

#[derive(Debug, Clone)]
pub struct PproxyCase {
    pub id: &'static str,
    pub feature_id: &'static str,
    pub pproxy_args: Vec<String>,
    pub eggress_args: Vec<String>,
    pub expected_comparisons: Vec<ComparisonCategory>,
    pub timeout: Duration,
    pub manifest_evidence: EvidenceLevel,
}

#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub case_id: String,
    pub results: Vec<(ComparisonCategory, CaseResult)>,
    pub pproxy_output: ProcessOutput,
    pub eggress_output: ProcessOutput,
}

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
}

pub fn compare_payloads(expected: &[u8], actual: &[u8]) -> CaseResult {
    if expected == actual {
        CaseResult::Pass
    } else {
        CaseResult::Fail {
            details: format!(
                "payload mismatch: expected {} bytes, got {} bytes",
                expected.len(),
                actual.len()
            ),
        }
    }
}

pub fn compare_exit_codes(expected: Option<i32>, actual: Option<i32>) -> CaseResult {
    if expected == actual {
        CaseResult::Pass
    } else {
        CaseResult::Fail {
            details: format!(
                "exit code mismatch: expected {:?}, got {:?}",
                expected, actual
            ),
        }
    }
}

pub fn compare_stderr_pattern(pattern: &str, stderr: &[u8]) -> CaseResult {
    let stderr_str = String::from_utf8_lossy(stderr);
    if stderr_str.contains(pattern) {
        CaseResult::Pass
    } else {
        CaseResult::Fail {
            details: format!(
                "stderr pattern '{}' not found in output ({} bytes)",
                pattern,
                stderr.len()
            ),
        }
    }
}

pub fn assert_negative_equivalence(
    pproxy_result: &CaseResult,
    eggress_result: &CaseResult,
) -> CaseResult {
    let pproxy_is_failure = matches!(pproxy_result, CaseResult::Fail { .. });
    let eggress_is_failure = matches!(eggress_result, CaseResult::Fail { .. });

    if pproxy_is_failure && eggress_is_failure {
        CaseResult::Pass
    } else if pproxy_is_failure && !eggress_is_failure {
        CaseResult::Fail {
            details: format!(
                "negative case divergence: pproxy failed ({}) but eggress succeeded",
                match pproxy_result {
                    CaseResult::Fail { details } => details.as_str(),
                    _ => unreachable!(),
                }
            ),
        }
    } else if !pproxy_is_failure && eggress_is_failure {
        CaseResult::Fail {
            details: format!(
                "negative case divergence: pproxy succeeded but eggress failed ({})",
                match eggress_result {
                    CaseResult::Fail { details } => details.as_str(),
                    _ => unreachable!(),
                }
            ),
        }
    } else {
        CaseResult::Pass
    }
}
