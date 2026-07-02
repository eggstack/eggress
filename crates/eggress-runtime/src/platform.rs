use std::collections::HashMap;
use std::fmt;

/// Platform-specific capabilities for transparent proxy and Unix socket support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformCapability {
    /// SO_ORIGINAL_DST for IPv4 on Linux (requires nf_conntrack / iptables DNAT).
    LinuxOriginalDstIpv4,
    /// IPv6 equivalent of SO_ORIGINAL_DST on Linux.
    LinuxOriginalDstIpv6,
    /// IP_TRANSPARENT socket option on Linux (for transparent proxy binding).
    LinuxTransparentBind,
    /// macOS PF integration for original destination retrieval.
    MacosPfOriginalDst,
    /// Unix domain socket support (AF_UNIX).
    UnixDomainSockets,
}

/// Status of a platform capability check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityStatus {
    /// The capability is available on this system.
    Available,
    /// The capability exists but requires elevated privileges.
    MissingPrivilege,
    /// The capability is not supported on this platform.
    UnsupportedPlatform,
    /// The capability is not supported by the running kernel.
    KernelUnsupported,
    /// The capability was disabled at compile time.
    DisabledAtCompileTime,
}

impl fmt::Display for PlatformCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LinuxOriginalDstIpv4 => write!(f, "LinuxOriginalDstIpv4"),
            Self::LinuxOriginalDstIpv6 => write!(f, "LinuxOriginalDstIpv6"),
            Self::LinuxTransparentBind => write!(f, "LinuxTransparentBind"),
            Self::MacosPfOriginalDst => write!(f, "MacosPfOriginalDst"),
            Self::UnixDomainSockets => write!(f, "UnixDomainSockets"),
        }
    }
}

impl fmt::Display for CapabilityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Available => write!(f, "available"),
            Self::MissingPrivilege => write!(f, "missing privilege"),
            Self::UnsupportedPlatform => write!(f, "unsupported platform"),
            Self::KernelUnsupported => write!(f, "kernel unsupported"),
            Self::DisabledAtCompileTime => write!(f, "disabled at compile time"),
        }
    }
}

/// Capability check result with the capability that was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub capability: PlatformCapability,
    pub status: CapabilityStatus,
}

impl fmt::Display for CapabilityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.capability, self.status)
    }
}

/// Check the status of a specific platform capability by probing the system.
pub fn check_capability(cap: PlatformCapability) -> CapabilityStatus {
    match cap {
        PlatformCapability::UnixDomainSockets => check_unix_domain_sockets(),
        PlatformCapability::LinuxOriginalDstIpv4 => check_linux_original_dst_ipv4(),
        PlatformCapability::LinuxOriginalDstIpv6 => check_linux_original_dst_ipv6(),
        PlatformCapability::LinuxTransparentBind => check_linux_transparent_bind(),
        PlatformCapability::MacosPfOriginalDst => check_macos_pf_original_dst(),
    }
}

/// Check a platform capability, consulting an overrides map first.
///
/// If `overrides` contains the capability, its value is returned immediately.
/// Otherwise the real system is probed. This is the primary entry point for
/// tests that need deterministic results without global state.
pub fn check_capability_with_overrides(
    cap: PlatformCapability,
    overrides: Option<&HashMap<PlatformCapability, CapabilityStatus>>,
) -> CapabilityStatus {
    if let Some(overrides) = overrides {
        if let Some(status) = overrides.get(&cap) {
            return status.clone();
        }
    }
    check_capability(cap)
}

/// Return a summary of all platform capabilities and their statuses.
///
/// Useful for startup diagnostics to name which capabilities are missing.
pub fn platform_info() -> Vec<CapabilityReport> {
    ALL_CAPABILITIES
        .iter()
        .map(|&cap| CapabilityReport {
            capability: cap,
            status: check_capability(cap),
        })
        .collect()
}

/// Return a summary using provided overrides for specified capabilities.
pub fn platform_info_with_overrides(
    overrides: &HashMap<PlatformCapability, CapabilityStatus>,
) -> Vec<CapabilityReport> {
    ALL_CAPABILITIES
        .iter()
        .map(|&cap| CapabilityReport {
            capability: cap,
            status: check_capability_with_overrides(cap, Some(overrides)),
        })
        .collect()
}

/// All known platform capabilities.
const ALL_CAPABILITIES: &[PlatformCapability] = &[
    PlatformCapability::UnixDomainSockets,
    PlatformCapability::LinuxOriginalDstIpv4,
    PlatformCapability::LinuxOriginalDstIpv6,
    PlatformCapability::LinuxTransparentBind,
    PlatformCapability::MacosPfOriginalDst,
];

// ---------------------------------------------------------------------------
// Unix domain sockets
// ---------------------------------------------------------------------------

fn check_unix_domain_sockets() -> CapabilityStatus {
    #[cfg(unix)]
    {
        check_unix_domain_sockets_unix()
    }
    #[cfg(not(unix))]
    {
        CapabilityStatus::UnsupportedPlatform
    }
}

#[cfg(unix)]
fn check_unix_domain_sockets_unix() -> CapabilityStatus {
    use std::os::unix::net::UnixListener;

    let dir = std::env::temp_dir();
    let id: u64 = fastrand::u64(..);
    let path = dir.join(format!("eggress_cap_test_{id}.sock"));

    match UnixListener::bind(&path) {
        Ok(_listener) => {
            drop(std::fs::remove_file(&path));
            CapabilityStatus::Available
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            CapabilityStatus::MissingPrivilege
        }
        Err(_) => CapabilityStatus::KernelUnsupported,
    }
}

// ---------------------------------------------------------------------------
// Linux original destination (IPv4 and IPv6)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn check_linux_original_dst_ipv4() -> CapabilityStatus {
    check_linux_netfilter_proc("/proc/net/ip_tables_names")
}

#[cfg(target_os = "linux")]
fn check_linux_original_dst_ipv6() -> CapabilityStatus {
    check_linux_netfilter_proc("/proc/net/ip6_tables_names")
}

#[cfg(not(target_os = "linux"))]
fn check_linux_original_dst_ipv4() -> CapabilityStatus {
    CapabilityStatus::UnsupportedPlatform
}

#[cfg(not(target_os = "linux"))]
fn check_linux_original_dst_ipv6() -> CapabilityStatus {
    CapabilityStatus::UnsupportedPlatform
}

/// Check if a netfilter proc file exists and contains at least one table name.
#[cfg(target_os = "linux")]
fn check_linux_netfilter_proc(path: &str) -> CapabilityStatus {
    match std::fs::read_to_string(path) {
        Ok(content) if !content.trim().is_empty() => CapabilityStatus::Available,
        Ok(_) => CapabilityStatus::KernelUnsupported,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            CapabilityStatus::MissingPrivilege
        }
        Err(_) => CapabilityStatus::KernelUnsupported,
    }
}

// ---------------------------------------------------------------------------
// Linux transparent bind (IP_TRANSPARENT)
// ---------------------------------------------------------------------------
//
// `ip_nonlocal_bind=1` is a sysctl value: it does NOT mean the running
// process can successfully call setsockopt(IP_TRANSPARENT). Setting that
// option requires CAP_NET_ADMIN (or root) and an active socket. We surface
// the sysctl reading here for diagnostics only; the supervisor must treat
// any later bind failure as the authoritative answer.
//
// A successful probe only indicates that the sysctl knob has been flipped
// globally; it does not assert CAP_NET_ADMIN, nor that IP_TRANSPARENT will
// actually succeed. Treat this as a soft hint, not a privilege assertion.

#[cfg(target_os = "linux")]
fn check_linux_transparent_bind() -> CapabilityStatus {
    match std::fs::read_to_string("/proc/sys/net/ipv4/ip_nonlocal_bind") {
        Ok(content) => {
            let val = content.trim();
            if val == "1" {
                CapabilityStatus::Available
            } else {
                CapabilityStatus::KernelUnsupported
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            CapabilityStatus::MissingPrivilege
        }
        Err(_) => CapabilityStatus::KernelUnsupported,
    }
}

#[cfg(not(target_os = "linux"))]
fn check_linux_transparent_bind() -> CapabilityStatus {
    CapabilityStatus::UnsupportedPlatform
}

// ---------------------------------------------------------------------------
// macOS PF original destination
// ---------------------------------------------------------------------------
//
// macOS exposes PF via `/dev/pf`, but Eggress does not implement PF-based
// original-destination recovery (see ADR_macos_pf_transparent_proxy.md).
// Reporting `/dev/pf` as Available would falsely imply that running eggress
// on macOS yields full transparent proxy semantics; we instead always
// return UnsupportedPlatform so callers cannot route traffic on that
// assumption.

#[cfg(target_os = "macos")]
fn check_macos_pf_original_dst() -> CapabilityStatus {
    // Even when /dev/pf exists, Eggress has no PF integration.
    CapabilityStatus::KernelUnsupported
}

#[cfg(not(target_os = "macos"))]
fn check_macos_pf_original_dst() -> CapabilityStatus {
    CapabilityStatus::UnsupportedPlatform
}

// ---------------------------------------------------------------------------
// Display helpers for startup diagnostics
// ---------------------------------------------------------------------------

/// Format a list of capability reports as a human-readable diagnostic string.
pub fn format_capability_report(reports: &[CapabilityReport]) -> String {
    let mut out = String::from("Platform capabilities:\n");
    for report in reports {
        out.push_str(&format!("  {}: {}\n", report.capability, report.status));
    }
    out
}

/// Filter reports to only those that are not available, for startup warnings.
pub fn missing_capabilities(reports: &[CapabilityReport]) -> Vec<&CapabilityReport> {
    reports
        .iter()
        .filter(|r| r.status != CapabilityStatus::Available)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_platform_capability() {
        assert_eq!(
            PlatformCapability::LinuxOriginalDstIpv4.to_string(),
            "LinuxOriginalDstIpv4"
        );
        assert_eq!(
            PlatformCapability::UnixDomainSockets.to_string(),
            "UnixDomainSockets"
        );
    }

    #[test]
    fn display_capability_status() {
        assert_eq!(CapabilityStatus::Available.to_string(), "available");
        assert_eq!(
            CapabilityStatus::MissingPrivilege.to_string(),
            "missing privilege"
        );
        assert_eq!(
            CapabilityStatus::UnsupportedPlatform.to_string(),
            "unsupported platform"
        );
        assert_eq!(
            CapabilityStatus::KernelUnsupported.to_string(),
            "kernel unsupported"
        );
        assert_eq!(
            CapabilityStatus::DisabledAtCompileTime.to_string(),
            "disabled at compile time"
        );
    }

    #[test]
    fn capability_report_display() {
        let report = CapabilityReport {
            capability: PlatformCapability::UnixDomainSockets,
            status: CapabilityStatus::Available,
        };
        assert_eq!(report.to_string(), "UnixDomainSockets: available");
    }

    #[test]
    fn unix_domain_sockets_available_on_unix() {
        #[cfg(unix)]
        {
            assert_eq!(
                check_capability(PlatformCapability::UnixDomainSockets),
                CapabilityStatus::Available
            );
        }
    }

    #[test]
    fn override_returns_override_value() {
        let mut overrides = HashMap::new();
        overrides.insert(
            PlatformCapability::LinuxOriginalDstIpv4,
            CapabilityStatus::Available,
        );
        overrides.insert(
            PlatformCapability::UnixDomainSockets,
            CapabilityStatus::KernelUnsupported,
        );

        assert_eq!(
            check_capability_with_overrides(
                PlatformCapability::LinuxOriginalDstIpv4,
                Some(&overrides)
            ),
            CapabilityStatus::Available
        );
        assert_eq!(
            check_capability_with_overrides(
                PlatformCapability::UnixDomainSockets,
                Some(&overrides)
            ),
            CapabilityStatus::KernelUnsupported
        );
    }

    #[test]
    fn override_does_not_affect_unset_capabilities() {
        let mut overrides = HashMap::new();
        overrides.insert(
            PlatformCapability::LinuxTransparentBind,
            CapabilityStatus::Available,
        );

        // LinuxTransparentBind should return the override
        assert_eq!(
            check_capability_with_overrides(
                PlatformCapability::LinuxTransparentBind,
                Some(&overrides)
            ),
            CapabilityStatus::Available
        );

        // UnixDomainSockets has no override, falls through to real check
        let real_status = check_capability_with_overrides(
            PlatformCapability::UnixDomainSockets,
            Some(&overrides),
        );

        #[cfg(unix)]
        assert_eq!(real_status, CapabilityStatus::Available);
    }

    #[test]
    fn platform_info_returns_all_capabilities() {
        let info = platform_info();
        assert_eq!(info.len(), 5);

        let names: Vec<_> = info.iter().map(|r| r.capability.to_string()).collect();
        assert!(names.contains(&"UnixDomainSockets".to_string()));
        assert!(names.contains(&"LinuxOriginalDstIpv4".to_string()));
        assert!(names.contains(&"MacosPfOriginalDst".to_string()));
    }

    #[test]
    fn format_capability_report_contains_all_names() {
        let info = platform_info();
        let formatted = format_capability_report(&info);
        assert!(formatted.contains("UnixDomainSockets"));
        assert!(formatted.contains("LinuxOriginalDstIpv4"));
        assert!(formatted.contains("Platform capabilities:"));
    }

    #[test]
    fn missing_capabilities_filters_available() {
        let reports = vec![
            CapabilityReport {
                capability: PlatformCapability::UnixDomainSockets,
                status: CapabilityStatus::Available,
            },
            CapabilityReport {
                capability: PlatformCapability::LinuxOriginalDstIpv4,
                status: CapabilityStatus::UnsupportedPlatform,
            },
        ];
        let missing = missing_capabilities(&reports);
        assert_eq!(missing.len(), 1);
        assert_eq!(
            missing[0].capability,
            PlatformCapability::LinuxOriginalDstIpv4
        );
    }

    #[test]
    fn override_roundtrip() {
        let mut overrides = HashMap::new();
        overrides.insert(
            PlatformCapability::MacosPfOriginalDst,
            CapabilityStatus::Available,
        );

        // With override: returns Available
        assert_eq!(
            check_capability_with_overrides(
                PlatformCapability::MacosPfOriginalDst,
                Some(&overrides)
            ),
            CapabilityStatus::Available
        );

        // Without override: PF is intentionally not implemented on any
        // platform, so the real probe always reports either
        // KernelUnsupported (macOS) or UnsupportedPlatform (non-macOS).
        let real = check_capability(PlatformCapability::MacosPfOriginalDst);
        match real {
            CapabilityStatus::KernelUnsupported => {}
            #[cfg(not(target_os = "macos"))]
            CapabilityStatus::UnsupportedPlatform => {}
            other => panic!("unexpected status: {other:?}"),
        }
    }

    #[test]
    fn platform_info_respects_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert(
            PlatformCapability::LinuxOriginalDstIpv4,
            CapabilityStatus::Available,
        );
        overrides.insert(
            PlatformCapability::LinuxOriginalDstIpv6,
            CapabilityStatus::MissingPrivilege,
        );

        let info = platform_info_with_overrides(&overrides);
        assert_eq!(info.len(), 5);

        let ipv4 = info
            .iter()
            .find(|r| r.capability == PlatformCapability::LinuxOriginalDstIpv4)
            .unwrap();
        assert_eq!(ipv4.status, CapabilityStatus::Available);

        let ipv6 = info
            .iter()
            .find(|r| r.capability == PlatformCapability::LinuxOriginalDstIpv6)
            .unwrap();
        assert_eq!(ipv6.status, CapabilityStatus::MissingPrivilege);
    }

    #[test]
    fn linux_checks_return_unsupported_on_non_linux() {
        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(
                check_capability(PlatformCapability::LinuxOriginalDstIpv4),
                CapabilityStatus::UnsupportedPlatform
            );
            assert_eq!(
                check_capability(PlatformCapability::LinuxOriginalDstIpv6),
                CapabilityStatus::UnsupportedPlatform
            );
            assert_eq!(
                check_capability(PlatformCapability::LinuxTransparentBind),
                CapabilityStatus::UnsupportedPlatform
            );
        }
    }

    #[test]
    fn macos_check_returns_unsupported_on_non_macos() {
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(
                check_capability(PlatformCapability::MacosPfOriginalDst),
                CapabilityStatus::UnsupportedPlatform
            );
        }
    }

    #[test]
    fn none_overrides_probes_real_system() {
        let result = check_capability_with_overrides(PlatformCapability::UnixDomainSockets, None);
        #[cfg(unix)]
        assert_eq!(result, CapabilityStatus::Available);
        #[cfg(not(unix))]
        assert_eq!(result, CapabilityStatus::UnsupportedPlatform);
    }

    /// The `LinuxTransparentBind` capability reports the `ip_nonlocal_bind`
    /// sysctl value, not a verified privilege or kernel feature. Override
    /// paths verify that the sysctl returns the expected enum regardless
    /// of host state.
    #[test]
    fn linux_transparent_bind_override_paths() {
        for status in [
            CapabilityStatus::Available,
            CapabilityStatus::MissingPrivilege,
            CapabilityStatus::KernelUnsupported,
            CapabilityStatus::UnsupportedPlatform,
            CapabilityStatus::DisabledAtCompileTime,
        ] {
            let mut overrides = HashMap::new();
            overrides.insert(PlatformCapability::LinuxTransparentBind, status.clone());
            assert_eq!(
                check_capability_with_overrides(
                    PlatformCapability::LinuxTransparentBind,
                    Some(&overrides),
                ),
                status,
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_transparent_bind_real_probe_returns_known_status() {
        let status = check_capability(PlatformCapability::LinuxTransparentBind);
        match status {
            CapabilityStatus::Available | CapabilityStatus::KernelUnsupported => {}
            other => panic!(
                "LinuxTransparentBind real probe must be Available or KernelUnsupported (sysctl read), got {other:?}"
            ),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_original_dst_real_probe_returns_known_status() {
        let v4 = check_capability(PlatformCapability::LinuxOriginalDstIpv4);
        let v6 = check_capability(PlatformCapability::LinuxOriginalDstIpv6);
        for s in [&v4, &v6] {
            match s {
                CapabilityStatus::Available
                | CapabilityStatus::MissingPrivilege
                | CapabilityStatus::KernelUnsupported => {}
                other => panic!("unexpected real-probe status: {other:?}"),
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_pf_real_probe_always_kernel_unsupported() {
        let status = check_capability(PlatformCapability::MacosPfOriginalDst);
        assert_eq!(
            status,
            CapabilityStatus::KernelUnsupported,
            "PF integration is intentionally unimplemented; probe must not claim Available"
        );
    }
}
