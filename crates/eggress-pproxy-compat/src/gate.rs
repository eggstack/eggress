//! Shared execution-readiness gate for pproxy compatibility execution.
//!
//! Both user-facing compatibility execution paths (the standalone
//! `pproxy`/`eggress-pproxy-compat` binary and the `eggress pproxy run`
//! subcommand) must apply the same fail-closed policy before any
//! temporary config, system change, or runtime startup. This helper
//! encodes that policy without knowing about process I/O, exit codes,
//! or CLI types so it can be reused from any entry point.

use crate::args::PproxyArgs;
use crate::warnings::{CompatWarning, TranslationOutput, UnsupportedFeature};

/// One reason why pproxy compatibility execution cannot start.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockReason {
    /// The parser found an unrecognized flag.
    UnknownFlag(String),
    /// The parser recognized the flag but Eggress cannot satisfy it.
    Unsupported(UnsupportedFeature),
}

/// Aggregate result of the shared compatibility execution gate.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionGate {
    /// Reasons that prevent startup, in stable order.
    pub blockers: Vec<BlockReason>,
    /// Benign warnings that do not block startup.
    pub warnings: Vec<CompatWarning>,
}

impl ExecutionGate {
    /// Returns `true` when no blocker prevents startup.
    pub fn allows_start(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Render blockers as a stable, human-readable summary.
    pub fn blocker_summary(&self) -> String {
        self.blockers
            .iter()
            .map(|b| match b {
                BlockReason::UnknownFlag(f) => format!("unknown option '{f}'"),
                BlockReason::Unsupported(u) => format!("{u}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Compute the shared execution gate for a compatibility invocation.
///
/// The gate combines:
/// 1. parser-side unknown flags (always fatal),
/// 2. translator-side unsupported features (always fatal),
/// 3. benign warnings that do not block startup.
pub fn evaluate(args: &PproxyArgs, output: &TranslationOutput) -> ExecutionGate {
    let mut blockers: Vec<BlockReason> = Vec::new();
    for flag in &args.unknown_flags {
        blockers.push(BlockReason::UnknownFlag(flag.clone()));
    }
    for u in &output.unsupported {
        blockers.push(BlockReason::Unsupported(u.clone()));
    }
    ExecutionGate {
        blockers,
        warnings: output.warnings.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::PproxyArgs;

    fn parse(args: &[&str]) -> PproxyArgs {
        let raw: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        PproxyArgs::parse(&raw).expect("parser failed")
    }

    #[test]
    fn evaluate_allows_clean_translation() {
        let args = parse(&["-l", "http://:8080", "-r", "socks5://proxy:1080"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        assert!(gate.allows_start(), "unexpected blockers: {:?}", gate);
        assert!(gate.warnings.is_empty());
    }

    #[test]
    fn evaluate_blocks_unknown_flag() {
        let args = parse(&["-l", "http://:8080", "--bogus-flag"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        assert!(!gate.allows_start());
        assert!(matches!(
            gate.blockers.first(),
            Some(BlockReason::UnknownFlag(f)) if f == "--bogus-flag"
        ));
    }

    #[test]
    fn evaluate_blocks_daemon() {
        let args = parse(&["-l", "http://:8080", "--daemon"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        assert!(!gate.allows_start());
        assert!(gate
            .blockers
            .iter()
            .any(|b| matches!(b, BlockReason::Unsupported(u) if u.feature == "daemon")));
    }

    #[test]
    fn evaluate_allows_sys_with_warning() {
        let args = parse(&["-l", "http://:8080", "--sys"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        assert!(gate.allows_start(), "unexpected blockers: {:?}", gate);
    }

    #[test]
    fn evaluate_allows_auth_with_warning() {
        let args = parse(&["-l", "http://:8080", "--auth", "3600"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        assert!(gate.allows_start(), "unexpected blockers: {:?}", gate);
    }

    #[test]
    fn evaluate_blocks_malformed_auth() {
        let raw = vec![
            "-l".to_string(),
            "http://:8080".to_string(),
            "--auth".to_string(),
            "abc".to_string(),
        ];
        let err = PproxyArgs::parse(&raw).unwrap_err();
        // The parser itself rejects malformed --auth before reaching the
        // gate, so this path is handled by the parser exit branch.
        let _ = err;
    }

    #[test]
    fn evaluate_does_not_block_d_flag() {
        let args = parse(&["-l", "http://:8080", "-d"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        assert!(gate.allows_start(), "unexpected blockers: {:?}", gate);
    }

    #[test]
    fn evaluate_blocker_summary_lists_each_reason() {
        let args = parse(&["-l", "http://:8080", "--bogus-flag", "--daemon"]);
        let output = crate::translate::translate_pproxy_args(&args).unwrap();
        let gate = evaluate(&args, &output);
        let summary = gate.blocker_summary();
        assert!(summary.contains("--bogus-flag"));
        assert!(summary.contains("daemon"));
    }
}
