use std::collections::HashMap;
use std::fmt;

use crate::apply::Command;
use crate::backends;
use crate::capability::{
    system_proxy_platform_info, SystemProxyCapability, SystemProxyCapabilityReport,
    SystemProxyStatus,
};
use crate::command_runner::{CommandRunner, RealCommandRunner};
use crate::redaction::redact_proxy_settings;

/// System proxy settings read from the platform.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SystemProxySettings {
    /// Source description (e.g., "environment", "macos:networksetup:*Wi-Fi").
    pub source: String,
    /// HTTP proxy address (e.g., "http://proxy:8080").
    pub http_proxy: Option<String>,
    /// HTTPS proxy address.
    pub https_proxy: Option<String>,
    /// SOCKS proxy address.
    pub socks_proxy: Option<String>,
    /// No-proxy/bypass list.
    pub no_proxy: Option<String>,
    /// Raw key-value pairs from the source.
    pub raw: HashMap<String, String>,
}

impl fmt::Display for SystemProxySettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::redaction::redact_proxy_uri;
        write!(f, "System proxy settings (source: {})", self.source)?;
        if let Some(ref http) = self.http_proxy {
            write!(f, "\n  HTTP proxy: {}", redact_proxy_uri(http))?;
        }
        if let Some(ref https) = self.https_proxy {
            write!(f, "\n  HTTPS proxy: {}", redact_proxy_uri(https))?;
        }
        if let Some(ref socks) = self.socks_proxy {
            write!(f, "\n  SOCKS proxy: {}", redact_proxy_uri(socks))?;
        }
        if let Some(ref no_proxy) = self.no_proxy {
            write!(f, "\n  No proxy: {no_proxy}")?;
        }
        Ok(())
    }
}

/// Full inspection result including capabilities and settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InspectionResult {
    /// Detected platform.
    pub platform: String,
    /// Capability reports for all system proxy backends.
    pub capabilities: Vec<SystemProxyCapabilityReport>,
    /// Current proxy settings (if readable).
    ///
    /// Kept for in-process callers that need the unredacted values, but never
    /// serialized: OS and environment settings can carry `user:pass@`
    /// credentials, and `eggress system-proxy inspect --json` is a
    /// copy/paste-able diagnostic surface. JSON output therefore carries only
    /// [`Self::redacted_settings`].
    #[serde(default, skip_serializing)]
    pub settings: Option<SystemProxySettings>,
    /// Redacted version of settings (safe for logging and JSON output).
    ///
    /// Always derived from [`Self::settings`] by
    /// [`InspectionResult::new`]/`inspect_system_proxy`, so it is populated
    /// whenever raw settings exist.
    pub redacted_settings: Option<SystemProxySettings>,
    /// Whether apply/revert is supported on this platform.
    pub apply_supported: bool,
    /// Commands that would be used for apply (dry-run, redacted).
    pub dry_run_commands: Vec<Command>,
}

impl InspectionResult {
    /// Build an inspection result, deriving the redacted settings copy.
    ///
    /// This is the only constructor used by inspection so `redacted_settings`
    /// can never be missing while raw `settings` are present.
    pub fn new(
        platform: String,
        capabilities: Vec<SystemProxyCapabilityReport>,
        settings: Option<SystemProxySettings>,
        apply_supported: bool,
        dry_run_commands: Vec<Command>,
    ) -> Self {
        let redacted_settings = settings.as_ref().map(redact_settings);
        Self {
            platform,
            capabilities,
            settings,
            redacted_settings,
            apply_supported,
            dry_run_commands,
        }
    }
}

/// Owner-safe copy of settings: URI/credential-bearing values are redacted.
fn redact_settings(settings: &SystemProxySettings) -> SystemProxySettings {
    SystemProxySettings {
        source: settings.source.clone(),
        http_proxy: settings.http_proxy.as_ref().map(|v| redact_proxy_value(v)),
        https_proxy: settings.https_proxy.as_ref().map(|v| redact_proxy_value(v)),
        socks_proxy: settings.socks_proxy.as_ref().map(|v| redact_proxy_value(v)),
        no_proxy: settings.no_proxy.clone(),
        raw: redact_proxy_settings(&settings.raw),
    }
}

impl fmt::Display for InspectionResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Platform: {}", self.platform)?;
        writeln!(f, "\nCapabilities:")?;
        for cap in &self.capabilities {
            writeln!(f, "  {cap}")?;
        }
        if let Some(ref settings) = self.redacted_settings.as_ref().or(self.settings.as_ref()) {
            writeln!(f, "\nSettings:")?;
            writeln!(f, "{settings}")?;
        }
        writeln!(f, "\nApply supported: {}", self.apply_supported)?;
        if !self.dry_run_commands.is_empty() {
            writeln!(f, "\nDry-run commands:")?;
            for cmd in &self.dry_run_commands {
                writeln!(f, "  {cmd}")?;
            }
        }
        Ok(())
    }
}

/// Detect the current platform name.
pub fn detect_platform() -> String {
    if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Inspect system proxy settings using the best available backend.
///
/// This is the main entry point for read-only inspection. It probes
/// capabilities and uses the first available backend.
pub fn inspect_system_proxy() -> InspectionResult {
    inspect_system_proxy_with_runner(&RealCommandRunner)
}

/// Inspect system proxy settings with a custom command runner (for testing).
pub fn inspect_system_proxy_with_runner(runner: &dyn CommandRunner) -> InspectionResult {
    let platform = detect_platform();
    let capabilities = system_proxy_platform_info();

    let settings = match platform.as_str() {
        "macos" => inspect_macos_best(runner, &capabilities),
        "windows" => inspect_windows_best(runner, &capabilities),
        "linux" => inspect_linux_best(runner, &capabilities),
        _ => inspect_env_only(runner),
    };

    let redacted_settings = settings.as_ref().map(redact_settings);

    let apply_supported = capabilities.iter().any(|c| {
        matches!(c.status, SystemProxyStatus::Available)
            && matches!(
                c.capability,
                SystemProxyCapability::ApplyMacosNetworksetup
                    | SystemProxyCapability::ApplyWindowsInternetSettings
                    | SystemProxyCapability::ApplyGnomeSettings
                    | SystemProxyCapability::ApplyKdeSettings
            )
    });

    // Generate the display-only dry-run commands from the redacted copy:
    // backends embed the proxy value verbatim, so using raw settings here
    // would leak `user:pass@` through `--json` and the human report.
    let dry_run_commands = generate_dry_run_commands(&platform, redacted_settings.as_ref());

    InspectionResult {
        platform,
        capabilities,
        settings,
        redacted_settings,
        apply_supported,
        dry_run_commands,
    }
}

fn inspect_macos_best(
    runner: &dyn CommandRunner,
    capabilities: &[SystemProxyCapabilityReport],
) -> Option<SystemProxySettings> {
    let has_networksetup = capabilities.iter().any(|c| {
        c.capability == SystemProxyCapability::InspectMacosNetworksetup
            && c.status == SystemProxyStatus::Available
    });

    if has_networksetup {
        if let Ok(services) = backends::macos::list_network_services(runner) {
            if let Some(service) = services.first() {
                if let Ok(settings) = backends::macos::inspect_macos_proxy(runner, service) {
                    return Some(settings);
                }
            }
        }
    }

    Some(backends::env::inspect_environment(runner))
}

fn inspect_windows_best(
    runner: &dyn CommandRunner,
    capabilities: &[SystemProxyCapabilityReport],
) -> Option<SystemProxySettings> {
    let has_registry = capabilities.iter().any(|c| {
        c.capability == SystemProxyCapability::InspectWindowsInternetSettings
            && c.status == SystemProxyStatus::Available
    });

    if has_registry {
        if let Ok(settings) = backends::windows::inspect_windows_proxy(runner) {
            return Some(settings);
        }
    }

    Some(backends::env::inspect_environment(runner))
}

fn inspect_linux_best(
    runner: &dyn CommandRunner,
    capabilities: &[SystemProxyCapabilityReport],
) -> Option<SystemProxySettings> {
    let has_gnome = capabilities.iter().any(|c| {
        c.capability == SystemProxyCapability::InspectGnomeSettings
            && c.status == SystemProxyStatus::Available
    });

    if has_gnome {
        if let Ok(settings) = backends::linux::inspect_gnome_proxy(runner) {
            return Some(settings);
        }
    }

    Some(backends::env::inspect_environment(runner))
}

fn inspect_env_only(runner: &dyn CommandRunner) -> Option<SystemProxySettings> {
    Some(backends::env::inspect_environment(runner))
}

fn generate_dry_run_commands(
    platform: &str,
    settings: Option<&SystemProxySettings>,
) -> Vec<Command> {
    let settings = match settings {
        Some(s) => s,
        None => return Vec::new(),
    };

    match platform {
        "macos" => {
            let service = settings.source.split(':').next_back().unwrap_or("*Wi-Fi");
            backends::macos::generate_macos_apply_commands(
                service,
                settings.http_proxy.as_deref(),
                settings.https_proxy.as_deref(),
                settings.socks_proxy.as_deref(),
                settings.no_proxy.as_deref(),
            )
        }
        "windows" => backends::windows::generate_windows_apply_commands(
            settings.http_proxy.as_deref(),
            settings.https_proxy.as_deref(),
            settings.socks_proxy.as_deref(),
            settings.no_proxy.as_deref(),
        ),
        "linux" => backends::linux::generate_gnome_apply_commands(
            settings.http_proxy.as_deref(),
            settings.https_proxy.as_deref(),
            settings.socks_proxy.as_deref(),
            settings.no_proxy.as_deref(),
        )
        .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn redact_proxy_value(value: &str) -> String {
    crate::redaction::redact_proxy_uri(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_runner::MockCommandRunner;

    #[test]
    fn detect_platform_returns_known_value() {
        let platform = detect_platform();
        assert!(["macos", "linux", "windows", "unknown"].contains(&platform.as_str()));
    }

    #[test]
    fn inspection_result_serializes() {
        let result = InspectionResult {
            platform: "test".to_string(),
            capabilities: Vec::new(),
            settings: None,
            redacted_settings: None,
            apply_supported: false,
            dry_run_commands: Vec::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"platform\":\"test\""));
    }

    /// JSON output is the diagnostic surface: raw settings (which can carry
    /// `user:pass@` from OS/environment proxy configuration) must never be
    /// serialized, while in-process callers keep them.
    #[test]
    fn inspection_result_json_omits_raw_credentials() {
        let mut raw = std::collections::HashMap::new();
        raw.insert(
            "http_proxy".to_string(),
            "http://user:pass@proxy:8080".to_string(),
        );
        raw.insert(
            "HTTPS_PROXY".to_string(),
            "http://user:pass@proxy:8443".to_string(),
        );
        let settings = SystemProxySettings {
            source: "environment".to_string(),
            http_proxy: Some("http://user:pass@proxy:8080".to_string()),
            https_proxy: Some("http://user:pass@proxy:8443".to_string()),
            socks_proxy: Some("socks5://admin:secret@proxy:1080".to_string()),
            no_proxy: Some("localhost".to_string()),
            raw,
        };
        let result = InspectionResult::new(
            "linux".to_string(),
            Vec::new(),
            Some(settings),
            false,
            Vec::new(),
        );
        assert!(
            result.settings.is_some(),
            "raw settings stay available to in-process callers"
        );
        assert!(
            result
                .redacted_settings
                .as_ref()
                .and_then(|s| s.http_proxy.as_deref())
                == Some("http://****@proxy:8080"),
            "redacted settings are derived whenever settings exist"
        );

        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("user:pass@"), "{json}");
        assert!(!json.contains("admin:secret@"), "{json}");
        assert!(
            !json.contains("\"settings\""),
            "raw settings must not be serialized: {json}"
        );
        assert!(json.contains("\"redacted_settings\""), "{json}");
        assert!(json.contains("****@proxy:8080"), "{json}");

        // The redacted-only shape still deserializes.
        let round_trip: InspectionResult = serde_json::from_str(&json).unwrap();
        assert!(round_trip.settings.is_none());
        assert!(round_trip.redacted_settings.is_some());
    }

    /// Dry-run commands are display-only and must be built from the redacted
    /// copy: backends embed the proxy value verbatim.
    #[test]
    fn dry_run_commands_use_redacted_values() {
        let settings = SystemProxySettings {
            source: "macos:networksetup:*Wi-Fi".to_string(),
            http_proxy: Some("http://user:pass@proxy:8080".to_string()),
            https_proxy: Some("http://user:pass@proxy:8443".to_string()),
            socks_proxy: Some("socks5://admin:secret@proxy:1080".to_string()),
            no_proxy: Some("localhost".to_string()),
            raw: std::collections::HashMap::new(),
        };
        let redacted = redact_settings(&settings);
        for platform in ["macos", "windows", "linux"] {
            let commands = generate_dry_run_commands(platform, Some(&redacted));
            for command in &commands {
                let rendered = command.to_string();
                assert!(
                    !rendered.contains("user:pass@") && !rendered.contains("admin:secret@"),
                    "{platform} dry-run command leaked credentials: {rendered}"
                );
            }
        }
    }

    #[test]
    fn settings_display_format() {
        let settings = SystemProxySettings {
            source: "test".to_string(),
            http_proxy: Some("http://proxy:8080".to_string()),
            https_proxy: None,
            socks_proxy: None,
            no_proxy: Some("localhost".to_string()),
            raw: std::collections::HashMap::new(),
        };
        let display = settings.to_string();
        assert!(display.contains("HTTP proxy: http://proxy:8080"));
        assert!(display.contains("No proxy: localhost"));
    }

    #[test]
    fn inspection_with_mock_runner() {
        let runner = MockCommandRunner::new();
        let result = inspect_system_proxy_with_runner(&runner);
        assert!(!result.platform.is_empty());
        assert!(!result.capabilities.is_empty());
    }
}
