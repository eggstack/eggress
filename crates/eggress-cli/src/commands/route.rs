//! `eggress route`: offline and live route explanation.
//!
//! The command only parses/validates user target/listener/protocol inputs,
//! chooses local vs remote explanation, calls the router or the
//! [`eggress_admin::client`] transport, renders text/JSON, and maps errors
//! to process outcomes. No HTTP mechanics live here.

use eggress_cli::{
    EXIT_CLI_PARSE_ERROR, EXIT_CONFIG_VALIDATION, EXIT_RUNTIME_FAILURE, EXIT_SUCCESS,
};
use eggress_core::ProtocolId;

use crate::cli::{CliContext, RouteExplain, RouteProtocol};

/// Explain the routing decision for a target. Returns the process exit code.
pub async fn handle_route_explain(ctx: &CliContext, args: &RouteExplain) -> i32 {
    if let Some(ref admin_url) = args.admin {
        return handle_route_explain_remote(args, admin_url).await;
    }

    let (router, is_online) = match &ctx.config {
        Some(path) => match eggress_config::compile::load_and_compile(path) {
            Ok(rt) => match build_router_from_config(&rt) {
                Ok(r) => (r, true),
                Err(e) => {
                    eprintln!("failed to build router from config: {e}");
                    return EXIT_CONFIG_VALIDATION;
                }
            },
            Err(e) => {
                eprintln!("failed to load config: {e}");
                return EXIT_CONFIG_VALIDATION;
            }
        },
        None => (
            eggress_routing::Router::new(vec![], eggress_routing::RouteActionSpec::Direct),
            false,
        ),
    };

    let target: eggress_core::TargetAddr = match args.target.parse() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_CLI_PARSE_ERROR;
        }
    };

    let protocol = args
        .protocol
        .map(RouteProtocol::to_protocol_id)
        .unwrap_or(ProtocolId::Http);
    let listener = args.listener.as_deref().unwrap_or("cli");

    let request = eggress_routing::RouteRequest {
        target: &target,
        source: None,
        listener,
        inbound_protocol: protocol,
        identity: &eggress_core::ClientIdentity::Anonymous,
        transport: eggress_routing::TransportKind::Tcp,
    };

    let explanation = router.explain(&request, 0);

    if args.json {
        match serde_json::to_string_pretty(&explanation) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("failed to serialize explanation: {e}");
                return EXIT_RUNTIME_FAILURE;
            }
        }
    } else {
        print_explanation(&explanation, is_online);
    }
    EXIT_SUCCESS
}

/// Explain a route through a live admin server via the shared
/// [`eggress_admin::client`] transport.
async fn handle_route_explain_remote(args: &RouteExplain, admin_url: &str) -> i32 {
    // Validate the target locally so malformed targets keep the CLI-parse
    // exit code instead of surfacing as a remote 400.
    if let Err(e) = args.target.parse::<eggress_core::TargetAddr>() {
        eprintln!("{e}");
        return EXIT_CLI_PARSE_ERROR;
    }
    let protocol = args.protocol.map(RouteProtocol::as_str).unwrap_or("http");
    let listener = args.listener.as_deref().unwrap_or("default");

    match eggress_admin::client::route_explain(admin_url, &args.target, listener, protocol).await {
        Ok(explanation) => {
            if args.json {
                match serde_json::to_string_pretty(&explanation) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        eprintln!("failed to serialize explanation: {e}");
                        return EXIT_RUNTIME_FAILURE;
                    }
                }
            } else {
                print_explanation(&explanation, true);
            }
            EXIT_SUCCESS
        }
        Err(e) => {
            // `AdminClientError` already distinguishes user errors (bad URL)
            // from operational failures; only the exit-code mapping lives
            // here.
            match &e {
                eggress_admin::client::AdminClientError::InvalidUrl { .. } => {
                    eprintln!("{e}");
                    EXIT_CLI_PARSE_ERROR
                }
                _ => {
                    eprintln!("{e}");
                    EXIT_RUNTIME_FAILURE
                }
            }
        }
    }
}

fn print_explanation(explanation: &eggress_routing::RouteExplanation, is_online: bool) {
    println!("Target: {}", explanation.target);
    println!("Listener: {}", explanation.listener);
    println!("Protocol: {}", explanation.protocol);
    if let Some(ref rule) = explanation.matched_rule {
        println!("Matched rule: {rule}");
    }
    println!("Action: {}", explanation.action);
    if let Some(ref group) = explanation.upstream_group {
        println!("Upstream group: {group}");
    }
    if let Some(ref scheduler) = explanation.scheduler {
        println!("Scheduler: {scheduler}");
    }
    if !explanation.eligible_upstreams.is_empty() {
        println!("Eligible upstreams:");
        for u in &explanation.eligible_upstreams {
            println!(
                "  {}  {}  active={}  in_flight={}",
                u.id, u.health, u.active, u.in_flight
            );
        }
    }
    if let Some(ref upstream) = explanation.selected_upstream {
        println!("Selected upstream: {upstream}");
    }
    if let Some(ref chain) = explanation.chain {
        println!("Chain: {chain}");
    }
    if is_online {
        println!("Config generation: {}", explanation.generation);
    } else {
        println!("Mode: offline");
        println!("Generation: not-live");
    }
}

fn build_router_from_config(
    rt: &eggress_config::RuntimeConfig,
) -> Result<eggress_routing::Router, Box<dyn std::error::Error + Send + Sync>> {
    use std::sync::Arc;

    let mut seen_upstream_ids = std::collections::HashSet::new();
    let mut upstreams = Vec::new();

    for u in &rt.upstreams {
        if !seen_upstream_ids.insert(u.id.clone()) {
            return Err(format!("duplicate upstream ID '{}'", u.id).into());
        }
        let id = eggress_routing::UpstreamGroupId(Arc::from(u.id.as_str()));
        let runtime = eggress_routing::upstream::UpstreamRuntime::new(
            eggress_core::UpstreamId::new(u.id.clone()),
            u.chain.clone(),
        );
        upstreams.push((id, runtime));
    }

    let upstream_map: std::collections::HashMap<
        String,
        Arc<eggress_routing::upstream::UpstreamRuntime>,
    > = upstreams
        .into_iter()
        .map(|(id, runtime)| (id.0.to_string(), Arc::new(runtime)))
        .collect();

    let mut seen_group_ids = std::collections::HashSet::new();
    let mut groups = Vec::new();

    for g in &rt.groups {
        if !seen_group_ids.insert(g.id.clone()) {
            return Err(format!("duplicate group ID '{}'", g.id).into());
        }
        let mut members = Vec::new();
        for m in &g.members {
            let member = upstream_map
                .get(m)
                .ok_or_else(|| format!("group '{}' references unknown upstream '{}'", g.id, m))?;
            members.push(member.clone());
        }
        if members.is_empty() {
            return Err(format!("group '{}' has no valid members", g.id).into());
        }

        let fallback = match g.fallback {
            eggress_config::compile::GroupFallback::Reject => {
                eggress_routing::upstream::GroupFallback::Reject
            }
            eggress_config::compile::GroupFallback::Direct => {
                eggress_routing::upstream::GroupFallback::Direct
            }
            eggress_config::compile::GroupFallback::UseUnhealthy => {
                eggress_routing::upstream::GroupFallback::UseUnhealthy
            }
        };

        groups.push((
            g.id.clone(),
            eggress_routing::upstream::UpstreamGroup::new(
                g.id.clone(),
                g.scheduler,
                Arc::from(members),
                fallback,
            ),
        ));
    }

    let group_ids: std::collections::HashSet<_> = groups.iter().map(|(id, _)| id.clone()).collect();

    let mut rules = Vec::new();
    for r in &rt.rules {
        let action = match &r.action {
            eggress_routing::RouteActionSpec::Direct => eggress_routing::RouteActionSpec::Direct,
            eggress_routing::RouteActionSpec::UpstreamGroup(gid) => {
                if !group_ids.contains(gid) {
                    return Err(
                        format!("rule '{}' references unknown group '{}'", r.id, gid).into(),
                    );
                }
                eggress_routing::RouteActionSpec::UpstreamGroup(gid.clone())
            }
            eggress_routing::RouteActionSpec::Reject(reason) => {
                eggress_routing::RouteActionSpec::Reject(reason.clone())
            }
        };
        rules.push(eggress_routing::CompiledRule {
            id: r.id.clone(),
            matcher: r.matcher.clone(),
            action,
        });
    }

    Ok(eggress_routing::Router::with_groups(
        rules,
        rt.default_action.clone(),
        groups,
    ))
}
