use super::report::CertificationProfile;
use super::scenario::{OracleScenario, ScenarioCategory};

pub const STRUCTURAL_GATE: &str = "EGRESS_ORACLE";
pub const DIFFERENTIAL_GATE: &str = "EGRESS_ORACLE_EXTENDED";
pub const PLATFORM_GATE: &str = "EGRESS_ORACLE_PLATFORM";

pub fn profile_enabled(profile: CertificationProfile) -> bool {
    let var = match profile {
        CertificationProfile::Structural => STRUCTURAL_GATE,
        CertificationProfile::Differential => DIFFERENTIAL_GATE,
        CertificationProfile::Platform => PLATFORM_GATE,
    };
    std::env::var(var).map(|v| v == "1").unwrap_or(false)
}

pub fn default_profile(scenario: &OracleScenario) -> CertificationProfile {
    match scenario.category {
        ScenarioCategory::CliDefaults => CertificationProfile::Structural,
        ScenarioCategory::HttpSocksTcp => CertificationProfile::Differential,
        ScenarioCategory::Chains => CertificationProfile::Differential,
        ScenarioCategory::Rules => CertificationProfile::Differential,
        ScenarioCategory::Udp => CertificationProfile::Differential,
    }
}

pub fn assign_profiles(
    scenarios: &[OracleScenario],
) -> Vec<(CertificationProfile, &OracleScenario)> {
    scenarios
        .iter()
        .map(|s| {
            let mut profile = default_profile(s);

            if s.platform.requires_root || s.platform.required_os.is_some() {
                profile = CertificationProfile::Platform;
            }

            if profile == CertificationProfile::Structural && s.id.starts_with("ext.") {
                profile = CertificationProfile::Differential;
            }

            (profile, s)
        })
        .collect()
}

pub fn scenarios_for_profile(
    scenarios: &[OracleScenario],
    profile: CertificationProfile,
) -> Vec<&OracleScenario> {
    scenarios
        .iter()
        .filter(|s| {
            let assigned = default_profile(s);
            assigned == profile
                || (profile == CertificationProfile::Differential
                    && assigned == CertificationProfile::Structural)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct CertificationProfileConfig {
    pub profile: CertificationProfile,
    pub gate_var: &'static str,
    pub description: &'static str,
    pub required: bool,
}

pub fn all_profile_configs() -> Vec<CertificationProfileConfig> {
    vec![
        CertificationProfileConfig {
            profile: CertificationProfile::Structural,
            gate_var: STRUCTURAL_GATE,
            description: "Structural: schema validation, startup, port binding",
            required: true,
        },
        CertificationProfileConfig {
            profile: CertificationProfile::Differential,
            gate_var: DIFFERENTIAL_GATE,
            description: "Differential: HTTP, SOCKS, CLI, UDP with pinned pproxy",
            required: true,
        },
        CertificationProfileConfig {
            profile: CertificationProfile::Platform,
            gate_var: PLATFORM_GATE,
            description: "Platform-specific: root, OS-specific subsets",
            required: false,
        },
    ]
}

pub fn generate_profile_summary(scenarios: &[OracleScenario]) -> String {
    let profiles = assign_profiles(scenarios);
    let mut summary = String::new();

    for config in all_profile_configs() {
        let count = profiles
            .iter()
            .filter(|(p, _)| *p == config.profile)
            .count();
        summary.push_str(&format!(
            "{}: {} scenarios (gate: {})\n",
            config.description, count, config.gate_var
        ));
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_assignment_cli_defaults() {
        let scenarios =
            super::super::scenario::scenarios_for_category(ScenarioCategory::CliDefaults);
        for s in &scenarios {
            let profile = default_profile(s);
            assert_eq!(
                profile,
                CertificationProfile::Structural,
                "CLI scenario {} should be Structural",
                s.id
            );
        }
    }

    #[test]
    fn profile_assignment_http_socks() {
        let scenarios =
            super::super::scenario::scenarios_for_category(ScenarioCategory::HttpSocksTcp);
        for s in &scenarios {
            let profile = default_profile(s);
            assert_eq!(
                profile,
                CertificationProfile::Differential,
                "HTTP/SOCKS scenario {} should be Differential",
                s.id
            );
        }
    }

    #[test]
    fn profile_assignment_chains() {
        let scenarios = super::super::scenario::scenarios_for_category(ScenarioCategory::Chains);
        for s in &scenarios {
            let profile = default_profile(s);
            assert_eq!(
                profile,
                CertificationProfile::Differential,
                "Chain scenario {} should be Differential",
                s.id
            );
        }
    }

    #[test]
    fn profile_assignment_udp() {
        let scenarios = super::super::scenario::scenarios_for_category(ScenarioCategory::Udp);
        for s in &scenarios {
            let profile = default_profile(s);
            assert_eq!(
                profile,
                CertificationProfile::Differential,
                "UDP scenario {} should be Differential",
                s.id
            );
        }
    }

    #[test]
    fn profile_gate_defaults() {
        std::env::remove_var(STRUCTURAL_GATE);
        std::env::remove_var(DIFFERENTIAL_GATE);
        assert!(!profile_enabled(CertificationProfile::Structural));
        assert!(!profile_enabled(CertificationProfile::Differential));
    }

    #[test]
    fn all_profile_configs_complete() {
        let configs = all_profile_configs();
        assert_eq!(configs.len(), 3);
        let mut profiles: Vec<_> = configs.iter().map(|c| c.profile).collect();
        profiles.sort_by_key(|p| format!("{:?}", p));
        profiles.dedup();
        assert_eq!(profiles.len(), 3);
    }

    #[test]
    fn scenarios_for_profile_filtering() {
        let all = super::super::scenario::all_scenarios();
        let structural = scenarios_for_profile(&all, CertificationProfile::Structural);
        assert!(!structural.is_empty());
        for s in &structural {
            assert_eq!(default_profile(s), CertificationProfile::Structural);
        }
    }

    #[test]
    fn assign_profiles_returns_all_scenarios() {
        let all = super::super::scenario::all_scenarios();
        let profiles = assign_profiles(&all);
        assert_eq!(profiles.len(), all.len());
    }
}
