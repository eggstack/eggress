//! Prebuilt-target mapping shared by the updater, the installers, and the
//! release workflow.
//!
//! Exactly the five targets the binary pipeline publishes. Do not add a
//! target here until `release-binaries.yml` actually produces and
//! smoke-tests its archive.

/// Canonical prebuilt targets in release-workflow order.
pub const SUPPORTED_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

/// GitHub owner/repository serving the release assets.
pub const RELEASE_REPO: &str = "eggstack/eggress";

/// Why no prebuilt release exists for this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedTarget {
    /// `std::env::consts::OS` value observed.
    pub os: String,
    /// `std::env::consts::ARCH` value observed.
    pub arch: String,
}

impl std::fmt::Display for UnsupportedTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no prebuilt Eggress release is available for {}-{tarch}; update this installation with Cargo/source instead (`cargo install eggress-cli --locked --force`)",
            self.os,
            tarch = self.arch
        )
    }
}

/// Map an OS/arch pair to its canonical Rust target.
pub fn target_for(os: &str, arch: &str) -> Result<&'static str, UnsupportedTarget> {
    let target = match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => {
            return Err(UnsupportedTarget {
                os: os.to_string(),
                arch: arch.to_string(),
            });
        }
    };
    Ok(target)
}

/// Detect the canonical target for this host.
pub fn detect_target() -> Result<&'static str, UnsupportedTarget> {
    target_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Archive filename for a target (`eggress-<target>.tar.gz`/`.zip`).
/// The tag supplies the version namespace, so names stay stable within a
/// release and never duplicate `X.Y.Z`.
pub fn archive_name(target: &str) -> String {
    if target == "x86_64-pc-windows-msvc" {
        format!("eggress-{target}.zip")
    } else {
        format!("eggress-{target}.tar.gz")
    }
}

/// Checksum sidecar filename for a target archive.
pub fn checksum_name(target: &str) -> String {
    format!("{}.sha256", archive_name(target))
}

/// Exact asset URLs for a release tag and target.
pub fn asset_urls(download_base: &str, target: &str) -> (String, String) {
    debug_assert!(
        SUPPORTED_TARGETS.contains(&target),
        "asset URLs are only defined for published prebuilt targets"
    );
    let base = download_base.trim_end_matches('/');
    let archive = archive_name(target);
    let checksum = checksum_name(target);
    (format!("{base}/{archive}"), format!("{base}/{checksum}"))
}

/// Download base for an exact tag (`.../releases/download/vX.Y.Z`).
/// Pinned updates use the exact tag and never fall forward to latest.
pub fn download_base_for_tag(tag: &str) -> String {
    format!("https://github.com/{RELEASE_REPO}/releases/download/{tag}")
}

/// GitHub API URL resolving the latest non-draft, non-prerelease release.
pub fn latest_release_api_url() -> String {
    format!("https://api.github.com/repos/{RELEASE_REPO}/releases/latest")
}

/// `true` for the Windows target (zip archives, `.exe` siblings).
pub fn is_windows_target(target: &str) -> bool {
    target == "x86_64-pc-windows-msvc"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_supported_hosts() {
        assert_eq!(
            target_for("linux", "x86_64"),
            Ok("x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            target_for("linux", "aarch64"),
            Ok("aarch64-unknown-linux-gnu")
        );
        assert_eq!(target_for("macos", "x86_64"), Ok("x86_64-apple-darwin"));
        assert_eq!(target_for("macos", "aarch64"), Ok("aarch64-apple-darwin"));
        assert_eq!(
            target_for("windows", "x86_64"),
            Ok("x86_64-pc-windows-msvc")
        );
    }

    #[test]
    fn rejects_unsupported_hosts_with_cargo_guidance() {
        for (os, arch) in [
            ("linux", "arm"),
            ("linux", "x86"),
            ("windows", "aarch64"),
            ("freebsd", "x86_64"),
        ] {
            let err = target_for(os, arch).unwrap_err();
            assert!(
                err.to_string().contains("cargo install eggress-cli"),
                "unsupported {os}-{arch} must point at Cargo/source: {err}"
            );
        }
    }

    #[test]
    fn archive_and_checksum_names_match_release_contract() {
        // Mirrors `.github/workflows/release-binaries.yml` and both
        // installers: one archive per target containing both binaries, plus
        // a `.sha256` sidecar. The tag is the version namespace, so names
        // never embed `X.Y.Z`.
        // The tag supplies the version namespace: names are exactly
        // `eggress-<target>.<ext>` with no embedded `X.Y.Z`.
        let expected = [
            (
                "x86_64-unknown-linux-gnu",
                "eggress-x86_64-unknown-linux-gnu.tar.gz",
            ),
            (
                "aarch64-unknown-linux-gnu",
                "eggress-aarch64-unknown-linux-gnu.tar.gz",
            ),
            ("x86_64-apple-darwin", "eggress-x86_64-apple-darwin.tar.gz"),
            (
                "aarch64-apple-darwin",
                "eggress-aarch64-apple-darwin.tar.gz",
            ),
            (
                "x86_64-pc-windows-msvc",
                "eggress-x86_64-pc-windows-msvc.zip",
            ),
        ];
        assert_eq!(SUPPORTED_TARGETS.len(), expected.len());
        for (target, archive) in expected {
            assert!(SUPPORTED_TARGETS.contains(&target));
            assert_eq!(archive_name(target), archive);
            assert_eq!(checksum_name(target), format!("{archive}.sha256"));
        }
        assert_eq!(
            archive_name("x86_64-unknown-linux-gnu"),
            "eggress-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            archive_name("x86_64-pc-windows-msvc"),
            "eggress-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn asset_urls_use_exact_tag_base() {
        let (archive_url, checksum_url) = asset_urls(
            "https://github.com/eggstack/eggress/releases/download/v1.2.3",
            "x86_64-unknown-linux-gnu",
        );
        assert_eq!(
            archive_url,
            "https://github.com/eggstack/eggress/releases/download/v1.2.3/eggress-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            checksum_url,
            "https://github.com/eggstack/eggress/releases/download/v1.2.3/eggress-x86_64-unknown-linux-gnu.tar.gz.sha256"
        );
    }
}
