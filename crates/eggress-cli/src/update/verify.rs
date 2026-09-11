//! Archive checksum and staged-executable identity verification.
//!
//! Verification order is mandatory and shared with the bootstrap
//! installers: SHA-256 of the complete archive first, then extraction, then
//! both staged executables are executed and their reported versions must
//! equal the release tag exactly and agree with one another. Only then may
//! replacement begin. Any mismatch is a hard error with no fallback to
//! another install source.

use std::path::Path;
use std::process::Command;

use super::version::ReleaseVersion;

/// Parse a `.sha256` sidecar to its expected lowercase hex digest.
///
/// Accepts the canonical `<hex>  <filename>` format (also tolerates a bare
/// hex digest). The sidecar travels with the GitHub Release: it detects
/// corruption or mismatched assets but is not an independent
/// signature/provenance guarantee.
pub fn parse_checksum_sidecar(text: &str) -> Result<String, String> {
    for line in text.lines() {
        let token = line.split_whitespace().next().unwrap_or("");
        if token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(token.to_ascii_lowercase());
        }
    }
    Err("checksum sidecar contains no SHA-256 digest".to_string())
}

/// Compute the SHA-256 hex digest of a file using `sha256sum` or
/// `shasum -a 256` (the same tools the bootstrap installer uses).
pub fn compute_sha256(path: &Path) -> Result<String, String> {
    for (program, args) in [("sha256sum", vec![]), ("shasum", vec!["-a", "256"])] {
        let output = match Command::new(program).args(&args).arg(path).output() {
            Ok(output) => output,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(format!("failed to run {program}: {e}"));
            }
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(digest) = stdout
            .split_whitespace()
            .next()
            .filter(|t| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Ok(digest.to_ascii_lowercase());
        }
    }
    Err("neither `sha256sum` nor `shasum -a 256` is available; install one or update with the bootstrap installer instead".to_string())
}

/// Verify an archive against its sidecar text. Fails closed on any mismatch.
pub fn verify_archive(archive: &Path, sidecar_text: &str) -> Result<(), String> {
    let expected = parse_checksum_sidecar(sidecar_text)?;
    let actual = compute_sha256(archive)?;
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {}: expected {expected}, got {actual}; leaving the current installation untouched",
            archive.display()
        ));
    }
    Ok(())
}

/// Parse `eggress version` output: exactly `eggress X.Y.Z` on one line.
pub fn parse_eggress_version(output: &str) -> Result<ReleaseVersion, String> {
    let line = output.trim();
    if line.lines().count() != 1 {
        return Err(format!("unexpected `eggress version` output: {output:?}"));
    }
    let version = line
        .strip_prefix("eggress ")
        .ok_or_else(|| format!("unexpected `eggress version` output: {output:?}"))?;
    super::version::parse_version(version.trim())
        .map_err(|_| format!("unexpected `eggress version` output: {output:?}"))
}

/// Parse `pproxy --version` output (`eggress-pproxy-compat X.Y.Z`).
/// The compatibility binary keeps its pproxy-style version surface; the
/// verifier parses the version deliberately rather than requiring the
/// binary to pretend its public name is `pproxy`.
pub fn parse_pproxy_version(output: &str) -> Result<ReleaseVersion, String> {
    let line = output.trim();
    if line.lines().count() != 1 {
        return Err(format!("unexpected `pproxy --version` output: {output:?}"));
    }
    let version = line
        .strip_prefix("eggress-pproxy-compat ")
        .ok_or_else(|| format!("unexpected `pproxy --version` output: {output:?}"))?;
    super::version::parse_version(version.trim())
        .map_err(|_| format!("unexpected `pproxy --version` output: {output:?}"))
}

/// Execute a staged candidate and capture its version output.
pub fn run_candidate_version(exe: &Path, arg: &str) -> Result<String, String> {
    let output = Command::new(exe)
        .arg(arg)
        .output()
        .map_err(|e| format!("failed to execute staged candidate {}: {e}", exe.display()))?;
    if !output.status.success() {
        return Err(format!(
            "staged candidate {} exited with {}",
            exe.display(),
            output.status.code().unwrap_or(-1)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| {
        format!(
            "staged candidate {} printed non-UTF-8 output: {e}",
            exe.display()
        )
    })
}

/// Verify both staged executables report `expected` and agree with one
/// another. This runs after checksum verification and before replacement.
pub fn verify_staged_pair(
    staged_eggress: &Path,
    staged_pproxy: &Path,
    expected: ReleaseVersion,
) -> Result<(), String> {
    let eggress_out = run_candidate_version(staged_eggress, "version")?;
    let eggress_version = parse_eggress_version(&eggress_out)?;
    let pproxy_out = run_candidate_version(staged_pproxy, "--version")?;
    let pproxy_version = parse_pproxy_version(&pproxy_out)?;
    if eggress_version != expected {
        return Err(format!(
            "staged `eggress version` reports {eggress_version} but the release tag is {expected}; leaving the current installation untouched"
        ));
    }
    if pproxy_version != expected {
        return Err(format!(
            "staged `pproxy --version` reports {pproxy_version} but the release tag is {expected}; leaving the current installation untouched"
        ));
    }
    if eggress_version != pproxy_version {
        return Err(format!(
            "staged versions disagree (eggress {eggress_version} vs pproxy {pproxy_version}); leaving the current installation untouched"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sidecar_and_rejects_garbage() {
        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(
            parse_checksum_sidecar(&format!(
                "{digest}  eggress-x86_64-unknown-linux-gnu.tar.gz\n"
            ))
            .unwrap(),
            digest
        );
        assert_eq!(parse_checksum_sidecar(digest).unwrap(), digest);
        assert!(parse_checksum_sidecar("not a checksum").is_err());
        assert!(parse_checksum_sidecar("").is_err());
    }

    #[test]
    fn parses_candidate_version_strings() {
        assert_eq!(
            parse_eggress_version("eggress 1.2.3\n")
                .unwrap()
                .to_string(),
            "1.2.3"
        );
        assert_eq!(
            parse_pproxy_version("eggress-pproxy-compat 1.2.3\n")
                .unwrap()
                .to_string(),
            "1.2.3"
        );
        assert!(parse_eggress_version("eggress-pproxy-compat 1.2.3").is_err());
        assert!(parse_pproxy_version("pproxy 1.2.3").is_err());
        assert!(parse_eggress_version("eggress 1.2.3\neggress 1.2.3").is_err());
        assert!(parse_eggress_version("").is_err());
    }

    #[test]
    fn staged_pair_must_agree_with_tag() {
        // Fixture candidates are shell scripts so no compilation is needed.
        let dir = std::env::temp_dir().join(format!("eggress-verify-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let write_script = |name: &str, body: &str| {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        };
        let expected = super::super::version::parse_version("9.9.9").unwrap();
        let good_eggress = write_script("eggress", "#!/bin/sh\necho 'eggress 9.9.9'\n");
        let good_pproxy = write_script("pproxy", "#!/bin/sh\necho 'eggress-pproxy-compat 9.9.9'\n");
        verify_staged_pair(&good_eggress, &good_pproxy, expected).unwrap();

        let wrong = write_script("eggress-wrong", "#!/bin/sh\necho 'eggress 9.9.8'\n");
        assert!(verify_staged_pair(&wrong, &good_pproxy, expected).is_err());
        let disagree = write_script(
            "pproxy-disagree",
            "#!/bin/sh\necho 'eggress-pproxy-compat 9.9.7'\n",
        );
        // Both equal the tag check happens first; craft a tag both miss
        // differently by reusing `expected` for one side only.
        assert!(verify_staged_pair(&good_eggress, &disagree, expected).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
