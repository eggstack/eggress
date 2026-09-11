//! Bounded `curl` downloads for self-update.
//!
//! Like the bootstrap installer, the updater shells out to `curl` with
//! explicit discovery, timeouts, redirect following, and failure capture
//! instead of adding a heavyweight HTTP client dependency to `eggress-cli`.
//! Downloaded content is only ever treated as data (archives/checksums), and
//! nothing fetched here is shell-evaluated.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Why a download failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// No `curl` executable is available on `PATH`.
    MissingCurl,
    /// The asset does not exist at this release (HTTP 404).
    NotFound(String),
    /// Any other transport failure with a captured detail.
    Transport(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::MissingCurl => write!(
                f,
                "curl is required for `eggress update` but was not found on PATH; \
                 install curl or update with the bootstrap installer instead \
                 (see docs/INSTALLATION.md)"
            ),
            DownloadError::NotFound(url) => write!(
                f,
                "release asset not found: {url} (the release may predate prebuilt binaries for this target)"
            ),
            DownloadError::Transport(detail) => write!(f, "download failed: {detail}"),
        }
    }
}

/// Locate `curl` on `PATH`.
pub fn find_curl() -> Result<PathBuf, DownloadError> {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let exe = if cfg!(windows) { "curl.exe" } else { "curl" };
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    // `curl.exe` ships with modern Windows even when PATH lookup above
    // misses it; let process spawning be the final arbiter there.
    if cfg!(windows) {
        return Ok(PathBuf::from(exe));
    }
    Err(DownloadError::MissingCurl)
}

/// Fetch a URL body as text (used for the small release-metadata JSON).
/// `file://` URLs are honored for fixture-based tests; anything else must
/// be `https://`.
pub fn curl_fetch_text(url: &str, user_agent: &str) -> Result<String, DownloadError> {
    let curl = find_curl()?;
    let mut cmd = Command::new(curl);
    cmd.arg("-fsSL")
        .arg("--connect-timeout")
        .arg("15")
        .arg("--max-time")
        .arg("60")
        .arg("-H")
        .arg("Accept: application/vnd.github+json")
        .arg("-A")
        .arg(user_agent);
    if url.starts_with("https://") {
        cmd.arg("--proto").arg("=https").arg("--tlsv1.2");
    } else if !url.starts_with("file://") {
        return Err(DownloadError::Transport(format!(
            "refusing non-HTTPS update URL: {}",
            redact_url(url)
        )));
    }
    cmd.arg(url);
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DownloadError::MissingCurl
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
pub fn curl_download(url: &str, dest: &Path, user_agent: &str) -> Result<(), DownloadError> {
    let curl = find_curl()?;
    let mut cmd = Command::new(curl);
    cmd.arg("-fsSL")
        .arg("--connect-timeout")
        .arg("15")
        .arg("--max-time")
        .arg("600")
        .arg("-A")
        .arg(user_agent);
    if url.starts_with("https://") {
        cmd.arg("--proto").arg("=https").arg("--tlsv1.2");
    } else if !url.starts_with("file://") {
        return Err(DownloadError::Transport(format!(
            "refusing non-HTTPS update URL: {}",
            redact_url(url)
        )));
    }
    cmd.arg("-o").arg(dest).arg(url);
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DownloadError::MissingCurl
        } else {
            DownloadError::Transport(format!("failed to run curl: {e}"))
        }
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(classify_curl_failure(
        url,
        output.status.code(),
        &output.stderr,
    ))
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
}
