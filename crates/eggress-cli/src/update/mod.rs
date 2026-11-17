//! `eggress update`: self-update from verified GitHub Release assets.
//!
//! The updater treats `eggress` and `pproxy` as one release unit and trusts
//! GitHub Releases (not crates.io) as the binary update authority, because
//! the updater installs GitHub-built binary archives. Release preflight
//! guarantees the tag, workspace, and Python versions agree, so both
//! channels advertise the same version for a tag.
//!
//! Hard guarantees:
//!
//! - no background update checks, no telemetry, no automatic `sudo`/UAC
//!   escalation, no implicit Cargo/source fallback;
//! - checksum and staged-executable identity/version are verified before
//!   replacement begins; any mismatch fails closed with no fallback to
//!   another install source;
//! - a failed update leaves the prior installation usable;
//! - normal operation never touches the network except for this explicit
//!   command; unit tests never depend on live GitHub availability
//!   (`EGRESS_UPDATE_BASE_URL` + `file://` fixtures cover the flow offline,
//!   compiled only for tests or the non-default `update-fixtures` feature);
//! - every update URL is HTTPS and every integrity decision is made
//!   in-process; `curl` is resolved from fixed system locations and never
//!   from `PATH`;
//! - staging is an unpredictable, owner-only directory created atomically,
//!   downloads are `create_new`, and the archive is re-hashed immediately
//!   before extraction.

pub mod download;
pub mod eggup;
pub mod install;
pub mod target;
pub mod verify;
pub mod version;

use std::path::{Path, PathBuf};

use eggress_cli::{
    EXIT_CLI_PARSE_ERROR, EXIT_EXTERNAL_DEPENDENCY, EXIT_PLATFORM_MISSING, EXIT_RUNTIME_FAILURE,
    EXIT_SUCCESS,
};

use download::{curl_download, curl_fetch_text, user_agent, DownloadError};
use eggup::install_pair_via_eggup;
use install::{check_destination_writable, sibling_pproxy_path};
use target::{asset_urls, detect_target, download_base_for_tag, latest_release_api_url};
use verify::{run_candidate_version, verify_archive};
use version::{parse_tag, tag_for, ReleaseVersion};

/// Handle `eggress update`: update the standalone installation to the
/// latest stable GitHub Release. Returns the process exit code.
///
/// Progress goes to stderr; the final outcome goes to stdout.
pub fn handle_update() -> i32 {
    match run_update() {
        Ok(outcome) => {
            println!("{outcome}");
            EXIT_SUCCESS
        }
        Err(failure) => {
            eprintln!("error: {failure}");
            failure.code()
        }
    }
}

/// Typed update failure mapped onto the shared CLI exit contract.
/// No new numeric exit codes are introduced here.
#[derive(Debug)]
enum UpdateFailure {
    /// Unsupported prebuilt target.
    UnsupportedTarget(target::UnsupportedTarget),
    /// Release discovery/download/tool failure.
    ExternalDependency(String),
    /// Invalid release metadata (malformed tag).
    InvalidRelease(String),
    /// Verification, staging, or replacement failure. The current install
    /// is untouched or was restored.
    Failed(String),
}

impl UpdateFailure {
    fn code(&self) -> i32 {
        match self {
            UpdateFailure::UnsupportedTarget(_) => EXIT_PLATFORM_MISSING,
            UpdateFailure::ExternalDependency(_) => EXIT_EXTERNAL_DEPENDENCY,
            UpdateFailure::InvalidRelease(_) => EXIT_CLI_PARSE_ERROR,
            UpdateFailure::Failed(_) => EXIT_RUNTIME_FAILURE,
        }
    }
}

impl std::fmt::Display for UpdateFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateFailure::UnsupportedTarget(e) => write!(f, "{e}"),
            UpdateFailure::ExternalDependency(e)
            | UpdateFailure::InvalidRelease(e)
            | UpdateFailure::Failed(e) => write!(f, "{e}"),
        }
    }
}

fn run_update() -> Result<String, UpdateFailure> {
    let current = ReleaseVersion::current();
    let target = detect_target().map_err(UpdateFailure::UnsupportedTarget)?;

    // Offline fixture redirect. Only tests or the non-default
    // `update-fixtures` feature can produce a value here; release builds
    // ignore `EGRESS_UPDATE_BASE_URL` entirely.
    let base_override = update_base_url_override();

    let latest = discover_latest_release(&base_override)?;
    if current >= latest {
        return Ok(format!(
            "eggress {current} is already the latest stable version"
        ));
    }

    let (installed_eggress, installed_pproxy) = installation_paths()?;
    let install_dir = installed_eggress
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Verify the sibling belongs to this installation before downloading
    // anything: never replace only `eggress` silently.
    verify_sibling_identity(&installed_pproxy, current)?;

    check_destination_writable(&install_dir).map_err(UpdateFailure::Failed)?;

    let tag = tag_for(latest);
    let download_base = base_override.unwrap_or_else(|| download_base_for_tag(&tag));
    let (archive_url, checksum_url) = asset_urls(&download_base, target);

    let stage = staging_dir()?;
    let archive_path = stage.path().join(target::archive_name(target));
    let checksum_path = stage.path().join(target::checksum_name(target));

    let outcome = update_from_urls(
        &archive_url,
        &checksum_url,
        target,
        latest,
        stage.path(),
        &archive_path,
        &checksum_path,
        &installed_eggress,
        &installed_pproxy,
    );

    // Dropping the guard removes this transaction's private staging tree
    // (its own path only; best-effort and never an unrelated path).
    drop(stage);
    let outcome = outcome?;

    Ok(format!("updated eggress {current} -> {latest}{outcome}"))
}

/// Offline fixture redirect for release discovery/downloads.
///
/// Compiled away for release/default builds, which therefore never read
/// `EGRESS_UPDATE_BASE_URL` and cannot be pointed at a `file://` tree.
#[cfg(any(test, feature = "update-fixtures"))]
fn update_base_url_override() -> Option<String> {
    std::env::var("EGRESS_UPDATE_BASE_URL").ok()
}

#[cfg(not(any(test, feature = "update-fixtures")))]
fn update_base_url_override() -> Option<String> {
    None
}

/// Download, verify, stage, and install one release. Factored out so
/// fixture-based tests can drive the full flow offline.
#[allow(clippy::too_many_arguments)]
fn update_from_urls(
    archive_url: &str,
    checksum_url: &str,
    target: &str,
    expected: ReleaseVersion,
    stage: &Path,
    archive_path: &Path,
    checksum_path: &Path,
    installed_eggress: &Path,
    installed_pproxy: &Path,
) -> Result<String, UpdateFailure> {
    let agent = user_agent();
    eprintln!("downloading {}", target::archive_name(target));
    curl_download(archive_url, archive_path, &agent).map_err(map_download_error)?;
    curl_download(checksum_url, checksum_path, &agent).map_err(map_download_error)?;

    let sidecar = std::fs::read_to_string(checksum_path).map_err(|e| {
        UpdateFailure::Failed(format!(
            "cannot read checksum sidecar: {e}; leaving the current installation untouched"
        ))
    })?;

    eprintln!("verifying checksum");
    verify_archive(archive_path, &sidecar).map_err(UpdateFailure::Failed)?;

    eprintln!("extracting archive and installing eggress {expected}");
    let install_dir = installed_eggress.parent().ok_or_else(|| {
        UpdateFailure::Failed(format!(
            "cannot resolve installation directory from {}",
            installed_eggress.display()
        ))
    })?;
    // The sibling pair requirement: both live destinations must sit in the
    // same installation directory the transaction will own.
    if installed_pproxy.parent() != Some(install_dir) {
        return Err(UpdateFailure::Failed(format!(
            "sibling `pproxy` is not next to {}; refusing to update a split pair — reinstall with the bootstrap installer to repair it (see docs/INSTALLATION.md)",
            installed_eggress.display()
        )));
    }
    // Re-hash immediately before extraction: the staging directory is
    // private and create-new, but a replacement after the first verification
    // must still fail closed instead of extracting unverified bytes.
    verify_archive_before_extraction(archive_path, &sidecar)?;
    install_pair_via_eggup(archive_path, target, expected, install_dir, stage)
        .map_err(UpdateFailure::Failed)
}

/// Last integrity gate before extraction.
///
/// `verify_archive` already ran once after download; this second hash of the
/// exact archive file closes the verify/extract window, so a post-verification
/// replacement is rejected instead of extracted.
fn verify_archive_before_extraction(archive: &Path, sidecar: &str) -> Result<(), UpdateFailure> {
    verify::verify_archive(archive, sidecar).map_err(|e| {
        UpdateFailure::Failed(format!(
            "{e} (the staged archive changed after checksum verification)"
        ))
    })
}

/// Resolve the latest stable release version from GitHub (or fixtures).
fn discover_latest_release(
    base_override: &Option<String>,
) -> Result<ReleaseVersion, UpdateFailure> {
    let (url, is_fixture) = match base_override {
        Some(base) => (format!("{}/latest.json", base.trim_end_matches('/')), true),
        None => (latest_release_api_url(), false),
    };
    let body = curl_fetch_text(&url, &user_agent()).map_err(map_download_error)?;
    let tag = if is_fixture {
        parse_fixture_tag(&body)?
    } else {
        parse_github_tag(&body)?
    };
    parse_tag(&tag).map_err(UpdateFailure::InvalidRelease)
}

/// Extract `tag_name` from the GitHub `releases/latest` response with a
/// minimal structural parse (no full API model to keep in sync).
fn parse_github_tag(body: &str) -> Result<String, UpdateFailure> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        UpdateFailure::ExternalDependency(format!("cannot parse release metadata: {e}"))
    })?;
    value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            UpdateFailure::ExternalDependency("release metadata is missing tag_name".to_string())
        })
}

/// Fixture metadata is the bare tag on one line.
fn parse_fixture_tag(body: &str) -> Result<String, UpdateFailure> {
    let tag = body.trim().to_string();
    if tag.is_empty() {
        return Err(UpdateFailure::ExternalDependency(
            "fixture release metadata is empty".to_string(),
        ));
    }
    Ok(tag)
}

/// Derive the installed pair from the running executable.
fn installation_paths() -> Result<(PathBuf, PathBuf), UpdateFailure> {
    let current = std::env::current_exe().map_err(|e| {
        UpdateFailure::Failed(format!("cannot resolve the running executable: {e}"))
    })?;
    let sibling = sibling_pproxy_path(&current).map_err(UpdateFailure::Failed)?;
    Ok((current, sibling))
}

/// Confirm the sibling exists and identifies as this installation's
/// version-aligned `pproxy` before anything is mutated.
fn verify_sibling_identity(sibling: &Path, current: ReleaseVersion) -> Result<(), UpdateFailure> {
    if !sibling.is_file() {
        return Err(UpdateFailure::Failed(format!(
            "no matching sibling Eggress `pproxy` next to {}; refusing to replace only `eggress` — reinstall with the bootstrap installer to repair the pair (see docs/INSTALLATION.md)",
            sibling.display()
        )));
    }
    let output = run_candidate_version(sibling, "--version").map_err(UpdateFailure::Failed)?;
    let sibling_version = verify::parse_pproxy_version(&output).map_err(UpdateFailure::Failed)?;
    if sibling_version != current {
        return Err(UpdateFailure::Failed(format!(
            "sibling `pproxy` reports {sibling_version} but this `eggress` is {current}; refusing to update a mismatched pair — reinstall with the bootstrap installer to repair it (see docs/INSTALLATION.md)"
        )));
    }
    Ok(())
}

/// Create the private staging directory for one update transaction.
///
/// `tempfile` creates the directory atomically under the default temp root
/// (so filesystem/installation semantics match the previous staging) with an
/// unpredictable name. The explicit owner-only mode matters: `tempfile`'s
/// platform default is the process umask (typically `0755`), which would let
/// another local user read the staged archive and race the output paths.
fn staging_dir() -> Result<tempfile::TempDir, UpdateFailure> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("eggress-update-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder
        .tempdir()
        .map_err(|e| UpdateFailure::Failed(format!("cannot create private staging directory: {e}")))
}

fn map_download_error(error: DownloadError) -> UpdateFailure {
    match error {
        DownloadError::MissingCurl(_)
        | DownloadError::NotFound(_)
        | DownloadError::Transport(_) => UpdateFailure::ExternalDependency(error.to_string()),
        DownloadError::UnsafeDestination(_) => UpdateFailure::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_latest_tag() {
        let body = r#"{"tag_name":"v1.2.3","draft":false,"prerelease":false}"#;
        assert_eq!(parse_github_tag(body).unwrap(), "v1.2.3");
        assert!(parse_github_tag(r#"{"name":"x"}"#).is_err());
        assert!(parse_github_tag("not json").is_err());
    }

    #[test]
    fn update_failures_use_shared_exit_contract() {
        assert_eq!(
            UpdateFailure::UnsupportedTarget(target::UnsupportedTarget {
                os: "x".into(),
                arch: "y".into()
            })
            .code(),
            eggress_cli::EXIT_PLATFORM_MISSING
        );
        assert_eq!(
            UpdateFailure::ExternalDependency("x".into()).code(),
            eggress_cli::EXIT_EXTERNAL_DEPENDENCY
        );
        assert_eq!(
            UpdateFailure::InvalidRelease("x".into()).code(),
            eggress_cli::EXIT_CLI_PARSE_ERROR
        );
        assert_eq!(
            UpdateFailure::Failed("x".into()).code(),
            eggress_cli::EXIT_RUNTIME_FAILURE
        );
    }

    /// End-to-end offline flow: fixture release metadata + archive +
    /// checksum over `file://`, installed into a temp pair directory.
    /// Requires a trusted `curl` and `tar` (present on CI runners).
    #[test]
    fn offline_fixture_update_replaces_pair_atomically() {
        let Some(target) = target::detect_target().ok() else {
            return;
        };
        if target::is_windows_target(target) {
            return; // fixture scripts are POSIX shell; Windows covered by installer tests.
        }
        let Some(tar) = absolute_tar() else {
            return;
        };
        if download::find_curl().is_err() {
            return;
        }

        let root = std::env::temp_dir().join(format!("eggress-update-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let release_dir = root.join("release");
        let stage_src = root.join("stage-src");
        let install_dir = root.join("bin");
        let work = root.join("work");
        for dir in [&release_dir, &stage_src, &install_dir, &work] {
            std::fs::create_dir_all(dir).unwrap();
        }

        // New-version fixture binaries (report a version newer than current).
        let current = ReleaseVersion::current();
        let latest = ReleaseVersion {
            major: current.major,
            minor: current.minor,
            patch: current.patch + 1,
        };
        assert!(latest > current);
        write_script(
            &stage_src.join("eggress"),
            &format!("echo 'eggress {latest}'"),
        );
        write_script(
            &stage_src.join("pproxy"),
            &format!("echo 'eggress-pproxy-compat {latest}'"),
        );
        let archive_name = target::archive_name(target);
        let archive_path = release_dir.join(&archive_name);
        let status = std::process::Command::new(&tar)
            .arg("-czf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&stage_src)
            .arg("eggress")
            .arg("pproxy")
            .status()
            .unwrap();
        assert!(status.success());
        let digest = verify::compute_sha256(&archive_path).unwrap();
        std::fs::write(
            release_dir.join(target::checksum_name(target)),
            format!("{digest}  {archive_name}\n"),
        )
        .unwrap();
        std::fs::write(release_dir.join("latest.json"), format!("v{latest}")).unwrap();

        // Old-version installed pair.
        write_script(
            &install_dir.join("eggress"),
            &format!("echo 'eggress {current}'"),
        );
        write_script(
            &install_dir.join("pproxy"),
            &format!("echo 'eggress-pproxy-compat {current}'"),
        );

        let base = format!("file://{}", release_dir.display());
        let (archive_url, checksum_url) = asset_urls(&base, target);
        let outcome = update_from_urls(
            &archive_url,
            &checksum_url,
            target,
            latest,
            &work,
            &work.join(&archive_name),
            &work.join(target::checksum_name(target)),
            &install_dir.join("eggress"),
            &install_dir.join("pproxy"),
        )
        .expect("offline fixture update must succeed");

        assert!(outcome.is_empty());
        let installed_eggress =
            run_candidate_version(&install_dir.join("eggress"), "version").unwrap();
        assert!(installed_eggress.contains(&latest.to_string()));
        let installed_pproxy =
            run_candidate_version(&install_dir.join("pproxy"), "--version").unwrap();
        assert!(installed_pproxy.contains(&latest.to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A tampered archive must fail closed with the old pair untouched.
    #[test]
    fn offline_fixture_checksum_mismatch_leaves_install_intact() {
        let Some(target) = target::detect_target().ok() else {
            return;
        };
        if target::is_windows_target(target) {
            return;
        }
        let Some(tar) = absolute_tar() else {
            return;
        };
        if download::find_curl().is_err() {
            return;
        }

        let root = std::env::temp_dir().join(format!("eggress-update-neg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let release_dir = root.join("release");
        let stage_src = root.join("stage-src");
        let install_dir = root.join("bin");
        let work = root.join("work");
        for dir in [&release_dir, &stage_src, &install_dir, &work] {
            std::fs::create_dir_all(dir).unwrap();
        }

        write_script(&stage_src.join("eggress"), "echo 'eggress 8.8.8'");
        write_script(
            &stage_src.join("pproxy"),
            "echo 'eggress-pproxy-compat 8.8.8'",
        );
        let archive_name = target::archive_name(target);
        let archive_path = release_dir.join(&archive_name);
        std::process::Command::new(&tar)
            .arg("-czf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&stage_src)
            .arg("eggress")
            .arg("pproxy")
            .status()
            .unwrap();
        // Tamper after packaging so the sidecar no longer matches.
        std::fs::write(&archive_path, b"tampered").unwrap();
        std::fs::write(
            release_dir.join(target::checksum_name(target)),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  x\n",
        )
        .unwrap();

        write_script(&install_dir.join("eggress"), "echo 'eggress 1.0.0'");
        write_script(
            &install_dir.join("pproxy"),
            "echo 'eggress-pproxy-compat 1.0.0'",
        );

        let base = format!("file://{}", release_dir.display());
        let (archive_url, checksum_url) = asset_urls(&base, target);
        let err = update_from_urls(
            &archive_url,
            &checksum_url,
            target,
            version::parse_version("8.8.8").unwrap(),
            &work,
            &work.join(&archive_name),
            &work.join(target::checksum_name(target)),
            &install_dir.join("eggress"),
            &install_dir.join("pproxy"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"), "{err}");

        // Old pair untouched.
        assert!(std::fs::read_to_string(install_dir.join("eggress"))
            .unwrap()
            .contains("1.0.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn write_script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// Resolve `tar` once, by absolute path, so a concurrently running test
    /// that temporarily rewrites `PATH` cannot make a fixture spawn fail.
    fn absolute_tar() -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        std::env::split_paths(&path_var)
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join("tar"))
            .find(|candidate| candidate.is_file())
    }

    /// Staging is created atomically with an unpredictable name and
    /// owner-only permissions; a second transaction never reuses or clears
    /// the first one's tree.
    #[test]
    fn staging_directory_is_private_and_unique() {
        let first = staging_dir().unwrap();
        let second = staging_dir().unwrap();
        assert_ne!(first.path(), second.path());
        std::fs::write(first.path().join("marker"), b"in-flight").unwrap();
        assert!(
            first.path().join("marker").is_file(),
            "creating a second staging directory must not disturb the first"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(first.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700, "staging directory must be owner-only");
        }
    }

    /// Concurrent updater attempts get distinct private directories instead
    /// of racing on one predictable path.
    #[test]
    fn concurrent_staging_attempts_do_not_share_a_directory() {
        let handles: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| staging_dir().unwrap()))
            .collect();
        let dirs: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("staging thread panicked"))
            .collect();
        let unique: std::collections::HashSet<_> =
            dirs.iter().map(|dir| dir.path().to_path_buf()).collect();
        assert_eq!(
            unique.len(),
            dirs.len(),
            "concurrent staging attempts must never share a path"
        );
    }

    /// A replacement after the first checksum verification must fail closed
    /// before any extraction happens.
    #[test]
    fn post_verification_archive_replacement_fails_closed() {
        let root =
            std::env::temp_dir().join(format!("eggress-update-toctou-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("eggress-x86_64-unknown-linux-gnu.tar.gz");
        std::fs::write(&archive, b"verified-bytes").unwrap();
        let pristine = verify::compute_sha256(&archive).unwrap();
        let sidecar = format!("{pristine}  eggress-x86_64-unknown-linux-gnu.tar.gz\n");
        verify::verify_archive(&archive, &sidecar).expect("first verification passes");

        // Swap the archive after the checksum gate accepted it.
        std::fs::write(&archive, b"attacker-replacement").unwrap();
        let err = verify_archive_before_extraction(&archive, &sidecar).unwrap_err();
        assert!(
            err.to_string()
                .contains("changed after checksum verification"),
            "post-verification replacement must fail closed: {err}"
        );
        assert_eq!(err.code(), eggress_cli::EXIT_RUNTIME_FAILURE);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `PATH` must never supply an integrity-critical tool: a planted
    /// `sha256sum` cannot change the digest and a planted `curl` is never
    /// selected.
    #[test]
    fn path_tools_are_not_trusted() {
        let root = std::env::temp_dir().join(format!("eggress-update-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // `PATH` is process-global; restore it even if an assertion panics.
        struct PathGuard(Option<std::ffi::OsString>);
        impl Drop for PathGuard {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(value) => std::env::set_var("PATH", value),
                    None => std::env::remove_var("PATH"),
                }
            }
        }

        write_script(
            &root.join("sha256sum"),
            "echo '0000000000000000000000000000000000000000000000000000000000000000  x'",
        );
        write_script(&root.join("curl"), "echo 'fake curl 0.0.0'");

        let guard = PathGuard(std::env::var_os("PATH"));
        std::env::set_var("PATH", &root);

        let sample = root.join("sample");
        std::fs::write(&sample, b"abc").unwrap();
        assert_eq!(
            verify::compute_sha256(&sample).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "a fake `sha256sum` on PATH must not influence verification"
        );
        if let Ok(resolved) = download::find_curl() {
            assert_ne!(
                resolved,
                root.join("curl"),
                "a PATH-only curl must never be selected"
            );
        }

        drop(guard);
        let _ = std::fs::remove_dir_all(&root);
    }
}
