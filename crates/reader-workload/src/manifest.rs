//! Pinned asset manifest parsing and the asset-validation seam.
//!
//! The manifest text is compiled into the library from
//! `fixtures/reader-workload/manifest.txt` (portable relative paths
//! only). Filesystem access happens only when a consumer explicitly
//! resolves and validates assets; the library never reads asset bytes
//! while constructing workload data, and validation never repairs,
//! downloads, or accepts placeholder files.
//!
//! Hashing is supplied by the caller (a `Fn(&Path) -> Result<String,
//! String>` returning lowercase hex) so this crate stays dependency-free
//! and portable; tests use Windows PowerShell's `Get-FileHash` facility.

use std::path::{Path, PathBuf};

/// Fixture-root-relative location of the fixture package.
pub const FIXTURE_ROOT_RELATIVE: &str = "fixtures/reader-workload";

/// Manifest text compiled into the library.
pub const MANIFEST_SOURCE: &str = include_str!("../../../fixtures/reader-workload/manifest.txt");

/// Parsed manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Fixture revision recorded in the manifest.
    pub fixture_revision: String,
    /// One record per listed asset/license/reference file.
    pub records: Vec<AssetRecord>,
}

/// One manifest record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRecord {
    /// Fixture-root-relative path (portable, never absolute).
    pub path: String,
    /// Lowercase SHA-256 of the file.
    pub sha256: String,
    /// Exact version/revision or generated-asset revision.
    pub version: String,
    /// Provenance/source description.
    pub provenance: String,
    /// Concrete source location (URL or repository path) when the file
    /// is third-party or excerpted; a project-authored marker otherwise.
    pub source: String,
    /// Applicable license identifier.
    pub license: String,
    /// Fixture-root-relative path of the license/notice text.
    pub license_path: String,
    /// Asset role (font role, image, license notice, reference doc).
    pub role: String,
    /// For images: intrinsic pixel dimensions as recorded in the manifest.
    pub dimensions: Option<(u32, u32)>,
}

/// Manifest parsing errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// Malformed manifest line.
    Syntax { line: usize, message: String },
    /// A key was repeated within one record.
    DuplicateKey { line: usize, key: String },
    /// Two records list the same path.
    DuplicatePath { line: usize, path: String },
    /// The manifest does not start with a fixture revision.
    MissingFixtureRevision,
    /// The manifest revision differs from the compiled fixture revision.
    RevisionMismatch { manifest: String, expected: String },
    /// The recorded checksum is not 64 lowercase hex digits.
    InvalidSha256 { line: usize, path: String },
    /// The recorded dimensions are not `<width>x<height>` digits.
    InvalidDimensions { line: usize, value: String },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Syntax { line, message } => {
                write!(f, "manifest line {line}: {message}")
            }
            ManifestError::DuplicateKey { line, key } => {
                write!(f, "manifest line {line}: duplicate key {key:?}")
            }
            ManifestError::DuplicatePath { line, path } => {
                write!(f, "manifest line {line}: duplicate path {path:?}")
            }
            ManifestError::MissingFixtureRevision => {
                write!(f, "manifest is missing a fixture-revision record")
            }
            ManifestError::RevisionMismatch { manifest, expected } => {
                write!(
                    f,
                    "manifest fixture revision {manifest:?} does not match library revision {expected:?}"
                )
            }
            ManifestError::InvalidSha256 { line, path } => {
                write!(f, "manifest line {line}: invalid SHA-256 for {path:?}")
            }
            ManifestError::InvalidDimensions { line, value } => {
                write!(f, "manifest line {line}: invalid dimensions {value:?}")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

/// Parse a manifest source string with strict validation.
pub fn parse_manifest(source: &str) -> Result<Manifest, ManifestError> {
    let mut revision: Option<String> = None;
    let mut records: Vec<AssetRecord> = Vec::new();
    let mut current: Option<AssetRecord> = None;
    let mut seen_paths: Vec<String> = Vec::new();

    fn close_record(
        current: Option<AssetRecord>,
        seen_paths: &mut Vec<String>,
        records: &mut Vec<AssetRecord>,
        line: usize,
    ) -> Result<(), ManifestError> {
        if let Some(record) = current {
            if seen_paths.contains(&record.path) {
                return Err(ManifestError::DuplicatePath {
                    line,
                    path: record.path,
                });
            }
            seen_paths.push(record.path.clone());
            records.push(record);
        }
        Ok(())
    }

    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(ManifestError::Syntax {
                line,
                message: format!("expected 'key: value', found {trimmed:?}"),
            });
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "fixture-revision" => {
                if revision.is_some() {
                    return Err(ManifestError::DuplicateKey {
                        line,
                        key: key.to_string(),
                    });
                }
                revision = Some(value.to_string());
            }
            "path" => {
                close_record(current.take(), &mut seen_paths, &mut records, line)?;
                current = Some(AssetRecord {
                    path: value.to_string(),
                    sha256: String::new(),
                    version: String::new(),
                    provenance: String::new(),
                    source: String::new(),
                    license: String::new(),
                    license_path: String::new(),
                    role: String::new(),
                    dimensions: None,
                });
            }
            "dimensions" => {
                let Some(record) = current.as_mut() else {
                    return Err(ManifestError::Syntax {
                        line,
                        message: "dimensions key appears before a record's path".to_string(),
                    });
                };
                if record.dimensions.is_some() {
                    return Err(ManifestError::DuplicateKey {
                        line,
                        key: key.to_string(),
                    });
                }
                let Some((width, height)) = value.split_once('x') else {
                    return Err(ManifestError::InvalidDimensions {
                        line,
                        value: value.to_string(),
                    });
                };
                if width.is_empty()
                    || height.is_empty()
                    || !width.bytes().all(|b| b.is_ascii_digit())
                    || !height.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(ManifestError::InvalidDimensions {
                        line,
                        value: value.to_string(),
                    });
                }
                record.dimensions = Some((
                    width
                        .parse::<u32>()
                        .map_err(|_| ManifestError::InvalidDimensions {
                            line,
                            value: value.to_string(),
                        })?,
                    height
                        .parse::<u32>()
                        .map_err(|_| ManifestError::InvalidDimensions {
                            line,
                            value: value.to_string(),
                        })?,
                ));
            }
            "sha256" | "version" | "provenance" | "source" | "license" | "license-path"
            | "role" => {
                let Some(record) = current.as_mut() else {
                    return Err(ManifestError::Syntax {
                        line,
                        message: format!("key {key:?} appears before a record's path"),
                    });
                };
                let slot = match key {
                    "sha256" => &mut record.sha256,
                    "version" => &mut record.version,
                    "provenance" => &mut record.provenance,
                    "source" => &mut record.source,
                    "license" => &mut record.license,
                    "license-path" => &mut record.license_path,
                    _ => &mut record.role,
                };
                if !slot.is_empty() {
                    return Err(ManifestError::DuplicateKey {
                        line,
                        key: key.to_string(),
                    });
                }
                *slot = value.to_string();
            }
            _ => {
                return Err(ManifestError::Syntax {
                    line,
                    message: format!("unknown key {key:?}"),
                });
            }
        }
    }
    close_record(
        current.take(),
        &mut seen_paths,
        &mut records,
        source.lines().count(),
    )?;

    let fixture_revision = revision.ok_or(ManifestError::MissingFixtureRevision)?;
    if fixture_revision != crate::FIXTURE_REVISION {
        return Err(ManifestError::RevisionMismatch {
            manifest: fixture_revision.clone(),
            expected: crate::FIXTURE_REVISION.to_string(),
        });
    }

    for record in &records {
        for field in [
            ("sha256", &record.sha256),
            ("version", &record.version),
            ("provenance", &record.provenance),
            ("license", &record.license),
            ("license-path", &record.license_path),
            ("role", &record.role),
        ] {
            if field.1.is_empty() {
                return Err(ManifestError::Syntax {
                    line: 0,
                    message: format!("record {:?} is missing its {} field", record.path, field.0),
                });
            }
        }
        let is_sha256 = record.sha256.len() == 64
            && record
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !is_sha256 {
            return Err(ManifestError::InvalidSha256 {
                line: 0,
                path: record.path.clone(),
            });
        }
    }

    Ok(Manifest {
        fixture_revision,
        records,
    })
}

/// Parse the manifest compiled into the library.
pub fn manifest() -> Result<Manifest, ManifestError> {
    parse_manifest(MANIFEST_SOURCE)
}

/// Fixture-root resolution errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixtureRootError {
    /// No ancestor of the start directory contains the fixture package.
    NotFound { start: String },
}

impl std::fmt::Display for FixtureRootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixtureRootError::NotFound { start } => {
                write!(f, "no ancestor of {start:?} contains {FIXTURE_ROOT:?}")
            }
        }
    }
}

impl std::error::Error for FixtureRootError {}

const FIXTURE_ROOT: &str = "fixtures/reader-workload";

/// Resolve the fixture root by walking up from `start` (inclusive) until
/// a directory containing `fixtures/reader-workload/manifest.txt` is
/// found. Returns the `fixtures/reader-workload` directory itself.
pub fn find_fixture_root(start: &Path) -> Result<PathBuf, FixtureRootError> {
    let manifest_rel = Path::new(FIXTURE_ROOT_RELATIVE).join("manifest.txt");
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(&manifest_rel);
        if candidate.is_file() {
            return Ok(dir.join(FIXTURE_ROOT_RELATIVE));
        }
        current = dir.parent();
    }
    Err(FixtureRootError::NotFound {
        start: start.display().to_string(),
    })
}

/// Resolve the fixture root starting from this crate's manifest
/// directory (the documented convention for in-repository consumers).
pub fn fixture_root() -> Result<PathBuf, FixtureRootError> {
    find_fixture_root(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Absolute filesystem path of a manifest record relative to `root`.
pub fn resolve_asset(root: &Path, record: &AssetRecord) -> PathBuf {
    root.join(&record.path)
}

/// Asset validation failures. Every variant carries a path-specific
/// description; validation never repairs, downloads, or substitutes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetValidationError {
    /// The manifest-recorded file does not exist under `root`.
    Missing { path: PathBuf },
    /// The file exists but its SHA-256 differs from the manifest.
    ChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// A record's license/notice file is missing.
    LicenseMissing { path: PathBuf },
    /// An image file's intrinsic dimensions differ from the manifest.
    DimensionsMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// A font/image file lacks its expected binary format signature
    /// (placeholder detection).
    InvalidFormat { path: PathBuf, detail: String },
    /// The manifest revision differs from the compiled fixture revision.
    RevisionMismatch { manifest: String, expected: String },
    /// The caller-supplied hash function failed.
    Hasher { message: String },
}

impl std::fmt::Display for AssetValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetValidationError::Missing { path } => {
                write!(f, "missing manifest asset: {}", path.display())
            }
            AssetValidationError::ChecksumMismatch {
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "checksum mismatch for {}: expected {expected}, got {actual}",
                    path.display()
                )
            }
            AssetValidationError::LicenseMissing { path } => {
                write!(f, "missing license/notice file: {}", path.display())
            }
            AssetValidationError::DimensionsMismatch {
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "image dimension mismatch for {}: expected {expected}, got {actual}",
                    path.display()
                )
            }
            AssetValidationError::InvalidFormat { path, detail } => {
                write!(f, "invalid asset format: {}: {detail}", path.display())
            }
            AssetValidationError::RevisionMismatch { manifest, expected } => {
                write!(
                    f,
                    "manifest fixture revision {manifest:?} does not match expected {expected:?}"
                )
            }
            AssetValidationError::Hasher { message } => {
                write!(f, "asset hasher failed: {message}")
            }
        }
    }
}

impl std::error::Error for AssetValidationError {}

/// Validate every manifest record against the files under `root`:
/// presence, SHA-256, license/notice presence, image dimensions, and
/// font/image format magic. The hash function is caller-supplied and
/// must return lowercase hex; failures are reported path-specifically.
pub fn validate_assets(
    root: &Path,
    manifest: &Manifest,
    sha256_hex: &dyn Fn(&Path) -> Result<String, String>,
) -> Result<(), AssetValidationError> {
    if manifest.fixture_revision != crate::FIXTURE_REVISION {
        return Err(AssetValidationError::RevisionMismatch {
            manifest: manifest.fixture_revision.clone(),
            expected: crate::FIXTURE_REVISION.to_string(),
        });
    }

    for record in &manifest.records {
        // The license/notice file must exist for every record.
        let license_path = root.join(&record.license_path);
        if !license_path.is_file() {
            return Err(AssetValidationError::LicenseMissing { path: license_path });
        }

        // The asset file itself must exist and match its checksum.
        let asset_path = resolve_asset(root, record);
        if !asset_path.is_file() {
            return Err(AssetValidationError::Missing { path: asset_path });
        }
        let actual =
            sha256_hex(&asset_path).map_err(|message| AssetValidationError::Hasher { message })?;
        if actual != record.sha256 {
            return Err(AssetValidationError::ChecksumMismatch {
                path: asset_path,
                expected: record.sha256.clone(),
                actual,
            });
        }

        // Font/image format signatures reject placeholder content.
        if record.path.ends_with(".ttf") {
            let magic = read_magic(&asset_path, 4)?;
            let is_font =
                magic == [0x00, 0x01, 0x00, 0x00] || magic == *b"OTTO" || magic == *b"true";
            if !is_font {
                return Err(AssetValidationError::InvalidFormat {
                    path: asset_path,
                    detail: format!(
                        "expected a TrueType/OpenType signature, found {}",
                        magic.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    ),
                });
            }
        }
        if record.path.ends_with(".png") {
            let bytes =
                std::fs::read(&asset_path).map_err(|e| AssetValidationError::InvalidFormat {
                    path: asset_path.clone(),
                    detail: e.to_string(),
                })?;
            if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
                return Err(AssetValidationError::InvalidFormat {
                    path: asset_path.clone(),
                    detail: "expected a PNG signature".to_string(),
                });
            }
            let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            if let Some((expected_w, expected_h)) = record.dimensions
                && (width, height) != (expected_w, expected_h)
            {
                return Err(AssetValidationError::DimensionsMismatch {
                    path: asset_path,
                    expected: format!("{expected_w}x{expected_h}"),
                    actual: format!("{width}x{height}"),
                });
            }
        }
    }
    Ok(())
}

fn read_magic(path: &Path, length: usize) -> Result<Vec<u8>, AssetValidationError> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| AssetValidationError::InvalidFormat {
        path: path.to_path_buf(),
        detail: format!("cannot open: {e}"),
    })?;
    let mut buffer = vec![0u8; length];
    file.read_exact(&mut buffer)
        .map_err(|e| AssetValidationError::InvalidFormat {
            path: path.to_path_buf(),
            detail: format!("cannot read signature: {e}"),
        })?;
    Ok(buffer)
}
