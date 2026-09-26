//! Fresh-checkout canonical-byte regression.
//!
//! The fixture contract includes exact file hashes and LF-only golden
//! strings. A fixture-local `.gitattributes` policy pins LF text and
//! binary assets; this test proves that a *fresh* Git checkout produces
//! byte-identical canonical files under both `core.autocrlf=true` and
//! `core.autocrlf=false`, without touching global Git configuration or
//! the project repository's index.
//!
//! The whole scenario runs inside positively owned temporary
//! directories: the fixture package is copied to a disposable
//! repository, committed to its index there, and exported with
//! `checkout-index` into a separate empty directory.

use reader_workload::FIXTURE_REVISION;
use reader_workload::manifest::{
    MANIFEST_SOURCE, find_fixture_root, parse_manifest, validate_assets,
};
use std::path::{Path, PathBuf};

/// Owned temp directory removed on drop.
struct OwnedTemp(PathBuf);

impl OwnedTemp {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "reader-workload-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create owned temp dir");
        OwnedTemp(dir)
    }
}

impl Drop for OwnedTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn git(args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("cannot launch git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {:?} failed: {}{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Copy the fixture package into `repo`, index it with `autocrlf`, and
/// export a fresh checkout into `export`.
fn fresh_export(autocrlf: &str, repo: &Path, export: &Path) -> Result<(), String> {
    let source = find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root");
    copy_dir(&source, repo).map_err(|e| format!("copy: {e}"))?;

    git(&["init", "-q", &repo.display().to_string()])?;
    git(&[
        "-c",
        &format!("core.autocrlf={autocrlf}"),
        "-C",
        &repo.display().to_string(),
        "add",
        "-A",
    ])?;
    let mut prefix = export.display().to_string();
    if !prefix.ends_with('/') && !prefix.ends_with('\\') {
        prefix.push('/');
    }
    git(&[
        "-c",
        &format!("core.autocrlf={autocrlf}"),
        "-C",
        &repo.display().to_string(),
        "checkout-index",
        "--all",
        &format!("--prefix={prefix}"),
    ])?;
    Ok(())
}

#[test]
fn fresh_checkout_preserves_canonical_bytes_under_both_autocrlf_settings() {
    let working = find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root");

    for autocrlf in ["true", "false"] {
        let repo = OwnedTemp::new("fresh-checkout-repo");
        let export = OwnedTemp::new("fresh-checkout-export");
        fresh_export(autocrlf, &repo.0, &export.0)
            .unwrap_or_else(|e| panic!("fresh export (autocrlf={autocrlf}) failed: {e}"));

        // The exported manifest parses and pins the compiled revision.
        let exported_manifest_bytes =
            std::fs::read(export.0.join("manifest.txt")).expect("exported manifest readable");
        let exported_manifest =
            parse_manifest(&String::from_utf8(exported_manifest_bytes).expect("manifest is UTF-8"))
                .expect("exported manifest parses");
        assert_eq!(
            exported_manifest.fixture_revision, FIXTURE_REVISION,
            "exported manifest revision (autocrlf={autocrlf})"
        );

        // Exported files exist, hash correctly, and pass format checks:
        // any line-ending normalization would break the checksums.
        let power_shell_sha256 = |path: &Path| -> Result<String, String> {
            let output = std::process::Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg("(Get-FileHash -LiteralPath $env:RW_ASSET -Algorithm SHA256).Hash.ToLowerInvariant()")
                .env("RW_ASSET", path)
                .output()
                .map_err(|e| format!("cannot launch PowerShell: {e}"))?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).to_string());
            }
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        };
        validate_assets(&export.0, &exported_manifest, &power_shell_sha256).unwrap_or_else(|e| {
            panic!("exported asset validation (autocrlf={autocrlf}) failed: {e}")
        });

        // Golden files stay LF-only and byte-identical to the canonical
        // working-tree copies.
        for entry in std::fs::read_dir(golden_dir_from(&working)).expect("golden dir") {
            let name = entry.expect("entry").file_name();
            let exported = std::fs::read(golden_dir_from(&export.0).join(&name))
                .expect("exported golden readable");
            let canonical = std::fs::read(golden_dir_from(&working).join(&name))
                .expect("canonical golden readable");
            assert!(
                !exported.windows(2).any(|w| w == b"\r\n"),
                "exported golden {} has CRLF (autocrlf={autocrlf})",
                name.to_string_lossy()
            );
            assert_eq!(
                exported,
                canonical,
                "exported golden {} differs from canonical bytes (autocrlf={autocrlf})",
                name.to_string_lossy()
            );
        }

        // The exported manifest text itself is canonical LF.
        assert!(
            !String::from_utf8(std::fs::read(export.0.join("manifest.txt")).expect("read"))
                .expect("utf8")
                .contains('\r'),
            "exported manifest has CR bytes (autocrlf={autocrlf})"
        );
    }
    // The checked-in manifest is unaffected by the diagnostics.
    assert!(parse_manifest(MANIFEST_SOURCE).is_ok());
}

fn golden_dir_from(root: &Path) -> PathBuf {
    root.join("references").join("expected-copy")
}
