//! Scenario-driven oracle harness for comparing eggress with pproxy.
//!
//! This module provides a structured framework for running equivalent
//! scenarios against both pproxy and eggress, normalizing outputs, and
//! generating JSON comparison reports.
//!
//! # Gating
//!
//! All oracle tests are gated on `EGGRESS_ORACLE=1` and require Python 3
//! with pproxy==2.7.9 installed. The non-gated test suite must never
//! require pproxy or internet access.

pub mod ci;
pub mod observations;
pub mod probes;
pub mod report;
pub mod scenario;
pub mod schema;
pub mod supervisor;

use std::time::Duration;

/// Environment variable that gates oracle tests.
pub const ORACLE_GATE_VAR: &str = "EGRESS_ORACLE";

/// Check if the oracle test gate is enabled.
pub fn oracle_gate_enabled() -> bool {
    std::env::var(ORACLE_GATE_VAR)
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Require the oracle gate to be enabled. Panics with a clear message if not.
pub fn require_oracle_gate() {
    if !oracle_gate_enabled() {
        panic!(
            "oracle tests require {}=1 and pproxy=={}",
            ORACLE_GATE_VAR,
            crate::differential::PINNED_PPROXY_VERSION
        );
    }
    if !pproxy_available() {
        panic!(
            "pproxy not available; install with: pip install pproxy=={}",
            crate::differential::PINNED_PPROXY_VERSION
        );
    }
}

fn pproxy_available() -> bool {
    let python = crate::differential::find_python_binary();
    std::process::Command::new(&python)
        .args(["-c", "import pproxy"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Default timeout for oracle scenario execution.
pub const DEFAULT_SCENARIO_TIMEOUT: Duration = Duration::from_secs(15);

/// Default timeout for process startup.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Default timeout for I/O operations.
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(3);
