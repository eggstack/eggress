use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParityReport {
    pub eggress_commit: Option<String>,
    pub pproxy_version: String,
    pub os_platform: String,
    pub rust_version: String,
    pub python_version: String,
    pub feature_gates: Vec<String>,
    pub features: Vec<FeatureReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureReport {
    pub feature_id: String,
    pub category: String,
    pub manifest_evidence: String,
    pub tests_executed: Vec<String>,
    pub status: String,
    pub skip_reason: Option<String>,
    pub observed_divergence: Option<String>,
    pub suggested_evidence: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ManifestEntry {
    pub feature_id: String,
    pub category: String,
    pub evidence: String,
    #[serde(default)]
    pub tests: Vec<String>,
}

impl Default for ParityReport {
    fn default() -> Self {
        Self::new()
    }
}

impl ParityReport {
    pub fn new() -> Self {
        let rust_version = Command::new("rustc")
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
            .unwrap_or_else(|| "unknown".to_string());

        let python_version = detect_python_version().unwrap_or_else(|| "unknown".to_string());
        let pproxy_version = detect_pproxy_version().unwrap_or_else(|| "unknown".to_string());

        let os_platform = std::env::consts::OS.to_string();

        Self {
            eggress_commit: detect_eggress_commit(),
            pproxy_version,
            os_platform,
            rust_version,
            python_version,
            feature_gates: Vec::new(),
            features: Vec::new(),
        }
    }

    pub fn add_feature(&mut self, feature: FeatureReport) {
        self.features.push(feature);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialization should not fail")
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# Eggress Parity Report\n\n");

        md.push_str("## Environment\n\n");
        md.push_str("| Field | Value |\n");
        md.push_str("|-------|-------|\n");
        md.push_str(&format!(
            "| eggress commit | {} |\n",
            self.eggress_commit.as_deref().unwrap_or("n/a")
        ));
        md.push_str(&format!("| pproxy version | {} |\n", self.pproxy_version));
        md.push_str(&format!("| OS platform | {} |\n", self.os_platform));
        md.push_str(&format!("| rust version | {} |\n", self.rust_version));
        md.push_str(&format!("| python version | {} |\n", self.python_version));

        if !self.feature_gates.is_empty() {
            md.push_str("\n## Feature Gates\n\n");
            for gate in &self.feature_gates {
                md.push_str(&format!("- `{}`\n", gate));
            }
        }

        md.push_str("\n## Feature Results\n\n");
        md.push_str("| Feature ID | Category | Status | Manifest Evidence | Tests Executed | Skip Reason | Divergence | Suggested Evidence |\n");
        md.push_str("|-----------|----------|--------|-------------------|----------------|-------------|------------|--------------------|\n");

        for f in &self.features {
            let tests = f.tests_executed.join(", ");
            let skip = f.skip_reason.as_deref().unwrap_or("-");
            let divergence = f.observed_divergence.as_deref().unwrap_or("-");
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                f.feature_id,
                f.category,
                f.status,
                f.manifest_evidence,
                tests,
                skip,
                divergence,
                f.suggested_evidence,
            ));
        }

        md
    }

    pub fn write_json(&self, path: &Path) -> std::io::Result<()> {
        fs::write(path, self.to_json())
    }

    pub fn write_markdown(&self, path: &Path) -> std::io::Result<()> {
        fs::write(path, self.to_markdown())
    }
}

pub fn detect_eggress_commit() -> Option<String> {
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

pub fn detect_python_version() -> Option<String> {
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

pub fn detect_pproxy_version() -> Option<String> {
    Command::new("python3")
        .args(["-c", "import pproxy; print(pproxy.__version__)"])
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

#[derive(Debug, Deserialize)]
struct ManifestFile {
    #[serde(default)]
    features: Vec<ManifestEntry>,
}

pub fn load_manifest(path: &Path) -> std::io::Result<Vec<ManifestEntry>> {
    let content = fs::read_to_string(path)?;
    let manifest: ManifestFile = toml::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(manifest.features)
}

pub fn manifest_entry_to_feature(entry: &ManifestEntry) -> FeatureReport {
    FeatureReport {
        feature_id: entry.feature_id.clone(),
        category: entry.category.clone(),
        manifest_evidence: entry.evidence.clone(),
        tests_executed: entry.tests.clone(),
        status: "skip".to_string(),
        skip_reason: None,
        observed_divergence: None,
        suggested_evidence: entry.evidence.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn sample_report() -> ParityReport {
        ParityReport {
            eggress_commit: Some("abc1234".to_string()),
            pproxy_version: "1.1.4".to_string(),
            os_platform: "macos".to_string(),
            rust_version: "rustc 1.75.0 (82e1608df 2023-12-21)".to_string(),
            python_version: "Python 3.12.0".to_string(),
            feature_gates: vec!["EGRESS_REQUIRE_SHADOWSOCKS_INTEROP".to_string()],
            features: vec![
                FeatureReport {
                    feature_id: "socks5_tcp".to_string(),
                    category: "protocol".to_string(),
                    manifest_evidence: "SOCKS5 supported".to_string(),
                    tests_executed: vec!["test_socks5_connect".to_string()],
                    status: "pass".to_string(),
                    skip_reason: None,
                    observed_divergence: None,
                    suggested_evidence: "SOCKS5 supported".to_string(),
                },
                FeatureReport {
                    feature_id: "shadowsocks_udp".to_string(),
                    category: "protocol".to_string(),
                    manifest_evidence: "SS UDP supported".to_string(),
                    tests_executed: vec![],
                    status: "skip".to_string(),
                    skip_reason: Some("pproxy not available".to_string()),
                    observed_divergence: None,
                    suggested_evidence: "SS UDP supported".to_string(),
                },
            ],
        }
    }

    #[test]
    fn json_round_trip() {
        let report = sample_report();
        let json = report.to_json();
        let parsed: ParityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, report);
    }

    #[test]
    fn markdown_generation() {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(md.contains("# Eggress Parity Report"));
        assert!(md.contains("| eggress commit | abc1234 |"));
        assert!(md.contains("| pproxy version | 1.1.4 |"));
        assert!(md.contains("| rust version | rustc 1.75.0 (82e1608df 2023-12-21) |"));
        assert!(md.contains("| python version | Python 3.12.0 |"));
        assert!(md.contains("| socks5_tcp | protocol | pass |"));
        assert!(md.contains("| shadowsocks_udp | protocol | skip |"));
        assert!(md.contains("EGRESS_REQUIRE_SHADOWSOCKS_INTEROP"));
    }

    #[test]
    fn manifest_loading() {
        let toml_content = r#"
[[features]]
feature_id = "test_feature"
category = "test_cat"
evidence = "test evidence"
tests = ["test_a", "test_b"]
"#;
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), toml_content).unwrap();

        let entries = load_manifest(file.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].feature_id, "test_feature");
        assert_eq!(entries[0].category, "test_cat");
        assert_eq!(entries[0].evidence, "test evidence");
        assert_eq!(entries[0].tests, vec!["test_a", "test_b"]);
    }

    #[test]
    fn manifest_entry_to_feature_conversion() {
        let entry = ManifestEntry {
            feature_id: "conv_test".to_string(),
            category: "convert".to_string(),
            evidence: "some evidence".to_string(),
            tests: vec!["t1".to_string()],
        };
        let feature = manifest_entry_to_feature(&entry);
        assert_eq!(feature.feature_id, "conv_test");
        assert_eq!(feature.status, "skip");
        assert_eq!(feature.manifest_evidence, "some evidence");
        assert_eq!(feature.suggested_evidence, "some evidence");
        assert!(feature.skip_reason.is_none());
    }

    #[test]
    fn manifest_loading_empty_array() {
        let toml_content = "# empty\n";
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), toml_content).unwrap();

        let entries = load_manifest(file.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn markdown_with_no_features() {
        let report = ParityReport {
            eggress_commit: None,
            pproxy_version: "unknown".to_string(),
            os_platform: "linux".to_string(),
            rust_version: "unknown".to_string(),
            python_version: "unknown".to_string(),
            feature_gates: vec![],
            features: vec![],
        };
        let md = report.to_markdown();
        assert!(md.contains("| eggress commit | n/a |"));
        assert!(md.contains("| pproxy version | unknown |"));
        assert!(md.contains("| Feature ID | Category | Status"));
    }

    #[test]
    fn write_json_to_file() {
        let report = sample_report();
        let file = NamedTempFile::new().unwrap();
        report.write_json(file.path()).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        let parsed: ParityReport = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed, report);
    }

    #[test]
    fn write_markdown_to_file() {
        let report = sample_report();
        let file = NamedTempFile::new().unwrap();
        report.write_markdown(file.path()).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("# Eggress Parity Report"));
        assert!(content.contains("socks5_tcp"));
    }
}
