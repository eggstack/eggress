use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use super::scenario::{
    EquivalenceTarget, NormalizationRules, OracleScenario, PlatformRequirements, ScenarioCategory,
};

const CURRENT_SCHEMA_VERSION: u32 = 1;

fn default_timeout_secs() -> u64 {
    15
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioFile {
    pub schema_version: u32,
    pub scenarios: Vec<ScenarioDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioDef {
    pub id: String,
    pub capability_ids: Vec<String>,
    pub description: String,
    pub pproxy_args: Vec<String>,
    pub eggress_toml: String,
    pub expected_equivalence: EquivalenceTarget,
    pub category: ScenarioCategory,
    #[serde(default)]
    pub normalization: NormalizationRulesDef,
    #[serde(default)]
    pub platform: PlatformRequirementsDef,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    pub client_action: ClientAction,
    #[serde(default)]
    pub comparison: ComparisonMode,
    #[serde(default)]
    pub expected_divergences: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientAction {
    Socks5TcpConnect,
    HttpConnect,
    HttpForwardGet,
    HttpForwardPost,
    Socks5ConnectRefused,
    Socks5AuthFailure,
    Socks5TcpConnectAuth,
    Socks4Connect,
    Socks4aConnect,
    UdpEchoRoundtrip,
    None,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMode {
    #[default]
    ExactPayload,
    CoarseResult,
    StatusCode,
    BindAddress,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NormalizationRulesDef {
    #[serde(default = "default_true")]
    pub strip_log_prefixes: bool,
    #[serde(default = "default_true")]
    pub normalize_ports: bool,
    #[serde(default)]
    pub normalize_line_endings: bool,
    #[serde(default)]
    pub strip_versions: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NormalizationRulesDef {
    fn default() -> Self {
        Self {
            strip_log_prefixes: true,
            normalize_ports: true,
            normalize_line_endings: false,
            strip_versions: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlatformRequirementsDef {
    #[serde(default)]
    pub requires_root: bool,
    #[serde(default)]
    pub requires_ipv6: bool,
    pub required_os: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScenarioValidationError {
    #[error("unsupported schema version {0} (expected {CURRENT_SCHEMA_VERSION})")]
    UnsupportedSchemaVersion(u32),
    #[error("scenario ID '{0}' is empty")]
    EmptyId(String),
    #[error("duplicate scenario ID: '{0}'")]
    DuplicateId(String),
    #[error("scenario '{0}' has no capability IDs")]
    NoCapabilityIds(String),
    #[error("scenario '{0}' has empty pproxy_args")]
    EmptyPproxyArgs(String),
    #[error("scenario '{0}' has empty eggress_toml")]
    EmptyEggressToml(String),
    #[error("scenario '{0}' has empty description")]
    EmptyDescription(String),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ScenarioValidationErrors = Vec<ScenarioValidationError>;

pub fn validate_scenario_file(file: &ScenarioFile) -> ScenarioValidationErrors {
    let mut errors = Vec::new();

    if file.schema_version != CURRENT_SCHEMA_VERSION {
        errors.push(ScenarioValidationError::UnsupportedSchemaVersion(
            file.schema_version,
        ));
    }

    let mut seen_ids = std::collections::HashSet::new();
    for scenario in &file.scenarios {
        if scenario.id.is_empty() {
            errors.push(ScenarioValidationError::EmptyId("<unnamed>".to_string()));
        } else if !seen_ids.insert(scenario.id.clone()) {
            errors.push(ScenarioValidationError::DuplicateId(scenario.id.clone()));
        }

        if scenario.capability_ids.is_empty() {
            errors.push(ScenarioValidationError::NoCapabilityIds(
                scenario.id.clone(),
            ));
        }

        if scenario.pproxy_args.is_empty() {
            errors.push(ScenarioValidationError::EmptyPproxyArgs(
                scenario.id.clone(),
            ));
        }

        if scenario.eggress_toml.is_empty() {
            errors.push(ScenarioValidationError::EmptyEggressToml(
                scenario.id.clone(),
            ));
        }

        if scenario.description.is_empty() {
            errors.push(ScenarioValidationError::EmptyDescription(
                scenario.id.clone(),
            ));
        }
    }

    errors
}

pub fn load_scenario_string(toml_str: &str) -> Result<ScenarioFile, ScenarioValidationError> {
    let file: ScenarioFile = toml::from_str(toml_str)?;
    let errors = validate_scenario_file(&file);
    if errors.is_empty() {
        Ok(file)
    } else {
        Err(errors.into_iter().next().unwrap())
    }
}

pub async fn load_scenario_file(path: &Path) -> Result<ScenarioFile, ScenarioValidationError> {
    let content = tokio::fs::read_to_string(path).await?;
    load_scenario_string(&content)
}

pub fn scenario_def_to_oracle(def: &ScenarioDef) -> OracleScenario {
    let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };

    OracleScenario {
        id: leak(&def.id),
        capability_ids: def.capability_ids.iter().map(|s| leak(s)).collect(),
        description: leak(&def.description),
        pproxy_args: def.pproxy_args.iter().map(|s| leak(s)).collect(),
        eggress_toml: leak(&def.eggress_toml),
        expected_equivalence: def.expected_equivalence,
        normalization: NormalizationRules {
            strip_log_prefixes: def.normalization.strip_log_prefixes,
            normalize_ports: def.normalization.normalize_ports,
            normalize_line_endings: def.normalization.normalize_line_endings,
            strip_versions: def.normalization.strip_versions,
        },
        platform: PlatformRequirements {
            requires_root: def.platform.requires_root,
            requires_ipv6: def.platform.requires_ipv6,
            requires_python_package: None,
            required_os: def.platform.required_os.as_deref().map(leak),
        },
        timeout: Duration::from_secs(def.timeout_secs),
        category: def.category,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_validation_rejects_bad_version() {
        let file = ScenarioFile {
            schema_version: 999,
            scenarios: vec![],
        };
        let errors = validate_scenario_file(&file);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            ScenarioValidationError::UnsupportedSchemaVersion(999)
        ));
    }

    #[test]
    fn schema_validation_rejects_empty_id() {
        let file = ScenarioFile {
            schema_version: 1,
            scenarios: vec![ScenarioDef {
                id: String::new(),
                capability_ids: vec!["cap1".to_string()],
                description: "test".to_string(),
                pproxy_args: vec!["-l".to_string()],
                eggress_toml: "test".to_string(),
                expected_equivalence: EquivalenceTarget::Payload,
                category: ScenarioCategory::CliDefaults,
                normalization: NormalizationRulesDef::default(),
                platform: PlatformRequirementsDef::default(),
                timeout_secs: 10,
                client_action: ClientAction::None,
                comparison: ComparisonMode::default(),
                expected_divergences: vec![],
            }],
        };
        let errors = validate_scenario_file(&file);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ScenarioValidationError::EmptyId(_))));
    }

    #[test]
    fn schema_validation_rejects_duplicate_ids() {
        let def = ScenarioDef {
            id: "dup".to_string(),
            capability_ids: vec!["cap1".to_string()],
            description: "test".to_string(),
            pproxy_args: vec!["-l".to_string()],
            eggress_toml: "test".to_string(),
            expected_equivalence: EquivalenceTarget::Payload,
            category: ScenarioCategory::CliDefaults,
            normalization: NormalizationRulesDef::default(),
            platform: PlatformRequirementsDef::default(),
            timeout_secs: 10,
            client_action: ClientAction::None,
            comparison: ComparisonMode::default(),
            expected_divergences: vec![],
        };
        let file = ScenarioFile {
            schema_version: 1,
            scenarios: vec![def.clone(), def],
        };
        let errors = validate_scenario_file(&file);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ScenarioValidationError::DuplicateId(s) if s == "dup")));
    }

    #[test]
    fn schema_validation_rejects_empty_capability_ids() {
        let file = ScenarioFile {
            schema_version: 1,
            scenarios: vec![ScenarioDef {
                id: "test".to_string(),
                capability_ids: vec![],
                description: "test".to_string(),
                pproxy_args: vec!["-l".to_string()],
                eggress_toml: "test".to_string(),
                expected_equivalence: EquivalenceTarget::Payload,
                category: ScenarioCategory::CliDefaults,
                normalization: NormalizationRulesDef::default(),
                platform: PlatformRequirementsDef::default(),
                timeout_secs: 10,
                client_action: ClientAction::None,
                comparison: ComparisonMode::default(),
                expected_divergences: vec![],
            }],
        };
        let errors = validate_scenario_file(&file);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ScenarioValidationError::NoCapabilityIds(_))));
    }

    #[test]
    fn schema_validation_accepts_valid_file() {
        let file = ScenarioFile {
            schema_version: 1,
            scenarios: vec![ScenarioDef {
                id: "valid".to_string(),
                capability_ids: vec!["cap1".to_string()],
                description: "a valid scenario".to_string(),
                pproxy_args: vec!["-l".to_string(), "socks5://127.0.0.1:1080".to_string()],
                eggress_toml: "version = 1".to_string(),
                expected_equivalence: EquivalenceTarget::CoarseResult,
                category: ScenarioCategory::HttpSocksTcp,
                normalization: NormalizationRulesDef::default(),
                platform: PlatformRequirementsDef::default(),
                timeout_secs: 10,
                client_action: ClientAction::Socks5TcpConnect,
                comparison: ComparisonMode::default(),
                expected_divergences: vec![],
            }],
        };
        let errors = validate_scenario_file(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn load_minimal_scenario() {
        let toml_str = r#"
schema_version = 1

[[scenarios]]
id = "minimal.test"
capability_ids = ["cap1"]
description = "minimal scenario"
pproxy_args = ["-l", "socks5://127.0.0.1:1080"]
eggress_toml = "version = 1"
expected_equivalence = "coarse_result"
category = "cli_defaults"
timeout_secs = 5
client_action = "none"
"#;
        let file = load_scenario_string(toml_str).unwrap();
        assert_eq!(file.schema_version, 1);
        assert_eq!(file.scenarios.len(), 1);
        assert_eq!(file.scenarios[0].id, "minimal.test");
        assert_eq!(file.scenarios[0].timeout_secs, 5);
        assert_eq!(
            file.scenarios[0].expected_equivalence,
            EquivalenceTarget::CoarseResult
        );
        assert_eq!(file.scenarios[0].category, ScenarioCategory::CliDefaults);
        assert_eq!(file.scenarios[0].client_action, ClientAction::None);
    }

    #[test]
    fn scenario_def_to_oracle_roundtrip() {
        let def = ScenarioDef {
            id: "roundtrip.test".to_string(),
            capability_ids: vec!["cap1".to_string(), "cap2".to_string()],
            description: "roundtrip test".to_string(),
            pproxy_args: vec!["-l".to_string(), "socks5://127.0.0.1:1080".to_string()],
            eggress_toml: "version = 1".to_string(),
            expected_equivalence: EquivalenceTarget::Payload,
            category: ScenarioCategory::Chains,
            normalization: NormalizationRulesDef {
                strip_log_prefixes: true,
                normalize_ports: false,
                normalize_line_endings: true,
                strip_versions: true,
            },
            platform: PlatformRequirementsDef {
                requires_root: true,
                requires_ipv6: false,
                required_os: Some("linux".to_string()),
            },
            timeout_secs: 30,
            client_action: ClientAction::Socks5TcpConnect,
            comparison: ComparisonMode::ExactPayload,
            expected_divergences: vec!["div1".to_string()],
        };

        let oracle = scenario_def_to_oracle(&def);
        assert_eq!(oracle.id, "roundtrip.test");
        assert_eq!(oracle.capability_ids, vec!["cap1", "cap2"]);
        assert_eq!(oracle.description, "roundtrip test");
        assert_eq!(oracle.pproxy_args, vec!["-l", "socks5://127.0.0.1:1080"]);
        assert_eq!(oracle.eggress_toml, "version = 1");
        assert_eq!(oracle.expected_equivalence, EquivalenceTarget::Payload);
        assert_eq!(oracle.category, ScenarioCategory::Chains);
        assert!(oracle.normalization.strip_log_prefixes);
        assert!(!oracle.normalization.normalize_ports);
        assert!(oracle.normalization.normalize_line_endings);
        assert!(oracle.normalization.strip_versions);
        assert!(oracle.platform.requires_root);
        assert!(!oracle.platform.requires_ipv6);
        assert_eq!(oracle.platform.required_os, Some("linux"));
        assert_eq!(oracle.timeout, Duration::from_secs(30));
    }

    #[test]
    fn default_normalization_rules() {
        let rules = NormalizationRulesDef::default();
        assert!(rules.strip_log_prefixes);
        assert!(rules.normalize_ports);
        assert!(!rules.normalize_line_endings);
        assert!(!rules.strip_versions);
    }

    #[test]
    fn default_comparison_mode() {
        assert_eq!(ComparisonMode::default(), ComparisonMode::ExactPayload);
    }

    #[test]
    fn load_scenario_with_defaults() {
        let toml_str = r#"
schema_version = 1

[[scenarios]]
id = "defaults.test"
capability_ids = ["cap1"]
description = "uses all defaults"
pproxy_args = ["-l", "socks5://127.0.0.1:1080"]
eggress_toml = "version = 1"
expected_equivalence = "payload"
category = "udp"
client_action = "udp_echo_roundtrip"
"#;
        let file = load_scenario_string(toml_str).unwrap();
        let scenario = &file.scenarios[0];
        assert_eq!(scenario.timeout_secs, 15);
        assert!(scenario.normalization.strip_log_prefixes);
        assert!(scenario.normalization.normalize_ports);
        assert!(!scenario.normalization.normalize_line_endings);
        assert!(!scenario.normalization.strip_versions);
        assert!(!scenario.platform.requires_root);
        assert!(!scenario.platform.requires_ipv6);
        assert!(scenario.platform.required_os.is_none());
        assert!(scenario.expected_divergences.is_empty());
        assert_eq!(scenario.comparison, ComparisonMode::ExactPayload);
    }

    #[test]
    fn load_real_scenario_files() {
        let scenarios_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/scenarios");
        if !scenarios_dir.exists() {
            return;
        }
        let entries = match std::fs::read_dir(&scenarios_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                    panic!("failed to read {}: {e}", path.display());
                });
                let file = load_scenario_string(&content).unwrap_or_else(|e| {
                    panic!("failed to validate {}: {e}", path.display());
                });
                let errors = validate_scenario_file(&file);
                assert!(
                    errors.is_empty(),
                    "validation errors in {}: {:?}",
                    path.display(),
                    errors
                );
            }
        }
    }
}
