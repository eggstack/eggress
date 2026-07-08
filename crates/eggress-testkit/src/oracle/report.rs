//! Oracle JSON report generation.
//!
//! Produces structured comparison reports after running oracle scenarios.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::scenario::ScenarioCategory;

/// Top-level oracle report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleReport {
    /// pproxy version used.
    pub pproxy_version: String,
    /// eggress commit hash.
    pub eggress_commit: Option<String>,
    /// OS platform.
    pub os_platform: String,
    /// Rust version.
    pub rust_version: String,
    /// Python version.
    pub python_version: String,
    /// Total elapsed time.
    pub elapsed_ms: u64,
    /// Per-scenario results.
    pub scenarios: Vec<ScenarioResult>,
    /// Summary counts.
    pub summary: ReportSummary,
}

/// Result of a single scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario ID.
    pub id: String,
    /// Scenario category.
    pub category: ScenarioCategory,
    /// Scenario description.
    pub description: String,
    /// Outcome status.
    pub status: ScenarioStatus,
    /// Comparison results.
    pub comparisons: Vec<ComparisonResult>,
    /// Elapsed time for this scenario.
    pub elapsed_ms: u64,
    /// Error message if status is Error.
    pub error: Option<String>,
    /// Skip reason if status is Skipped.
    pub skip_reason: Option<String>,
}

/// Status of a scenario execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStatus {
    /// Both sides ran and matched.
    Pass,
    /// Both sides ran but diverged.
    Fail,
    /// Scenario was skipped (missing prerequisites).
    Skipped,
    /// Scenario errored during execution.
    Error,
}

/// A single comparison within a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    /// What is being compared (e.g., "tcp_echo_payload", "exit_code").
    pub dimension: String,
    /// pproxy result (serialized).
    pub pproxy_value: String,
    /// eggress result (serialized).
    pub eggress_value: String,
    /// Whether the comparison matched.
    pub matched: bool,
    /// Details if mismatched.
    pub details: Option<String>,
}

/// Summary counts for the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
}

impl Default for OracleReport {
    fn default() -> Self {
        Self::new()
    }
}

impl OracleReport {
    /// Create a new empty report with environment metadata.
    pub fn new() -> Self {
        Self {
            pproxy_version: detect_pproxy_version().unwrap_or_else(|| "unknown".to_string()),
            eggress_commit: detect_eggress_commit(),
            os_platform: std::env::consts::OS.to_string(),
            rust_version: detect_rust_version(),
            python_version: detect_python_version().unwrap_or_else(|| "unknown".to_string()),
            elapsed_ms: 0,
            scenarios: Vec::new(),
            summary: ReportSummary {
                total: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
                errors: 0,
            },
        }
    }

    /// Add a scenario result and update summary.
    pub fn add_scenario(&mut self, result: ScenarioResult) {
        match result.status {
            ScenarioStatus::Pass => self.summary.passed += 1,
            ScenarioStatus::Fail => self.summary.failed += 1,
            ScenarioStatus::Skipped => self.summary.skipped += 1,
            ScenarioStatus::Error => self.summary.errors += 1,
        }
        self.summary.total += 1;
        self.scenarios.push(result);
    }

    /// Set total elapsed time.
    pub fn set_elapsed(&mut self, elapsed: Duration) {
        self.elapsed_ms = elapsed.as_millis() as u64;
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("oracle report serialization should not fail")
    }

    /// Write report to a JSON file.
    pub fn write_json(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_json())
    }
}

/// Normalize a value for comparison (strip ports, versions, etc.).
pub fn normalize_for_comparison(value: &str, scenario_id: &str) -> String {
    let mut result = value.to_string();

    // Replace dynamic port numbers with a placeholder
    if let Ok(re) = regex::Regex::new(r":\d{4,5}") {
        result = re.replace_all(&result, ":PORT").to_string();
    }

    // Strip pproxy-specific log prefixes
    if scenario_id.starts_with("cli.") {
        for prefix in &["INFO:", "WARNING:", "DEBUG:", "Listen: "] {
            result = result.replace(prefix, "");
        }
    }

    // Strip version strings
    result = result.replace("pproxy/", "");
    result = result.replace("eggress/", "");

    result.trim().to_string()
}

/// Create a comparison result.
pub fn make_comparison(
    dimension: &str,
    pproxy_value: &str,
    eggress_value: &str,
) -> ComparisonResult {
    let matched = pproxy_value == eggress_value;
    let details = if matched {
        None
    } else {
        Some(format!(
            "pproxy: {}, eggress: {}",
            truncate(pproxy_value, 200),
            truncate(eggress_value, 200)
        ))
    };
    ComparisonResult {
        dimension: dimension.to_string(),
        pproxy_value: pproxy_value.to_string(),
        eggress_value: eggress_value.to_string(),
        matched,
        details,
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... ({} bytes total)", &s[..max_len], s.len())
    }
}

fn detect_pproxy_version() -> Option<String> {
    let python = crate::differential::find_python_binary();
    Command::new(&python)
        .args([
            "-c",
            "import pproxy; print(getattr(pproxy, '__version__', 'unknown'))",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn detect_eggress_commit() -> Option<String> {
    if let Ok(val) = std::env::var("EGRESS_COMMIT") {
        if !val.is_empty() {
            return Some(val);
        }
    }
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn detect_python_version() -> Option<String> {
    Command::new("python3")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn detect_rust_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_json_roundtrip() {
        let mut report = OracleReport::new();
        report.add_scenario(ScenarioResult {
            id: "test".to_string(),
            category: ScenarioCategory::CliDefaults,
            description: "test scenario".to_string(),
            status: ScenarioStatus::Pass,
            comparisons: vec![],
            elapsed_ms: 100,
            error: None,
            skip_reason: None,
        });
        report.set_elapsed(Duration::from_secs(5));

        let json = report.to_json();
        let parsed: OracleReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.scenarios.len(), 1);
        assert_eq!(parsed.summary.total, 1);
        assert_eq!(parsed.summary.passed, 1);
        assert_eq!(parsed.elapsed_ms, 5000);
    }

    #[test]
    fn make_comparison_match() {
        let comp = make_comparison("payload", "hello", "hello");
        assert!(comp.matched);
        assert!(comp.details.is_none());
    }

    #[test]
    fn make_comparison_mismatch() {
        let comp = make_comparison("payload", "hello", "world");
        assert!(!comp.matched);
        assert!(comp.details.is_some());
    }

    #[test]
    fn scenario_category_serde() {
        let cat = ScenarioCategory::CliDefaults;
        let json = serde_json::to_string(&cat).unwrap();
        assert_eq!(json, "\"cli_defaults\"");
    }
}
