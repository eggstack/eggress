//! Installation-path resolution for the self-update transaction.
//!
//! `eggress` and `pproxy` are one logical release unit: the Eggup transaction
//! (see `super::eggup`) never intentionally leaves one new and the other old
//! on a successful path, and a failure before commit leaves the previous
//! pair intact. Privilege is never escalated — unwritable destinations fail
//! with a remediation message before anything is downloaded.
//!
//! Archive extraction and pair replacement used to live here (`extract_archive`
//! shelling out to `tar`/PowerShell, `replace_pair` with bespoke backups).
//! Delivery M003 deleted that generic machinery in favor of Eggup's qualified
//! extraction and multi-artifact transaction; this module keeps only the
//! Eggress-specific path policy.

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
}
