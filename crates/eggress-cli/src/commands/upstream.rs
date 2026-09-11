//! `eggress upstream test`: upstream diagnostics over the production
//! connector path.
//!
//! The probe executes through [`eggress_cli::run_upstream_test_with_mode`],
//! which traverses the compiled upstream chain with production connector
//! behavior for `proxy` mode. Only result formatting lives here.

use std::time::Duration;

use eggress_cli::{EXIT_CLI_PARSE_ERROR, EXIT_CONFIG_VALIDATION};

use crate::cli::{CliContext, UpstreamTest};

/// Run the upstream diagnostic and return the process exit code.
pub fn handle_upstream_test(ctx: &CliContext, args: &UpstreamTest) -> i32 {
    let Some(ref path) = ctx.config else {
        eprintln!("--config is required for upstream test");
        return EXIT_CLI_PARSE_ERROR;
    };
    let rt = match eggress_config::compile::load_and_compile(path) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to load config: {e}");
            return EXIT_CONFIG_VALIDATION;
        }
    };

    // Filter upstreams by --id if provided
    let rt = if let Some(ref id) = args.id {
        let mut filtered = rt;
        filtered.upstreams.retain(|u| &u.id == id);
        filtered
    } else {
        rt
    };

    if rt.upstreams.is_empty() {
        eprintln!("no upstreams found matching criteria");
        return EXIT_CONFIG_VALIDATION;
    }

    let timeout = Duration::from_secs(args.timeout);
    eggress_cli::run_upstream_test_with_mode(
        &rt,
        args.target.as_deref(),
        args.mode.as_str(),
        timeout,
        args.json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_upstream_test_reachable() {
        let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

        let result = eggress_cli::test_upstream_tcp(
            &echo_addr.ip().to_string(),
            echo_addr.port(),
            Duration::from_secs(5),
        )
        .await;

        assert!(result.reachable);
        assert!(result.latency_ms.is_some());
        assert!(result.error.is_none());

        echo_jh.abort();
    }

    #[tokio::test]
    async fn test_upstream_test_unreachable() {
        let result = eggress_cli::test_upstream_tcp("127.0.0.1", 1, Duration::from_secs(1)).await;

        assert!(!result.reachable);
        assert!(result.latency_ms.is_none());
        assert!(result.error.is_some());
    }

    #[test]
    fn test_upstream_test_json_output() {
        let result = eggress_cli::UpstreamTestResult {
            id: "test-upstream".to_string(),
            host: "127.0.0.1".to_string(),
            port: 1080,
            target: "example.com:443".to_string(),
            mode: "tcp".to_string(),
            reachable: true,
            latency_ms: Some(15),
            error: None,
            failure: None,
            failed_hop: None,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"reachable\": true"));
        assert!(json.contains("\"latency_ms\": 15"));
        assert!(!json.contains("secret"));
    }
}
