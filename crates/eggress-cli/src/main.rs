//! Native `eggress` binary: thin parse/dispatch/process boundary.
//!
//! Responsibilities are limited to parsing argv, resolving the selected
//! command and global context, initializing process-wide facilities that
//! must be initialized once (logging, inside command handlers), dispatching
//! to a command function, and translating the typed command result into an
//! [`ExitCode`]. Substantive logic lives in [`commands`], the schema in
//! [`cli`], logging policy in [`logging`], and self-update mechanics in
//! [`update`].

mod cli;
mod commands;
mod logging;
mod update;

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, SubCommand};
use eggress_cli::EXIT_RUNTIME_FAILURE;

#[tokio::main]
async fn main() -> ExitCode {
    let exit_code = run().await;
    let code = u8::try_from(exit_code).unwrap_or_else(|_| {
        debug_assert!(
            (0..=u8::MAX as i32).contains(&exit_code),
            "exit code {exit_code} does not fit in u8"
        );
        EXIT_RUNTIME_FAILURE as u8
    });
    ExitCode::from(code)
}

async fn run() -> i32 {
    let args = Cli::parse();
    let ctx = cli::CliContext::from_cli(&args);

    match args.command {
        Some(SubCommand::Version) => commands::version::handle_version(),
        Some(SubCommand::Route(explain_args)) => {
            commands::route::handle_route_explain(&ctx, &explain_args).await
        }
        Some(SubCommand::Upstream(upstream_cmd)) => match upstream_cmd.action {
            cli::UpstreamAction::Test(test_args) => {
                commands::upstream::handle_upstream_test(&ctx, &test_args)
            }
        },
        Some(SubCommand::Update(update_args)) => commands::update::handle_update(&update_args),
        #[cfg(feature = "pproxy-compat")]
        Some(SubCommand::Pproxy(pproxy_cmd)) => {
            commands::pproxy::handle_pproxy_command(&pproxy_cmd)
        }
        #[cfg(feature = "operations")]
        Some(SubCommand::SystemProxy(sysproxy_cmd)) => match sysproxy_cmd.action {
            cli::SystemProxyAction::Inspect(inspect_args) => {
                commands::system_proxy::handle_system_proxy_inspect(&inspect_args)
            }
        },
        None => commands::run::handle_native_startup(args, ctx).await,
    }
}
