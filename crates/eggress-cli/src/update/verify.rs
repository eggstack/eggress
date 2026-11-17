//! Archive checksum and staged-executable identity verification.
//!
//! Verification order is mandatory and shared with the bootstrap
//! installers: SHA-256 of the complete archive first, then extraction, then
//! both staged executables are executed and their reported versions must
//! equal the release tag exactly and agree with one another. Only then may
//! replacement begin. Any mismatch is a hard error with no fallback to
//! another install source.

use std::io::Read;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

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

/// Compute the SHA-256 hex digest of a file in-process.
///
/// This hash is the integrity authority. No `sha256sum`/`shasum` process is
/// discovered, executed, or parsed, so a `PATH`-planted tool can never
/// influence whether an archive is accepted.
pub fn compute_sha256(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        format!(
            "cannot open {} for SHA-256 verification: {e}",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| {
            format!(
                "cannot read {} for SHA-256 verification: {e}",
                path.display()
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    {
        use std::fmt::Write as _;
        for byte in digest {
            // Writing into a `String` cannot fail.
            let _ = write!(hex, "{byte:02x}");
        }
    }
    Ok(hex)
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

    /// FIPS 180-2 / RFC 6234 known-answer vectors: the in-process hasher must
    /// agree with the published SHA-256 values, not with a shell tool.
    #[test]
    fn sha256_matches_known_vectors() {
        let root = std::env::temp_dir().join(format!("eggress-sha256-vec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let empty = root.join("empty");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(
            compute_sha256(&empty).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let abc = root.join("abc");
        std::fs::write(&abc, b"abc").unwrap();
        assert_eq!(
            compute_sha256(&abc).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        // Multi-block input: 1,000,000 'a' bytes crosses the 64 KiB read
        // buffer many times and must still hash as one stream.
        let million = root.join("million");
        std::fs::write(&million, vec![b'a'; 1_000_000]).unwrap();
        assert_eq!(
            compute_sha256(&million).unwrap(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );

        assert!(compute_sha256(&root.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The sidecar remains the gate: the in-process digest of the exact
    /// archive file is what decides acceptance.
    #[test]
    fn sidecar_comparison_uses_the_in_process_digest() {
        let root = std::env::temp_dir().join(format!("eggress-sha256-side-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("eggress-x86_64-unknown-linux-gnu.tar.gz");
        std::fs::write(&archive, b"release-bytes").unwrap();

        let digest = compute_sha256(&archive).unwrap();
        let sidecar = format!("{digest}  eggress-x86_64-unknown-linux-gnu.tar.gz\n");
        verify_archive(&archive, &sidecar).unwrap();

        std::fs::write(&archive, b"tampered-bytes").unwrap();
        let err = verify_archive(&archive, &sidecar).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");

        // Same bytes written back: the digest is deterministic.
        std::fs::write(&archive, b"release-bytes").unwrap();
        verify_archive(&archive, &sidecar).unwrap();
        let _ = std::fs::remove_dir_all(&root);
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

    #[cfg(unix)]
    #[test]
    fn staged_pair_must_agree_with_tag() {
        // Fixture candidates are shell scripts so no compilation is needed.
        // Unix-only: Windows cannot execute POSIX shell fixtures (same
        // gating as the offline fixture flows in `mod.rs`).
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
