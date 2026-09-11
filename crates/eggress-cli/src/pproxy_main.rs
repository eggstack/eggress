//! Standalone `pproxy` compatibility binary: thin facade over the shared
//! [`eggress_cli::pproxy_exec`] pipeline.
//!
//! This binary keeps its exact pproxy-style `--version`/`--help` surface
//! and gains no Eggress-native subcommands. Business decisions (parse,
//! gate, translate, compile, test, supervise) are owned by the facade so
//! this entry point cannot diverge from `eggress pproxy run`; only
//! presentation (the `pproxy: ` diagnostic prefix, the startup banner)
//! lives here.

use std::process::ExitCode;

use eggress_cli::pproxy_exec::{self, PreparedAction};

const VERSION: &str = concat!("eggress-pproxy-compat ", env!("CARGO_PKG_VERSION"));

const HELP_TEXT: &str = "\
pproxy compatibility binary (eggress-pproxy-compat)

This binary provides compatibility with pproxy 2.7.9 command-line
interface. It translates pproxy-style arguments to eggress TOML configuration
and starts the eggress proxy service.

Unsupported options cause startup to fail with a non-zero exit code.
Run 'eggress pproxy check -- <args>' to inspect all classifications.

USAGE:
    pproxy [OPTIONS]

OPTIONS:
    -l <URI>                Local listener URI (repeatable)
    -r <URI>                Remote/upstream URI (repeatable)
    -ul <URI>               UDP listener URI (repeatable)
    -ur <URI>               UDP upstream URI (repeatable)
    -b <PATTERN>           Block rule pattern (regex)
    -a <SECONDS>           Alive/health check interval
    -s <SCHEDULER>         Scheduler (rr, fa, rc, lc)
    -d                     Debug-level compatibility diagnostics
    -v                     Increase compatibility tracing verbosity (repeatable)
    --ssl <CERT,KEY>       Enable TLS on listeners
    --pac <PATH>           Serve PAC content at PATH
    --test <URL>           Test the supplied target and exit
    --sys                  Apply the selected local HTTP/SOCKS5 listener as system proxy
    --reuse                Listener SO_REUSEPORT (Linux only)
    --auth <SECONDS>       Per-client source-IP auth reuse interval
    --get <PATH,FILE>      Serve FILE at PATH through the admin server
    --daemon               Linux daemon-compatible re-exec (optional feature)
    --version              Print version and exit
    -h, --help             Print this help and exit

EXAMPLES:
    pproxy -l http://:8080 -r socks5://127.0.0.1:1080
    pproxy -l socks5://:1080 -r http://proxy:8080 -r socks5://backup:1080
    pproxy -l http://:8080 -r socks5://127.0.0.1:1080 --ssl cert.pem,key.pem
    pproxy -l http://:8080 -r socks5://127.0.0.1:1080 --test http://example.com

NOTE:
    This is an eggress compatibility wrapper, not the original pproxy.
    Some features are unsupported or behave differently. Run
    'eggress pproxy check -- <args>' to see compatibility details.
";

/// Diagnostic prefix applied by the shared facade to entry-point messages.
const DIAG_PREFIX: &str = "pproxy: ";

fn print_version() {
    println!("{VERSION}");
}

fn print_help() {
    print!("{HELP_TEXT}");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    match pproxy_exec::prepare(&args) {
        Ok(PreparedAction::PrintVersion) => {
            print_version();
            return ExitCode::SUCCESS;
        }
        Ok(PreparedAction::PrintHelp) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Ok(PreparedAction::Run(gated)) => {
            if gated.test_target.is_none() {
                print_startup_banner(&gated.args);
            }

            let prepared = match pproxy_exec::compile(*gated) {
                Ok(prepared) => prepared,
                Err(failure) => {
                    eprintln!("{DIAG_PREFIX}error: {}", failure.message);
                    std::process::exit(failure.code);
                }
            };

            // `-d`/`-v` log policy is resolved here before runtime
            // construction via `default_log_level()` with explicit
            // `RUST_LOG` precedence. The generic supervisor consumes
            // ordinary tracing only.
            init_logging(&prepared.args);

            let code = pproxy_exec::execute(prepared, DIAG_PREFIX);
            std::process::exit(code);
        }
        Err(failure) => {
            eprintln!("{DIAG_PREFIX}error: {}", failure.message);
            std::process::exit(failure.code);
        }
    }

    #[allow(unreachable_code)]
    ExitCode::SUCCESS
}

fn init_logging(pproxy_args: &eggress_pproxy_compat::PproxyArgs) {
    let level = pproxy_args.default_log_level();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .pretty()
        .init();
}

fn print_startup_banner(pproxy_args: &eggress_pproxy_compat::PproxyArgs) {
    eprintln!("{VERSION}");
    for line in pproxy_exec::banner_lines(pproxy_args) {
        eprintln!("{line}");
    }
    eprintln!();
    eprintln!("pproxy started, waiting for connections...");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_flag() {
        assert!(HELP_TEXT.contains("--help"));
        assert!(HELP_TEXT.contains("--version"));
        assert!(HELP_TEXT.contains("-l"));
        assert!(HELP_TEXT.contains("-r"));
        assert!(HELP_TEXT.contains("--test"));
        assert!(HELP_TEXT.contains("--sys"));
        assert!(HELP_TEXT.contains("--ssl"));
        assert!(HELP_TEXT.contains("--pac"));
        assert!(HELP_TEXT.contains("-d"));
        assert!(HELP_TEXT.contains("--reuse"));
        assert!(HELP_TEXT.contains("--auth"));
        assert!(HELP_TEXT.contains("--daemon"));
    }

    /// Help/parser drift guard: every option the frozen parser recognizes
    /// must appear in the static help text, so the two cannot silently
    /// diverge. Help text stays custom (exact pproxy compatibility), but
    /// its inventory is pinned to the parser metadata.
    #[test]
    fn help_covers_every_recognized_option() {
        for option in eggress_pproxy_compat::PproxyArgs::recognized_option_names() {
            assert!(
                HELP_TEXT.contains(option),
                "help text drifted from the parser: missing option '{option}'"
            );
        }
    }

    #[test]
    fn test_version_string() {
        assert!(VERSION.contains("eggress-pproxy-compat"));
        // The binary version line and the shared facade version string are
        // the same compatibility surface.
        assert_eq!(VERSION, eggress_pproxy_compat::PproxyArgs::version_string());
    }

    #[test]
    fn in_memory_config_from_translated_toml() {
        let args = vec![
            "-l".to_string(),
            "http://127.0.0.1:0".to_string(),
            "-r".to_string(),
            "socks5://127.0.0.1:1080".to_string(),
        ];
        let pproxy_args = eggress_pproxy_compat::PproxyArgs::parse(&args).unwrap();
        let output = eggress_pproxy_compat::translate_pproxy_args(&pproxy_args).unwrap();

        // TOML is available for diagnostics
        assert!(!output.toml.is_empty());
        assert!(output.toml.contains("[[listeners]]"));

        // Parse the TOML in-memory without touching the filesystem
        let (rt_config, _warnings) =
            eggress_config::validate_and_compile_toml_with_warnings(&output.toml).unwrap();
        assert_eq!(rt_config.listeners.len(), 1);
        assert!(rt_config.listeners[0].name.starts_with("pproxy-"));
    }

    #[test]
    fn in_memory_config_rejects_invalid_toml() {
        let bad_toml = "version = 1\n[[listeners]]\nname = \"bad\"\nbind = \"not-a-addr\"\nprotocols = [\"http\"]\nconnection_limit = 0\n";
        let result = eggress_config::validate_and_compile_toml_with_warnings(bad_toml);
        assert!(result.is_err());
    }

    #[test]
    fn translation_output_toml_is_parseable() {
        let args = vec![
            "-l".to_string(),
            "socks5://127.0.0.1:0".to_string(),
            "-r".to_string(),
            "http://proxy:8080".to_string(),
        ];
        let pproxy_args = eggress_pproxy_compat::PproxyArgs::parse(&args).unwrap();
        let output = eggress_pproxy_compat::translate_pproxy_args(&pproxy_args).unwrap();

        // The generated TOML must be valid and compile to a RuntimeConfig
        let result = eggress_config::validate_and_compile_toml(&output.toml);
        assert!(
            result.is_ok(),
            "translated TOML should be valid: {:?}",
            result.err()
        );
    }

    #[test]
    fn shared_facade_prepares_equivalent_config_for_both_entry_points() {
        // Both binaries delegate to `pproxy_exec::prepare`; the prepared
        // runtime configuration and failure classification must be
        // identical regardless of entry point.
        let args = vec![
            "-l".to_string(),
            "http://127.0.0.1:0".to_string(),
            "-r".to_string(),
            "http://127.0.0.1:8080".to_string(),
        ];
        let action = pproxy_exec::prepare(&args).expect("supported args must prepare");
        match action {
            PreparedAction::Run(gated) => {
                assert!(gated.warnings.is_empty());
                let prepared = pproxy_exec::compile(*gated).expect("translated TOML must compile");
                assert_eq!(prepared.rt_config.listeners.len(), 1);
            }
            _ => panic!("expected a prepared run"),
        }
    }
}
