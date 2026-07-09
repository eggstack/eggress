use std::process::ExitCode;

const VERSION: &str = concat!("eggress-pproxy-compat ", env!("CARGO_PKG_VERSION"));

const HELP_TEXT: &str = "\
pproxy compatibility binary (eggress-pproxy-compat)

This binary provides drop-in compatibility with pproxy 2.7.9 command-line
interface. It translates pproxy-style arguments to eggress TOML configuration
and starts the eggress proxy service.

USAGE:
    pproxy [OPTIONS] <LISTENER_URI> <UPSTREAM_URI>
    pproxy [OPTIONS] -l <URI> -r <URI>

OPTIONS:
    -l, --listen <URI>     Local listener URI (e.g., http://0.0.0.0:8080)
    -r, --remote <URI>     Remote/upstream URI (e.g., socks5://127.0.0.1:1080)
    -ul, --udp-listen <A>  UDP listener address (e.g., socks5://:1081)
    -ur, --udp-remote <U>  UDP upstream URI
    -b <PATTERN>           Block rule pattern (regex)
    -a <SECONDS>           Alive/health check interval
    -s <SCHEDULER>         Scheduler (rr, fa, rc, lc)
    -v                     Verbose mode (sets RUST_LOG=debug)
    --ssl <CERT,KEY>       Enable TLS on listeners
    --rulefile <PATH>      Load routing rules from file
    --log <PATH>           Log file path (ignored; stderr used)
    --pac                  Enable PAC file serving
    --test                 Test upstream connectivity and exit
    --sys                  Inspect system proxy settings before starting
    --reuse                Connection pooling (intentional non-parity)
    --get                  Fetch URL via proxy (use curl instead)
    --daemon, -d           Daemon mode (not supported; use systemd)
    --version              Print version and exit
    -h, --help             Print this help and exit

EXAMPLES:
    pproxy -l http://:8080 -r socks5://127.0.0.1:1080
    pproxy -l socks5://:1080 -r http://proxy:8080 -r socks5://backup:1080
    pproxy -l http://:8080 -r socks5://127.0.0.1:1080 --ssl cert.pem,key.pem
    pproxy -l http://:8080 -r socks5://127.0.0.1:1080 --test

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

/// Resolve the `eggress` binary path.
///
/// First tries to find a sibling `eggress` binary next to the current executable.
/// Falls back to just `"eggress"` and lets the system PATH resolve it.
fn resolve_eggress_binary() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("eggress");
            if sibling.exists() {
                return sibling;
            }
        }
    }
    std::path::PathBuf::from("eggress")
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

    let output = match eggress_pproxy_compat::translate_pproxy_args(&pproxy_args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("pproxy: error: {e}");
            std::process::exit(3); // EXIT_CONFIG_VALIDATION
        }
    };

    if output.has_unsupported() {
        for u in &output.unsupported {
            eprintln!("pproxy: warning: {u}");
        }
        eprintln!();
        eprintln!(
            "Some features are not supported by eggress. Service may not behave as expected."
        );
        eprintln!("Run 'eggress pproxy check -- <args>' for detailed compatibility report.");
    }

    for w in &output.warnings {
        eprintln!("pproxy: note: {w}");
    }

    let has_sys = pproxy_args.raw_flags.iter().any(|f| f == "sys");
    if has_sys {
        let result = eggress_system_proxy::inspect_system_proxy();
        print_system_proxy_inspection(&result);
    }

    let has_test = pproxy_args.raw_flags.iter().any(|f| f == "test");

    let tmp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("pproxy: failed to create temp directory: {e}");
            std::process::exit(1); // EXIT_RUNTIME_FAILURE
        }
    };
    let config_path = tmp_dir.path().join("pproxy-compat.toml");
    if let Err(e) = std::fs::write(&config_path, &output.toml) {
        eprintln!("pproxy: failed to write config: {e}");
        std::process::exit(1); // EXIT_RUNTIME_FAILURE
    }

    print_startup_banner(&pproxy_args, &output);

    if has_test {
        // Resolve the eggress binary: look for a sibling 'eggress' binary
        // next to the current executable (pproxy). This avoids the recursion
        // problem where current_exe() resolves to the pproxy binary itself.
        let eggress_bin = resolve_eggress_binary();
        let status = std::process::Command::new(&eggress_bin)
            .args([
                "upstream",
                "test",
                "-c",
                config_path.to_str().unwrap_or_default(),
            ])
            .status();
        match status {
            Ok(s) => std::process::exit(s.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("pproxy: failed to run upstream test: {e}");
                std::process::exit(1); // EXIT_RUNTIME_FAILURE
            }
        }
    }

    init_logging(&pproxy_args);

    tracing::info!("starting eggress with pproxy-compatible config");

    match eggress_runtime::ServiceSupervisor::start(config_path.to_str().unwrap_or_default()) {
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
    let level = match pproxy_args.verbose_level {
        0 => "info",
        1 => "debug",
        2 => "debug",
        _ => "trace",
    };

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
        .raw_flags
        .iter()
        .any(|f| f.starts_with("udp-listen="));
    if has_udp {
        for flag in &pproxy_args.raw_flags {
            if let Some(addr) = flag.strip_prefix("udp-listen=") {
                eprintln!("  udp:      {addr}");
            }
        }
    }

    let has_ssl = pproxy_args.raw_flags.iter().any(|f| f.starts_with("ssl="));
    if has_ssl {
        eprintln!("  tls:      enabled");
    }

    let has_pac = pproxy_args.raw_flags.iter().any(|f| f == "pac");
    if has_pac {
        eprintln!("  pac:      enabled");
    }

    eprintln!();
    eprintln!("pproxy started, waiting for connections...");
}

fn print_system_proxy_inspection(result: &eggress_system_proxy::InspectionResult) {
    eprintln!();
    eprintln!("System Proxy Inspection");
    eprintln!("=======================");
    eprintln!("Platform: {}", result.platform);
    eprintln!();

    for cap in &result.capabilities {
        eprintln!("  {cap}");
    }
    eprintln!();

    if let Some(ref settings) = result.settings {
        eprintln!("Current Settings (source: {}):", settings.source);
        if let Some(ref http) = settings.http_proxy {
            eprintln!("  HTTP proxy:  {http}");
        }
        if let Some(ref https) = settings.https_proxy {
            eprintln!("  HTTPS proxy: {https}");
        }
        if let Some(ref socks) = settings.socks_proxy {
            eprintln!("  SOCKS proxy: {socks}");
        }
        if let Some(ref no_proxy) = settings.no_proxy {
            eprintln!("  No proxy:    {no_proxy}");
        }
    } else {
        eprintln!("No proxy settings detected.");
    }
    eprintln!();
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
    }

    #[test]
    fn test_version_string() {
        assert!(VERSION.contains("eggress-pproxy-compat"));
    }
}
