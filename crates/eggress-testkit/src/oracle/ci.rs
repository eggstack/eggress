use super::report::CiTier;
use super::scenario::{OracleScenario, ScenarioCategory};

pub const STRUCTURAL_GATE: &str = "EGRESS_ORACLE";
pub const DIFFERENTIAL_GATE: &str = "EGRESS_ORACLE_EXTENDED";
pub const PLATFORM_GATE: &str = "EGRESS_ORACLE_PLATFORM";

pub fn tier_gate_enabled(tier: CiTier) -> bool {
    let var = match tier {
        CiTier::Structural => STRUCTURAL_GATE,
        CiTier::Differential => DIFFERENTIAL_GATE,
        CiTier::Platform => PLATFORM_GATE,
    };
    std::env::var(var).map(|v| v == "1").unwrap_or(false)
}

pub fn default_tier(scenario: &OracleScenario) -> CiTier {
    match scenario.category {
        ScenarioCategory::CliDefaults => CiTier::Structural,
        ScenarioCategory::HttpSocksTcp => CiTier::Differential,
        ScenarioCategory::Chains => CiTier::Differential,
        ScenarioCategory::Rules => CiTier::Differential,
        ScenarioCategory::Udp => CiTier::Differential,
    }
}

pub fn assign_tiers(scenarios: &[OracleScenario]) -> Vec<(CiTier, &OracleScenario)> {
    scenarios
        .iter()
        .map(|s| {
            let mut tier = default_tier(s);

            if s.platform.requires_root || s.platform.required_os.is_some() {
                tier = CiTier::Platform;
            }

            if tier == CiTier::Structural && s.id.starts_with("ext.") {
                tier = CiTier::Differential;
            }

            (tier, s)
        })
        .collect()
}

pub fn scenarios_for_tier(scenarios: &[OracleScenario], tier: CiTier) -> Vec<&OracleScenario> {
    scenarios
        .iter()
        .filter(|s| {
            let assigned = default_tier(s);
            assigned == tier || (tier == CiTier::Differential && assigned == CiTier::Structural)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct CiTierConfig {
    pub tier: CiTier,
    pub gate_var: &'static str,
    pub description: &'static str,
    pub required: bool,
}

pub fn all_tier_configs() -> Vec<CiTierConfig> {
    vec![
        CiTierConfig {
            tier: CiTier::Structural,
            gate_var: STRUCTURAL_GATE,
            description: "Structural: schema validation, startup, port binding",
            required: true,
        },
        CiTierConfig {
            tier: CiTier::Differential,
            gate_var: DIFFERENTIAL_GATE,
            description: "Differential: HTTP, SOCKS, CLI, UDP with pinned pproxy",
            required: true,
        },
        CiTierConfig {
            tier: CiTier::Platform,
            gate_var: PLATFORM_GATE,
            description: "Platform-specific: root, OS-specific subsets",
            required: false,
        },
    ]
}

pub fn generate_ci_summary(scenarios: &[OracleScenario]) -> String {
    let tiered = assign_tiers(scenarios);
    let mut summary = String::new();

    for config in all_tier_configs() {
        let count = tiered
            .iter()
            .filter(|(tier, _)| *tier == config.tier)
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
    fn tier_assignment_cli_defaults() {
        let scenarios =
            super::super::scenario::scenarios_for_category(ScenarioCategory::CliDefaults);
        for s in &scenarios {
            let tier = default_tier(s);
            assert_eq!(
                tier,
                CiTier::Structural,
                "CLI scenario {} should be Structural",
                s.id
            );
        }
    }

    #[test]
    fn tier_assignment_http_socks() {
        let scenarios =
            super::super::scenario::scenarios_for_category(ScenarioCategory::HttpSocksTcp);
        for s in &scenarios {
            let tier = default_tier(s);
            assert_eq!(
                tier,
                CiTier::Differential,
                "HTTP/SOCKS scenario {} should be Differential",
                s.id
            );
        }
    }

    #[test]
    fn tier_assignment_chains() {
        let scenarios = super::super::scenario::scenarios_for_category(ScenarioCategory::Chains);
        for s in &scenarios {
            let tier = default_tier(s);
            assert_eq!(
                tier,
                CiTier::Differential,
                "Chain scenario {} should be Differential",
                s.id
            );
        }
    }

    #[test]
    fn tier_assignment_udp() {
        let scenarios = super::super::scenario::scenarios_for_category(ScenarioCategory::Udp);
        for s in &scenarios {
            let tier = default_tier(s);
            assert_eq!(
                tier,
                CiTier::Differential,
                "UDP scenario {} should be Differential",
                s.id
            );
        }
    }

    #[test]
    fn tier_gate_defaults() {
        std::env::remove_var(STRUCTURAL_GATE);
        std::env::remove_var(DIFFERENTIAL_GATE);
        assert!(!tier_gate_enabled(CiTier::Structural));
        assert!(!tier_gate_enabled(CiTier::Differential));
    }

    #[test]
    fn all_tier_configs_complete() {
        let configs = all_tier_configs();
        assert_eq!(configs.len(), 3);
        let mut tiers: Vec<_> = configs.iter().map(|c| c.tier).collect();
        tiers.sort_by_key(|t| format!("{:?}", t));
        tiers.dedup();
        assert_eq!(tiers.len(), 3);
    }

    #[test]
    fn scenarios_for_tier_filtering() {
        let all = super::super::scenario::all_scenarios();
        let structural = scenarios_for_tier(&all, CiTier::Structural);
        assert!(!structural.is_empty());
        for s in &structural {
            assert_eq!(default_tier(s), CiTier::Structural);
        }
    }

    #[test]
    fn assign_tiers_returns_all_scenarios() {
        let all = super::super::scenario::all_scenarios();
        let tiered = assign_tiers(&all);
        assert_eq!(tiered.len(), all.len());
    }
}
