//! Native `eggress` command-line schema.
//!
//! This module owns argument parsing only: the Clap types, closed value
//! domains, and the resolved global context handed to command handlers. It
//! performs no I/O, no runtime construction, and no process termination.

use clap::{Parser, Subcommand, ValueEnum};

/// Closed log-format domain. Invalid values (e.g. `--log-format jsno`) fail
/// at parse time through the normal Clap error path instead of silently
/// falling back to pretty output.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    /// Human-friendly multi-line output.
    Pretty,
    /// Single-line terse output.
    Compact,
    /// Structured JSON output.
    Json,
}

impl LogFormat {
    /// The `str` spelling accepted on the command line.
    pub const fn as_str(self) -> &'static str {
        match self {
            LogFormat::Pretty => "pretty",
            LogFormat::Compact => "compact",
            LogFormat::Json => "json",
        }
    }
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Closed upstream-test mode domain.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum UpstreamTestMode {
    /// Traverse the compiled upstream chain with production connector
    /// behavior.
    Proxy,
    /// Intentionally simpler first-hop TCP reachability check.
    Tcp,
}

impl UpstreamTestMode {
    /// The `str` spelling accepted on the command line and recorded in
    /// [`crate::UpstreamTestResult::mode`].
    pub const fn as_str(self) -> &'static str {
        match self {
            UpstreamTestMode::Proxy => "proxy",
            UpstreamTestMode::Tcp => "tcp",
        }
    }
}

/// Closed route-explain protocol domain.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum RouteProtocol {
    /// HTTP (CONNECT/forward) inbound protocol.
    Http,
    /// SOCKS4 inbound protocol.
    Socks4,
    /// SOCKS5 inbound protocol.
    Socks5,
}

impl RouteProtocol {
    /// The `str` spelling accepted on the command line and sent to the admin
    /// `/-/route-explain` endpoint.
    pub const fn as_str(self) -> &'static str {
        match self {
            RouteProtocol::Http => "http",
            RouteProtocol::Socks4 => "socks4",
            RouteProtocol::Socks5 => "socks5",
        }
    }

    /// Convert to the typed core protocol identifier.
    pub const fn to_protocol_id(self) -> eggress_core::ProtocolId {
        match self {
            RouteProtocol::Http => eggress_core::ProtocolId::Http,
            RouteProtocol::Socks4 => eggress_core::ProtocolId::Socks4,
            RouteProtocol::Socks5 => eggress_core::ProtocolId::Socks5,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "eggress", version, about = "A multi-protocol TCP proxy")]
pub struct Cli {
    #[arg(short = 'l', long = "listen", value_name = "URI")]
    pub listeners: Vec<String>,

    #[arg(short = 'r', long = "remote", value_name = "URI")]
    pub upstreams: Vec<String>,

    #[arg(long = "log-format", value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,

    /// Single authoritative configuration source (global: may appear before
    /// or after the subcommand).
    #[arg(short = 'c', long = "config", global = true, value_name = "PATH")]
    pub config: Option<String>,

    #[arg(long = "rules-file", value_name = "PATH")]
    pub rules_file: Option<String>,

    #[command(subcommand)]
    pub command: Option<SubCommand>,
}

#[derive(Subcommand, Debug)]
pub enum SubCommand {
    /// Print the installed eggress version (`eggress X.Y.Z`) and exit.
    Version,
    /// Explain the routing decision for a target.
    Route(RouteExplain),
    /// Diagnose upstream connectivity.
    Upstream(UpstreamCommand),
    /// Update the standalone installation from verified GitHub Release assets.
    Update(UpdateArgs),
    #[cfg(feature = "pproxy-compat")]
    /// pproxy migration and compatibility tooling.
    Pproxy(PproxyCommand),
    #[cfg(feature = "operations")]
    /// Operating-system integration.
    SystemProxy(SystemProxyCommand),
}

#[derive(Parser, Debug)]
pub struct UpdateArgs {}

#[derive(Parser, Debug)]
pub struct UpstreamCommand {
    #[command(subcommand)]
    pub action: UpstreamAction,
}

#[derive(Subcommand, Debug)]
pub enum UpstreamAction {
    /// Test upstream connectivity and exit.
    Test(UpstreamTest),
}

#[derive(Parser, Debug)]
pub struct UpstreamTest {
    #[arg(short, long, value_name = "ID")]
    pub id: Option<String>,

    #[arg(short, long, value_name = "HOST:PORT")]
    pub target: Option<String>,

    #[arg(long, default_value = "5")]
    pub timeout: u64,

    #[arg(long, value_enum, default_value_t = UpstreamTestMode::Proxy)]
    pub mode: UpstreamTestMode,

    #[arg(long)]
    pub json: bool,
}

#[derive(Parser, Debug)]
pub struct RouteExplain {
    /// Target to explain, e.g. `example.com:443`.
    pub target: String,

    #[arg(long)]
    pub listener: Option<String>,

    #[arg(long, value_enum)]
    pub protocol: Option<RouteProtocol>,

    #[arg(long)]
    pub json: bool,

    /// Live admin endpoint for remote explanation, e.g.
    /// `http://127.0.0.1:9090`.
    #[arg(long, value_name = "URL")]
    pub admin: Option<String>,
}

#[cfg(feature = "pproxy-compat")]
#[derive(Parser, Debug)]
pub struct PproxyCommand {
    #[command(subcommand)]
    pub action: PproxyAction,
}

#[cfg(feature = "pproxy-compat")]
#[derive(Subcommand, Debug)]
pub enum PproxyAction {
    /// Translate pproxy arguments to Eggress TOML
    Translate(PproxyTranslate),
    /// Check pproxy arguments and report parity tier
    Check(PproxyCheck),
    /// Translate and run pproxy-style arguments
    Run(PproxyRun),
}

#[cfg(feature = "pproxy-compat")]
#[derive(Parser, Debug)]
pub struct PproxyTranslate {
    /// pproxy-style arguments (after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,

    /// Add explanatory comments to generated TOML
    #[arg(long)]
    pub annotate: bool,
}

#[cfg(feature = "pproxy-compat")]
#[derive(Parser, Debug)]
pub struct PproxyCheck {
    /// pproxy-style arguments (after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,

    /// Output results as JSON
    #[arg(long)]
    pub json: bool,
}

#[cfg(feature = "pproxy-compat")]
#[derive(Parser, Debug)]
pub struct PproxyRun {
    /// pproxy-style arguments (after --)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,

    #[arg(long = "log-format", value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,
}

#[cfg(feature = "operations")]
#[derive(Parser, Debug)]
pub struct SystemProxyCommand {
    #[command(subcommand)]
    pub action: SystemProxyAction,
}

#[cfg(feature = "operations")]
#[derive(Subcommand, Debug)]
pub enum SystemProxyAction {
    /// Inspect current system proxy settings (read-only)
    Inspect(SystemProxyInspect),
}

#[cfg(feature = "operations")]
#[derive(Parser, Debug)]
pub struct SystemProxyInspect {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Resolved global CLI state passed to every command handler. There is one
/// authoritative parsed config path in the native CLI model; route and
/// upstream commands consume it from here instead of defining their own
/// duplicate `--config` options.
#[derive(Debug, Clone)]
pub struct CliContext {
    /// Value of the global `--config` option, if present.
    pub config: Option<String>,
    /// Value of the top-level `--log-format` option.
    pub log_format: LogFormat,
}

impl CliContext {
    /// Resolve global state from already-parsed top-level arguments.
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            config: cli.config.clone(),
            log_format: cli.log_format,
        }
    }
}
