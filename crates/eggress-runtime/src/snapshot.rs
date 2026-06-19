use std::collections::HashMap;
use std::sync::Arc;

use eggress_config::compile::{
    AdminConfig, GroupFallback, ListenerConfig, RuntimeConfig, UpstreamConfig,
};
use eggress_routing::upstream::{UpstreamGroup, UpstreamRuntime};
use eggress_routing::{RouteActionSpec, Router};

pub struct CompiledRuntimeSnapshot {
    pub generation: u64,
    pub upstreams: HashMap<String, Arc<UpstreamRuntime>>,
    pub router: Arc<Router>,
    pub health_config: eggress_routing::health::HealthConfig,
    pub listeners: Vec<ListenerConfig>,
    pub admin: Option<AdminConfig>,
}

/// Check whether an existing `UpstreamRuntime` is compatible with a new config,
/// meaning its chain specification hasn't changed and we can reuse the Arc.
fn upstream_runtime_compatible(old: &UpstreamRuntime, new: &UpstreamConfig) -> bool {
    *old.chain == new.chain && old.health_config == new.health
}

/// Build a `CompiledRuntimeSnapshot` from a `RuntimeConfig`.
///
/// Upstream runtimes are created first and shared with groups/router so that
/// the same `Arc<UpstreamRuntime>` objects are used for health probing and routing.
pub fn compile_runtime_snapshot(
    rt: &RuntimeConfig,
    previous: Option<&CompiledRuntimeSnapshot>,
) -> Result<CompiledRuntimeSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    let empty_map = HashMap::new();
    let previous_upstreams = previous.map(|p| &p.upstreams).unwrap_or(&empty_map);

    let mut upstreams: HashMap<String, Arc<UpstreamRuntime>> = HashMap::new();

    for u in &rt.upstreams {
        let runtime = if let Some(existing) = previous_upstreams.get(&u.id) {
            if upstream_runtime_compatible(existing, u) {
                existing.clone()
            } else {
                build_one_upstream_runtime(u)
            }
        } else {
            build_one_upstream_runtime(u)
        };
        upstreams.insert(u.id.clone(), runtime);
    }

    let group_ids: std::collections::HashSet<_> = rt.groups.iter().map(|g| g.id.clone()).collect();

    let mut groups = Vec::new();
    for g in &rt.groups {
        let mut members = Vec::new();
        for m in &g.members {
            let member = upstreams
                .get(m)
                .ok_or_else(|| format!("group '{}' references unknown upstream '{}'", g.id, m))?;
            members.push(member.clone());
        }
        if members.is_empty() {
            return Err(format!("group '{}' has no valid members", g.id).into());
        }

        let fallback = match g.fallback {
            GroupFallback::Reject => eggress_routing::upstream::GroupFallback::Reject,
            GroupFallback::Direct => eggress_routing::upstream::GroupFallback::Direct,
            GroupFallback::UseUnhealthy => eggress_routing::upstream::GroupFallback::UseUnhealthy,
        };

        groups.push((
            g.id.clone(),
            UpstreamGroup::new(g.id.clone(), g.scheduler, Arc::from(members), fallback),
        ));
    }

    let mut rules = Vec::new();
    for r in &rt.rules {
        let action = match &r.action {
            RouteActionSpec::Direct => RouteActionSpec::Direct,
            RouteActionSpec::UpstreamGroup(gid) => {
                if !group_ids.contains(gid) {
                    return Err(
                        format!("rule '{}' references unknown group '{}'", r.id, gid).into(),
                    );
                }
                RouteActionSpec::UpstreamGroup(gid.clone())
            }
            RouteActionSpec::Reject(reason) => RouteActionSpec::Reject(reason.clone()),
        };
        rules.push(eggress_routing::CompiledRule {
            id: r.id.clone(),
            matcher: r.matcher.clone(),
            action,
        });
    }

    let router = Router::with_groups(rules, rt.default_action.clone(), groups);
    let gen = previous.map(|p| p.generation + 1).unwrap_or(0);

    Ok(CompiledRuntimeSnapshot {
        generation: gen,
        upstreams,
        router: Arc::new(router),
        health_config: eggress_routing::health::HealthConfig::default(),
        listeners: rt.listeners.clone(),
        admin: rt.admin.clone(),
    })
}

fn build_one_upstream_runtime(u: &UpstreamConfig) -> Arc<UpstreamRuntime> {
    let id = eggress_core::UpstreamId::new(u.id.clone());
    let mut runtime =
        UpstreamRuntime::new(id, u.chain.clone()).with_health_config(u.health.clone());

    if let Some(first_hop) = u.chain.hops.first() {
        let addr: Result<std::net::SocketAddr, _> =
            format!("{}:{}", first_hop.endpoint.host, first_hop.endpoint.port).parse();
        if let Ok(addr) = addr {
            runtime = runtime.with_health_probe(eggress_routing::health::HealthProbe::TcpConnect {
                target: addr,
                timeout: u.health.timeout,
            });
        }
    }

    Arc::new(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_config::compile::{
        GroupFallback, ProcessConfig, RuntimeConfig, TimeoutConfig, UpstreamConfig,
    };
    use eggress_routing::scheduler::SchedulerKind;
    use eggress_routing::UpstreamGroupId;
    use eggress_uri::ProxyChainSpec;

    fn default_health() -> eggress_routing::health::HealthConfig {
        eggress_routing::health::HealthConfig::default()
    }

    fn empty_config() -> RuntimeConfig {
        RuntimeConfig {
            process: ProcessConfig::default(),
            timeouts: TimeoutConfig::default(),
            listeners: vec![],
            upstreams: vec![],
            groups: vec![],
            rules: vec![],
            default_action: RouteActionSpec::Direct,
            admin: None,
        }
    }

    #[test]
    fn snapshot_empty_config() {
        let snap = compile_runtime_snapshot(&empty_config(), None).unwrap();
        assert_eq!(snap.generation, 0);
        assert!(snap.upstreams.is_empty());
        assert!(snap.router.rules().is_empty());
    }

    #[test]
    fn snapshot_single_upstream() {
        let mut cfg = empty_config();
        cfg.upstreams = vec![UpstreamConfig {
            id: "proxy1".to_string(),
            chain: ProxyChainSpec { hops: vec![] },
            health: default_health(),
        }];
        let snap = compile_runtime_snapshot(&cfg, None).unwrap();
        assert_eq!(snap.upstreams.len(), 1);
        assert!(snap.upstreams.contains_key("proxy1"));
    }

    #[test]
    fn snapshot_group_uses_shared_upstream_arc() {
        let mut cfg = empty_config();
        cfg.upstreams = vec![UpstreamConfig {
            id: "proxy1".to_string(),
            chain: ProxyChainSpec { hops: vec![] },
            health: default_health(),
        }];
        cfg.groups = vec![eggress_config::compile::UpstreamGroupConfig {
            id: UpstreamGroupId(Arc::from("main")),
            scheduler: SchedulerKind::RoundRobin,
            members: vec!["proxy1".to_string()],
            fallback: GroupFallback::Reject,
        }];
        let snap = compile_runtime_snapshot(&cfg, None).unwrap();
        let upstream_arc = snap.upstreams.get("proxy1").unwrap();
        let group = snap
            .router
            .groups()
            .get(&UpstreamGroupId(Arc::from("main")))
            .unwrap();
        let group_member = &group.members[0];
        assert!(Arc::ptr_eq(upstream_arc, group_member));
    }

    #[test]
    fn unchanged_upstream_retains_arc_identity_after_reload() {
        let mut cfg = empty_config();
        cfg.upstreams = vec![UpstreamConfig {
            id: "proxy1".to_string(),
            chain: ProxyChainSpec { hops: vec![] },
            health: default_health(),
        }];
        let snap1 = compile_runtime_snapshot(&cfg, None).unwrap();
        let original_arc = snap1.upstreams.get("proxy1").unwrap().clone();

        let snap2 = compile_runtime_snapshot(&cfg, Some(&snap1)).unwrap();
        let reused_arc = snap2.upstreams.get("proxy1").unwrap().clone();

        assert!(Arc::ptr_eq(&original_arc, &reused_arc));
    }

    #[test]
    fn changed_upstream_gets_fresh_arc() {
        let mut cfg1 = empty_config();
        cfg1.upstreams = vec![UpstreamConfig {
            id: "proxy1".to_string(),
            chain: ProxyChainSpec { hops: vec![] },
            health: default_health(),
        }];
        let snap1 = compile_runtime_snapshot(&cfg1, None).unwrap();
        let original_arc = snap1.upstreams.get("proxy1").unwrap().clone();

        let mut cfg2 = empty_config();
        cfg2.upstreams = vec![UpstreamConfig {
            id: "proxy1".to_string(),
            chain: ProxyChainSpec {
                hops: vec![eggress_uri::ProxyHopSpec {
                    protocols: vec![eggress_uri::ProtocolSpec::Socks5],
                    endpoint: eggress_uri::EndpointSpec {
                        host: "newhost".to_string(),
                        port: 1080,
                    },
                    credentials: None,
                    rule: None,
                    local_bind: None,
                }],
            },
            health: default_health(),
        }];
        let snap2 = compile_runtime_snapshot(&cfg2, Some(&snap1)).unwrap();
        let new_arc = snap2.upstreams.get("proxy1").unwrap().clone();

        assert!(!Arc::ptr_eq(&original_arc, &new_arc));
    }

    #[test]
    fn no_duplicate_upstream_runtime_objects_for_one_id() {
        let mut cfg = empty_config();
        cfg.upstreams = vec![UpstreamConfig {
            id: "proxy1".to_string(),
            chain: ProxyChainSpec { hops: vec![] },
            health: default_health(),
        }];
        cfg.groups = vec![eggress_config::compile::UpstreamGroupConfig {
            id: UpstreamGroupId(Arc::from("main")),
            scheduler: SchedulerKind::RoundRobin,
            members: vec!["proxy1".to_string()],
            fallback: GroupFallback::Reject,
        }];
        let snap = compile_runtime_snapshot(&cfg, None).unwrap();
        let upstream_arc = snap.upstreams.get("proxy1").unwrap();
        let group = snap
            .router
            .groups()
            .get(&UpstreamGroupId(Arc::from("main")))
            .unwrap();
        let group_member = &group.members[0];

        assert!(Arc::ptr_eq(upstream_arc, group_member));
    }

    #[test]
    fn generation_increments_on_reload() {
        let cfg = empty_config();
        let snap1 = compile_runtime_snapshot(&cfg, None).unwrap();
        assert_eq!(snap1.generation, 0);
        let snap2 = compile_runtime_snapshot(&cfg, Some(&snap1)).unwrap();
        assert_eq!(snap2.generation, 1);
        let snap3 = compile_runtime_snapshot(&cfg, Some(&snap2)).unwrap();
        assert_eq!(snap3.generation, 2);
    }

    #[test]
    fn group_references_unknown_upstream() {
        let mut cfg = empty_config();
        cfg.groups = vec![eggress_config::compile::UpstreamGroupConfig {
            id: UpstreamGroupId(Arc::from("main")),
            scheduler: SchedulerKind::RoundRobin,
            members: vec!["nonexistent".to_string()],
            fallback: GroupFallback::Reject,
        }];
        let result = compile_runtime_snapshot(&cfg, None);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("nonexistent"));
    }

    #[test]
    fn rule_references_unknown_group() {
        let mut cfg = empty_config();
        cfg.rules = vec![eggress_routing::CompiledRule {
            id: eggress_routing::RuleId(Arc::from("r1")),
            matcher: eggress_routing::MatchExpr::Any,
            action: RouteActionSpec::UpstreamGroup(UpstreamGroupId(Arc::from("missing"))),
        }];
        let result = compile_runtime_snapshot(&cfg, None);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("missing"));
    }

    #[test]
    fn multiple_upstreams_all_shared() {
        let mut cfg = empty_config();
        cfg.upstreams = vec![
            UpstreamConfig {
                id: "p1".to_string(),
                chain: ProxyChainSpec { hops: vec![] },
                health: default_health(),
            },
            UpstreamConfig {
                id: "p2".to_string(),
                chain: ProxyChainSpec { hops: vec![] },
                health: default_health(),
            },
        ];
        cfg.groups = vec![eggress_config::compile::UpstreamGroupConfig {
            id: UpstreamGroupId(Arc::from("grp")),
            scheduler: SchedulerKind::RoundRobin,
            members: vec!["p1".to_string(), "p2".to_string()],
            fallback: GroupFallback::Reject,
        }];
        let snap = compile_runtime_snapshot(&cfg, None).unwrap();
        let group = snap
            .router
            .groups()
            .get(&UpstreamGroupId(Arc::from("grp")))
            .unwrap();
        assert!(Arc::ptr_eq(
            snap.upstreams.get("p1").unwrap(),
            &group.members[0]
        ));
        assert!(Arc::ptr_eq(
            snap.upstreams.get("p2").unwrap(),
            &group.members[1]
        ));
    }

    #[test]
    fn partial_upstream_change_preserves_others() {
        let mut cfg1 = empty_config();
        cfg1.upstreams = vec![
            UpstreamConfig {
                id: "p1".to_string(),
                chain: ProxyChainSpec { hops: vec![] },
                health: default_health(),
            },
            UpstreamConfig {
                id: "p2".to_string(),
                chain: ProxyChainSpec { hops: vec![] },
                health: default_health(),
            },
        ];
        let snap1 = compile_runtime_snapshot(&cfg1, None).unwrap();
        let p1_original = snap1.upstreams.get("p1").unwrap().clone();
        let p2_original = snap1.upstreams.get("p2").unwrap().clone();

        let mut cfg2 = empty_config();
        cfg2.upstreams = vec![
            UpstreamConfig {
                id: "p1".to_string(),
                chain: ProxyChainSpec { hops: vec![] },
                health: default_health(),
            },
            UpstreamConfig {
                id: "p2".to_string(),
                chain: ProxyChainSpec {
                    hops: vec![eggress_uri::ProxyHopSpec {
                        protocols: vec![eggress_uri::ProtocolSpec::Http],
                        endpoint: eggress_uri::EndpointSpec {
                            host: "changed".to_string(),
                            port: 8080,
                        },
                        credentials: None,
                        rule: None,
                        local_bind: None,
                    }],
                },
                health: default_health(),
            },
        ];
        let snap2 = compile_runtime_snapshot(&cfg2, Some(&snap1)).unwrap();

        assert!(Arc::ptr_eq(
            &p1_original,
            snap2.upstreams.get("p1").unwrap()
        ));
        assert!(!Arc::ptr_eq(
            &p2_original,
            snap2.upstreams.get("p2").unwrap()
        ));
    }
}
