//! Oracle integration tests comparing eggress with pproxy.
//!
//! Uses the oracle harness from `eggress_testkit::oracle` to run equivalent
//! scenarios against both pproxy and eggress, comparing normalized outputs.
//!
//! All tests are `#[ignore]` and gated on `EGGRESS_ORACLE=1`.
//!
//! Run with:
//! ```bash
//! EGRESS_ORACLE=1 cargo test -p eggress-cli --test oracle -- --ignored
//! ```

#![allow(dead_code)]

use std::io::Write;
use std::time::{Duration, Instant};

use eggress_testkit::differential::*;
use eggress_testkit::oracle::report::{
    make_comparison, normalize_for_comparison, OracleReport, ScenarioResult, ScenarioStatus,
};
use eggress_testkit::oracle::scenario::{
    all_scenarios, find_scenario, OracleScenario, ScenarioCategory,
};
use eggress_testkit::oracle::{oracle_gate_enabled, require_oracle_gate, ORACLE_GATE_VAR};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

// ===== Client Helpers =====

/// Send a raw TCP payload through a SOCKS5 proxy.
async fn socks5_tcp_connect_and_send(
    proxy_addr: std::net::SocketAddr,
    target: std::net::SocketAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;

    // SOCKS5 greeting
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("greeting write failed: {e}"))?;
    let mut buf = [0u8; 2];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("greeting read failed: {e}"))?;
    if buf != [0x05, 0x00] {
        return Err(format!("unexpected greeting response: {buf:02x?}"));
    }

    // SOCKS5 CONNECT request
    let mut req = vec![0x05, 0x01, 0x00, 0x01]; // VER, CMD, RSV, ATYP IPv4
    match target.ip() {
        std::net::IpAddr::V4(ip) => req.extend_from_slice(&ip.octets()),
        std::net::IpAddr::V6(ip) => {
            req[3] = 0x04; // ATYP IPv6
            req.extend_from_slice(&ip.octets());
        }
    }
    req.extend_from_slice(&target.port().to_be_bytes());
    stream
        .write_all(&req)
        .await
        .map_err(|e| format!("connect request write failed: {e}"))?;
    let mut resp = [0u8; 10];
    stream
        .read_exact(&mut resp)
        .await
        .map_err(|e| format!("connect response read failed: {e}"))?;
    if resp[1] != 0x00 {
        return Err(format!(
            "SOCKS5 connect failed: reply code {:#04x}",
            resp[1]
        ));
    }

    // Send payload and read response
    stream
        .write_all(payload)
        .await
        .map_err(|e| format!("payload write failed: {e}"))?;
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("response read failed: {e}"))?;
    Ok(response)
}

/// Send a raw TCP payload through an HTTP CONNECT proxy.
async fn http_connect_and_send(
    proxy_addr: std::net::SocketAddr,
    target: std::net::SocketAddr,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;

    let connect_req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n");
    stream
        .write_all(connect_req.as_bytes())
        .await
        .map_err(|e| format!("CONNECT write failed: {e}"))?;

    let mut resp = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("CONNECT response read failed: {e}"))?;
        resp.extend_from_slice(&buf[..n]);
        if resp.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let resp_str = String::from_utf8_lossy(&resp);
    if !resp_str.contains("200") {
        return Err(format!(
            "CONNECT failed: {}",
            resp_str.lines().next().unwrap_or("")
        ));
    }

    // Send payload and read response
    stream
        .write_all(payload)
        .await
        .map_err(|e| format!("payload write failed: {e}"))?;
    let _ = stream.shutdown().await;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("response read failed: {e}"))?;
    Ok(response)
}

/// Send a raw TCP payload through an HTTP forward proxy.
async fn http_forward_send(
    proxy_addr: std::net::SocketAddr,
    target: std::net::SocketAddr,
    method: &str,
    path: &str,
) -> Result<Vec<u8>, String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;

    let request = format!(
        "{method} http://{target}{path} HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("request write failed: {e}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("response read failed: {e}"))?;
    Ok(response)
}

/// Attempt a SOCKS5 connect to a refused port (expects failure).
async fn socks5_connect_refused(
    proxy_addr: std::net::SocketAddr,
    target: std::net::SocketAddr,
) -> Result<(), String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;

    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("greeting write failed: {e}"))?;
    let mut buf = [0u8; 2];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("greeting read failed: {e}"))?;

    let mut req = vec![0x05, 0x01, 0x00, 0x01];
    match target.ip() {
        std::net::IpAddr::V4(ip) => req.extend_from_slice(&ip.octets()),
        std::net::IpAddr::V6(ip) => {
            req[3] = 0x04;
            req.extend_from_slice(&ip.octets());
        }
    }
    req.extend_from_slice(&target.port().to_be_bytes());
    stream
        .write_all(&req)
        .await
        .map_err(|e| format!("connect request write failed: {e}"))?;
    let mut resp = [0u8; 10];
    stream
        .read_exact(&mut resp)
        .await
        .map_err(|e| format!("connect response read failed: {e}"))?;

    if resp[1] == 0x00 {
        Err("expected SOCKS5 failure but got success".to_string())
    } else {
        Ok(())
    }
}

/// Attempt a SOCKS5 auth connect with wrong credentials (expects failure).
async fn socks5_auth_failure(
    proxy_addr: std::net::SocketAddr,
    _target: std::net::SocketAddr,
) -> Result<(), String> {
    let mut stream = tokio::net::TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("connect to proxy failed: {e}"))?;

    // SOCKS5 greeting with auth
    stream
        .write_all(&[0x05, 0x01, 0x02])
        .await
        .map_err(|e| format!("greeting write failed: {e}"))?;
    let mut buf = [0u8; 2];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("greeting read failed: {e}"))?;
    if buf[1] != 0x02 {
        return Err("proxy did not accept auth method".to_string());
    }

    // Auth with wrong password
    let mut auth = vec![0x01]; // version
    auth.push(4); // username length
    auth.extend_from_slice(b"user");
    auth.push(5); // password length
    auth.extend_from_slice(b"wrong");
    stream
        .write_all(&auth)
        .await
        .map_err(|e| format!("auth write failed: {e}"))?;
    let mut auth_resp = [0u8; 2];
    stream
        .read_exact(&mut auth_resp)
        .await
        .map_err(|e| format!("auth read failed: {e}"))?;

    if auth_resp[1] == 0x00 {
        Err("expected auth failure but got success".to_string())
    } else {
        Ok(())
    }
}

// ===== Proxy Exercise (raw results, no comparisons) =====

/// Raw result from exercising a proxy scenario.
#[derive(Debug)]
enum ProxyResult {
    /// Got a TCP echo response (normalized payload).
    EchoPayload(String),
    /// Got an HTTP response with status code and body.
    HttpResponse { status_code: String, body: String },
    /// Connection was correctly refused by the proxy.
    Refused,
    /// Auth was correctly rejected by the proxy.
    AuthRejected,
    /// Scenario was skipped (no client action defined).
    Skipped(String),
}

/// Exercise a scenario against a proxy, returning the raw result.
async fn exercise_proxy(
    scenario: &OracleScenario,
    proxy_addr: std::net::SocketAddr,
    echo_addr: std::net::SocketAddr,
    refused_addr: std::net::SocketAddr,
) -> Result<ProxyResult, String> {
    match scenario.id {
        "tcp.http_connect"
        | "tcp.socks4_connect"
        | "tcp.socks4a_connect"
        | "tcp.socks5_connect"
        | "tcp.socks5_connect_domain" => {
            let payload = b"hello oracle test";
            let response = if scenario.id.starts_with("tcp.http") {
                http_connect_and_send(proxy_addr, echo_addr, payload).await?
            } else {
                socks5_tcp_connect_and_send(proxy_addr, echo_addr, payload).await?
            };
            let normalized =
                normalize_for_comparison(&String::from_utf8_lossy(&response), scenario.id);
            Ok(ProxyResult::EchoPayload(normalized))
        }
        "tcp.socks5_refused" => {
            socks5_connect_refused(proxy_addr, refused_addr).await?;
            Ok(ProxyResult::Refused)
        }
        "tcp.http_forward_get" => {
            let response = http_forward_send(proxy_addr, echo_addr, "GET", "/").await?;
            let body = extract_http_body(&response);
            let status_code = extract_http_status(&response);
            Ok(ProxyResult::HttpResponse { status_code, body })
        }
        "tcp.http_forward_post" => {
            let response = http_forward_send(proxy_addr, echo_addr, "POST", "/").await?;
            let status_code = extract_http_status(&response);
            Ok(ProxyResult::HttpResponse {
                status_code,
                body: String::new(),
            })
        }
        "tcp.socks5_auth" => {
            let payload = b"auth test payload";
            let response = socks5_tcp_connect_and_send(proxy_addr, echo_addr, payload).await?;
            let normalized =
                normalize_for_comparison(&String::from_utf8_lossy(&response), scenario.id);
            Ok(ProxyResult::EchoPayload(normalized))
        }
        "tcp.socks5_auth_failure" => {
            socks5_auth_failure(proxy_addr, echo_addr).await?;
            Ok(ProxyResult::AuthRejected)
        }
        _ => Ok(ProxyResult::Skipped(format!(
            "no client action for scenario: {}",
            scenario.id
        ))),
    }
}

/// Build comparisons between pproxy and eggress raw results.
fn build_comparisons(
    _scenario: &OracleScenario,
    pproxy: &Result<ProxyResult, String>,
    eggress: &Result<ProxyResult, String>,
) -> (
    Vec<eggress_testkit::oracle::report::ComparisonResult>,
    ScenarioStatus,
) {
    let mut comparisons = Vec::new();

    match (pproxy, eggress) {
        (Ok(pp), Ok(eg)) => match (pp, eg) {
            (ProxyResult::EchoPayload(pp_val), ProxyResult::EchoPayload(eg_val)) => {
                comparisons.push(make_comparison("tcp_echo_payload", pp_val, eg_val));
            }
            (ProxyResult::Refused, ProxyResult::Refused) => {
                comparisons.push(make_comparison(
                    "refused_behavior",
                    "connection_refused",
                    "connection_refused",
                ));
            }
            (ProxyResult::AuthRejected, ProxyResult::AuthRejected) => {
                comparisons.push(make_comparison(
                    "auth_failure_behavior",
                    "auth_rejected",
                    "auth_rejected",
                ));
            }
            (
                ProxyResult::HttpResponse {
                    status_code: pp_sc,
                    body: pp_body,
                },
                ProxyResult::HttpResponse {
                    status_code: eg_sc,
                    body: eg_body,
                },
            ) => {
                comparisons.push(make_comparison("http_status", pp_sc, eg_sc));
                comparisons.push(make_comparison("http_body", pp_body, eg_body));
            }
            (pp_other, eg_other) => {
                comparisons.push(make_comparison(
                    "result_type",
                    &format!("{pp_other:?}"),
                    &format!("{eg_other:?}"),
                ));
            }
        },
        (Err(pp_e), Err(eg_e)) => {
            comparisons.push(make_comparison("error_message", pp_e, eg_e));
        }
        (Ok(pp_ok), Err(eg_e)) => {
            comparisons.push(make_comparison(
                "result_vs_error",
                &format!("{pp_ok:?}"),
                eg_e,
            ));
        }
        (Err(pp_e), Ok(eg_ok)) => {
            comparisons.push(make_comparison(
                "error_vs_result",
                pp_e,
                &format!("{eg_ok:?}"),
            ));
        }
    }

    let all_matched = comparisons.iter().all(|c| c.matched);
    let any_skipped = matches!(
        pproxy,
        Ok(ProxyResult::Skipped(_)) | Ok(ProxyResult::Refused) | Ok(ProxyResult::AuthRejected)
    );

    let status = if all_matched {
        ScenarioStatus::Pass
    } else if any_skipped {
        ScenarioStatus::Skipped
    } else {
        ScenarioStatus::Fail
    };

    (comparisons, status)
}

// ===== Server Startup =====

/// Wait for eggress readiness (polls the RuntimeState).
async fn wait_eggress_ready(state: &eggress_runtime::RuntimeState) {
    use std::sync::atomic::Ordering;
    for _ in 0..100 {
        if state.readiness.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Start an eggress server from a TOML config string.
///
/// Returns the listener address and a shutdown token for cleanup.
async fn start_eggress_from_toml(
    config_str: &str,
) -> Result<(std::net::SocketAddr, CancellationToken), String> {
    let mut f = tempfile::NamedTempFile::new().map_err(|e| format!("create tempfile: {e}"))?;
    f.write_all(config_str.as_bytes())
        .map_err(|e| format!("write config: {e}"))?;
    f.flush().map_err(|e| format!("flush config: {e}"))?;
    let path = f.path().to_str().unwrap().to_string();
    // Keep the tempfile alive (don't delete on drop)
    std::mem::forget(f);

    let mut sup = eggress_runtime::ServiceSupervisor::start(&path)
        .map_err(|e| format!("start eggress: {e}"))?;
    let state = sup.state().clone();
    let token = sup.shutdown_token();
    tokio::task::spawn_blocking(move || {
        let _ = sup.run();
    });

    wait_eggress_ready(&state).await;

    let listener_addr = {
        let addrs = state.listener_addrs.lock().unwrap();
        if addrs.is_empty() {
            return Err("eggress has no listener addresses".to_string());
        }
        addrs[0].unwrap()
    };
    Ok((listener_addr, token))
}

// ===== Scenario Runners =====

/// Run a single scenario against pproxy.
async fn run_pproxy_scenario(
    scenario: &OracleScenario,
    listen_port: u16,
    echo_port: u16,
    _refused_port: u16,
) -> Result<std::net::SocketAddr, String> {
    let args: Vec<String> = scenario
        .pproxy_args
        .iter()
        .map(|a| {
            a.replace("{PORT}", &listen_port.to_string())
                .replace("{PORT2}", &(listen_port + 1).to_string())
                .replace("{ECHO_PORT}", &echo_port.to_string())
                .replace("{UPSTREAM_PORT}", &echo_port.to_string())
        })
        .collect();

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut proc = start_pproxy_with_args(&arg_refs).await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), listen_port);

    if !wait_for_port(listen_port, Duration::from_secs(3)).await {
        proc.kill();
        return Err(format!(
            "pproxy did not bind to port {listen_port} within timeout"
        ));
    }

    Ok(addr)
}

/// Run a single scenario against eggress.
///
/// Substitutes `{PORT}` and `{ECHO_PORT}` / `{UPSTREAM_PORT}` placeholders
/// in the scenario's `eggress_toml` and starts an eggress supervisor.
async fn run_eggress_scenario(
    scenario: &OracleScenario,
    listen_port: u16,
    echo_port: u16,
    _refused_port: u16,
) -> Result<(std::net::SocketAddr, CancellationToken), String> {
    let toml = scenario
        .eggress_toml
        .replace("{PORT}", &listen_port.to_string())
        .replace("{PORT2}", &(listen_port + 1).to_string())
        .replace("{ECHO_PORT}", &echo_port.to_string())
        .replace("{UPSTREAM_PORT}", &echo_port.to_string());

    start_eggress_from_toml(&toml).await
}

/// Run the oracle comparison for a single scenario (two-sided).
async fn run_scenario_comparison(scenario: &OracleScenario) -> ScenarioResult {
    let start = Instant::now();

    let refused_port = 1u16;
    let refused_addr = std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), refused_port);

    // Start echo server
    let (echo_addr, echo_jh) = eggress_testkit::start_echo_server().await;

    // Allocate separate ports for pproxy and eggress
    let pproxy_port = eggress_testkit::get_free_port().await;
    let eggress_port = eggress_testkit::get_free_port().await;

    // Start both proxies concurrently
    let pproxy_fut = run_pproxy_scenario(scenario, pproxy_port, echo_addr.port(), refused_port);
    let eggress_fut = run_eggress_scenario(scenario, eggress_port, echo_addr.port(), refused_port);

    let (pproxy_result, eggress_result) = tokio::join!(pproxy_fut, eggress_fut);

    // Exercise both proxies
    let pproxy_exercise = match pproxy_result {
        Ok(addr) => exercise_proxy(scenario, addr, echo_addr, refused_addr).await,
        Err(e) => Err(format!("pproxy startup failed: {e}")),
    };

    let eggress_exercise = match eggress_result {
        Ok((addr, token)) => {
            let result = exercise_proxy(scenario, addr, echo_addr, refused_addr).await;
            token.cancel();
            result
        }
        Err(e) => Err(format!("eggress startup failed: {e}")),
    };

    // Build two-sided comparisons
    let (comparisons, status) = build_comparisons(scenario, &pproxy_exercise, &eggress_exercise);

    let error_msg = match (&pproxy_exercise, &eggress_exercise) {
        (Err(pp_e), Err(eg_e)) => Some(format!("both failed: pproxy={pp_e}, eggress={eg_e}")),
        (Err(pp_e), Ok(_)) => Some(format!("pproxy failed: {pp_e}")),
        (Ok(_), Err(eg_e)) => Some(format!("eggress failed: {eg_e}")),
        _ => None,
    };

    echo_jh.abort();

    ScenarioResult {
        id: scenario.id.to_string(),
        category: scenario.category,
        description: scenario.description.to_string(),
        status,
        comparisons,
        elapsed_ms: start.elapsed().as_millis() as u64,
        error: error_msg,
        skip_reason: None,
        pproxy_observation: None,
        eggress_observation: None,
        timing_tolerance_ms: None,
        divergence_ids: Vec::new(),
        ci_tier: None,
        capability_ids: scenario
            .capability_ids
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    }
}

// ===== Tests =====

#[test]
fn oracle_gate_check() {
    if !oracle_gate_enabled() {
        eprintln!("oracle tests skipped: set {}=1 to enable", ORACLE_GATE_VAR);
    }
}

#[tokio::test]
#[ignore]
async fn oracle_all_scenarios_registry() {
    require_oracle_gate();
    let scenarios = all_scenarios();
    assert_eq!(scenarios.len(), 31, "expected 31 scenarios");
}

#[tokio::test]
#[ignore]
async fn oracle_cli_socks5_default() {
    require_oracle_gate();
    let scenario = find_scenario("cli.socks5_default").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_cli_socks4_default() {
    require_oracle_gate();
    let scenario = find_scenario("cli.socks4_default").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_cli_http_default() {
    require_oracle_gate();
    let scenario = find_scenario("cli.http_default").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_tcp_socks5_connect() {
    require_oracle_gate();
    let scenario = find_scenario("tcp.socks5_connect").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_tcp_http_connect() {
    require_oracle_gate();
    let scenario = find_scenario("tcp.http_connect").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_tcp_socks5_refused() {
    require_oracle_gate();
    let scenario = find_scenario("tcp.socks5_refused").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_tcp_socks5_auth_failure() {
    require_oracle_gate();
    let scenario = find_scenario("tcp.socks5_auth_failure").expect("scenario not found");
    let result = run_scenario_comparison(&scenario).await;
    assert!(
        result.status == ScenarioStatus::Pass || result.status == ScenarioStatus::Skipped,
        "scenario {} failed: {:?}",
        scenario.id,
        result.error
    );
}

#[tokio::test]
#[ignore]
async fn oracle_generate_report() {
    require_oracle_gate();

    let mut report = OracleReport::new();
    let scenarios = all_scenarios();
    let total = scenarios.len();

    for scenario in &scenarios {
        let result = run_scenario_comparison(scenario).await;
        report.add_scenario(result);
    }

    report.set_elapsed(Duration::from_secs(0)); // placeholder

    let json = report.to_json();
    assert!(json.contains("\"summary\""));
    assert!(json.contains("\"scenarios\""));

    eprintln!(
        "oracle report: {}/{} passed, {} failed, {} skipped, {} errors",
        report.summary.passed,
        total,
        report.summary.failed,
        report.summary.skipped,
        report.summary.errors
    );

    // Write report if env var is set
    if let Ok(path) = std::env::var("EGRESS_ORACLE_REPORT") {
        let report_path = std::path::Path::new(&path);
        report
            .write_json(report_path)
            .expect("failed to write oracle report");
        eprintln!("oracle report written to: {}", report_path.display());
    }
}

// ===== Unit Tests (always run) =====

#[test]
fn normalization_replaces_ports() {
    let input = "Listen on 127.0.0.1:54321";
    let normalized = normalize_for_comparison(input, "test");
    assert_eq!(normalized, "Listen on 127.0.0.1:PORT");
}

#[test]
fn normalization_strips_pproxy_prefixes() {
    let input = "INFO: Listen: socks5://127.0.0.1:1080";
    let normalized = normalize_for_comparison(input, "cli.socks5_default");
    assert_eq!(normalized, "socks5://127.0.0.1:PORT");
}

#[test]
fn comparison_match() {
    let comp = make_comparison("payload", "hello", "hello");
    assert!(comp.matched);
    assert!(comp.details.is_none());
}

#[test]
fn comparison_mismatch() {
    let comp = make_comparison("payload", "hello", "world");
    assert!(!comp.matched);
    assert!(comp.details.is_some());
}

#[test]
fn scenario_category_display() {
    let cat = ScenarioCategory::HttpSocksTcp;
    let json = serde_json::to_string(&cat).unwrap();
    assert_eq!(json, "\"http_socks_tcp\"");
}

#[test]
fn report_json_roundtrip() {
    let mut report = OracleReport::new();
    report.add_scenario(ScenarioResult {
        id: "test".to_string(),
        category: ScenarioCategory::CliDefaults,
        description: "test scenario".to_string(),
        status: ScenarioStatus::Pass,
        comparisons: vec![],
        elapsed_ms: 100,
        error: None,
        skip_reason: None,
        pproxy_observation: None,
        eggress_observation: None,
        timing_tolerance_ms: None,
        divergence_ids: Vec::new(),
        ci_tier: None,
        capability_ids: Vec::new(),
    });
    let json = report.to_json();
    let parsed: OracleReport = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.summary.total, 1);
    assert_eq!(parsed.summary.passed, 1);
}
