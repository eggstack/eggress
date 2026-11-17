//! Bounded `curl` downloads for self-update.
//!
//! Like the bootstrap installer, the updater shells out to `curl` with
//! timeouts, redirect following, and failure capture instead of adding a
//! heavyweight HTTP client dependency to `eggress-cli`. Downloaded content is
//! only ever treated as data (archives/checksums), and nothing fetched here is
//! shell-evaluated.
//!
//! Trust boundaries:
//!
//! - `curl` is resolved only from fixed system locations
//!   (`/usr/bin/curl`, `/bin/curl`, or `%SystemRoot%\System32\curl.exe`). The
//!   updater never scans `PATH`, and it rejects a symlinked, non-regular, or
//!   group/other-writable candidate.
//! - Every URL must be `https://`; `file://` is accepted only by test builds
//!   or the non-default `update-fixtures` feature, never by a release build.
//! - Download outputs are created with `create_new` (owner-only, no symlink
//!   following) inside the private staging directory, so a planted path can
//!   only fail the update, never redirect the write.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Why a download failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// No trustworthy `curl` executable exists at a fixed system location.
    MissingCurl(String),
    /// The staging output path already exists or is not a safe regular file
    /// (for example a symlink planted at the archive path).
    UnsafeDestination(String),
    /// The asset does not exist at this release (HTTP 404).
    NotFound(String),
    /// Any other transport failure with a captured detail.
    Transport(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::MissingCurl(detail) => write!(
                f,
                "curl is required for `eggress update` but no trusted executable was found \
                 (looked only at {}; {detail}); install curl as a system package or update \
                 with the bootstrap installer instead (see docs/INSTALLATION.md)",
                trusted_locations_description()
            ),
            DownloadError::UnsafeDestination(detail) => write!(f, "{detail}"),
            DownloadError::NotFound(url) => write!(
                f,
                "release asset not found: {url} (the release may predate prebuilt binaries for this target)"
            ),
            DownloadError::Transport(detail) => write!(f, "download failed: {detail}"),
        }
    }
}

/// Fixed system locations for the integrity-critical `curl` executable.
///
/// `PATH` is deliberately absent: a writable or user-controlled `PATH` entry
/// must never be able to supply the binary that fetches release assets.
#[cfg(windows)]
fn trusted_curl_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os("SystemRoot") {
        candidates.push(PathBuf::from(root).join("System32").join("curl.exe"));
    }
    let fallback = PathBuf::from(r"C:\Windows\System32\curl.exe");
    if !candidates.contains(&fallback) {
        candidates.push(fallback);
    }
    candidates
}

#[cfg(not(windows))]
fn trusted_curl_candidates() -> Vec<PathBuf> {
    vec![PathBuf::from("/usr/bin/curl"), PathBuf::from("/bin/curl")]
}

fn trusted_locations_description() -> &'static str {
    #[cfg(windows)]
    {
        r"%SystemRoot%\System32\curl.exe"
    }
    #[cfg(not(windows))]
    {
        "/usr/bin/curl and /bin/curl"
    }
}

/// Test-only `curl` override (`EGRESS_UPDATE_CURL`).
///
/// Compiled for unit tests and the non-default `update-fixtures` feature only,
/// so release builds cannot be pointed at an attacker-supplied executable.
#[cfg(any(test, feature = "update-fixtures"))]
fn fixture_curl_override() -> Option<PathBuf> {
    std::env::var_os("EGRESS_UPDATE_CURL").map(PathBuf::from)
}

/// Whether `url` is an offline fixture URL.
///
/// Release/default builds always answer `false`, which makes every
/// `file://` (and any other non-HTTPS) update URL fail closed.
#[cfg(any(test, feature = "update-fixtures"))]
fn is_fixture_url(url: &str) -> bool {
    url.starts_with("file://")
}

#[cfg(not(any(test, feature = "update-fixtures")))]
fn is_fixture_url(_url: &str) -> bool {
    false
}

/// Locate a trusted `curl` executable.
///
/// Only fixed system locations are considered. A candidate that is missing,
/// symlinked, not a regular file, or writable by group/other is rejected with
/// its reason; if every candidate is rejected the update fails closed.
pub fn find_curl() -> Result<PathBuf, DownloadError> {
    #[cfg(any(test, feature = "update-fixtures"))]
    {
        if let Some(path) = fixture_curl_override() {
            return Ok(path);
        }
    }
    let mut rejections = Vec::new();
    for candidate in trusted_curl_candidates() {
        match validate_trusted_executable(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(reason) => rejections.push(reason),
        }
    }
    Err(DownloadError::MissingCurl(rejections.join("; ")))
}

/// Reject a tool path that is not an untampered regular file.
fn validate_trusted_executable(path: &Path) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{}: symbolic links are not trusted",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!("{}: not a regular file", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o022 != 0 {
            return Err(format!(
                "{}: writable by group or other (mode {:o})",
                path.display(),
                mode & 0o7777
            ));
        }
    }
    Ok(())
}

/// Build the base `curl` invocation shared by metadata fetches and asset
/// downloads, including the HTTPS-only protocol policy.
fn curl_command(
    url: &str,
    user_agent: &str,
    max_time_secs: &str,
    github_accept: bool,
) -> Result<Command, DownloadError> {
    let curl = find_curl()?;
    let mut cmd = Command::new(curl);
    cmd.arg("-fsSL")
        .arg("--connect-timeout")
        .arg("15")
        .arg("--max-time")
        .arg(max_time_secs);
    if github_accept {
        cmd.arg("-H").arg("Accept: application/vnd.github+json");
    }
    cmd.arg("-A").arg(user_agent);
    if url.starts_with("https://") {
        cmd.arg("--proto").arg("=https").arg("--tlsv1.2");
    } else if !is_fixture_url(url) {
        return Err(DownloadError::Transport(format!(
            "refusing non-HTTPS update URL: {}",
            redact_url(url)
        )));
    }
    cmd.arg(url);
    Ok(cmd)
}

/// Fetch a URL body as text (used for the small release-metadata JSON).
///
/// `file://` URLs are honored only for fixtures; everything else must be
/// `https://`.
pub fn curl_fetch_text(url: &str, user_agent: &str) -> Result<String, DownloadError> {
    let mut cmd = curl_command(url, user_agent, "60", true)?;
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DownloadError::MissingCurl("the trusted curl disappeared before execution".to_string())
        } else {
            DownloadError::Transport(format!("failed to run curl: {e}"))
        }
    })?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|e| DownloadError::Transport(format!("release metadata is not UTF-8: {e}")));
    }
    Err(classify_curl_failure(
        url,
        output.status.code(),
        &output.stderr,
    ))
}

/// Download a URL to `dest` (used for archives and checksum sidecars).
///
/// The destination is created with `create_new` (owner-only, never following a
/// symlink), and `curl` writes into that exact file handle through stdout, so
/// no output path is re-resolved after the trust decision.
pub fn curl_download(url: &str, dest: &Path, user_agent: &str) -> Result<(), DownloadError> {
    let mut cmd = curl_command(url, user_agent, "600", false)?;
    let output_file = create_private_output(dest)?;
    cmd.stdout(Stdio::from(output_file)).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DownloadError::MissingCurl("the trusted curl disappeared before execution".to_string())
        } else {
            DownloadError::Transport(format!("failed to run curl: {e}"))
        }
    })?;
    // `wait_with_output` drains the piped stderr while the child runs; stdout
    // is the already-open staging file, so no path is ever followed.
    let output = child
        .wait_with_output()
        .map_err(|e| DownloadError::Transport(format!("failed to run curl: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(classify_curl_failure(
        url,
        output.status.code(),
        &output.stderr,
    ))
}

/// Create a download output path without following symlinks and without
/// overwriting an existing file.
///
/// `create_new` maps to `O_CREAT|O_EXCL`, which fails (rather than following)
/// when the path is a symlink and refuses any pre-existing file, so a local
/// user cannot redirect the download or pre-seed its content.
fn create_private_output(dest: &Path) -> Result<File, DownloadError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(dest).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            DownloadError::UnsafeDestination(format!(
                "refusing to reuse existing update staging path {}; another update \
                 transaction or a planted path may own it",
                dest.display()
            ))
        } else {
            DownloadError::Transport(format!(
                "cannot create update staging file {}: {e}",
                dest.display()
            ))
        }
    })
}

fn classify_curl_failure(url: &str, code: Option<i32>, stderr: &[u8]) -> DownloadError {
    let detail = String::from_utf8_lossy(stderr);
    // `curl -f` exits 22 on HTTP errors; a 404 means the asset is absent
    // from an otherwise reachable release.
    if code == Some(22) && detail.contains("404") {
        return DownloadError::NotFound(redact_url(url));
    }
    let last_line = detail.lines().last().unwrap_or("curl failed").trim();
    DownloadError::Transport(format!("{} (curl exit {})", last_line, code.unwrap_or(-1)))
}

/// Redact any accidental credentials from logged URLs (update URLs never
/// carry credentials by construction).
fn redact_url(url: &str) -> String {
    if let Some(at) = url.rfind('@') {
        if let Some(scheme_end) = url.find("://") {
            if at > scheme_end + 3 {
                return format!("{}://{}<redacted>", &url[..scheme_end], &url[at..]);
            }
        }
    }
    url.to_string()
}

/// User-Agent identifying Eggress/version. Optional for correctness, but
/// polite to the release endpoint.
pub fn user_agent() -> String {
    format!("eggress-update/{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_non_https_update_urls() {
        let dir = std::env::temp_dir();
        let dest = dir.join("eggress-update-refused-test");
        let _ = std::fs::remove_file(&dest);
        let err = curl_download("http://example.com/eggress.tar.gz", &dest, "test").unwrap_err();
        assert!(
            matches!(err, DownloadError::Transport(_)),
            "plain-HTTP update URLs must be refused: {err:?}"
        );
        assert!(!dest.exists());
    }

    #[test]
    fn redacts_credentials_from_logged_urls() {
        assert_eq!(
            redact_url("https://user:secret@github.com/x"),
            "https://@github.com/x<redacted>"
        );
        assert_eq!(
            redact_url("https://github.com/eggstack/eggress/releases/latest"),
            "https://github.com/eggstack/eggress/releases/latest"
        );
    }

    #[test]
    fn trusted_candidates_are_fixed_system_locations() {
        let candidates = trusted_curl_candidates();
        assert!(!candidates.is_empty());
        for candidate in &candidates {
            assert!(
                candidate.is_absolute(),
                "trusted curl candidates must be absolute: {}",
                candidate.display()
            );
        }
        // The candidate set is a constant: it is not computed from `PATH`.
        #[cfg(not(windows))]
        assert_eq!(
            candidates,
            vec![PathBuf::from("/usr/bin/curl"), PathBuf::from("/bin/curl")]
        );
    }

    #[test]
    fn rejects_symlinked_or_writable_tools() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let root =
                std::env::temp_dir().join(format!("eggress-curl-trust-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();

            let real = root.join("curl-real");
            std::fs::write(&real, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
            validate_trusted_executable(&real).unwrap();

            let link = root.join("curl-link");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let err = validate_trusted_executable(&link).unwrap_err();
            assert!(err.contains("symbolic link"), "{err}");

            let writable = root.join("curl-writable");
            std::fs::write(&writable, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&writable, std::fs::Permissions::from_mode(0o777)).unwrap();
            let err = validate_trusted_executable(&writable).unwrap_err();
            assert!(err.contains("group or other"), "{err}");

            let dir_at_path = root.join("curl-dir");
            std::fs::create_dir_all(&dir_at_path).unwrap();
            let err = validate_trusted_executable(&dir_at_path).unwrap_err();
            assert!(err.contains("not a regular file"), "{err}");

            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn staging_outputs_are_created_new_and_owner_only() {
        let root = std::env::temp_dir().join(format!("eggress-curl-output-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dest = root.join("archive.tar.gz");

        drop(create_private_output(&dest).unwrap());
        let err = create_private_output(&dest).unwrap_err();
        assert!(
            matches!(err, DownloadError::UnsafeDestination(_)),
            "an existing staging path must never be reused: {err:?}"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "staging files must be owner-only");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pre_existing_symlink_at_archive_path_is_refused() {
        let Ok(_) = find_curl() else {
            return; // no trusted curl on this host; fail-closed covered above.
        };
        #[cfg(unix)]
        {
            let root =
                std::env::temp_dir().join(format!("eggress-curl-link-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let victim = root.join("victim");
            std::fs::write(&victim, b"do-not-touch").unwrap();
            let source = root.join("source.tar.gz");
            std::fs::write(&source, b"archive-bytes").unwrap();
            let dest = root.join("archive.tar.gz");
            std::os::unix::fs::symlink(&victim, &dest).unwrap();

            let url = format!("file://{}", source.display());
            let err = curl_download(&url, &dest, "test").unwrap_err();
            assert!(
                matches!(err, DownloadError::UnsafeDestination(_)),
                "a symlinked archive path must fail closed: {err:?}"
            );
            assert_eq!(std::fs::read(&victim).unwrap(), b"do-not-touch");
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
