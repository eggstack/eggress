//! Release/installer contract: the binary distribution surface must stay
//! aligned across the installer scripts, the release workflow, and the docs.
//!
//! These are static drift checks (no network, no matrix builds). Behavior is
//! covered by `packaging/tests/test-install.sh` (local file:// fixtures) and
//! the native smoke steps inside `.github/workflows/release-binaries.yml`.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve")
}

fn read_repo(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("must read {rel}: {e}"))
}

const UNIX_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

#[test]
fn installer_scripts_exist() {
    assert!(
        repo_root().join("packaging/install.sh").is_file(),
        "packaging/install.sh must exist"
    );
    assert!(
        repo_root().join("packaging/install.ps1").is_file(),
        "packaging/install.ps1 must exist"
    );
    assert!(
        repo_root().join("scripts/release-preflight.sh").is_file(),
        "scripts/release-preflight.sh must exist"
    );
}

#[test]
fn installer_covers_all_unix_targets() {
    let installer = read_repo("packaging/install.sh");
    for target in UNIX_TARGETS {
        assert!(
            installer.contains(target),
            "install.sh must map host to {target}"
        );
    }
    assert!(
        installer.contains("cargo install eggress-cli --locked"),
        "install.sh unsupported hosts must point to the Cargo alternative"
    );
}

#[test]
fn archive_naming_contract() {
    let installer = read_repo("packaging/install.sh");
    let workflow = read_repo(".github/workflows/release-binaries.yml");
    let ps1 = read_repo("packaging/install.ps1");
    for target in UNIX_TARGETS {
        let archive = format!("eggress-{target}.tar.gz");
        assert!(
            workflow.contains(&archive),
            "release workflow must produce {archive}"
        );
    }
    assert!(
        workflow.contains("eggress-x86_64-pc-windows-msvc.zip"),
        "release workflow must produce the Windows zip"
    );
    assert!(
        installer.contains("eggress-${TARGET}.tar.gz"),
        "install.sh must construct the archive name from the target"
    );
    assert!(
        ps1.contains("eggress-$Target.zip"),
        "install.ps1 must construct the Windows archive name from the target"
    );
    // The tag supplies the version namespace; archive names stay stable.
    for target in UNIX_TARGETS
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(WINDOWS_TARGET.to_string()))
    {
        assert!(
            !workflow.contains(&format!("eggress-{target}-v")),
            "archive names must not duplicate the version for {target}"
        );
    }
}

#[test]
fn installers_verify_both_binaries_and_checksums() {
    let installer = read_repo("packaging/install.sh");
    assert!(
        installer.contains("sha256sum") && installer.contains("shasum -a 256"),
        "install.sh must support both sha256sum and shasum"
    );
    assert!(
        installer.contains("\"${TMPDIR}/eggress\" version"),
        "install.sh must execute the staged eggress version"
    );
    assert!(
        installer.contains("\"${TMPDIR}/pproxy\" --version"),
        "install.sh must execute the staged pproxy version"
    );
    assert!(
        installer.contains("versions disagree"),
        "install.sh must refuse when the staged pair disagree"
    );

    let ps1 = read_repo("packaging/install.ps1");
    assert!(
        ps1.contains("Get-FileHash"),
        "install.ps1 must use Get-FileHash"
    );
    assert!(
        ps1.contains("eggress.exe") && ps1.contains("pproxy.exe"),
        "install.ps1 must handle both executables together"
    );
    assert!(
        ps1.contains("versions disagree"),
        "install.ps1 must refuse when the staged pair disagree"
    );
}

#[test]
fn installers_never_escalate_or_mutate_shell_state() {
    let installer = read_repo("packaging/install.sh");
    for line in installer.lines() {
        let code = line.split('#').next().unwrap_or("");
        assert!(
            !code.contains("sudo"),
            "install.sh must never invoke sudo: {line}"
        );
    }
    assert!(
        installer.contains("not in PATH"),
        "install.sh must advise about PATH instead of mutating shell files"
    );
    let ps1 = read_repo("packaging/install.ps1");
    assert!(
        ps1.contains("not in PATH") || ps1.contains("notcontains"),
        "install.ps1 must give PATH advice"
    );
    assert!(
        !ps1.to_lowercase().contains("set-executionpolicy"),
        "install.ps1 must not change the execution policy itself"
    );
}

#[test]
fn checksum_language_claims_no_signatures() {
    for rel in ["packaging/install.sh", "packaging/install.ps1"] {
        let content = read_repo(rel);
        assert!(
            content.contains("not an independent signature"),
            "{rel} must state SHA-256 is not a signature guarantee"
        );
        let lower = content.to_lowercase();
        assert!(
            !lower.contains("sigstore") && !lower.contains("sbom"),
            "{rel} must not imply signing/SBOM provenance"
        );
    }
}

#[test]
fn release_binaries_use_default_features_locked() {
    let workflow = read_repo(".github/workflows/release-binaries.yml");
    assert!(workflow.contains("cargo zigbuild --release --locked -p eggress-cli"));
    assert!(workflow.contains("cargo build --release --locked -p eggress-cli"));
    // Release binaries must not silently enable every optional feature.
    for line in workflow.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("cargo ") && trimmed.contains("-p eggress-cli") {
            assert!(
                !trimmed.contains("--features") && !trimmed.contains("--all-features"),
                "release builds must use default features (no --features flag): {line}"
            );
        }
        assert!(
            !trimmed.contains("cargo publish"),
            "binary workflow must never publish crates.io packages: {line}"
        );
    }
}

#[test]
fn release_workflow_does_not_duplicate_ordinary_ci() {
    let workflow = read_repo(".github/workflows/release-binaries.yml");
    assert!(
        !workflow.contains("cargo test --workspace"),
        "release workflow must not rerun the ordinary workspace suite"
    );
    assert!(
        !workflow.contains("cargo clippy"),
        "release workflow must not duplicate clippy"
    );
    let ci = read_repo(".github/workflows/ci.yml");
    assert!(
        !ci.contains("release-binaries") && !ci.contains("zigbuild"),
        "ordinary CI must not build the release matrix"
    );
}

#[test]
fn release_smoke_covers_both_binaries() {
    let workflow = read_repo(".github/workflows/release-binaries.yml");
    assert!(workflow.contains("eggress version"));
    assert!(workflow.contains("eggress --help"));
    assert!(workflow.contains("pproxy --version") || workflow.contains("pproxy.exe\" --version"));
    assert!(workflow.contains("pproxy --help") || workflow.contains("pproxy.exe\" --help"));
    assert!(
        workflow.contains("bind smoke"),
        "release jobs must include a lightweight bind smoke"
    );
}

#[test]
fn release_job_refuses_missing_artifacts_and_verifies_checksums() {
    let workflow = read_repo(".github/workflows/release-binaries.yml");
    assert!(
        workflow.contains("refuses incomplete artifact set")
            || workflow.contains("missing: {missing}"),
        "release job must refuse missing target artifacts"
    );
    assert!(
        workflow.contains("sha256sum -c"),
        "release job must verify checksums before publishing"
    );
    for target in UNIX_TARGETS
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(WINDOWS_TARGET.to_string()))
    {
        assert!(
            workflow.contains(&format!("eggress-{target}")),
            "assemble job must expect assets for {target}"
        );
    }
    assert!(workflow.contains("release-assets/install.sh"));
    assert!(workflow.contains("release-assets/install.ps1"));
}

#[test]
fn preflight_enforces_tag_and_lockstep_versions() {
    let preflight = read_repo("scripts/release-preflight.sh");
    assert!(preflight.contains("--tag"));
    assert!(preflight.contains("vMAJOR.MINOR.PATCH") || preflight.contains("v[0-9]"));
    assert!(preflight.contains("workspace"));
    assert!(preflight.contains("eggress-python/pyproject.toml"));
    assert!(preflight.contains("python-pproxy-compat/pyproject.toml"));
    assert!(preflight.contains("tagged commit"));
    assert!(preflight.contains("working tree"));
}

#[test]
fn update_targets_match_release_matrix() {
    // The updater's target list must stay identical to the workflow's
    // published matrix; otherwise `eggress update` would request assets
    // the release never builds.
    let target_rs = read_repo("crates/eggress-cli/src/update/target.rs");
    let workflow = read_repo(".github/workflows/release-binaries.yml");
    for target in UNIX_TARGETS
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(WINDOWS_TARGET.to_string()))
    {
        assert!(
            target_rs.contains(&format!("\"{target}\"")),
            "src/update/target.rs must list prebuilt target {target}"
        );
        assert!(
            workflow.contains(&format!("eggress-{target}")),
            "release workflow must publish assets for updater target {target}"
        );
    }
    let installer = read_repo("packaging/install.sh");
    for target in UNIX_TARGETS {
        assert!(
            installer.contains(target),
            "install.sh and the updater must cover the same Unix targets ({target})"
        );
    }
}

#[test]
fn update_documented_with_accurate_provenance() {
    let installation = read_repo("docs/INSTALLATION.md");
    assert!(
        installation.contains("eggress update"),
        "docs/INSTALLATION.md must document `eggress update`"
    );
    assert!(
        installation.contains("Does not touch a Python environment"),
        "update docs must state Python environments are untouched"
    );
    assert!(
        installation.contains("Never invokes `sudo`"),
        "update docs must state there is no privilege escalation"
    );
    assert!(
        installation.contains("no background update check")
            || installation.contains("No background update check")
            || installation.contains("There is no background update check"),
        "update docs must state there are no background update checks"
    );
    let operations = read_repo("docs/OPERATIONS.md");
    assert!(
        operations.contains("eggress update"),
        "docs/OPERATIONS.md must list `eggress update`"
    );
}

#[test]
fn updater_never_escalates_or_falls_back() {
    for rel in [
        "crates/eggress-cli/src/update/mod.rs",
        "crates/eggress-cli/src/update/install.rs",
        "crates/eggress-cli/src/update/download.rs",
    ] {
        let content = read_repo(rel);
        for line in content.lines() {
            let code = line.split("//").next().unwrap_or("");
            assert!(
                !code.contains("sudo"),
                "{rel} must never invoke sudo: {line}"
            );
        }
        assert!(
            !content.contains("cargo install eggress-cli --force")
                && !content.contains("cargo-install-fallback"),
            "{rel} must not fall back to Cargo/source builds"
        );
    }
}

#[test]
fn install_docs_reference_real_assets() {
    let installation = read_repo("docs/INSTALLATION.md");
    assert!(
        installation.contains("packaging/install.sh")
            || installation.contains("releases/latest/download/install.sh"),
        "docs/INSTALLATION.md must reference the real bootstrap URL"
    );
    assert!(
        installation.contains("eggress-x86_64-unknown-linux-gnu"),
        "docs/INSTALLATION.md must list the produced target matrix"
    );
    assert!(
        installation.contains("glibc") || installation.contains("2.17"),
        "docs/INSTALLATION.md must state the Linux compatibility floor"
    );
}

#[test]
fn readme_points_at_canonical_install_surface() {
    let readme = read_repo("README.md");
    assert!(
        readme.contains("releases/latest/download/install.sh"),
        "README.md must reference the real binary bootstrap URL"
    );
    assert!(
        readme.contains("docs/INSTALLATION.md"),
        "README.md must link to the canonical install guide"
    );
    let installer_pos = readme
        .find("releases/latest/download/install.sh")
        .expect("installer URL must exist");
    let cargo_pos = readme
        .find("cargo install eggress-cli")
        .expect("Cargo alternative must remain documented");
    assert!(
        installer_pos < cargo_pos,
        "README.md must present the binary installer before the Cargo alternative"
    );
}

#[test]
fn pproxy_migration_examples_stay_flat() {
    // The standalone `pproxy` binary stays flat; nested
    // `translate/check/run` tooling lives only under native `eggress`.
    // Catch docs regressions where a fenced example shows
    // `pproxy translate|check|run` as if the compat binary accepted it.
    for rel in [
        "README.md",
        "docs/OPERATIONS.md",
        "docs/PPROXY_MIGRATION.md",
        "docs/INSTALLATION.md",
        "crates/eggress-cli/README.md",
    ] {
        let content = read_repo(rel);
        let mut in_fence = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if !in_fence {
                continue;
            }
            let code = trimmed.strip_prefix('$').unwrap_or(trimmed).trim_start();
            for sub in ["pproxy translate", "pproxy check", "pproxy run"] {
                if code == sub
                    || code.starts_with(&format!("{sub} "))
                    || code.starts_with(&format!("{sub}\t"))
                {
                    panic!("{rel} shows flat-binary `{sub}` as a subcommand: {line}");
                }
            }
        }
    }
}
