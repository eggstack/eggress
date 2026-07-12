use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyObservation {
    pub bound_addr: Option<std::net::SocketAddr>,
    pub exit_code: Option<i32>,
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
    pub connection_result: ConnectionResult,
    pub protocol_reply: Option<ProtocolReply>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub auth_result: Option<AuthResult>,
    pub half_close: Option<HalfCloseBehavior>,
    pub timing: TimingObservation,
    pub cleanup: CleanupObservation,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionResult {
    Success,
    Refused,
    Timeout,
    Reset,
    ProxyRejected,
    #[default]
    NotAttempted,
    Partial,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtocolReply {
    pub protocol: String,
    pub status_code: Option<u16>,
    pub status_text: Option<String>,
    pub raw_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthResult {
    Success,
    Rejected,
    NotAttempted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HalfCloseBehavior {
    RespondedAfterHalfClose,
    ClosedAfterHalfClose,
    Ignored,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimingObservation {
    pub startup_ms: Option<u64>,
    pub time_to_first_byte_ms: Option<u64>,
    pub total_connection_ms: Option<u64>,
    pub shutdown_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupObservation {
    pub processes_cleaned: bool,
    pub sockets_removed: bool,
    pub temp_files_cleaned: bool,
    pub leftover_artifacts: Vec<String>,
}

pub fn compare_observations(
    pproxy: &ProxyObservation,
    eggress: &ProxyObservation,
    equivalence: &super::scenario::EquivalenceTarget,
) -> Vec<super::report::ComparisonResult> {
    let mut results = Vec::new();

    let pp_cr = serde_json::to_string(&pproxy.connection_result).unwrap_or_default();
    let eg_cr = serde_json::to_string(&eggress.connection_result).unwrap_or_default();
    results.push(super::report::make_comparison(
        "connection_result",
        &pp_cr,
        &eg_cr,
    ));

    match equivalence {
        super::scenario::EquivalenceTarget::Payload => {
            let pp_sent = pproxy.bytes_sent.to_string();
            let eg_sent = eggress.bytes_sent.to_string();
            results.push(super::report::make_comparison(
                "bytes_sent",
                &pp_sent,
                &eg_sent,
            ));

            let pp_recv = pproxy.bytes_received.to_string();
            let eg_recv = eggress.bytes_received.to_string();
            results.push(super::report::make_comparison(
                "bytes_received",
                &pp_recv,
                &eg_recv,
            ));
        }
        super::scenario::EquivalenceTarget::StatusCode => {
            if let (Some(pp), Some(eg)) = (&pproxy.protocol_reply, &eggress.protocol_reply) {
                let pp_sc = pp.status_code.map(|c| c.to_string()).unwrap_or_default();
                let eg_sc = eg.status_code.map(|c| c.to_string()).unwrap_or_default();
                results.push(super::report::make_comparison(
                    "status_code",
                    &pp_sc,
                    &eg_sc,
                ));
            }
        }
        super::scenario::EquivalenceTarget::CoarseResult => {}
        super::scenario::EquivalenceTarget::BindAddress => {
            let pp_addr = pproxy.bound_addr.map(|a| a.to_string()).unwrap_or_default();
            let eg_addr = eggress
                .bound_addr
                .map(|a| a.to_string())
                .unwrap_or_default();
            results.push(super::report::make_comparison(
                "bind_address",
                &pp_addr,
                &eg_addr,
            ));
        }
    }

    if let (Some(pp_auth), Some(eg_auth)) = (&pproxy.auth_result, &eggress.auth_result) {
        let pp_s = serde_json::to_string(pp_auth).unwrap_or_default();
        let eg_s = serde_json::to_string(eg_auth).unwrap_or_default();
        results.push(super::report::make_comparison("auth_result", &pp_s, &eg_s));
    }

    results
}

pub fn observation_from_process(
    bound_addr: Option<std::net::SocketAddr>,
    exit_code: Option<i32>,
    stdout: &[String],
    stderr: &[String],
    error: Option<String>,
) -> ProxyObservation {
    ProxyObservation {
        bound_addr,
        exit_code,
        stdout_lines: stdout.to_vec(),
        stderr_lines: stderr.to_vec(),
        connection_result: if error.is_some() {
            ConnectionResult::NotAttempted
        } else {
            ConnectionResult::Success
        },
        protocol_reply: None,
        bytes_sent: 0,
        bytes_received: 0,
        auth_result: None,
        half_close: None,
        timing: TimingObservation::default(),
        cleanup: CleanupObservation::default(),
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_serialization_roundtrip() {
        let obs = ProxyObservation {
            bound_addr: Some("127.0.0.1:8080".parse().unwrap()),
            exit_code: None,
            stdout_lines: vec!["listen on 127.0.0.1:8080".to_string()],
            stderr_lines: vec![],
            connection_result: ConnectionResult::Success,
            protocol_reply: Some(ProtocolReply {
                protocol: "socks5".to_string(),
                status_code: Some(0),
                status_text: Some("success".to_string()),
                raw_bytes: None,
            }),
            bytes_sent: 17,
            bytes_received: 17,
            auth_result: None,
            half_close: None,
            timing: TimingObservation::default(),
            cleanup: CleanupObservation::default(),
            error: None,
        };

        let json = serde_json::to_string(&obs).unwrap();
        let parsed: ProxyObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.connection_result, ConnectionResult::Success);
        assert_eq!(parsed.bytes_sent, 17);
    }

    #[test]
    fn compare_observations_coarse_pass() {
        let mut pp = ProxyObservation {
            ..Default::default()
        };
        pp.connection_result = ConnectionResult::Success;
        let mut eg = ProxyObservation {
            ..Default::default()
        };
        eg.connection_result = ConnectionResult::Success;

        let results = compare_observations(
            &pp,
            &eg,
            &super::super::scenario::EquivalenceTarget::CoarseResult,
        );
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_observations_coarse_fail() {
        let mut pp = ProxyObservation {
            ..Default::default()
        };
        pp.connection_result = ConnectionResult::Success;
        let mut eg = ProxyObservation {
            ..Default::default()
        };
        eg.connection_result = ConnectionResult::Refused;

        let results = compare_observations(
            &pp,
            &eg,
            &super::super::scenario::EquivalenceTarget::CoarseResult,
        );
        assert!(!results.iter().all(|r| r.matched));
    }

    #[test]
    fn compare_observations_payload() {
        let mut pp = ProxyObservation {
            ..Default::default()
        };
        pp.bytes_sent = 17;
        pp.bytes_received = 17;
        let mut eg = ProxyObservation {
            ..Default::default()
        };
        eg.bytes_sent = 17;
        eg.bytes_received = 17;

        let results = compare_observations(
            &pp,
            &eg,
            &super::super::scenario::EquivalenceTarget::Payload,
        );
        assert!(results.iter().all(|r| r.matched));
    }

    #[test]
    fn observation_from_process_minimal() {
        let obs = observation_from_process(
            Some("127.0.0.1:8080".parse().unwrap()),
            None,
            &["listen".to_string()],
            &[],
            None,
        );
        assert_eq!(obs.connection_result, ConnectionResult::Success);
        assert!(obs.error.is_none());
    }

    #[test]
    fn observation_from_process_error() {
        let obs = observation_from_process(
            None,
            Some(1),
            &[],
            &["failed".to_string()],
            Some("startup failed".to_string()),
        );
        assert_eq!(obs.connection_result, ConnectionResult::NotAttempted);
        assert!(obs.error.is_some());
    }
}
