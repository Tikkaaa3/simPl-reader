//! Manifest parsing and asset-integrity behavior, exercised through the
//! library's public interface. Hashes are computed with Windows
//! PowerShell's `Get-FileHash` facility; missing/modified-asset failure
//! cases run against an isolated owned copy, never the checked-in files.

use reader_workload::FIXTURE_REVISION;
use reader_workload::manifest::{
    AssetValidationError, FixtureRootError, MANIFEST_SOURCE, ManifestError, find_fixture_root,
    manifest, parse_manifest, validate_assets,
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

/// SHA-256 via Windows PowerShell (lowercase hex), as allowed by the
/// task contract. The path travels through an environment variable so
/// bracket characters in filenames stay literal.
fn powershell_sha256(path: &Path) -> Result<String, String> {
    let output = std::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("(Get-FileHash -LiteralPath $env:RW_ASSET -Algorithm SHA256).Hash.ToLowerInvariant()")
        .env("RW_ASSET", path)
        .output()
        .map_err(|e| format!("cannot launch PowerShell: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Get-FileHash failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("unexpected hash output {hash:?}"));
    }
    Ok(hash)
}

#[test]
fn manifest_parses_and_matches_the_compiled_revision() {
    let parsed = manifest().expect("checked-in manifest parses");
    assert_eq!(parsed.fixture_revision, FIXTURE_REVISION);
    // 6 fonts + 1 image + 4 OFL texts + 1 Unicode license + 1 BiDi excerpt
    // + 2 tools + 3 docs + 1 authorship notice = 19 records.
    assert!(parsed.records.len() >= 15);
    for record in &parsed.records {
        assert!(
            record.path.starts_with("assets/")
                || record.path.starts_with("licenses/")
                || record.path.starts_with("references/")
                || record.path.starts_with("tools/")
                || record.path == "README.md"
        );
        assert!(
            !Path::new(&record.path).is_absolute(),
            "manifest paths must be portable relative paths"
        );
        assert_eq!(record.sha256.len(), 64);
        assert!(
            record
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert!(!record.version.is_empty());
        assert!(!record.provenance.is_empty());
        assert!(!record.license.is_empty());
        assert!(!record.role.is_empty());
    }
    let image = parsed
        .records
        .iter()
        .find(|r| r.path.ends_with("reader-sample.png"))
        .expect("image record present");
    assert_eq!(image.dimensions, Some((480, 320)));
    assert_eq!(parse_manifest(MANIFEST_SOURCE).unwrap(), parsed);
}

#[test]
fn font_roles_and_licenses_are_recorded() {
    let parsed = manifest().expect("checked-in manifest parses");
    for role in [
        "latin-regular",
        "latin-bold",
        "latin-italic",
        "arabic",
        "hebrew",
        "japanese",
    ] {
        assert!(
            parsed
                .records
                .iter()
                .any(|r| r.role.starts_with(role) && r.path.starts_with("assets/fonts/")),
            "font role {role} must be recorded"
        );
    }
    for record in parsed
        .records
        .iter()
        .filter(|r| r.path.starts_with("assets/fonts/"))
    {
        assert!(
            parsed.records.iter().any(|r| r.path == record.license_path),
            "license file {:?} of {:?} must itself be recorded",
            record.license_path,
            record.path
        );
    }
}

#[test]
fn all_manifest_files_exist_and_match_their_checksums() {
    let parsed = manifest().expect("checked-in manifest parses");
    let root = find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root");
    validate_assets(&root, &parsed, &powershell_sha256).expect("checked-in assets validate");
}

#[test]
fn image_is_a_real_png_with_manifest_dimensions() {
    let parsed = manifest().expect("checked-in manifest parses");
    let root = find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root");
    let record = parsed
        .records
        .iter()
        .find(|r| r.path.ends_with("reader-sample.png"))
        .expect("image record");
    let bytes = std::fs::read(root.join(&record.path)).expect("image readable");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(record.dimensions, Some((480, 320)));
    // IHDR width/height at fixed offsets.
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    assert_eq!((width, height), (480, 320));
}

#[test]
fn fonts_are_real_ttf_binaries() {
    let parsed = manifest().expect("checked-in manifest parses");
    let root = find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root");
    for record in parsed.records.iter().filter(|r| r.path.ends_with(".ttf")) {
        let bytes = std::fs::read(root.join(&record.path)).expect("font readable");
        assert!(
            bytes.len() > 1000,
            "font {} suspiciously small; placeholder files are rejected",
            record.path
        );
        assert_eq!(
            &bytes[..4],
            &[0x00, 0x01, 0x00, 0x00],
            "TTF magic expected in {}",
            record.path
        );
    }
}

#[test]
fn missing_asset_fails_with_a_path_specific_error() {
    let parsed = manifest().expect("checked-in manifest parses");
    let temp = OwnedTemp::new("missing-asset");
    copy_dir(
        &find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root"),
        &temp.0,
    )
    .expect("isolated copy");
    let victim = temp
        .0
        .join("assets")
        .join("fonts")
        .join("NotoSansHebrew-Regular.ttf");
    std::fs::remove_file(&victim).expect("remove one file in the owned copy");
    let err = validate_assets(&temp.0, &parsed, &powershell_sha256)
        .expect_err("missing asset must fail validation");
    match err {
        AssetValidationError::Missing { path } => {
            assert!(path.ends_with("NotoSansHebrew-Regular.ttf"));
            assert!(path.starts_with(&temp.0));
        }
        other => panic!("expected Missing, got {other:?}"),
    }
}

#[test]
fn modified_asset_fails_the_checksum_check() {
    let parsed = manifest().expect("checked-in manifest parses");
    let temp = OwnedTemp::new("modified-asset");
    copy_dir(
        &find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("fixture root"),
        &temp.0,
    )
    .expect("isolated copy");
    // Corrupt a small asset in place: the placeholder rejection.
    let victim = temp.0.join("licenses").join("Unicode-License.txt");
    let mut bytes = std::fs::read(&victim).expect("read owned copy");
    bytes[10] = bytes[10].wrapping_add(1);
    std::fs::write(&victim, &bytes).expect("write modified copy");
    let err = validate_assets(&temp.0, &parsed, &powershell_sha256)
        .expect_err("modified asset must fail validation");
    match err {
        AssetValidationError::ChecksumMismatch { path, .. } => {
            assert!(path.ends_with("Unicode-License.txt"));
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}

#[test]
fn parse_manifest_rejects_malformed_input() {
    // Bad sha256.
    let bad_sha = MANIFEST_SOURCE.replace(
        "sha256: 478c558ea716033cd60c03438f628dfa75694dcf6b5f6d505a2f05fd2b4f3823",
        "sha256: not-a-hash",
    );
    assert!(matches!(
        parse_manifest(&bad_sha),
        Err(ManifestError::InvalidSha256 { .. })
    ));
    // Unknown key.
    let unknown_key =
        format!("# c\nfixture-revision: {FIXTURE_REVISION}\npath: x.txt\nbogus-key: 1\n");
    assert!(matches!(
        parse_manifest(&unknown_key),
        Err(ManifestError::Syntax { .. })
    ));
    // Missing fixture revision.
    assert!(matches!(
        parse_manifest(
            "path: x.txt\nsha256: 0000000000000000000000000000000000000000000000000000000000000000\n"
        ),
        Err(ManifestError::MissingFixtureRevision)
    ));
    // Revision mismatch.
    let wrong_revision = MANIFEST_SOURCE.replacen(
        &format!("fixture-revision: {FIXTURE_REVISION}"),
        "fixture-revision: reader-workload-fx-0",
        1,
    );
    assert!(matches!(
        parse_manifest(&wrong_revision),
        Err(ManifestError::RevisionMismatch { .. })
    ));
    // Duplicate path: append the first record again.
    let lines: Vec<&str> = MANIFEST_SOURCE.lines().collect();
    let record_start = lines
        .iter()
        .position(|line| line.starts_with("path: "))
        .expect("a record exists");
    let mut record_end = record_start;
    while record_end + 1 < lines.len() && !lines[record_end + 1].trim().is_empty() {
        record_end += 1;
    }
    let record = lines[record_start..=record_end].join("\n");
    let duplicated = format!("{}\n{}\n", MANIFEST_SOURCE.trim_end(), record);
    assert!(matches!(
        parse_manifest(&duplicated),
        Err(ManifestError::DuplicatePath { .. })
    ));
    // Bad dimensions.
    let bad_dims = MANIFEST_SOURCE.replace("dimensions: 480x320", "dimensions: 480-320");
    assert!(matches!(
        parse_manifest(&bad_dims),
        Err(ManifestError::InvalidDimensions { .. })
    ));
}

#[test]
fn fixture_root_walks_up_and_reports_not_found() {
    // Inside the repository: walking up from the crate directory resolves.
    let root = find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("resolves");
    assert!(root.join("manifest.txt").is_file());
    assert!(
        root.ends_with("fixtures/reader-workload") || root.ends_with("fixtures\\reader-workload")
    );
    // A nonexistent anchor chain fails with a useful error.
    assert!(matches!(
        find_fixture_root(Path::new(
            "C:\\definitely\\not\\a\\real\\path\\for\\this\\test"
        )),
        Err(FixtureRootError::NotFound { .. })
    ));
}
