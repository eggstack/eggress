//! Oracle JSON report generation.
//!
//! Produces structured comparison reports after running oracle scenarios.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::observations::ProxyObservation;
use super::scenario::ScenarioCategory;

/// CI tier for scenario filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiTier {
    FastStructural,
    CoreDifferential,
    ExtendedDifferential,
    PlatformDifferential,
    PrivilegedExternal,
}

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
    /// Observations from pproxy execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pproxy_observation: Option<ProxyObservation>,
    /// Observations from eggress execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eggress_observation: Option<ProxyObservation>,
    /// Timing tolerance in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_tolerance_ms: Option<u64>,
    /// Divergence IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub divergence_ids: Vec<String>,
    /// CI tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_tier: Option<CiTier>,
    /// Capability IDs from the scenario definition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_ids: Vec<String>,
}

/// Status of a scenario execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStatus {
    Pass,
    Fail,
    Skipped,
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

    /// Generate a human-readable Markdown report.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Oracle Differential Report\n\n");
        md.push_str(&format!("**pproxy version:** {}\n", self.pproxy_version));
        md.push_str(&format!(
            "**eggress commit:** {}\n",
            self.eggress_commit.as_deref().unwrap_or("unknown")
        ));
        md.push_str(&format!("**platform:** {}\n", self.os_platform));
        md.push_str(&format!("**elapsed:** {}ms\n\n", self.elapsed_ms));

        md.push_str("## Summary\n\n");
        md.push_str("| Metric | Count |\n|--------|-------|\n");
        md.push_str(&format!("| Total | {} |\n", self.summary.total));
        md.push_str(&format!("| Passed | {} |\n", self.summary.passed));
        md.push_str(&format!("| Failed | {} |\n", self.summary.failed));
        md.push_str(&format!("| Skipped | {} |\n", self.summary.skipped));
        md.push_str(&format!("| Errors | {} |\n\n", self.summary.errors));

        let mut by_category: BTreeMap<String, Vec<&ScenarioResult>> = BTreeMap::new();
        for s in &self.scenarios {
            let cat = format!("{:?}", s.category);
            by_category.entry(cat).or_default().push(s);
        }

        for (cat, scenarios) in &by_category {
            md.push_str(&format!("## {}\n\n", cat));
            for s in scenarios {
                let status_icon = match s.status {
                    ScenarioStatus::Pass => "✅",
                    ScenarioStatus::Fail => "❌",
                    ScenarioStatus::Skipped => "⏭️",
                    ScenarioStatus::Error => "⚠️",
                };
                md.push_str(&format!(
                    "### {} {} {}\n\n",
                    status_icon, s.id, s.description
                ));

                if let Some(err) = &s.error {
                    md.push_str(&format!("**Error:** {}\n\n", err));
                }
                if let Some(skip) = &s.skip_reason {
                    md.push_str(&format!("**Skip reason:** {}\n\n", skip));
                }

                if !s.comparisons.is_empty() {
                    md.push_str("| Dimension | Match | Pproxy | Eggress |\n");
                    md.push_str("|-----------|-------|--------|----------|\n");
                    for c in &s.comparisons {
                        let match_icon = if c.matched { "✅" } else { "❌" };
                        md.push_str(&format!(
                            "| {} | {} | {} | {} |\n",
                            c.dimension,
                            match_icon,
                            truncate_md(&c.pproxy_value, 50),
                            truncate_md(&c.eggress_value, 50)
                        ));
                    }
                    md.push('\n');
                }

                if !s.divergence_ids.is_empty() {
                    md.push_str(&format!(
                        "**Divergences:** {}\n\n",
                        s.divergence_ids.join(", ")
                    ));
                }
            }
        }

        md
    }

    /// Check manifest consistency: verify every scenario references valid capability IDs.
    pub fn check_manifest_consistency(&self, valid_capability_ids: &[&str]) -> Vec<String> {
        let mut warnings = Vec::new();
        for s in &self.scenarios {
            for cap_id in &s.capability_ids {
                if !valid_capability_ids.contains(&cap_id.as_str()) {
                    warnings.push(format!(
                        "scenario '{}' references unknown capability '{}'",
                        s.id, cap_id
                    ));
                }
            }
        }
        warnings
    }

    /// Filter scenarios by CI tier.
    pub fn scenarios_for_tier(&self, tier: CiTier) -> Vec<&ScenarioResult> {
        self.scenarios
            .iter()
            .filter(|s| s.ci_tier == Some(tier))
            .collect()
    }
}

impl ScenarioResult {
    pub fn new(id: &str, category: ScenarioCategory, description: &str) -> Self {
        Self {
            id: id.to_string(),
            category,
            description: description.to_string(),
            status: ScenarioStatus::Skipped,
            comparisons: Vec::new(),
            elapsed_ms: 0,
            error: None,
            skip_reason: None,
            pproxy_observation: None,
            eggress_observation: None,
            timing_tolerance_ms: None,
            divergence_ids: Vec::new(),
            ci_tier: None,
            capability_ids: Vec::new(),
        }
    }

    pub fn with_status(mut self, status: ScenarioStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed_ms = elapsed.as_millis() as u64;
        self
    }

    pub fn with_comparisons(mut self, comparisons: Vec<ComparisonResult>) -> Self {
        self.comparisons = comparisons;
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    pub fn with_skip_reason(mut self, reason: String) -> Self {
        self.skip_reason = Some(reason);
        self.status = ScenarioStatus::Skipped;
        self
    }

    pub fn with_pproxy_observation(mut self, obs: ProxyObservation) -> Self {
        self.pproxy_observation = Some(obs);
        self
    }

    pub fn with_eggress_observation(mut self, obs: ProxyObservation) -> Self {
        self.eggress_observation = Some(obs);
        self
    }

    pub fn with_timing_tolerance(mut self, tolerance_ms: u64) -> Self {
        self.timing_tolerance_ms = Some(tolerance_ms);
        self
    }

    pub fn with_divergences(mut self, ids: Vec<String>) -> Self {
        self.divergence_ids = ids;
        self
    }

    pub fn with_ci_tier(mut self, tier: CiTier) -> Self {
        self.ci_tier = Some(tier);
        self
    }

    pub fn with_capability_ids(mut self, ids: Vec<String>) -> Self {
        self.capability_ids = ids;
        self
    }
}

fn truncate_md(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.replace('|', "\\|")
    } else {
        format!("{}... ({} bytes)", s[..max].replace('|', "\\|"), s.len())
    }
}

/// Normalize a value for comparison (strip ports, versions, etc.).
pub fn normalize_for_comparison(value: &str, scenario_id: &str) -> String {
    let mut result = value.to_string();

    if let Ok(re) = regex::Regex::new(r":\d{4,5}") {
        result = re.replace_all(&result, ":PORT").to_string();
    }

    if scenario_id.starts_with("cli.") {
        for prefix in &["INFO:", "WARNING:", "DEBUG:", "Listen: "] {
            result = result.replace(prefix, "");
        }
    }

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
        report.add_scenario(
            ScenarioResult::new("test", ScenarioCategory::CliDefaults, "test scenario")
                .with_status(ScenarioStatus::Pass)
                .with_elapsed(Duration::from_millis(100)),
        );
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

    #[test]
    fn markdown_report_generation() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("test.pass", ScenarioCategory::CliDefaults, "passing test")
                .with_status(ScenarioStatus::Pass)
                .with_comparisons(vec![make_comparison("payload", "hello", "hello")]),
        );
        report.add_scenario(
            ScenarioResult::new("test.fail", ScenarioCategory::HttpSocksTcp, "failing test")
                .with_status(ScenarioStatus::Fail)
                .with_error("mismatch".to_string()),
        );

        let md = report.to_markdown();
        assert!(md.contains("# Oracle Differential Report"));
        assert!(md.contains("test.pass"));
        assert!(md.contains("test.fail"));
        assert!(md.contains("✅"));
        assert!(md.contains("❌"));
    }

    #[test]
    fn manifest_consistency_check() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("test", ScenarioCategory::CliDefaults, "test")
                .with_capability_ids(vec!["valid.cap".to_string(), "unknown.cap".to_string()]),
        );

        let warnings = report.check_manifest_consistency(&["valid.cap"]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown.cap"));
    }

    #[test]
    fn manifest_consistency_no_warnings_for_valid() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("test", ScenarioCategory::CliDefaults, "test")
                .with_capability_ids(vec!["cap.a".to_string(), "cap.b".to_string()]),
        );

        let warnings = report.check_manifest_consistency(&["cap.a", "cap.b"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn tier_filtering() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("fast", ScenarioCategory::CliDefaults, "fast")
                .with_ci_tier(CiTier::FastStructural),
        );
        report.add_scenario(
            ScenarioResult::new("core", ScenarioCategory::HttpSocksTcp, "core")
                .with_ci_tier(CiTier::CoreDifferential),
        );
        report.add_scenario(ScenarioResult::new(
            "no_tier",
            ScenarioCategory::Chains,
            "no tier",
        ));

        let fast = report.scenarios_for_tier(CiTier::FastStructural);
        assert_eq!(fast.len(), 1);
        assert_eq!(fast[0].id, "fast");

        let core = report.scenarios_for_tier(CiTier::CoreDifferential);
        assert_eq!(core.len(), 1);
        assert_eq!(core[0].id, "core");

        let extended = report.scenarios_for_tier(CiTier::ExtendedDifferential);
        assert!(extended.is_empty());
    }

    #[test]
    fn scenario_result_builder() {
        let result = ScenarioResult::new("id", ScenarioCategory::Chains, "desc")
            .with_status(ScenarioStatus::Pass)
            .with_elapsed(Duration::from_secs(1))
            .with_comparisons(vec![make_comparison("dim", "a", "a")])
            .with_divergences(vec!["div1".to_string()])
            .with_ci_tier(CiTier::CoreDifferential)
            .with_capability_ids(vec!["cap1".to_string()]);

        assert_eq!(result.status, ScenarioStatus::Pass);
        assert_eq!(result.elapsed_ms, 1000);
        assert_eq!(result.divergence_ids, vec!["div1"]);
        assert_eq!(result.ci_tier, Some(CiTier::CoreDifferential));
        assert_eq!(result.capability_ids, vec!["cap1"]);
    }

    #[test]
    fn scenario_result_builder_skip_reason_sets_status() {
        let result = ScenarioResult::new("id", ScenarioCategory::CliDefaults, "desc")
            .with_status(ScenarioStatus::Pass)
            .with_skip_reason("missing deps".to_string());
        assert_eq!(result.status, ScenarioStatus::Skipped);
    }

    #[test]
    fn ci_tier_serde() {
        let tier = CiTier::FastStructural;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"fast_structural\"");
        let parsed: CiTier = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tier);
    }

    #[test]
    fn markdown_includes_divergences() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("div_test", ScenarioCategory::Udp, "udp test")
                .with_status(ScenarioStatus::Pass)
                .with_divergences(vec!["div.timing".to_string(), "div.payload".to_string()]),
        );

        let md = report.to_markdown();
        assert!(md.contains("**Divergences:** div.timing, div.payload"));
    }

    #[test]
    fn markdown_includes_skip_reason() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("skip_test", ScenarioCategory::CliDefaults, "skipped")
                .with_skip_reason("needs root".to_string()),
        );

        let md = report.to_markdown();
        assert!(md.contains("**Skip reason:** needs root"));
        assert!(md.contains("⏭️"));
    }

    #[test]
    fn markdown_truncates_long_values() {
        let mut report = OracleReport::new();
        let long_value = "x".repeat(200);
        report.add_scenario(
            ScenarioResult::new("long", ScenarioCategory::HttpSocksTcp, "long values")
                .with_status(ScenarioStatus::Fail)
                .with_comparisons(vec![make_comparison("dim", &long_value, "short")]),
        );

        let md = report.to_markdown();
        assert!(md.contains("... (200 bytes)"));
    }

    #[test]
    fn json_roundtrip_preserves_new_fields() {
        let mut report = OracleReport::new();
        report.add_scenario(
            ScenarioResult::new("full", ScenarioCategory::Chains, "full fields")
                .with_status(ScenarioStatus::Pass)
                .with_timing_tolerance(50)
                .with_divergences(vec!["d1".to_string()])
                .with_ci_tier(CiTier::ExtendedDifferential)
                .with_capability_ids(vec!["cap.x".to_string()]),
        );

        let json = report.to_json();
        let parsed: OracleReport = serde_json::from_str(&json).unwrap();
        let s = &parsed.scenarios[0];
        assert_eq!(s.timing_tolerance_ms, Some(50));
        assert_eq!(s.divergence_ids, vec!["d1"]);
        assert_eq!(s.ci_tier, Some(CiTier::ExtendedDifferential));
        assert_eq!(s.capability_ids, vec!["cap.x"]);
    }
}
