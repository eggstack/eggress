use std::collections::HashMap;
use std::fmt;

/// Platform-specific capabilities for system proxy inspection and mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SystemProxyCapability {
    /// Read HTTP_PROXY / HTTPS_PROXY / ALL_PROXY environment variables.
    InspectEnvironment,
    /// Read proxy settings via macOS `networksetup` command.
    InspectMacosNetworksetup,
    /// Apply proxy settings via macOS `networksetup` command.
    ApplyMacosNetworksetup,
    /// Read proxy settings via Windows Internet Settings (registry).
    InspectWindowsInternetSettings,
    /// Apply proxy settings via Windows Internet Settings (registry).
    ApplyWindowsInternetSettings,
    /// Read proxy settings via GNOME `gsettings` or `dconf`.
    InspectGnomeSettings,
    /// Apply proxy settings via GNOME `gsettings`.
    ApplyGnomeSettings,
    /// Read proxy settings via KDE `kwriteconfig5`.
    InspectKdeSettings,
    /// Apply proxy settings via KDE `kwriteconfig5`.
    ApplyKdeSettings,
}

/// Status of a system proxy capability check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SystemProxyStatus {
    /// The capability is available on this system.
    Available,
    /// The capability exists but requires elevated privileges.
    MissingPrivilege,
    /// The capability is not supported on this platform.
    UnsupportedPlatform,
    /// The required tool is not installed.
    ToolMissing,
    /// The capability was disabled at compile time.
    DisabledAtCompileTime,
}

impl fmt::Display for SystemProxyCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InspectEnvironment => write!(f, "InspectEnvironment"),
            Self::InspectMacosNetworksetup => write!(f, "InspectMacosNetworksetup"),
            Self::ApplyMacosNetworksetup => write!(f, "ApplyMacosNetworksetup"),
            Self::InspectWindowsInternetSettings => write!(f, "InspectWindowsInternetSettings"),
            Self::ApplyWindowsInternetSettings => write!(f, "ApplyWindowsInternetSettings"),
            Self::InspectGnomeSettings => write!(f, "InspectGnomeSettings"),
            Self::ApplyGnomeSettings => write!(f, "ApplyGnomeSettings"),
            Self::InspectKdeSettings => write!(f, "InspectKdeSettings"),
            Self::ApplyKdeSettings => write!(f, "ApplyKdeSettings"),
        }
    }
}

impl fmt::Display for SystemProxyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Available => write!(f, "available"),
            Self::MissingPrivilege => write!(f, "missing privilege"),
            Self::UnsupportedPlatform => write!(f, "unsupported platform"),
            Self::ToolMissing => write!(f, "tool missing"),
            Self::DisabledAtCompileTime => write!(f, "disabled at compile time"),
        }
    }
}

/// A single capability check result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SystemProxyCapabilityReport {
    pub capability: SystemProxyCapability,
    pub status: SystemProxyStatus,
}

impl fmt::Display for SystemProxyCapabilityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.capability, self.status)
    }
}

/// Check the status of a specific system proxy capability by probing the system.
pub fn check_system_proxy_capability(cap: SystemProxyCapability) -> SystemProxyStatus {
    match cap {
        SystemProxyCapability::InspectEnvironment => check_env_inspection(),
        SystemProxyCapability::InspectMacosNetworksetup => check_tool_available("networksetup"),
        SystemProxyCapability::ApplyMacosNetworksetup => check_tool_available("networksetup"),
        SystemProxyCapability::InspectWindowsInternetSettings => check_windows_internet_settings(),
        SystemProxyCapability::ApplyWindowsInternetSettings => check_windows_internet_settings(),
        SystemProxyCapability::InspectGnomeSettings => check_tool_available("gsettings"),
        SystemProxyCapability::ApplyGnomeSettings => check_tool_available("gsettings"),
        SystemProxyCapability::InspectKdeSettings => check_tool_available("kwriteconfig5"),
        SystemProxyCapability::ApplyKdeSettings => check_tool_available("kwriteconfig5"),
    }
}

/// Check with overrides for testing.
pub fn check_system_proxy_capability_with_overrides(
    cap: SystemProxyCapability,
    overrides: Option<&HashMap<SystemProxyCapability, SystemProxyStatus>>,
) -> SystemProxyStatus {
    if let Some(overrides) = overrides {
        if let Some(status) = overrides.get(&cap) {
            return status.clone();
        }
    }
    check_system_proxy_capability(cap)
}

/// Return all system proxy capabilities and their statuses.
pub fn system_proxy_platform_info() -> Vec<SystemProxyCapabilityReport> {
    ALL_SYSTEM_PROXY_CAPABILITIES
        .iter()
        .map(|&cap| SystemProxyCapabilityReport {
            capability: cap,
            status: check_system_proxy_capability(cap),
        })
        .collect()
}

/// All known system proxy capabilities.
const ALL_SYSTEM_PROXY_CAPABILITIES: &[SystemProxyCapability] = &[
    SystemProxyCapability::InspectEnvironment,
    SystemProxyCapability::InspectMacosNetworksetup,
    SystemProxyCapability::ApplyMacosNetworksetup,
    SystemProxyCapability::InspectWindowsInternetSettings,
    SystemProxyCapability::ApplyWindowsInternetSettings,
    SystemProxyCapability::InspectGnomeSettings,
    SystemProxyCapability::ApplyGnomeSettings,
    SystemProxyCapability::InspectKdeSettings,
    SystemProxyCapability::ApplyKdeSettings,
];

// ---------------------------------------------------------------------------
// Capability check implementations
// ---------------------------------------------------------------------------

fn check_env_inspection() -> SystemProxyStatus {
    SystemProxyStatus::Available
}

fn check_tool_available(tool: &str) -> SystemProxyStatus {
    #[cfg(unix)]
    {
        check_tool_available_unix(tool)
    }
    #[cfg(not(unix))]
    {
        let _ = tool;
        SystemProxyStatus::UnsupportedPlatform
    }
}

#[cfg(unix)]
fn check_tool_available_unix(tool: &str) -> SystemProxyStatus {
    use std::process::Command;

    match Command::new("which").arg(tool).output() {
        Ok(output) if output.status.success() => SystemProxyStatus::Available,
        Ok(_) => SystemProxyStatus::ToolMissing,
        Err(_) => SystemProxyStatus::ToolMissing,
    }
}

fn check_windows_internet_settings() -> SystemProxyStatus {
    #[cfg(target_os = "windows")]
    {
        SystemProxyStatus::Available
    }
    #[cfg(not(target_os = "windows"))]
    {
        SystemProxyStatus::UnsupportedPlatform
    }
}

/// Format system proxy capability reports as a human-readable string.
pub fn format_system_proxy_capability_report(reports: &[SystemProxyCapabilityReport]) -> String {
    let mut out = String::from("System proxy capabilities:\n");
    for report in reports {
        out.push_str(&format!("  {}: {}\n", report.capability, report.status));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_system_proxy_capability() {
        assert_eq!(
            SystemProxyCapability::InspectEnvironment.to_string(),
            "InspectEnvironment"
        );
        assert_eq!(
            SystemProxyCapability::ApplyMacosNetworksetup.to_string(),
            "ApplyMacosNetworksetup"
        );
    }

    #[test]
    fn display_system_proxy_status() {
        assert_eq!(SystemProxyStatus::Available.to_string(), "available");
        assert_eq!(
            SystemProxyStatus::MissingPrivilege.to_string(),
            "missing privilege"
        );
        assert_eq!(
            SystemProxyStatus::UnsupportedPlatform.to_string(),
            "unsupported platform"
        );
        assert_eq!(SystemProxyStatus::ToolMissing.to_string(), "tool missing");
        assert_eq!(
            SystemProxyStatus::DisabledAtCompileTime.to_string(),
            "disabled at compile time"
        );
    }

    #[test]
    fn capability_report_display() {
        let report = SystemProxyCapabilityReport {
            capability: SystemProxyCapability::InspectEnvironment,
            status: SystemProxyStatus::Available,
        };
        assert_eq!(report.to_string(), "InspectEnvironment: available");
    }

    #[test]
    fn env_inspection_always_available() {
        assert_eq!(
            check_system_proxy_capability(SystemProxyCapability::InspectEnvironment),
            SystemProxyStatus::Available
        );
    }

    #[test]
    fn override_returns_override_value() {
        let mut overrides = HashMap::new();
        overrides.insert(
            SystemProxyCapability::ApplyMacosNetworksetup,
            SystemProxyStatus::ToolMissing,
        );

        assert_eq!(
            check_system_proxy_capability_with_overrides(
                SystemProxyCapability::ApplyMacosNetworksetup,
                Some(&overrides)
            ),
            SystemProxyStatus::ToolMissing
        );
    }

    #[test]
    fn override_does_not_affect_unset_capabilities() {
        let mut overrides = HashMap::new();
        overrides.insert(
            SystemProxyCapability::ApplyMacosNetworksetup,
            SystemProxyStatus::Available,
        );

        assert_eq!(
            check_system_proxy_capability_with_overrides(
                SystemProxyCapability::ApplyMacosNetworksetup,
                Some(&overrides)
            ),
            SystemProxyStatus::Available
        );

        let real_status = check_system_proxy_capability_with_overrides(
            SystemProxyCapability::InspectEnvironment,
            Some(&overrides),
        );
        assert_eq!(real_status, SystemProxyStatus::Available);
    }

    #[test]
    fn platform_info_returns_all_capabilities() {
        let info = system_proxy_platform_info();
        assert_eq!(info.len(), 9);

        let names: Vec<_> = info.iter().map(|r| r.capability.to_string()).collect();
        assert!(names.contains(&"InspectEnvironment".to_string()));
        assert!(names.contains(&"InspectMacosNetworksetup".to_string()));
    }

    #[test]
    fn format_report_contains_names() {
        let info = system_proxy_platform_info();
        let formatted = format_system_proxy_capability_report(&info);
        assert!(formatted.contains("System proxy capabilities:"));
        assert!(formatted.contains("InspectEnvironment"));
    }

    #[test]
    fn windows_internet_settings_unsupported_on_non_windows() {
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(
                check_system_proxy_capability(
                    SystemProxyCapability::InspectWindowsInternetSettings
                ),
                SystemProxyStatus::UnsupportedPlatform
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_networksetup_available_on_macos() {
        assert_eq!(
            check_system_proxy_capability(SystemProxyCapability::InspectMacosNetworksetup),
            SystemProxyStatus::Available
        );
    }
}
