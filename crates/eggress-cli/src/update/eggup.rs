//! Self-update through Eggup's qualified archive extraction and transaction.
//!
//! After the whole-archive SHA-256 gate passes, this module replaces shell
//! extraction (`tar`/PowerShell) and the bespoke pair backup/rollback with:
//!
//! ```text
//! verified archive
//!   -> bounded eggup-archive extraction
//!   -> PersistedExtraction::into_bound_sources
//!   -> BoundSources -> InstallPlan::prepare_with_bound_sources
//!   -> staged candidate version checks
//!   -> Eggup commit/rollback
//! ```
//!
//! Eggress-owned policy stays local: GitHub authority, target mapping,
//! checksum sidecars, exact staged-version execution, CLI exit mapping, and
//! the sibling-pair requirement. Integrity continuity is whole verified
//! archive -> bounded extraction from that exact file -> object-bound member
//! evidence (size/digest computed at extraction) -> staged objects. Member
//! size/digest expectations stay `None` because releases publish no member
//! manifest; synthesizing expectations from extracted paths is forbidden, so
//! the verified-archive gate plus extraction-computed evidence is the trust
//! chain.

use std::path::{Path, PathBuf};

use eggup_archive::DeferredCleanup;
use eggup_archive::{extract, ArchiveFormat, ArchiveLimits, ArchiveMember, ArchivePlan};
use eggup_core::{
    AbsentPolicy, AllValidators, ArtifactMember, ArtifactSet, BoundSources, CommitOwnership,
    InstallPlan, IntegrityRequirement, MemberId, Ownership, OwnershipVerifier, PermissionsIntent,
    ProductId, ReleaseId, TransactionDisposition,
};

use super::target;
use super::verify::verify_staged_pair;
use super::version::ReleaseVersion;

/// Updater archive budget.
///
/// Measured against the v1.0.10 release assets (largest archive
/// `eggress-x86_64-pc-windows-msvc.zip` at 9,388,730 bytes): every ceiling
/// is finite, comfortably above current releases, and small enough that an
/// unexpected asset fails closed with a clear budget error instead of
/// consuming unbounded disk.
fn updater_archive_limits() -> Result<ArchiveLimits, String> {
    ArchiveLimits::new(
        64 * 1024 * 1024,  // max archive file: ~7x the largest current asset
        8,                 // max entries: exactly 2 declared; small extras bound
        256,               // max path bytes per entry
        64 * 1024 * 1024,  // max single member: far above either binary
        128 * 1024 * 1024, // max total uncompressed members
    )
    .map_err(|e| format!("invalid updater archive budget: {e}"))
}

/// Archive format for a release target: zip on Windows, tar.gz elsewhere.
pub fn archive_format_for_target(target: &str) -> ArchiveFormat {
    if target::is_windows_target(target) {
        ArchiveFormat::Zip
    } else {
        ArchiveFormat::TarGz
    }
}

/// `(eggress, pproxy)` member file names for a release target.
pub fn member_names_for_target(target: &str) -> (&'static str, &'static str) {
    if target::is_windows_target(target) {
        ("eggress.exe", "pproxy.exe")
    } else {
        ("eggress", "pproxy")
    }
}

/// Ownership proof for the exact installed sibling pair.
///
/// Only the two paths derived from the canonical installation root can ever
/// classify as [`Ownership::Owned`]: an existing regular file at either
/// path (the sibling identity pre-check already proved the pair belongs to
/// this version-aligned installation). A missing pair path is
/// [`Ownership::Absent`]; anything that is not exactly one of the two paths
/// is [`Ownership::Foreign`] whether or not it exists, so no creation policy
/// can ever authorize it. Ambiguous I/O is [`Ownership::Unknown`] — never
/// `Owned`.
#[derive(Debug, Clone)]
pub struct SiblingPairVerifier {
    eggress: PathBuf,
    pproxy: PathBuf,
}

impl SiblingPairVerifier {
    /// Bind the verifier to the exact installed pair. Both paths must be the
    /// canonical destinations the transaction will mutate.
    pub fn new(eggress: PathBuf, pproxy: PathBuf) -> Self {
        Self { eggress, pproxy }
    }
}

impl OwnershipVerifier for SiblingPairVerifier {
    fn verify(&self, _member: &MemberId, destination: &Path) -> Ownership {
        if destination != self.eggress.as_path() && destination != self.pproxy.as_path() {
            return Ownership::Foreign;
        }
        match std::fs::symlink_metadata(destination) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ownership::Absent,
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ownership::Owned,
            Ok(_) => Ownership::Foreign,
            Err(_) => Ownership::Unknown,
        }
    }
}

/// Map an Eggup commit receipt onto the updater outcome contract.
///
/// `Committed` is success. `RolledBack` distinguishes whether anything was
/// mutated (`rollback_performed`): pre-mutation failures leave the install
/// untouched, post-mutation failures restore the previous pair.
/// `RecoveryRequired` is a hard failure that preserves the retained evidence
/// path for the operator. Pure over its inputs so every branch is unit
/// tested; the production path derives the inputs from the live receipt.
pub fn map_commit_outcome(
    disposition: TransactionDisposition,
    rollback_performed: bool,
    failure_detail: Option<&str>,
    recovery_path: Option<&Path>,
) -> Result<String, String> {
    let detail = failure_detail.unwrap_or("unknown update failure");
    match disposition {
        TransactionDisposition::Committed => Ok(String::new()),
        TransactionDisposition::RolledBack if !rollback_performed => Err(format!(
            "{detail}; leaving the current installation untouched"
        )),
        TransactionDisposition::RolledBack => {
            Err(format!("{detail}; the previous installation was restored"))
        }
        TransactionDisposition::RecoveryRequired => {
            let evidence = recovery_path.map(|p| p.display().to_string());
            Err(format!(
                "{detail}; recovery required: retained evidence at {}; reinstall with the bootstrap installer to repair the pair (see docs/INSTALLATION.md)",
                evidence.as_deref().unwrap_or("an unknown location"),
            ))
        }
    }
}

/// Finish extraction-root cleanup after bound handles are consumed.
///
/// Eggup's handle-authorized cleanup empties the owned tree but always
/// reports the now-empty directory as residue (no portable object-bound
/// root unlink exists). Remove that empty residue directory here:
/// `remove_dir` only removes an actually-empty real directory, so a path
/// that was replaced, refilled, or left non-empty fails closed into the
/// warning below instead of deleting anything unexpected. Warn only when
/// real residue remains; the residue lives under our temp staging area.
fn finish_extraction_cleanup(cleanup: DeferredCleanup) {
    let root = cleanup.root().to_path_buf();
    let _ = cleanup.cleanup();
    match std::fs::remove_dir(&root) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            eprintln!(
                "warning: extraction residue remains at {}: {e}",
                root.display()
            );
        }
    }
}

/// A privately staged pair that has not performed live mutation.
struct StagedPair {
    prepared: eggup_core::PreparedTransaction,
    verifier: SiblingPairVerifier,
    eggress_id: MemberId,
    pproxy_id: MemberId,
}

/// Extract a verified archive and stage the pair through object-bound
/// handles. Returns the staged transaction plus the ownership proof the
/// commit step must use. Extraction residue is cleaned before returning;
/// only the Eggup-owned private stage remains.
fn stage_pair(
    archive_path: &Path,
    target: &str,
    expected: ReleaseVersion,
    install_dir: &Path,
    extraction_parent: &Path,
) -> Result<StagedPair, String> {
    let (eggress_name, pproxy_name) = member_names_for_target(target);
    let format = archive_format_for_target(target);
    let plan = ArchivePlan::new(
        archive_path,
        format,
        vec![
            ArchiveMember::new(eggress_name, eggress_name, None, None)
                .map_err(|e| format!("invalid updater archive declaration: {e}"))?,
            ArchiveMember::new(pproxy_name, pproxy_name, None, None)
                .map_err(|e| format!("invalid updater archive declaration: {e}"))?,
        ],
        updater_archive_limits()?,
    )
    .map_err(|e| {
        format!(
            "invalid updater archive declaration: {e}; leaving the current installation untouched"
        )
    })?;

    // Canonicalize once so the verifier's exact paths textually match the
    // destinations Eggup resolves under the installation root (e.g. `/tmp`
    // versus `/private/tmp` on macOS must not read as foreign).
    let canonical_root = std::fs::canonicalize(install_dir).map_err(|e| {
        format!(
            "cannot resolve installation directory {}: {e}; leaving the current installation untouched",
            install_dir.display()
        )
    })?;
    let installed_eggress = canonical_root.join(eggress_name);
    let installed_pproxy = canonical_root.join(pproxy_name);
    let verifier = SiblingPairVerifier::new(installed_eggress.clone(), installed_pproxy.clone());

    let extracted = extract(&plan, extraction_parent).map_err(|e| {
        format!(
            "failed to extract release archive: {e}; leaving the current installation untouched"
        )
    })?;
    let bound = extracted.persist().into_bound_sources().map_err(|e| {
        format!("failed to bind extracted members: {e}; leaving the current installation untouched")
    })?;
    let (members, cleanup) = bound.into_members();
    if members.len() != 2
        || members
            .iter()
            .any(|m| m.output_name() != eggress_name && m.output_name() != pproxy_name)
    {
        finish_extraction_cleanup(cleanup);
        return Err(
            "extracted members do not match the declared pair; leaving the current installation untouched"
                .to_string(),
        );
    }

    let mut sources = BoundSources::new();
    let mut staged = Vec::with_capacity(2);
    for member in members {
        let name = member.output_name().to_string();
        let digest = member.sha256();
        let advisory = member.advisory_path().to_path_buf();
        let id = MemberId::new(name.clone()).map_err(|e| format!("invalid updater member: {e}"))?;
        sources.insert(id.clone(), member.into_open_object());
        staged.push(
            ArtifactMember::new(id, advisory, name)
                .map_err(|e| {
                    format!(
                        "invalid updater member: {e}; leaving the current installation untouched"
                    )
                })?
                .with_permissions(PermissionsIntent::Executable)
                .with_integrity(IntegrityRequirement::Sha256(digest)),
        );
    }
    let set = ArtifactSet::new(staged).map_err(|e| format!("invalid updater member set: {e}"))?;
    let install = InstallPlan::new(
        ProductId::new("eggress").map_err(|e| format!("invalid updater product identity: {e}"))?,
        ReleaseId::new(expected.to_string())
            .map_err(|e| format!("invalid updater release identity: {e}"))?,
        canonical_root,
        set,
    )
    .map_err(|e| {
        format!("invalid updater install plan: {e}; leaving the current installation untouched")
    })?;

    let prepared = match install.prepare_with_bound_sources(sources) {
        Ok(prepared) => prepared,
        Err(e) => {
            finish_extraction_cleanup(cleanup);
            return Err(format!("{e}; leaving the current installation untouched"));
        }
    };
    // Staged bytes are private copies now; the extraction residue can go
    // before candidate checks run.
    finish_extraction_cleanup(cleanup);

    let eggress_id =
        MemberId::new(eggress_name).map_err(|e| format!("invalid updater member: {e}"))?;
    let pproxy_id =
        MemberId::new(pproxy_name).map_err(|e| format!("invalid updater member: {e}"))?;
    Ok(StagedPair {
        prepared,
        verifier,
        eggress_id,
        pproxy_id,
    })
}

/// Verify staged candidates and commit a staged pair.
///
/// Staged files are normalized executable (preserving the installed-mode
/// contract) before the exact version checks run; integrity and candidate
/// validation precede the commit, and absent destinations fail closed (the
/// sibling-pair requirement).
fn verify_candidates_and_commit(
    staged: StagedPair,
    expected: ReleaseVersion,
) -> Result<String, String> {
    let StagedPair {
        prepared,
        verifier,
        eggress_id,
        pproxy_id,
    } = staged;
    let staged_eggress = prepared
        .staged_path(&eggress_id)
        .map_err(|e| format!("{e}; leaving the current installation untouched"))?;
    let staged_pproxy = prepared
        .staged_path(&pproxy_id)
        .map_err(|e| format!("{e}; leaving the current installation untouched"))?;
    // Preserve the installed-mode contract: release archives carry executable
    // bits and the previous flow chmodded staged files before installing, so
    // normalize the owner-private staged copies to 0755 on Unix before the
    // candidates execute. Commit preserves staged modes.
    normalize_staged_executable(&staged_eggress)?;
    normalize_staged_executable(&staged_pproxy)?;

    verify_staged_pair(&staged_eggress, &staged_pproxy, expected)?;

    let receipt = prepared
        .verify_integrity()
        .and_then(|verified| verified.validate(&AllValidators::new()))
        .map_err(|e| format!("{e}; leaving the current installation untouched"))?;
    let receipt = receipt
        .commit(CommitOwnership::new(&verifier, AbsentPolicy::DenyCreate))
        .map_err(|e| format!("{e}; leaving the current installation untouched"))?;
    map_commit_outcome(
        receipt.disposition(),
        receipt.rollback_performed(),
        receipt.failure().map(|report| report.detail()),
        receipt.recovery_path(),
    )
}

/// Extract a verified release archive and install the pair through Eggup.
///
/// `install_dir` is the directory holding the running pair; `extraction_parent`
/// is a scratch directory under which Eggup owns its private extraction root
/// (it must differ from `install_dir`). Staged candidates are executed for
/// exact version agreement before any live mutation; absent destinations fail
/// closed (the sibling-pair requirement). Returns the empty success suffix on
/// success so the caller keeps its existing outcome message.
pub fn install_pair_via_eggup(
    archive_path: &Path,
    target: &str,
    expected: ReleaseVersion,
    install_dir: &Path,
    extraction_parent: &Path,
) -> Result<String, String> {
    let staged = stage_pair(
        archive_path,
        target,
        expected,
        install_dir,
        extraction_parent,
    )?;
    verify_candidates_and_commit(staged, expected)
}

/// Make a staged file executable on Unix (release archives already carry the
/// bit; fixture-built files may not). Mirrors the previous staged-chmod step
/// so installed binaries keep their executable contract.
fn normalize_staged_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| {
                format!(
                    "cannot stat staged file {}: {e}; leaving the current installation untouched",
                    path.display()
                )
            })?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(|e| {
            format!(
                "cannot make {} executable: {e}; leaving the current installation untouched",
                path.display()
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggup_archive::ExtractionErrorKind;
    use eggup_core::MutationLock;
    use std::io::Write as _;

    const UNIX_TARGET: &str = "x86_64-unknown-linux-gnu";
    const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

    fn unique_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "eggress-eggup-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn build_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::default());
            let mut tar = tar::Builder::new(gz);
            for (name, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_entry_type(tar::EntryType::Regular);
                tar.append_data(&mut header, name, *data).unwrap();
            }
            tar.into_inner().unwrap().finish().unwrap();
        }
        bytes
    }

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, data) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        bytes
    }

    #[cfg(unix)]
    fn write_script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn write_archive(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn maps_member_names_and_format_for_all_targets() {
        for target in target::SUPPORTED_TARGETS {
            let (eggress, pproxy) = member_names_for_target(target);
            let format = archive_format_for_target(target);
            if target::is_windows_target(target) {
                assert_eq!((eggress, pproxy), ("eggress.exe", "pproxy.exe"));
                assert!(matches!(format, ArchiveFormat::Zip));
            } else {
                assert_eq!((eggress, pproxy), ("eggress", "pproxy"));
                assert!(matches!(format, ArchiveFormat::TarGz));
            }
        }
    }

    #[test]
    fn archive_budget_is_finite() {
        // The policy itself must construct; enforcement is proven by the
        // oversize/entry-count extractions below.
        updater_archive_limits().unwrap();
    }

    #[test]
    fn oversize_archive_fails_closed_before_reading_content() {
        // A sparse file reaches the budget without allocating it: extraction
        // must refuse on size before touching bytes.
        let root = unique_root("oversize");
        let sparse = root.join("big.tar.gz");
        let file = std::fs::File::create(&sparse).unwrap();
        file.set_len(64 * 1024 * 1024 + 1).unwrap();
        drop(file);
        let plan = ArchivePlan::new(
            &sparse,
            ArchiveFormat::TarGz,
            vec![
                ArchiveMember::new("eggress", "eggress", None, None).unwrap(),
                ArchiveMember::new("pproxy", "pproxy", None, None).unwrap(),
            ],
            updater_archive_limits().unwrap(),
        )
        .unwrap();
        let err = extract(&plan, &root).unwrap_err();
        assert_eq!(err.kind(), ExtractionErrorKind::ArchiveTooLarge);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn too_many_entries_fail_closed() {
        let root = unique_root("entries");
        let names: Vec<String> = (0..9).map(|i| format!("extra-{i}")).collect();
        let borrowed: Vec<(&str, &[u8])> = names
            .iter()
            .map(|n| (n.as_str(), b"x".as_slice()))
            .collect();
        let archive = write_archive(&root, "crowded.tar.gz", &build_tar_gz(&borrowed));
        let plan = ArchivePlan::new(
            &archive,
            ArchiveFormat::TarGz,
            vec![
                ArchiveMember::new("eggress", "eggress", None, None).unwrap(),
                ArchiveMember::new("pproxy", "pproxy", None, None).unwrap(),
            ],
            updater_archive_limits().unwrap(),
        )
        .unwrap();
        let err = extract(&plan, &root).unwrap_err();
        assert_eq!(err.kind(), ExtractionErrorKind::TooManyEntries);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn verifier_classifies_only_the_exact_pair_as_owned() {
        let root = unique_root("ownership");
        let eggress = root.join("eggress");
        let pproxy = root.join("pproxy");
        std::fs::write(&eggress, b"e").unwrap();
        std::fs::write(&pproxy, b"p").unwrap();
        let verifier = SiblingPairVerifier::new(eggress.clone(), pproxy.clone());
        let id = MemberId::new("eggress").unwrap();
        assert_eq!(verifier.verify(&id, &eggress), Ownership::Owned);
        assert_eq!(verifier.verify(&id, &pproxy), Ownership::Owned);

        // Anything that is not exactly one of the two paths is foreign,
        // even when it exists — and a path that is not ours can never
        // classify as absent either, so no creation policy can ever
        // authorize it.
        let other = root.join("other");
        std::fs::write(&other, b"x").unwrap();
        assert_eq!(verifier.verify(&id, &other), Ownership::Foreign);
        let missing_other = root.join("missing-other");
        assert_eq!(verifier.verify(&id, &missing_other), Ownership::Foreign);

        // A missing sibling at an exact pair path is absent (commit denies
        // creation); a directory at a member path is foreign.
        let missing_pair = root.join("pair-missing");
        let missing_verifier = SiblingPairVerifier::new(missing_pair.clone(), pproxy.clone());
        assert_eq!(
            missing_verifier.verify(&id, &missing_pair),
            Ownership::Absent
        );
        let dir_member = root.join("dir-member");
        std::fs::create_dir_all(&dir_member).unwrap();
        let dir_verifier = SiblingPairVerifier::new(dir_member.clone(), pproxy.clone());
        assert_eq!(dir_verifier.verify(&id, &dir_member), Ownership::Foreign);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn commit_outcome_mapping_covers_every_disposition() {
        // Success carries the empty suffix so the caller keeps its message.
        assert_eq!(
            map_commit_outcome(TransactionDisposition::Committed, false, None, None).unwrap(),
            ""
        );
        // Pre-mutation failure: nothing was touched.
        let err = map_commit_outcome(
            TransactionDisposition::RolledBack,
            false,
            Some("ownership failed"),
            None,
        )
        .unwrap_err();
        assert!(
            err.contains("leaving the current installation untouched"),
            "{err}"
        );
        assert!(err.contains("ownership failed"), "{err}");
        // Post-mutation failure: the previous pair was restored.
        let err = map_commit_outcome(
            TransactionDisposition::RolledBack,
            true,
            Some("commit failed"),
            None,
        )
        .unwrap_err();
        assert!(
            err.contains("the previous installation was restored"),
            "{err}"
        );
        // Unrecoverable state keeps the evidence path for the operator.
        let err = map_commit_outcome(
            TransactionDisposition::RecoveryRequired,
            true,
            Some("rollback failed"),
            Some(Path::new("/tmp/evidence")),
        )
        .unwrap_err();
        assert!(err.contains("recovery required"), "{err}");
        assert!(err.contains("/tmp/evidence"), "{err}");
        assert!(err.contains("bootstrap installer"), "{err}");
    }

    #[test]
    fn extraction_cleanup_removes_the_private_root() {
        let root = unique_root("cleanup");
        let archive = write_archive(
            &root,
            "pair.tar.gz",
            &build_tar_gz(&[("eggress", b"e"), ("pproxy", b"p")]),
        );
        let plan = ArchivePlan::new(
            &archive,
            ArchiveFormat::TarGz,
            vec![
                ArchiveMember::new("eggress", "eggress", None, None).unwrap(),
                ArchiveMember::new("pproxy", "pproxy", None, None).unwrap(),
            ],
            updater_archive_limits().unwrap(),
        )
        .unwrap();
        let extracted = extract(&plan, &root).unwrap();
        let extraction_root = extracted.root().to_path_buf();
        assert!(extraction_root.is_dir());
        let bound = extracted.persist().into_bound_sources().unwrap();
        let (members, cleanup) = bound.into_members();
        assert_eq!(members.len(), 2);
        assert_eq!(cleanup.root(), extraction_root.as_path());
        // The helper removes the emptied residue directory itself; the
        // raw cleanup call always reports the residue by design.
        finish_extraction_cleanup(cleanup);
        assert!(!extraction_root.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn held_lock_blocks_a_second_committer() {
        // The same commit call the production path makes must fail closed
        // while another holder owns the installation-domain lock; nothing
        // is mutated and no candidate ever executes.
        let root = unique_root("contention");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(install_dir.join("eggress"), b"old-e").unwrap();
        std::fs::write(install_dir.join("pproxy"), b"old-p").unwrap();
        let archive = write_archive(
            &scratch,
            "pair.tar.gz",
            &build_tar_gz(&[("eggress", b"new-e"), ("pproxy", b"new-p")]),
        );

        let staged = stage_pair(
            &archive,
            UNIX_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap();
        let canonical = std::fs::canonicalize(&install_dir).unwrap();
        let _held = MutationLock::acquire(
            &canonical,
            &ProductId::new("eggress").unwrap(),
            &ReleaseId::new("contention-probe").unwrap(),
        )
        .unwrap();
        let receipt = staged
            .prepared
            .verify_integrity()
            .and_then(|verified| verified.validate(&AllValidators::new()))
            .unwrap()
            .commit(CommitOwnership::new(
                &staged.verifier,
                AbsentPolicy::DenyCreate,
            ));
        let err = receipt.unwrap_err();
        assert!(
            err.to_string().contains("already in progress"),
            "second committer must report lock contention: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("eggress")).unwrap(),
            "old-e"
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("pproxy")).unwrap(),
            "old-p"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn extra_members_are_ignored_with_bounds() {
        // Eggress policy: undeclared entries are drained within limits, not
        // treated as integrity failures — the whole-archive SHA-256 gate
        // already covers origin. Only the declared pair stages.
        let root = unique_root("extras");
        let archive = write_archive(
            &root,
            "pair.tar.gz",
            &build_tar_gz(&[("eggress", b"e"), ("pproxy", b"p"), ("notes.txt", b"extra")]),
        );
        let plan = ArchivePlan::new(
            &archive,
            ArchiveFormat::TarGz,
            vec![
                ArchiveMember::new("eggress", "eggress", None, None).unwrap(),
                ArchiveMember::new("pproxy", "pproxy", None, None).unwrap(),
            ],
            updater_archive_limits().unwrap(),
        )
        .unwrap();
        let extracted = extract(&plan, &root).unwrap();
        assert_eq!(extracted.members().len(), 2);
        let bound = extracted.persist().into_bound_sources().unwrap();
        let (members, cleanup) = bound.into_members();
        assert_eq!(members.len(), 2);
        finish_extraction_cleanup(cleanup);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_member_fails_before_mutation() {
        let root = unique_root("missing");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(install_dir.join("eggress"), b"old-e").unwrap();
        std::fs::write(install_dir.join("pproxy"), b"old-p").unwrap();
        let archive = write_archive(
            &scratch,
            "pair.tar.gz",
            &build_tar_gz(&[("eggress", b"new-e")]),
        );
        let err = install_pair_via_eggup(
            &archive,
            UNIX_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap_err();
        assert!(err.contains("failed to extract"), "{err}");
        assert!(err.contains("untouched"), "{err}");
        assert_eq!(
            std::fs::read_to_string(install_dir.join("eggress")).unwrap(),
            "old-e"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn unix_tar_gz_flow_installs_verified_pair() {
        let root = unique_root("flow-tar");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        write_script(&install_dir.join("eggress"), "echo 'eggress 1.0.0'");
        write_script(
            &install_dir.join("pproxy"),
            "echo 'eggress-pproxy-compat 1.0.0'",
        );
        let archive = write_archive(
            &scratch,
            "pair.tar.gz",
            &build_tar_gz(&[
                ("eggress", b"#!/bin/sh\necho 'eggress 9.9.9'\n"),
                ("pproxy", b"#!/bin/sh\necho 'eggress-pproxy-compat 9.9.9'\n"),
            ]),
        );

        let outcome = install_pair_via_eggup(
            &archive,
            UNIX_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap();
        assert!(outcome.is_empty());

        // Installed contents are the new candidates with executable modes.
        for name in ["eggress", "pproxy"] {
            let path = install_dir.join(name);
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("9.9.9"), "{name}: {content}");
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        // No transaction residue escapes into the install tree.
        for entry in [&install_dir, &root] {
            for name in std::fs::read_dir(entry)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            {
                assert!(
                    !name.starts_with(".eggup-") && !name.ends_with(".bak"),
                    "transaction residue left behind: {name}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn unix_zip_flow_installs_windows_named_pair() {
        let root = unique_root("flow-zip");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        write_script(&install_dir.join("eggress.exe"), "echo 'eggress 1.0.0'");
        write_script(
            &install_dir.join("pproxy.exe"),
            "echo 'eggress-pproxy-compat 1.0.0'",
        );
        let archive = write_archive(
            &scratch,
            "pair.zip",
            &build_zip(&[
                ("eggress.exe", b"#!/bin/sh\necho 'eggress 9.9.9'\n"),
                (
                    "pproxy.exe",
                    b"#!/bin/sh\necho 'eggress-pproxy-compat 9.9.9'\n",
                ),
            ]),
        );

        install_pair_via_eggup(
            &archive,
            WINDOWS_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap();

        for name in ["eggress.exe", "pproxy.exe"] {
            let content = std::fs::read_to_string(install_dir.join(name)).unwrap();
            assert!(content.contains("9.9.9"), "{name}: {content}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn wrong_candidate_version_leaves_install_untouched() {
        let root = unique_root("wrong-version");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        write_script(&install_dir.join("eggress"), "echo 'eggress 1.0.0'");
        write_script(
            &install_dir.join("pproxy"),
            "echo 'eggress-pproxy-compat 1.0.0'",
        );
        let archive = write_archive(
            &scratch,
            "pair.tar.gz",
            &build_tar_gz(&[
                ("eggress", b"#!/bin/sh\necho 'eggress 8.8.8'\n"),
                ("pproxy", b"#!/bin/sh\necho 'eggress-pproxy-compat 9.9.9'\n"),
            ]),
        );

        let err = install_pair_via_eggup(
            &archive,
            UNIX_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap_err();
        assert!(err.contains("untouched"), "{err}");
        let installed = std::fs::read_to_string(install_dir.join("eggress")).unwrap();
        assert!(installed.contains("1.0.0"), "{installed}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn foreign_destination_fails_before_mutation() {
        let root = unique_root("foreign");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        // A directory where the `eggress` destination must be is foreign:
        // ownership cannot be proven, so nothing may mutate.
        std::fs::create_dir_all(install_dir.join("eggress")).unwrap();
        write_script(
            &install_dir.join("pproxy"),
            "echo 'eggress-pproxy-compat 1.0.0'",
        );
        let archive = write_archive(
            &scratch,
            "pair.tar.gz",
            &build_tar_gz(&[
                ("eggress", b"#!/bin/sh\necho 'eggress 9.9.9'\n"),
                ("pproxy", b"#!/bin/sh\necho 'eggress-pproxy-compat 9.9.9'\n"),
            ]),
        );

        let err = install_pair_via_eggup(
            &archive,
            UNIX_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap_err();
        assert!(err.contains("untouched"), "{err}");
        assert!(install_dir.join("eggress").is_dir());
        let installed = std::fs::read_to_string(install_dir.join("pproxy")).unwrap();
        assert!(installed.contains("1.0.0"), "{installed}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn absent_destination_fails_closed_without_creating() {
        // The sibling-pair requirement denies creation: a missing member of
        // the installed pair fails instead of materializing a half pair.
        let root = unique_root("absent");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        write_script(&install_dir.join("eggress"), "echo 'eggress 1.0.0'");
        let archive = write_archive(
            &scratch,
            "pair.tar.gz",
            &build_tar_gz(&[
                ("eggress", b"#!/bin/sh\necho 'eggress 9.9.9'\n"),
                ("pproxy", b"#!/bin/sh\necho 'eggress-pproxy-compat 9.9.9'\n"),
            ]),
        );

        let err = install_pair_via_eggup(
            &archive,
            UNIX_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap_err();
        assert!(err.contains("untouched"), "{err}");
        assert!(!install_dir.join("pproxy").exists());
        let installed = std::fs::read_to_string(install_dir.join("eggress")).unwrap();
        assert!(installed.contains("1.0.0"), "{installed}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn zip_extract_stage_commit_without_candidate_execution() {
        // Windows-lane coverage: every updater/archive step except candidate
        // *execution* (fixture scripts are POSIX shell) runs through the
        // real Eggup path — extraction, bound staging, integrity,
        // validation, ownership, commit — and installs both members.
        let root = unique_root("win-flow");
        let install_dir = root.join("bin");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(install_dir.join("eggress.exe"), b"old-e").unwrap();
        std::fs::write(install_dir.join("pproxy.exe"), b"old-p").unwrap();
        let archive = write_archive(
            &scratch,
            "pair.zip",
            &build_zip(&[("eggress.exe", b"new-e"), ("pproxy.exe", b"new-p")]),
        );

        let staged = stage_pair(
            &archive,
            WINDOWS_TARGET,
            crate::update::version::parse_version("9.9.9").unwrap(),
            &install_dir,
            &scratch,
        )
        .unwrap();
        let receipt = staged
            .prepared
            .verify_integrity()
            .and_then(|verified| verified.validate(&AllValidators::new()))
            .unwrap()
            .commit(CommitOwnership::new(
                &staged.verifier,
                AbsentPolicy::DenyCreate,
            ))
            .unwrap();
        assert_eq!(receipt.disposition(), TransactionDisposition::Committed);
        assert_eq!(
            std::fs::read_to_string(install_dir.join("eggress.exe")).unwrap(),
            "new-e"
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("pproxy.exe")).unwrap(),
            "new-p"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
