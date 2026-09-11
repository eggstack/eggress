//! Strict `vX.Y.Z` release-version parsing and comparison.
//!
//! The updater only ever moves between stable releases published under an
//! exact `v<major>.<minor>.<patch>` tag. Anything else (missing `v`,
//! pre-release suffixes, non-numeric components) is rejected so a stable
//! installation can never jump to a malformed or pre-release candidate.

use std::fmt;

/// A stable release version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReleaseVersion {
    /// Major component.
    pub major: u64,
    /// Minor component.
    pub minor: u64,
    /// Patch component.
    pub patch: u64,
}

impl ReleaseVersion {
    /// The version this binary was built as.
    pub fn current() -> Self {
        parse_version(env!("CARGO_PKG_VERSION"))
            .expect("CARGO_PKG_VERSION is always a valid release version")
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parse a bare `X.Y.Z` version (no tag prefix, no pre-release).
pub fn parse_version(value: &str) -> Result<ReleaseVersion, String> {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("invalid version '{value}' (expected X.Y.Z)"));
    }
    let parse = |part: &str| {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("invalid version '{value}' (expected X.Y.Z)"));
        }
        part.parse::<u64>()
            .map_err(|_| format!("invalid version '{value}' (expected X.Y.Z)"))
    };
    Ok(ReleaseVersion {
        major: parse(parts[0])?,
        minor: parse(parts[1])?,
        patch: parse(parts[2])?,
    })
}

/// Parse an exact release tag of the form `vX.Y.Z`.
///
/// Pre-release tags (`v1.2.3-rc.1`), missing prefixes, and anything that is
/// not exactly three numeric components are rejected: the default update
/// channel is latest-stable only.
pub fn parse_tag(tag: &str) -> Result<ReleaseVersion, String> {
    let bare = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("invalid release tag '{tag}' (expected vX.Y.Z)"))?;
    parse_version(bare).map_err(|_| format!("invalid release tag '{tag}' (expected vX.Y.Z)"))
}

/// Render the exact tag for a version (`vX.Y.Z`).
pub fn tag_for(version: ReleaseVersion) -> String {
    format!("v{version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_stable_tags() {
        let v = parse_tag("v1.2.3").unwrap();
        assert_eq!(
            v,
            ReleaseVersion {
                major: 1,
                minor: 2,
                patch: 3,
            }
        );
        assert_eq!(tag_for(v), "v1.2.3");
    }

    #[test]
    fn rejects_non_stable_tags() {
        for bad in [
            "1.2.3",
            "v1.2",
            "v1.2.3.4",
            "v1.2.3-rc.1",
            "v1.2.3+build",
            "vX.Y.Z",
            "latest",
            "",
            "v",
        ] {
            assert!(
                parse_tag(bad).is_err(),
                "tag '{bad}' must not become an update candidate"
            );
        }
    }

    #[test]
    fn compares_semver_numerically() {
        let current = parse_version("1.0.4").unwrap();
        assert!(parse_version("1.0.5").unwrap() > current);
        assert!(parse_version("1.1.0").unwrap() > current);
        assert!(parse_version("2.0.0").unwrap() > current);
        assert_eq!(parse_version("1.0.4").unwrap(), current);
        assert!(parse_version("1.0.3").unwrap() < current);
        assert!(parse_version("0.9.9").unwrap() < current);
        // Numeric, not lexicographic: 1.0.10 > 1.0.9.
        assert!(parse_version("1.0.10").unwrap() > parse_version("1.0.9").unwrap());
    }

    #[test]
    fn current_matches_package_version() {
        assert_eq!(
            ReleaseVersion::current().to_string(),
            env!("CARGO_PKG_VERSION")
        );
    }
}
