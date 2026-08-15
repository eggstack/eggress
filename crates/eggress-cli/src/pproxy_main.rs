use std::process::ExitCode;
use std::time::Duration;

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
    -d                     Debug traceback/error visibility (repeatable)
    -v                     Verbose connection output (repeatable; -vv adds traffic stats)
    --ssl <CERT,KEY>       Enable TLS on listeners
    --pac <PATH>           Serve PAC content at PATH
    --test <URL>           Test the supplied target and exit
    --sys                  Apply the selected local HTTP/SOCKS5 listener as system proxy
    --reuse                Listener SO_REUSEPORT (Linux only)
    --auth <SECONDS>       Per-client source-IP auth reuse interval
    --get <PATH,FILE>      Serve FILE at PATH through the admin server
    --daemon               Daemon mode (unsupported; use systemd)
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

    if args.iter().any(|a| a == "--version") {
        print_version();
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let pproxy_args = if eggress_pproxy_compat::PproxyArgs::has_args(&args) {
        match eggress_pproxy_compat::PproxyArgs::parse(&args) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("pproxy: error: {e}");
                std::process::exit(2); // EXIT_CLI_PARSE_ERROR
            }
        }
    } else {
        eggress_pproxy_compat::PproxyArgs::default_args()
    };

    if let Some(flag) = pproxy_args.strict_parser_violations().first() {
        eprintln!("pproxy: error: unknown option or positional argument '{flag}'");
        std::process::exit(2);
    }

    if let Err(e) = pproxy_args.validate_strict_values() {
        eprintln!("pproxy: error: {e}");
        std::process::exit(2);
    }

    let output = match eggress_pproxy_compat::translate_pproxy_args(&pproxy_args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("pproxy: error: {e}");
            std::process::exit(3); // EXIT_CONFIG_VALIDATION
        }
    };

    // Fatal gating: unknown flags and unsupported features stop startup.
    // The shared gate is the single source of truth for the fail-closed
    // policy applied by every compatibility execution entry point.
    let gate = eggress_pproxy_compat::evaluate_execution_gate(&pproxy_args, &output);
    if !gate.allows_start() {
        for blocker in &gate.blockers {
            match blocker {
                eggress_pproxy_compat::BlockReason::UnknownFlag(flag) => {
                    eprintln!("pproxy: error: unknown option '{flag}'");
                }
                eggress_pproxy_compat::BlockReason::Unsupported(u) => {
                    eprintln!("pproxy: error: {u}");
                }
            }
        }
        eprintln!();
        if gate
            .blockers
            .iter()
            .any(|b| matches!(b, eggress_pproxy_compat::BlockReason::UnknownFlag(_)))
        {
            eprintln!("Run 'eggress pproxy check -- <args>' for supported options.");
            std::process::exit(2); // EXIT_CLI_PARSE_ERROR
        }
        eprintln!("These features are not supported by eggress and prevent startup.");
        eprintln!("Run 'eggress pproxy check -- <args>' for detailed compatibility report.");
        std::process::exit(5); // EXIT_UNSUPPORTED_FEATURE
    }

    for w in &gate.warnings {
        eprintln!("pproxy: note: {w}");
    }

    let test_target = pproxy_args.test_target();

    if test_target.is_none() {
        print_startup_banner(&pproxy_args, &output);
    }

    // Parse translated TOML into a validated RuntimeConfig in-memory.
    // No temporary file is created; the config lives entirely in process memory.
    let (rt_config, _warnings) =
        match eggress_config::validate_and_compile_toml_with_warnings(&output.toml) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pproxy: config error: {e}");
                std::process::exit(3); // EXIT_CONFIG_VALIDATION
            }
        };

    if let Some(target) = test_target {
        let timeout = Duration::from_secs(10);
        let target = match eggress_cli::parse_pproxy_test_target(target) {
            Ok(target) => target.to_string(),
            Err(error) => {
                eprintln!("pproxy: error: {error}");
                std::process::exit(2);
            }
        };
        if rt_config.upstreams.is_empty() {
            std::process::exit(0);
        }
        let exit_code = eggress_cli::run_upstream_test(&rt_config, Some(&target), timeout, false);
        std::process::exit(exit_code);
    }

    init_logging(&pproxy_args);

    tracing::info!("starting eggress with pproxy-compatible config");

    // Start from the in-memory RuntimeConfig. No config file path is provided,
    // so SIGHUP reload is disabled (there is no stable user-authored config
    // file to reload from in compatibility mode).
    let compatibility_options = eggress_runtime::CompatibilityOptions {
        auth_timeout: Some(pproxy_args.effective_auth_timeout()),
        system_proxy: pproxy_args.system_proxy,
        debug: pproxy_args.debug,
        verbose_level: pproxy_args.verbose_level,
    };
    match eggress_runtime::ServiceSupervisor::start_from_config_with_options(
        rt_config,
        None,
        compatibility_options,
    ) {
        Ok(mut supervisor) => {
            if let Err(e) = supervisor.run() {
                eprintln!("pproxy: runtime error: {e}");
                std::process::exit(1); // EXIT_RUNTIME_FAILURE
            }
        }
        Err(e) => {
            eprintln!("pproxy: runtime error: {e}");
            std::process::exit(1); // EXIT_RUNTIME_FAILURE
        }
    }

    ExitCode::SUCCESS
}

fn init_logging(pproxy_args: &eggress_pproxy_compat::PproxyArgs) {
    let level = pproxy_args.default_log_level();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .compact()
        .init();
}

fn print_startup_banner(
    pproxy_args: &eggress_pproxy_compat::PproxyArgs,
    _output: &eggress_pproxy_compat::TranslationOutput,
) {
    eprintln!("{VERSION}");

    for local in &pproxy_args.local {
        eprintln!("  listen:   {local}");
    }
    for remote in &pproxy_args.remotes {
        eprintln!("  remote:   {remote}");
    }

    let has_udp = pproxy_args
        .known_unsupported
        .iter()
        .any(|f| f.starts_with("udp-listen="));
    if has_udp {
        for flag in &pproxy_args.known_unsupported {
            if let Some(addr) = flag.strip_prefix("udp-listen=") {
                eprintln!("  udp:      {addr}");
            }
        }
    }

    let has_ssl = pproxy_args
        .known_unsupported
        .iter()
        .any(|f| f.starts_with("ssl="));
    if has_ssl {
        eprintln!("  tls:      enabled");
    }

    let has_pac = pproxy_args
        .known_unsupported
        .iter()
        .any(|f| f.starts_with("pac="));
    if has_pac {
        eprintln!("  pac:      enabled");
    }

    if pproxy_args.reuse_port {
        eprintln!("  reuse:    SO_REUSEPORT");
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

    #[test]
    fn test_version_string() {
        assert!(VERSION.contains("eggress-pproxy-compat"));
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
}
