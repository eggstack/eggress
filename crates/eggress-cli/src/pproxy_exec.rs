//! One shared pproxy compatibility execution pipeline.
//!
//! The standalone `pproxy` binary and `eggress pproxy run` are alternate
//! entry points to the same compatibility intent. This facade owns their
//! common sequence — parse, strict validation, translation, execution gate,
//! config compilation, `--test` diagnostics, daemon transition, and
//! supervisor startup — so the two entry points cannot drift into different
//! translation/gating/runtime behavior.
//!
//! Process-specific rendering stays in the binaries, but only as a
//! `diag_prefix` (`""` for the nested command, `"pproxy: "` for the
//! standalone binary) applied to facade-chosen messages. Business decisions
//! (what fails, with which exit code, in which order) are returned as typed
//! results. Do not force the standalone parser through the native Clap
//! hierarchy; preserving pproxy argument behavior takes priority, so this
//! facade consumes already-collected argv like both binaries always have.

use std::time::Duration;

use crate::{
    parse_pproxy_test_target, run_upstream_test, EXIT_CLI_PARSE_ERROR, EXIT_CONFIG_VALIDATION,
    EXIT_SUCCESS, EXIT_UNSUPPORTED_FEATURE,
};

/// What a compatibility invocation should do after parsing.
#[derive(Debug)]
pub enum PreparedAction {
    /// Print the compatibility version string and exit 0.
    PrintVersion,
    /// Print entry-point-specific help and exit 0.
    PrintHelp,
    /// Run the prepared compatibility execution.
    Run(Box<GatedRun>),
}

/// A compatibility invocation that passed parsing, strict validation,
/// translation, and the fail-closed execution gate, but has not yet been
/// compiled to a runtime configuration.
///
/// The split exists so the standalone binary can print its startup banner
/// after gating and before config compilation, matching the historical
/// presentation order.
#[derive(Debug)]
pub struct GatedRun {
    /// Parsed pproxy arguments (drives logging policy, banner, hooks).
    pub args: eggress_pproxy_compat::PproxyArgs,
    /// Translation output (TOML plus warnings).
    pub output: eggress_pproxy_compat::TranslationOutput,
    /// `--test` target, if the invocation is a diagnostic probe.
    pub test_target: Option<String>,
    /// Benign translation warnings; rendered by the caller, never fatal.
    pub warnings: Vec<eggress_pproxy_compat::CompatWarning>,
}

/// A compatibility invocation that passed parsing, strict validation,
/// translation, gating, and config compilation.
#[derive(Debug)]
pub struct PreparedRun {
    /// Parsed pproxy arguments (drives logging policy, banner, hooks).
    pub args: eggress_pproxy_compat::PproxyArgs,
    /// Compiled runtime configuration translated from the pproxy arguments.
    pub rt_config: eggress_config::compile::RuntimeConfig,
    /// `--test` target, if the invocation is a diagnostic probe.
    pub test_target: Option<String>,
    /// Benign translation warnings; rendered by the caller, never fatal.
    pub warnings: Vec<eggress_pproxy_compat::CompatWarning>,
}

/// A fatal preparation failure: numeric exit code plus the unprefixed
/// message. Binaries render it as `{diag_prefix}error: {message}`.
#[derive(Debug)]
pub struct PrepareFailure {
    /// Process exit code for this failure.
    pub code: i32,
    /// Human-readable reason without any binary-specific prefix.
    pub message: String,
}

/// Run the shared preparation sequence over already-collected argv.
///
/// Passing an empty slice selects the same default listener configuration as
/// invoking the compatibility binary with no arguments.
pub fn prepare(argv: &[String]) -> Result<PreparedAction, PrepareFailure> {
    let pproxy_args = if argv.is_empty() {
        eggress_pproxy_compat::PproxyArgs::default_args()
    } else {
        eggress_pproxy_compat::PproxyArgs::parse(argv).map_err(|e| PrepareFailure {
            code: EXIT_CLI_PARSE_ERROR,
            message: e.to_string(),
        })?
    };

    if pproxy_args.version {
        return Ok(PreparedAction::PrintVersion);
    }
    if pproxy_args.help {
        return Ok(PreparedAction::PrintHelp);
    }

    if let Some(flag) = pproxy_args.strict_parser_violations().first() {
        return Err(PrepareFailure {
            code: EXIT_CLI_PARSE_ERROR,
            message: format!("unknown option or positional argument '{flag}'"),
        });
    }

    if let Err(e) = pproxy_args.validate_strict_values() {
        return Err(PrepareFailure {
            code: EXIT_CLI_PARSE_ERROR,
            message: e.to_string(),
        });
    }

    let output =
        eggress_pproxy_compat::translate_pproxy_args(&pproxy_args).map_err(|e| PrepareFailure {
            code: EXIT_CONFIG_VALIDATION,
            message: e.to_string(),
        })?;

    // Fatal gating: unknown flags and unsupported features stop startup.
    // The shared gate is the single source of truth for the fail-closed
    // policy applied by every compatibility execution entry point.
    let gate = eggress_pproxy_compat::evaluate_execution_gate(&pproxy_args, &output);
    if !gate.allows_start() {
        let mut message = String::new();
        for blocker in &gate.blockers {
            match blocker {
                eggress_pproxy_compat::BlockReason::UnknownFlag(flag) => {
                    message.push_str(&format!("unknown option '{flag}'\n"));
                }
                eggress_pproxy_compat::BlockReason::Unsupported(u) => {
                    message.push_str(&format!("{u}\n"));
                }
            }
        }
        message.push('\n');
        let has_unknown = gate
            .blockers
            .iter()
            .any(|b| matches!(b, eggress_pproxy_compat::BlockReason::UnknownFlag(_)));
        if has_unknown {
            message.push_str("Run 'eggress pproxy check -- <args>' for supported options.");
            return Err(PrepareFailure {
                code: EXIT_CLI_PARSE_ERROR,
                message,
            });
        }
        message.push_str(
            "These features are not supported by eggress and prevent startup.\n\
             Run 'eggress pproxy check -- <args>' for detailed compatibility report.",
        );
        return Err(PrepareFailure {
            code: EXIT_UNSUPPORTED_FEATURE,
            message,
        });
    }

    Ok(PreparedAction::Run(Box::new(GatedRun {
        test_target: pproxy_args.test_target().map(str::to_string),
        warnings: output.warnings(),
        output,
        args: pproxy_args,
    })))
}

/// Compile a gated run's translated TOML into a validated in-memory
/// `RuntimeConfig`. No temporary file is created; the config lives entirely
/// in process memory.
pub fn compile(gated: GatedRun) -> Result<PreparedRun, PrepareFailure> {
    let GatedRun {
        args,
        output,
        test_target,
        warnings,
    } = gated;
    let (rt_config, _warnings) =
        eggress_config::validate_and_compile_toml_with_warnings(&output.toml).map_err(|e| {
            PrepareFailure {
                code: EXIT_CONFIG_VALIDATION,
                message: format!("config error: {e}"),
            }
        })?;
    Ok(PreparedRun {
        args,
        rt_config,
        test_target,
        warnings,
    })
}

/// Execute a prepared compatibility run: warnings, `--test` diagnostics,
/// daemon transition, then the supervised service.
///
/// `diag_prefix` is `""` for `eggress pproxy run` and `"pproxy: "` for the
/// standalone binary. Logging must already be initialized by the caller:
/// verbosity policy differs per entry point (`--log-format` vs pretty).
/// Returns the process exit code for `--test` probes; a serving instance
/// runs until shutdown and exits the process itself on supervisor failure.
pub fn execute(prepared: PreparedRun, diag_prefix: &str) -> i32 {
    let PreparedRun {
        args: pproxy_args,
        rt_config,
        test_target,
        warnings,
    } = prepared;

    for w in &warnings {
        eprintln!("{diag_prefix}warning: {w}");
    }

    if let Some(target) = test_target {
        let timeout = Duration::from_secs(10);
        let target = match parse_pproxy_test_target(&target) {
            Ok(target) => target.to_string(),
            Err(error) => {
                eprintln!("{diag_prefix}error: {error}");
                return EXIT_CLI_PARSE_ERROR;
            }
        };
        if rt_config.upstreams.is_empty() {
            return EXIT_SUCCESS;
        }
        return run_upstream_test(&rt_config, Some(&target), timeout, false);
    }

    #[cfg(feature = "pproxy-daemon")]
    if let Err(error) = crate::maybe_daemonize(pproxy_args.daemon) {
        eprintln!("{diag_prefix}error: {error}");
        return error.exit_code();
    }

    tracing::info!("starting eggress with pproxy-compatible config");
    // Deterministic verbosity markers for process-level regression tests.
    tracing::debug!("compatibility debug verbosity active");
    tracing::trace!("compatibility trace verbosity active");

    #[cfg(feature = "ssh")]
    if !eggress_runtime::ssh_insecure_acknowledged() {
        tracing::warn!(
            "compatibility mode would disable SSH host-key verification; \
             keeping known_hosts verification enabled. To explicitly \
             accept unverified SSH host keys (MITM risk), set \
             EGRESS_SSH_INSECURE_HOST_KEYS=1"
        );
    }

    // Start from the in-memory RuntimeConfig. No config file path is
    // provided, so SIGHUP reload is disabled (there is no stable
    // user-authored config file to reload from in compatibility mode).
    //
    // `-d`/`-v` were already resolved into the tracing filter by the
    // caller's logging init; runtime receives only typed hooks.
    let hooks = eggress_runtime::CompatibilityRuntimeHooks::from_facade(
        pproxy_args.effective_auth_timeout(),
        pproxy_args.system_proxy,
        eggress_runtime::ssh_insecure_acknowledged(),
    );
    match eggress_runtime::ServiceSupervisor::start_from_config_with_compatibility(
        rt_config, None, hooks,
    ) {
        Ok(mut supervisor) => {
            if let Err(e) = supervisor.run() {
                eprintln!("{diag_prefix}runtime error: {e}");
                std::process::exit(crate::runtime_error_exit_code(&e));
            }
        }
        Err(e) => {
            eprintln!("{diag_prefix}runtime error: {e}");
            std::process::exit(crate::runtime_error_exit_code(&e));
        }
    }
    EXIT_SUCCESS
}

/// Structured startup-banner detail lines for the standalone binary.
///
/// Consumes the authoritative parser state
/// ([`eggress_pproxy_compat::PproxyArgs::udp_listen_addrs`],
/// [`eggress_pproxy_compat::PproxyArgs::tls_requested`],
/// [`eggress_pproxy_compat::PproxyArgs::pac_requested`]) instead of scanning
/// diagnostic strings at the presentation site. The caller prints its
/// version line before these and the `waiting for connections` footer after.
pub fn banner_lines(pproxy_args: &eggress_pproxy_compat::PproxyArgs) -> Vec<String> {
    let mut lines = Vec::new();
    for local in &pproxy_args.local {
        lines.push(format!("  listen:   {}", redact_uri(local)));
    }
    for remote in &pproxy_args.remotes {
        lines.push(format!("  remote:   {}", redact_uri(remote)));
    }
    for addr in pproxy_args.udp_listen_addrs() {
        lines.push(format!("  udp:      {addr}"));
    }
    if pproxy_args.tls_requested() {
        lines.push("  tls:      enabled".to_string());
    }
    if pproxy_args.pac_requested() {
        lines.push("  pac:      enabled".to_string());
    }
    if pproxy_args.reuse_port {
        lines.push("  reuse:    SO_REUSEPORT".to_string());
    }
    lines
}

fn redact_uri(uri: &str) -> String {
    eggress_uri::parse_proxy_chain(uri)
        .map(|chain| eggress_uri::RedactedUri::new(&chain).to_string())
        // Listener URIs may use an empty host as a bind address, while
        // outbound proxy hops reject empty hosts during parsing.
        .unwrap_or_else(|_| eggress_uri::redact_proxy_uri(uri))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn prepare_selects_defaults_for_empty_argv() {
        let action = prepare(&[]).expect("empty argv must prepare");
        assert!(
            matches!(action, PreparedAction::Run(_)),
            "empty argv must prepare a default run"
        );
    }

    #[test]
    fn prepare_reports_version_and_help() {
        assert!(matches!(
            prepare(&argv(&["--version"])),
            Ok(PreparedAction::PrintVersion)
        ));
        assert!(matches!(
            prepare(&argv(&["--help"])),
            Ok(PreparedAction::PrintHelp)
        ));
    }

    #[test]
    fn prepare_rejects_unknown_flags_as_parse_errors() {
        let err = prepare(&argv(&["-l", "http://:8080", "--bogus-flag"])).unwrap_err();
        assert_eq!(err.code, EXIT_CLI_PARSE_ERROR);
        assert!(err.message.contains("--bogus-flag"));
    }

    #[test]
    fn prepare_blocks_unsupported_features() {
        // `--daemon` without the `pproxy-daemon` feature is fatal via the
        // shared gate; with the feature it prepares a run instead.
        let result = prepare(&argv(&["-l", "http://:8080", "--daemon"]));
        #[cfg(feature = "pproxy-daemon")]
        assert!(matches!(result, Ok(PreparedAction::Run(_))));
        #[cfg(not(feature = "pproxy-daemon"))]
        {
            let err = result.unwrap_err();
            assert_eq!(err.code, EXIT_UNSUPPORTED_FEATURE);
        }
    }

    #[test]
    fn prepare_compiles_equivalent_runtime_config() {
        let action = prepare(&argv(&[
            "-l",
            "http://127.0.0.1:0",
            "-r",
            "socks5://127.0.0.1:1080",
        ]))
        .expect("supported args must prepare");
        match action {
            PreparedAction::Run(gated) => {
                assert!(gated.test_target.is_none());
                let prepared = compile(*gated).expect("translated TOML must compile");
                assert_eq!(prepared.rt_config.listeners.len(), 1);
                assert!(prepared.rt_config.listeners[0].name.starts_with("pproxy-"));
            }
            _ => panic!("expected a prepared run"),
        }
    }

    #[test]
    fn banner_consumes_structured_parser_state() {
        let args = eggress_pproxy_compat::PproxyArgs::parse(&argv(&[
            "-l",
            "http://:8080",
            "--ssl",
            "cert.pem,key.pem",
            "--pac",
            "/proxy.pac",
        ]))
        .unwrap();
        let lines = banner_lines(&args);
        let joined = lines.join("\n");
        assert!(joined.contains("listen:"));
        assert!(joined.contains("tls:      enabled"));
        assert!(joined.contains("pac:      enabled"));
    }
}
