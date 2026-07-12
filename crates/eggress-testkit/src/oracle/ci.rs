use super::report::CiTier;
use super::scenario::{OracleScenario, ScenarioCategory};

pub const TIER_FAST_GATE: &str = "EGRESS_ORACLE";
pub const TIER_CORE_GATE: &str = "EGRESS_ORACLE";
pub const TIER_EXTENDED_GATE: &str = "EGRESS_ORACLE_EXTENDED";
pub const TIER_PLATFORM_GATE: &str = "EGRESS_ORACLE_PLATFORM";
pub const TIER_PRIVILEGED_GATE: &str = "EGRESS_ORACLE_PRIVILEGED";

pub fn tier_gate_enabled(tier: CiTier) -> bool {
    let var = match tier {
        CiTier::FastStructural | CiTier::CoreDifferential => TIER_FAST_GATE,
        CiTier::ExtendedDifferential => TIER_EXTENDED_GATE,
        CiTier::PlatformDifferential => TIER_PLATFORM_GATE,
        CiTier::PrivilegedExternal => TIER_PRIVILEGED_GATE,
    };
    std::env::var(var).map(|v| v == "1").unwrap_or(false)
}

pub fn default_tier(scenario: &OracleScenario) -> CiTier {
    match scenario.category {
        ScenarioCategory::CliDefaults => CiTier::FastStructural,
        ScenarioCategory::HttpSocksTcp => CiTier::CoreDifferential,
        ScenarioCategory::Chains => CiTier::CoreDifferential,
        ScenarioCategory::Rules => CiTier::CoreDifferential,
        ScenarioCategory::Udp => CiTier::ExtendedDifferential,
    }
}

pub fn assign_tiers(scenarios: &[OracleScenario]) -> Vec<(CiTier, &OracleScenario)> {
    scenarios
        .iter()
        .map(|s| {
            let mut tier = default_tier(s);

            if s.platform.requires_root || s.platform.required_os.is_some() {
                tier = CiTier::PlatformDifferential;
            }

            if tier == CiTier::FastStructural && s.id.starts_with("ext.") {
                tier = CiTier::ExtendedDifferential;
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
            assigned == tier
                || (tier == CiTier::CoreDifferential && assigned == CiTier::FastStructural)
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
            tier: CiTier::FastStructural,
            gate_var: TIER_FAST_GATE,
            description: "Fast structural tests: schema validation, startup, port binding",
            required: true,
        },
        CiTierConfig {
            tier: CiTier::CoreDifferential,
            gate_var: TIER_CORE_GATE,
            description: "Core differential: HTTP, SOCKS, CLI with pinned pproxy",
            required: true,
        },
        CiTierConfig {
            tier: CiTier::ExtendedDifferential,
            gate_var: TIER_EXTENDED_GATE,
            description: "Extended differential: UDP, TLS, Shadowsocks, Trojan, routing",
            required: false,
        },
        CiTierConfig {
            tier: CiTier::PlatformDifferential,
            gate_var: TIER_PLATFORM_GATE,
            description: "Platform-specific: macOS, Windows, Linux-specific subsets",
            required: false,
        },
        CiTierConfig {
            tier: CiTier::PrivilegedExternal,
            gate_var: TIER_PRIVILEGED_GATE,
            description: "Privileged/external: transparent proxy, packet capture",
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
                CiTier::FastStructural,
                "CLI scenario {} should be FastStructural",
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
                CiTier::CoreDifferential,
                "HTTP/SOCKS scenario {} should be CoreDifferential",
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
                CiTier::CoreDifferential,
                "Chain scenario {} should be CoreDifferential",
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
                CiTier::ExtendedDifferential,
                "UDP scenario {} should be ExtendedDifferential",
                s.id
            );
        }
    }

    #[test]
    fn tier_gate_defaults() {
        std::env::remove_var(TIER_FAST_GATE);
        std::env::remove_var(TIER_EXTENDED_GATE);
        assert!(!tier_gate_enabled(CiTier::FastStructural));
        assert!(!tier_gate_enabled(CiTier::ExtendedDifferential));
    }

    #[test]
    fn all_tier_configs_complete() {
        let configs = all_tier_configs();
        assert_eq!(configs.len(), 5);
        let mut tiers: Vec<_> = configs.iter().map(|c| c.tier).collect();
        tiers.sort_by_key(|t| format!("{:?}", t));
        tiers.dedup();
        assert_eq!(tiers.len(), 5);
    }

    #[test]
    fn scenarios_for_tier_filtering() {
        let all = super::super::scenario::all_scenarios();
        let fast = scenarios_for_tier(&all, CiTier::FastStructural);
        assert!(!fast.is_empty());
        for s in &fast {
            assert_eq!(default_tier(s), CiTier::FastStructural);
        }
    }

    #[test]
    fn assign_tiers_returns_all_scenarios() {
        let all = super::super::scenario::all_scenarios();
        let tiered = assign_tiers(&all);
        assert_eq!(tiered.len(), all.len());
    }
}
