use super::report::CertificationProfile;
use super::scenario::{OracleScenario, ScenarioCategory};

pub const DIFFERENTIAL_GATE: &str = "EGRESS_PPROXY_CERTIFY";
pub const PLATFORM_GATE: &str = "EGRESS_PPROXY_PLATFORM";

pub fn profile_enabled(profile: CertificationProfile) -> bool {
    let var = match profile {
        CertificationProfile::Differential => DIFFERENTIAL_GATE,
        CertificationProfile::Platform => PLATFORM_GATE,
    };
    std::env::var(var).map(|v| v == "1").unwrap_or(false)
}

pub fn certification_profile(scenario: &OracleScenario) -> Option<CertificationProfile> {
    if scenario.platform.requires_root || scenario.platform.required_os.is_some() {
        return Some(CertificationProfile::Platform);
    }

    match scenario.category {
        ScenarioCategory::CliDefaults => None,
        ScenarioCategory::HttpSocksTcp
        | ScenarioCategory::Chains
        | ScenarioCategory::Rules
        | ScenarioCategory::Udp => Some(CertificationProfile::Differential),
    }
}

pub fn assign_profiles(
    scenarios: &[OracleScenario],
) -> Vec<(Option<CertificationProfile>, &OracleScenario)> {
    scenarios
        .iter()
        .map(|s| (certification_profile(s), s))
        .collect()
}

pub fn scenarios_for_profile(
    scenarios: &[OracleScenario],
    profile: CertificationProfile,
) -> Vec<&OracleScenario> {
    scenarios
        .iter()
        .filter(|s| certification_profile(s) == Some(profile))
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

    let structural_count = profiles.iter().filter(|(p, _)| p.is_none()).count();
    summary.push_str(&format!(
        "Structural (ungated): {} scenarios\n",
        structural_count
    ));

    for config in all_profile_configs() {
        let count = profiles
            .iter()
            .filter(|(p, _)| *p == Some(config.profile))
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
    fn profile_classification_cli_defaults() {
        let scenarios =
            super::super::scenario::scenarios_for_category(ScenarioCategory::CliDefaults);
        for s in &scenarios {
            let profile = certification_profile(s);
            assert_eq!(
                profile, None,
                "CLI scenario {} should be unprofiled (structural)",
                s.id
            );
        }
    }

    #[test]
    fn profile_classification_http_socks() {
        let scenarios =
            super::super::scenario::scenarios_for_category(ScenarioCategory::HttpSocksTcp);
        for s in &scenarios {
            let profile = certification_profile(s);
            assert!(
                profile == Some(CertificationProfile::Differential),
                "HTTP/SOCKS scenario {} should be Differential, got {:?}",
                s.id,
                profile
            );
        }
    }

    #[test]
    fn profile_classification_chains() {
        let scenarios = super::super::scenario::scenarios_for_category(ScenarioCategory::Chains);
        for s in &scenarios {
            let profile = certification_profile(s);
            assert!(
                profile == Some(CertificationProfile::Differential),
                "Chain scenario {} should be Differential, got {:?}",
                s.id,
                profile
            );
        }
    }

    #[test]
    fn profile_classification_udp() {
        let scenarios = super::super::scenario::scenarios_for_category(ScenarioCategory::Udp);
        for s in &scenarios {
            let profile = certification_profile(s);
            assert!(
                profile == Some(CertificationProfile::Differential),
                "UDP scenario {} should be Differential, got {:?}",
                s.id,
                profile
            );
        }
    }

    #[test]
    fn profile_gate_defaults() {
        std::env::remove_var(DIFFERENTIAL_GATE);
        std::env::remove_var(PLATFORM_GATE);
        assert!(!profile_enabled(CertificationProfile::Differential));
        assert!(!profile_enabled(CertificationProfile::Platform));
    }

    #[test]
    fn all_profile_configs_complete() {
        let configs = all_profile_configs();
        assert_eq!(configs.len(), 2);
        let mut profiles: Vec<_> = configs.iter().map(|c| c.profile).collect();
        profiles.sort_by_key(|p| format!("{:?}", p));
        profiles.dedup();
        assert_eq!(profiles.len(), 2);
    }

    #[test]
    fn scenarios_for_profile_filtering() {
        let all = super::super::scenario::all_scenarios();
        let differential = scenarios_for_profile(&all, CertificationProfile::Differential);
        assert!(!differential.is_empty());
        for s in &differential {
            assert_eq!(
                certification_profile(s),
                Some(CertificationProfile::Differential)
            );
        }
    }

    #[test]
    fn assign_profiles_returns_all_scenarios() {
        let all = super::super::scenario::all_scenarios();
        let profiles = assign_profiles(&all);
        assert_eq!(profiles.len(), all.len());
    }
}
