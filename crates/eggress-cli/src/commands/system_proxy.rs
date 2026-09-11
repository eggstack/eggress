//! `eggress system-proxy inspect`: read-only OS proxy inspection.

use eggress_cli::EXIT_RUNTIME_FAILURE;

use crate::cli::SystemProxyInspect;

/// Inspect current system proxy settings and render the result.
/// Returns the process exit code.
pub fn handle_system_proxy_inspect(args: &SystemProxyInspect) -> i32 {
    let result = eggress_system_proxy::inspect_system_proxy();

    if args.json {
        match serde_json::to_string_pretty(&result) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("failed to serialize inspection result: {e}");
                return EXIT_RUNTIME_FAILURE;
            }
        }
    } else {
        print_inspection_result(&result);
    }
    eggress_cli::EXIT_SUCCESS
}

fn print_inspection_result(result: &eggress_system_proxy::InspectionResult) {
    println!("System Proxy Inspection");
    println!("=======================");
    println!("Platform: {}", result.platform);
    println!();

    println!("Capabilities:");
    for cap in &result.capabilities {
        println!("  {cap}");
    }
    println!();

    if let Some(ref settings) = result.settings {
        println!("Current Settings (source: {}):", settings.source);
        if let Some(ref http) = settings.http_proxy {
            println!("  HTTP proxy:  {http}");
        }
        if let Some(ref https) = settings.https_proxy {
            println!("  HTTPS proxy: {https}");
        }
        if let Some(ref socks) = settings.socks_proxy {
            println!("  SOCKS proxy: {socks}");
        }
        if let Some(ref no_proxy) = settings.no_proxy {
            println!("  No proxy:    {no_proxy}");
        }
    } else {
        println!("No proxy settings detected.");
    }
    println!();

    println!("Apply supported: {}", result.apply_supported);

    if !result.dry_run_commands.is_empty() {
        println!();
        println!("Dry-run apply commands:");
        for cmd in &result.dry_run_commands {
            println!("  {cmd}");
        }
    }
}
