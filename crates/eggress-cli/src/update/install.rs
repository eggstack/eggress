//! Installation-path resolution and transactional two-binary replacement.
//!
//! `eggress` and `pproxy` are one logical release unit: an update never
//! intentionally leaves one new and the other old on a successful path,
//! and a failure before commit leaves the previous pair intact. Privilege
//! is never escalated — unwritable destinations fail with a remediation
//! message before either binary is touched.

use std::path::{Path, PathBuf};

/// Derive the expected sibling `pproxy` path from the running `eggress`
/// executable. Never scans `PATH`: only the sibling installed next to this
/// executable belongs to this installation.
pub fn sibling_pproxy_path(current_exe: &Path) -> Result<PathBuf, String> {
    let dir = current_exe.parent().ok_or_else(|| {
        format!(
            "cannot resolve installation directory from {}",
            current_exe.display()
        )
    })?;
    let name = current_exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            format!(
                "cannot resolve executable name from {}",
                current_exe.display()
            )
        })?;
    let sibling_name = if name == "eggress.exe" {
        "pproxy.exe"
    } else if name == "eggress" {
        "pproxy"
    } else {
        return Err(format!(
            "unexpected executable name '{name}'; refusing to guess which pproxy belongs to this installation"
        ));
    };
    Ok(dir.join(sibling_name))
}

/// Fail before mutation unless `dir` is writable. Never invokes `sudo` or
/// credential prompts; the caller renders the remediation.
pub fn check_destination_writable(dir: &Path) -> Result<(), String> {
    let probe = dir.join(".eggress-update-write-test");
    match std::fs::write(&probe, b"writable") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(format!(
            "destination {} is not writable ({e}); rerun from a shell with write access or reinstall to a user-writable directory with --dir",
            dir.display()
        )),
    }
}

/// Make a staged file executable on Unix. Archives from the release pipeline
/// already carry the bit; fixture-built files may not.
pub fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| format!("cannot stat staged file {}: {e}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)
            .map_err(|e| format!("cannot make {} executable: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Extract a release archive (`.tar.gz` on Unix targets, `.zip` on Windows)
/// into `dest_dir`.
pub fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String> {
    let name = archive.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".zip") {
        // Prefer `tar` (present on modern Windows and all Unix runners);
        // fall back to PowerShell's Expand-Archive on Windows.
        if std::process::Command::new("tar")
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(dest_dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        #[cfg(windows)]
        {
            let status = std::process::Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    archive.display(),
                    dest_dir.display()
                ))
                .status()
                .map_err(|e| format!("failed to run Expand-Archive: {e}"))?;
            if status.success() {
                return Ok(());
            }
            return Err("failed to extract release archive".to_string());
        }
        #[cfg(not(windows))]
        return Err("failed to extract release archive (is `tar` installed?)".to_string());
    }
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(dest_dir)
        .status()
        .map_err(|e| format!("failed to run tar: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("failed to extract release archive".to_string())
    }
}

/// Replace the installed `eggress`/`pproxy` pair as one logical transaction.
///
/// Same-filesystem staging with backups: each destination is renamed aside
/// (`<name>.bak`), the staged candidate takes its place, and any failure
/// before commit restores the prior files. Renaming aside (rather than
/// overwriting in place) keeps this working for the currently running
/// executable on both Unix and Windows. Backups are removed after a
/// successful commit.
pub fn replace_pair(
    installed_eggress: &Path,
    installed_pproxy: &Path,
    staged_eggress: &Path,
    staged_pproxy: &Path,
) -> Result<(), String> {
    let backup = |installed: &Path| {
        let mut bak = installed.as_os_str().to_owned();
        bak.push(".bak");
        PathBuf::from(bak)
    };
    let eggress_bak = backup(installed_eggress);
    let pproxy_bak = backup(installed_pproxy);

    // Stage backups of both current binaries first; nothing is lost yet.
    std::fs::rename(installed_eggress, &eggress_bak).map_err(|e| {
        format!(
            "cannot back up {}: {e}; the current installation is untouched",
            installed_eggress.display()
        )
    })?;
    let restore_eggress = || {
        let _ = std::fs::rename(&eggress_bak, installed_eggress);
    };
    if let Err(e) = std::fs::rename(installed_pproxy, &pproxy_bak) {
        restore_eggress();
        return Err(format!(
            "cannot back up {}: {e}; the current installation is untouched",
            installed_pproxy.display()
        ));
    }
    let restore_both = || {
        let _ = std::fs::rename(&pproxy_bak, installed_pproxy);
        restore_eggress();
    };

    // Commit: sibling first, then the running executable. Either order is
    // safe here because both destinations are free names now.
    if let Err(e) = copy_or_rename(staged_pproxy, installed_pproxy) {
        restore_both();
        return Err(format!(
            "cannot install {}: {e}; the previous installation was restored",
            installed_pproxy.display()
        ));
    }
    if let Err(e) = copy_or_rename(staged_eggress, installed_eggress) {
        let _ = std::fs::remove_file(installed_pproxy);
        restore_both();
        return Err(format!(
            "cannot install {}: {e}; the previous installation was restored",
            installed_eggress.display()
        ));
    }

    let _ = std::fs::remove_file(&eggress_bak);
    let _ = std::fs::remove_file(&pproxy_bak);
    Ok(())
}

/// Place `staged` at `dest` (a free name after the backup rename). A rename
/// keeps the operation atomic on one filesystem; a copy fallback covers
/// staging areas mounted on a different filesystem.
fn copy_or_rename(staged: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::rename(staged, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(staged, dest).map(|_| ())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::metadata(staged)?.permissions();
                let mut dest_perms = std::fs::metadata(dest)?.permissions();
                dest_perms.set_mode(perms.mode());
                std::fs::set_permissions(dest, dest_perms)?;
            }
            std::fs::remove_file(staged)?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_sibling_paths_without_scanning_path() {
        #[cfg(windows)]
        {
            assert_eq!(
                sibling_pproxy_path(Path::new("C:\\bin\\eggress.exe")).unwrap(),
                Path::new("C:\\bin\\pproxy.exe")
            );
        }
        #[cfg(not(windows))]
        {
            assert_eq!(
                sibling_pproxy_path(Path::new("/usr/local/bin/eggress")).unwrap(),
                Path::new("/usr/local/bin/pproxy")
            );
            assert_eq!(
                sibling_pproxy_path(Path::new("/usr/local/bin/pproxy")).unwrap_err().to_string(),
                "unexpected executable name 'pproxy'; refusing to guess which pproxy belongs to this installation"
            );
        }
        assert!(sibling_pproxy_path(Path::new("/opt/bin/something-else")).is_err());
    }

    fn fixture_pair(dir: &Path, eggress_body: &str, pproxy_body: &str) -> (PathBuf, PathBuf) {
        let eggress = dir.join("eggress");
        let pproxy = dir.join("pproxy");
        std::fs::write(&eggress, eggress_body).unwrap();
        std::fs::write(&pproxy, pproxy_body).unwrap();
        (eggress, pproxy)
    }

    #[test]
    fn replace_pair_commits_both_and_cleans_backups() {
        let root =
            std::env::temp_dir().join(format!("eggress-install-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let installed = root.join("bin");
        let staged = root.join("stage");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        let (old_e, old_p) = fixture_pair(&installed, "old-eggress", "old-pproxy");
        let (new_e, new_p) = fixture_pair(&staged, "new-eggress", "new-pproxy");

        replace_pair(&old_e, &old_p, &new_e, &new_p).unwrap();

        assert_eq!(std::fs::read_to_string(&old_e).unwrap(), "new-eggress");
        assert_eq!(std::fs::read_to_string(&old_p).unwrap(), "new-pproxy");
        assert!(!installed.join("eggress.bak").exists());
        assert!(!installed.join("pproxy.bak").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn replace_pair_restores_old_pair_when_commit_fails() {
        let root =
            std::env::temp_dir().join(format!("eggress-rollback-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let installed = root.join("bin");
        let staged = root.join("stage");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        let (old_e, old_p) = fixture_pair(&installed, "old-eggress", "old-pproxy");
        let (_, new_p) = fixture_pair(&staged, "new-eggress", "new-pproxy");
        // A missing staged `eggress` forces the second commit to fail after
        // the sibling commit already landed.
        let missing_staged_eggress = staged.join("eggress-missing");

        let err = replace_pair(&old_e, &old_p, &missing_staged_eggress, &new_p).unwrap_err();
        assert!(err.contains("previous installation was restored"), "{err}");

        assert_eq!(std::fs::read_to_string(&old_e).unwrap(), "old-eggress");
        assert_eq!(std::fs::read_to_string(&old_p).unwrap(), "old-pproxy");
        let _ = std::fs::remove_dir_all(&root);
    }
}
