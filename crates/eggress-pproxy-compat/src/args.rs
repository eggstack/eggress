use std::time::Duration;

use crate::error::CompatError;
use crate::uri::{PproxyChain, PproxyUri};
use crate::warnings::{CompatWarning, TranslationOutput};

fn take_required_value(
    raw: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, CompatError> {
    *index += 1;
    let value = raw
        .get(*index)
        .cloned()
        .ok_or_else(|| CompatError::MissingArgument(format!("{flag} requires a value")))?;
    if value.starts_with('-') && !(matches!(flag, "-a" | "--auth") && value.parse::<i64>().is_ok())
    {
        return Err(CompatError::MissingArgument(format!(
            "{flag} requires a value"
        )));
    }
    Ok(value)
}

fn parse_auth_duration(value: &str) -> Result<Duration, CompatError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CompatError::InvalidArgs {
            message: "--auth requires a non-empty numeric value".to_string(),
        });
    }
    let seconds: i64 = trimmed.parse().map_err(|_| CompatError::InvalidArgs {
        message: format!("--auth value '{}' is not a valid integer", trimmed),
    })?;
    // argparse accepts negative integers. pproxy's AuthTable then expires
    // those entries immediately, so preserve that behavior as a zero-length
    // Rust duration instead of inventing a parser-side upper/lower bound.
    Ok(Duration::from_secs(seconds.max(0) as u64))
}

/// Parsed pproxy-compatible CLI arguments.
#[derive(Debug, Clone)]
pub struct PproxyArgs {
    /// Local listener URIs (from `-l` flags).
    pub local: Vec<String>,
    /// Remote/upstream URIs (from `-r` flags).
    pub remotes: Vec<String>,
    /// Verbosity level derived from `-v`/`-vv`/`-vvv` flags.
    pub verbose_level: u8,
    /// `-d` flag: debug-level compatibility diagnostics.
    pub debug: bool,
    /// Number of `-d` occurrences, including clustered short options.
    pub debug_level: u8,
    /// `--daemon` flag: daemon mode request (unsupported).
    pub daemon: bool,
    /// `--reuse` flag: listener SO_REUSEPORT.
    pub reuse_port: bool,
    /// `--auth <seconds>`: per-client authentication reuse interval.
    pub auth_timeout: Option<Duration>,
    /// `--sys` flag: apply the selected compatibility listener as the system proxy.
    pub system_proxy: bool,
    /// `-h`/`--help` was requested.
    pub help: bool,
    /// `--version` was requested.
    pub version: bool,
    /// Known-but-unsupported flags that require a translation decision.
    pub known_unsupported: Vec<String>,
    /// Unknown flags that are not recognized.
    pub unknown_flags: Vec<String>,
    /// Options accepted by the migration translator but not by the frozen
    /// pproxy 2.7.9 parser, plus positional arguments and long aliases.
    /// Compatibility execution treats these as parser errors.
    pub strict_violations: Vec<String>,
}

impl PproxyArgs {
    /// Check whether any arguments were provided.
    pub fn has_args(raw: &[String]) -> bool {
        !raw.is_empty()
    }

    /// Return the target supplied to `--test`, preserving the parser's
    /// value-taking semantics for both compatibility execution entry points.
    pub fn test_target(&self) -> Option<&str> {
        self.known_unsupported
            .iter()
            .find_map(|flag| flag.strip_prefix("test="))
    }

    /// Create default pproxy args equivalent to running `pproxy` with no arguments.
    ///
    /// Real pproxy defaults to a mixed HTTP/SOCKS4/SOCKS5 listener on `:8080`
    /// with direct routing.
    pub fn default_args() -> Self {
        Self {
            local: vec!["http+socks4+socks5://:8080".to_string()],
            remotes: vec![],
            verbose_level: 0,
            debug: false,
            debug_level: 0,
            daemon: false,
            reuse_port: false,
            auth_timeout: None,
            system_proxy: false,
            help: false,
            version: false,
            known_unsupported: vec![],
            unknown_flags: vec![],
            strict_violations: vec![],
        }
    }

    /// Parse from raw argument list (excluding argv[0]).
    pub fn parse(raw: &[String]) -> Result<Self, CompatError> {
        let mut local = Vec::new();
        let mut remotes = Vec::new();
        let mut verbose_level: u8 = 0;
        let mut debug = false;
        let mut debug_level: u8 = 0;
        let mut daemon = false;
        let mut reuse_port = false;
        let mut auth_timeout: Option<Duration> = None;
        let mut system_proxy = false;
        let mut help = false;
        let mut version = false;
        let mut known_unsupported = Vec::new();
        let mut unknown_flags = Vec::new();
        let mut strict_violations = Vec::new();
        let mut i = 0;

        while i < raw.len() {
            let arg = &raw[i];
            match arg.as_str() {
                "-h" | "--help" => {
                    help = true;
                }
                "--version" => {
                    version = true;
                }
                "-l" | "--listen" => {
                    if arg == "--listen" {
                        strict_violations.push(arg.clone());
                    }
                    local.push(take_required_value(raw, &mut i, arg)?);
                }
                "-r" | "--remote" => {
                    if arg == "--remote" {
                        strict_violations.push(arg.clone());
                    }
                    remotes.push(take_required_value(raw, &mut i, arg)?);
                }
                "--daemon" => {
                    daemon = true;
                }
                "-d" => {
                    debug = true;
                    debug_level = debug_level.saturating_add(1);
                }
                "--log" | "-log" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("log={value}"));
                    strict_violations.push(arg.clone());
                }
                "-ul" | "--udp-listen" => {
                    if arg == "--udp-listen" {
                        strict_violations.push(arg.clone());
                    }
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("udp-listen={value}"));
                }
                "-ur" | "--udp-remote" => {
                    if arg == "--udp-remote" {
                        strict_violations.push(arg.clone());
                    }
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("udp-remote={value}"));
                }
                "--rulefile" | "-rulefile" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("rulefile={value}"));
                    strict_violations.push(arg.clone());
                }
                "-v" | "-vv" | "-vvv" => {
                    verbose_level = verbose_level.saturating_add(arg.len() as u8 - 1);
                }
                short
                    if short.len() > 2
                        && short.starts_with('-')
                        && short[1..].chars().all(|c| c == 'd' || c == 'v') =>
                {
                    for flag in short[1..].chars() {
                        if flag == 'd' {
                            debug = true;
                            debug_level = debug_level.saturating_add(1);
                        } else {
                            verbose_level = verbose_level.saturating_add(1);
                        }
                    }
                }
                "-s" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    if !matches!(
                        value.as_str(),
                        "fa" | "rr"
                            | "rc"
                            | "lc"
                            | "first_available"
                            | "round_robin"
                            | "random_choice"
                            | "least_connection"
                    ) {
                        return Err(CompatError::InvalidArgs {
                            message: format!(
                                "invalid choice: '{}' (choose from fa, rr, rc, lc)",
                                value
                            ),
                        });
                    }
                    if !matches!(value.as_str(), "fa" | "rr" | "rc" | "lc") {
                        // Keep migration-only scheduler aliases parseable, but
                        // make strict executable validation reject them.
                        strict_violations.push(format!("-s {value}"));
                    }
                    known_unsupported.push(format!("scheduler={value}"));
                }
                "-a" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("alive={value}"));
                }
                "--ssl" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("ssl={value}"));
                }
                "-b" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("block={value}"));
                }
                "--pac" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("pac={value}"));
                }
                "--test" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("test={value}"));
                }
                "--sys" => {
                    system_proxy = true;
                }
                "--reuse" => {
                    reuse_port = true;
                }
                "--get" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    known_unsupported.push(format!("get={value}"));
                }
                "--auth" => {
                    let value = take_required_value(raw, &mut i, arg)?;
                    auth_timeout = Some(parse_auth_duration(&value)?);
                }
                other if other.starts_with('-') => {
                    unknown_flags.push(other.to_string());
                }
                other => {
                    // The migration translator historically accepted positional
                    // URIs. Keep parsing them for API callers, but mark them so
                    // strict executable entry points can reject them like
                    // argparse does.
                    if local.is_empty() {
                        local.push(other.to_string());
                    } else {
                        remotes.push(other.to_string());
                    }
                    strict_violations.push(other.to_string());
                }
            }
            i += 1;
        }

        Ok(PproxyArgs {
            local,
            remotes,
            verbose_level,
            debug,
            debug_level,
            daemon,
            reuse_port,
            auth_timeout,
            system_proxy,
            help,
            version,
            known_unsupported,
            unknown_flags,
            strict_violations,
        })
    }

    /// Identify unrecognized flags and return diagnostics for them.
    pub fn unknown_flag_diagnostics(&self) -> Vec<CompatWarning> {
        let mut warnings = Vec::new();
        for flag in &self.unknown_flags {
            warnings.push(CompatWarning {
                category: "unknown-flag",
                message: format!("unrecognized option '{}'", flag),
            });
        }
        warnings
    }

    /// Return a TranslationOutput containing the unknown-flag diagnostics.
    pub fn unknown_flag_translation_output(&self) -> TranslationOutput {
        let warnings = self.unknown_flag_diagnostics();
        TranslationOutput::new(String::new()).with_warnings(warnings)
    }

    /// Check if there are any unknown or unsupported flags.
    pub fn has_unknown_or_unsupported(&self) -> bool {
        !self.unknown_flags.is_empty() || !self.known_unsupported.is_empty() || self.daemon
    }

    /// Return parser violations against the exact 2.7.9 command surface.
    pub fn strict_parser_violations(&self) -> Vec<String> {
        self.unknown_flags
            .iter()
            .cloned()
            .chain(self.strict_violations.iter().cloned())
            .collect()
    }

    /// Version string used by all compatibility entry points.
    pub fn version_string() -> String {
        format!("eggress-pproxy-compat {}", env!("CARGO_PKG_VERSION"))
    }

    /// Validate values whose upstream argparse `type=` callbacks run during
    /// parsing. URI failures therefore stay in the CLI-parse category rather
    /// than being reported as a later runtime/configuration failure.
    pub fn validate_strict_values(&self) -> Result<(), CompatError> {
        self.parse_local_uris()
            .map_err(|error| CompatError::InvalidArgs {
                message: format!("invalid -l URI: {error}"),
            })?;
        self.parse_remote_chains()
            .map_err(|error| CompatError::InvalidArgs {
                message: format!("invalid -r URI: {error}"),
            })?;
        if let Some(scheduler) = self
            .known_unsupported
            .iter()
            .find_map(|flag| flag.strip_prefix("scheduler="))
        {
            if !matches!(scheduler, "fa" | "rr" | "rc" | "lc") {
                return Err(CompatError::InvalidArgs {
                    message: format!(
                        "invalid choice: '{}' (choose from fa, rr, rc, lc)",
                        scheduler
                    ),
                });
            }
        }
        if let Some(interval) = self
            .known_unsupported
            .iter()
            .find_map(|flag| flag.strip_prefix("alive="))
        {
            interval
                .parse::<i64>()
                .map_err(|_| CompatError::InvalidArgs {
                    message: format!("-a value '{}' is not a valid integer", interval),
                })?;
        }
        for kind in ["udp-listen=", "udp-remote="] {
            for value in self
                .known_unsupported
                .iter()
                .filter_map(|flag| flag.strip_prefix(kind))
            {
                crate::uri::parse_pproxy_uri(value).map_err(|error| CompatError::InvalidArgs {
                    message: format!("invalid {} URI: {error}", kind.trim_end_matches('=')),
                })?;
            }
        }
        Ok(())
    }

    /// Return the pproxy default re-authentication interval when `--auth` was
    /// omitted. The field remains optional so translation/API callers can
    /// distinguish an explicit compatibility option from the parser default.
    pub fn effective_auth_timeout(&self) -> Duration {
        self.auth_timeout
            .unwrap_or_else(|| Duration::from_secs(86_400 * 30))
    }

    /// Default tracing log level chosen from `-d` and `-v` verbosity flags.
    ///
    /// Resolution order (highest precedence first):
    ///
    /// 1. `-vvv` -> `trace`
    /// 2. `-v` / `-vv` / `-d` -> `debug`
    /// 3. otherwise -> `info`
    ///
    /// `-d` is independent of `-v` and of `--daemon`. This helper is the
    /// single source of truth used by the standalone compatibility binary
    /// (and tests) so the observable behavior of `-d` is testable without
    /// depending on tracing internals.
    ///
    /// An explicit `RUST_LOG` environment variable remains authoritative
    /// at the `tracing_subscriber` layer; this helper is only consulted when
    /// `RUST_LOG` is unset or invalid.
    pub fn default_log_level(&self) -> &'static str {
        if self.verbose_level >= 3 {
            "trace"
        } else if self.verbose_level >= 1 || self.debug {
            "debug"
        } else {
            "info"
        }
    }

    /// Parse all local URIs into typed representations.
    pub fn parse_local_uris(&self) -> Result<Vec<PproxyUri>, CompatError> {
        self.local
            .iter()
            .map(|s| crate::uri::parse_pproxy_uri(s))
            .collect()
    }

    /// Parse all remote URIs into typed representations.
    pub fn parse_remote_uris(&self) -> Result<Vec<PproxyUri>, CompatError> {
        self.remotes
            .iter()
            .map(|s| crate::uri::parse_pproxy_uri(s))
            .collect()
    }

    /// Parse all remote URIs into chain representations (supports `__` separators).
    pub fn parse_remote_chains(&self) -> Result<Vec<PproxyChain>, CompatError> {
        self.remotes
            .iter()
            .map(|s| crate::uri::parse_pproxy_chain(s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
        ])
        .unwrap();
        assert_eq!(args.local.len(), 1);
        assert_eq!(args.remotes.len(), 1);
        assert_eq!(args.local[0], "socks5://127.0.0.1:1080");
        assert_eq!(args.remotes[0], "http://proxy:8080");
    }

    #[test]
    fn test_parse_help_and_version_actions() {
        let help = PproxyArgs::parse(&["--help".into()]).unwrap();
        assert!(help.help);
        assert!(!help.version);
        assert!(help.strict_parser_violations().is_empty());

        let version = PproxyArgs::parse(&["--version".into()]).unwrap();
        assert!(version.version);
        assert!(!version.help);
        assert_eq!(
            PproxyArgs::version_string(),
            format!("eggress-pproxy-compat {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn test_parse_positional() {
        let args =
            PproxyArgs::parse(&["socks5://127.0.0.1:1080".into(), "http://proxy:8080".into()])
                .unwrap();
        assert_eq!(args.local.len(), 1);
        assert_eq!(args.remotes.len(), 1);
    }

    #[test]
    fn test_parse_multiple_remotes() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy1:8080".into(),
            "-r".into(),
            "socks5://proxy2:1080".into(),
        ])
        .unwrap();
        assert_eq!(args.remotes.len(), 2);
    }

    #[test]
    fn test_parse_missing_value() {
        let result = PproxyArgs::parse(&["-l".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_daemon_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--daemon".into(),
        ])
        .unwrap();
        assert!(args.daemon);
        assert!(!args.debug);
    }

    #[test]
    fn test_parse_debug_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-d".into()])
            .unwrap();
        assert!(args.debug);
        assert_eq!(args.debug_level, 1);
        assert!(!args.daemon);
    }

    #[test]
    fn test_count_actions_and_short_clusters() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://:8080".into(),
            "-dd".into(),
            "-v".into(),
            "-vv".into(),
        ])
        .unwrap();
        assert_eq!(args.debug_level, 2);
        assert_eq!(args.verbose_level, 3);

        let clustered =
            PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-dv".into()]).unwrap();
        assert_eq!(clustered.debug_level, 1);
        assert_eq!(clustered.verbose_level, 1);
    }

    #[test]
    fn test_strict_parser_rejects_extensions_and_positionals() {
        let args = PproxyArgs::parse(&[
            "--log".into(),
            "access.log".into(),
            "proxy://listener".into(),
        ])
        .unwrap();
        assert!(args
            .strict_parser_violations()
            .iter()
            .any(|value| value == "--log"));
        assert!(args
            .strict_parser_violations()
            .iter()
            .any(|value| value == "proxy://listener"));
    }

    #[test]
    fn test_target_preserves_value_taking_option() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://:8080".into(),
            "--test".into(),
            "https://example.invalid/health".into(),
        ])
        .unwrap();
        assert_eq!(args.test_target(), Some("https://example.invalid/health"));
    }

    #[test]
    fn test_d_and_daemon_independent() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-d".into(),
            "--daemon".into(),
        ])
        .unwrap();
        assert!(args.debug);
        assert!(args.daemon);
    }

    #[test]
    fn test_d_never_sets_daemon() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-d".into()]).unwrap();
        assert!(args.debug);
        assert!(!args.daemon);
    }

    #[test]
    fn test_daemon_never_sets_debug() {
        let args =
            PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "--daemon".into()]).unwrap();
        assert!(!args.debug);
        assert!(args.daemon);
    }

    #[test]
    fn test_parse_verbose_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-v".into()])
            .unwrap();
        assert_eq!(args.verbose_level, 1);
    }

    #[test]
    fn test_parse_scheduler_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-s".into(),
            "rr".into(),
        ])
        .unwrap();
        assert!(args.known_unsupported.contains(&"scheduler=rr".to_string()));
    }

    #[test]
    fn test_scheduler_migration_alias_is_not_strict_executable_surface() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-s".into(),
            "round_robin".into(),
        ])
        .unwrap();
        assert!(args
            .strict_parser_violations()
            .iter()
            .any(|value| value == "-s round_robin"));
        assert!(args.validate_strict_values().is_err());
    }

    #[test]
    fn test_parse_alive_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-a".into(),
            "10".into(),
        ])
        .unwrap();
        assert!(args.known_unsupported.contains(&"alive=10".to_string()));
    }

    #[test]
    fn test_parse_ssl_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        assert!(args
            .known_unsupported
            .contains(&"ssl=cert.pem,key.pem".to_string()));
    }

    #[test]
    fn test_parse_block_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-b".into(),
            ".*\\.example\\.com".into(),
        ])
        .unwrap();
        assert!(args
            .known_unsupported
            .contains(&"block=.*\\.example\\.com".to_string()));
    }

    #[test]
    fn test_parse_log_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--log".into(),
            "access.log".into(),
        ])
        .unwrap();
        assert!(args
            .known_unsupported
            .contains(&"log=access.log".to_string()));
    }

    #[test]
    fn test_parse_udp_flags() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-ul".into(),
            "socks5://:1081".into(),
            "-ur".into(),
            "socks5://proxy:1080".into(),
        ])
        .unwrap();
        assert!(args
            .known_unsupported
            .contains(&"udp-listen=socks5://:1081".to_string()));
        assert!(args
            .known_unsupported
            .contains(&"udp-remote=socks5://proxy:1080".to_string()));
    }

    #[test]
    fn test_parse_rulefile_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--rulefile".into(),
            "rules.txt".into(),
        ])
        .unwrap();
        assert!(args
            .known_unsupported
            .contains(&"rulefile=rules.txt".to_string()));
    }

    #[test]
    fn test_unknown_flag_diagnostics() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--unknown-flag".into(),
            "-x".into(),
        ])
        .unwrap();
        let warnings = args.unknown_flag_diagnostics();
        assert_eq!(warnings.len(), 2);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("--unknown-flag")));
        assert!(warnings.iter().any(|w| w.message.contains("-x")));
    }

    #[test]
    fn test_known_flags_no_unknown_warnings() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-v".into(),
            "-s".into(),
            "rr".into(),
            "-a".into(),
            "10".into(),
            "--daemon".into(),
            "--log".into(),
            "access.log".into(),
            "-ul".into(),
            "socks5://:1081".into(),
            "-ur".into(),
            "socks5://proxy:1080".into(),
            "--rulefile".into(),
            "rules.txt".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
            "-b".into(),
            ".*\\.example\\.com".into(),
        ])
        .unwrap();
        let warnings = args.unknown_flag_diagnostics();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_scheduler_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-s".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_log_missing_value() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--log".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_udp_listen_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-ul".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_udp_remote_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-ur".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_rulefile_missing_value() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--rulefile".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_alive_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-a".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_ssl_missing_value() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_block_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-b".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_known_flags_pac_test_sys_reuse_get() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--pac".into(),
            "/proxy.pac".into(),
            "--test".into(),
            "http://example.com".into(),
            "--sys".into(),
            "--reuse".into(),
            "--get".into(),
            "/index.html,body.txt".into(),
        ])
        .unwrap();
        assert!(args.system_proxy);
        assert!(args.reuse_port);
        let warnings = args.unknown_flag_diagnostics();
        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    }

    #[test]
    fn test_verbose_level_single() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-v".into()]).unwrap();
        assert_eq!(args.verbose_level, 1);
    }

    #[test]
    fn test_verbose_level_double() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-vv".into()]).unwrap();
        assert_eq!(args.verbose_level, 2);
    }

    #[test]
    fn test_verbose_level_triple() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-vvv".into()]).unwrap();
        assert_eq!(args.verbose_level, 3);
    }

    #[test]
    fn test_verbose_level_default_zero() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into()]).unwrap();
        assert_eq!(args.verbose_level, 0);
    }

    #[test]
    fn test_verbose_level_max_of_multiple() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://:8080".into(),
            "-v".into(),
            "-vvv".into(),
        ])
        .unwrap();
        assert_eq!(args.verbose_level, 4);
    }

    #[test]
    fn test_has_args_true() {
        assert!(PproxyArgs::has_args(&["-l".into(), "http://:8080".into()]));
    }

    #[test]
    fn test_has_args_false_empty() {
        assert!(!PproxyArgs::has_args(&[]));
    }

    #[test]
    fn test_default_args() {
        let args = PproxyArgs::default_args();
        assert_eq!(args.local, vec!["http+socks4+socks5://:8080"]);
        assert!(args.remotes.is_empty());
        assert_eq!(args.verbose_level, 0);
        assert!(!args.debug);
        assert!(!args.daemon);
        assert!(!args.reuse_port);
        assert!(args.auth_timeout.is_none());
        assert!(!args.system_proxy);
    }

    #[test]
    fn test_default_args_translates() {
        let args = PproxyArgs::default_args();
        let output = super::super::translate::translate_pproxy_args(&args).unwrap();
        assert!(output.toml.contains("8080"));
        assert!(output.toml.contains("socks5") || output.toml.contains("http"));
        assert!(!output.has_unsupported());
    }

    #[test]
    fn test_parse_reuse_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--reuse".into(),
        ])
        .unwrap();
        assert!(args.reuse_port);
    }

    #[test]
    fn test_parse_auth_valid() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--auth".into(),
            "3600".into(),
        ])
        .unwrap();
        assert_eq!(args.auth_timeout, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_parse_auth_zero() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--auth".into(),
            "0".into(),
        ])
        .unwrap();
        assert_eq!(args.auth_timeout, Some(Duration::from_secs(0)));
    }

    #[test]
    fn test_parse_auth_invalid_non_numeric() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--auth".into(),
            "abc".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_overflow() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--auth".into(),
            "9223372036854775808".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_negative_matches_argparse_integer() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://127.0.0.1:0".into(),
            "--auth".into(),
            "-1".into(),
        ])
        .unwrap();
        assert_eq!(args.auth_timeout, Some(Duration::ZERO));
    }

    #[test]
    fn test_parse_auth_missing_value() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--auth".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_sys_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--sys".into(),
        ])
        .unwrap();
        assert!(args.system_proxy);
    }

    #[test]
    fn test_has_unknown_or_unsupported_with_unknown() {
        let args =
            PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "--bogus".into()]).unwrap();
        assert!(args.has_unknown_or_unsupported());
    }

    #[test]
    fn test_has_unknown_or_unsupported_with_daemon() {
        let args =
            PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "--daemon".into()]).unwrap();
        assert!(args.has_unknown_or_unsupported());
    }

    #[test]
    fn test_no_raw_flags_field() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into()]).unwrap();
        assert!(args.known_unsupported.is_empty());
        assert!(args.unknown_flags.is_empty());
    }

    #[test]
    fn test_default_log_level_no_flags() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into()]).unwrap();
        assert_eq!(args.default_log_level(), "info");
    }

    #[test]
    fn test_default_log_level_d_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-d".into()]).unwrap();
        assert_eq!(args.default_log_level(), "debug");
    }

    #[test]
    fn test_default_log_level_v_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-v".into()]).unwrap();
        assert_eq!(args.default_log_level(), "debug");
    }

    #[test]
    fn test_default_log_level_vv_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-vv".into()]).unwrap();
        assert_eq!(args.default_log_level(), "debug");
    }

    #[test]
    fn test_default_log_level_vvv_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "-vvv".into()]).unwrap();
        assert_eq!(args.default_log_level(), "trace");
    }

    #[test]
    fn test_default_log_level_d_and_vv() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://:8080".into(),
            "-d".into(),
            "-vv".into(),
        ])
        .unwrap();
        assert_eq!(args.default_log_level(), "debug");
    }

    #[test]
    fn test_default_log_level_d_and_vvv() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://:8080".into(),
            "-d".into(),
            "-vvv".into(),
        ])
        .unwrap();
        assert_eq!(args.default_log_level(), "trace");
    }

    #[test]
    fn test_default_log_level_daemon_only_keeps_daemon_separate() {
        let args =
            PproxyArgs::parse(&["-l".into(), "http://:8080".into(), "--daemon".into()]).unwrap();
        assert!(args.daemon);
        assert!(!args.debug);
        assert_eq!(args.default_log_level(), "info");
    }

    #[test]
    fn test_default_log_level_d_still_works_with_daemon() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "http://:8080".into(),
            "-d".into(),
            "--daemon".into(),
        ])
        .unwrap();
        assert!(args.debug);
        assert_eq!(args.default_log_level(), "debug");
    }

    #[test]
    fn test_default_log_level_default_args() {
        let args = PproxyArgs::default_args();
        assert_eq!(args.default_log_level(), "info");
    }

    /// Table-driven arity test sourced from the checked-in baseline.
    /// Ensures that every value-taking option in the baseline correctly
    /// requires a value and fails when missing.
    #[test]
    fn test_baseline_value_arity() {
        // Each entry: (flag, expects_value)
        // "value" means the flag requires a following argument.
        let cases: &[(&[&str], bool)] = &[
            // Value-taking options (must have next arg)
            (&["-l", "http://:8080"], true),
            (&["-r", "http://proxy:8080"], true),
            (&["-ul", "socks5://:1081"], true),
            (&["-ur", "socks5://proxy:1080"], true),
            (&["--ssl", "cert.pem,key.pem"], true),
            (&["--pac", "/proxy.pac"], true),
            (&["--test", "http://example.com"], true),
            (&["--auth", "3600"], true),
            (&["--get", "/index.html,body.txt"], true),
            (&["-s", "rr"], true),
            (&["-a", "10"], true),
            (&["-b", ".*\\.example\\.com"], true),
            (&["--rulefile", "rules.txt"], true),
            (&["--log", "access.log"], true),
            // Boolean flags (no value needed)
            (&["-v"], false),
            (&["-d"], false),
            (&["--daemon"], false),
            (&["--sys"], false),
            (&["--reuse"], false),
        ];

        for (args_slice, expects_value) in cases {
            let raw: Vec<String> = args_slice.iter().map(|s| s.to_string()).collect();
            let result = PproxyArgs::parse(&raw);
            if *expects_value && raw.len() == 1 {
                // Single value-taking flag with no value should fail
                assert!(
                    result.is_err(),
                    "expected error for {:?} (missing value), got Ok",
                    args_slice
                );
            } else {
                // Complete flag+value or boolean flag should succeed
                assert!(
                    result.is_ok(),
                    "expected Ok for {:?}, got Err: {:?}",
                    args_slice,
                    result.err()
                );
            }
        }
    }
}
