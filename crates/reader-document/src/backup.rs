//! Portable profile snapshots. Extraction is staged, verified, then swapped;
//! the former profile is retained beside the new one for recovery.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const VERSION: u32 = 1;
const MAX_FILES: usize = 50_000;
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 16 * 1024 * 1024 * 1024;
const MAX_MANIFEST: u64 = 16 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Options {
    pub documents: bool,
    pub dictionaries: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Item {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    source_root: PathBuf,
    options: Options,
    files: Vec<Item>,
}

fn windows_path(text: &str) -> bool {
    if text.starts_with("\\\\?\\UNC\\") {
        return true;
    }
    let text = text.strip_prefix("\\\\?\\").unwrap_or(text);
    text.starts_with("\\\\")
        || (text.as_bytes().get(1) == Some(&b':')
            && text.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && text
                .as_bytes()
                .get(2)
                .is_some_and(|b| matches!(b, b'\\' | b'/')))
}

fn portable_absolute(text: &str) -> bool {
    text.starts_with('/') || windows_path(text)
}

/// Old v1 archives hash paths using their source platform's encoding.
fn source_path_key(text: &str, windows: bool) -> String {
    let mut digest = Sha256::new();
    if windows {
        for unit in text.encode_utf16() {
            digest.update(unit.to_le_bytes());
        }
    } else {
        digest.update(text.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

#[derive(Clone, Debug)]
pub struct Summary {
    pub files: usize,
    pub bytes: u64,
    pub documents: bool,
    pub dictionaries: bool,
}

pub fn root() -> PathBuf {
    crate::position::storage_base().join("simPl")
}

fn plain(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Linked profile paths cannot be backed up or restored".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("Linked profile paths are not supported".into());
    }
    Ok(metadata)
}

fn allowed(name: &str, options: Options) -> bool {
    matches!(
        name,
        "preferences.json" | "library.json" | "recent.json" | "shelves.json"
    ) || ["annotations/", "positions/", "covers/", "reading/"]
        .iter()
        .any(|p| name.starts_with(p))
        || (options.documents && name.starts_with("documents/"))
        || (options.dictionaries && name.starts_with("dictionaries/"))
}

fn checked_name(name: &str, options: Options) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.len() > 4096
        || name.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        || !name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && ![
                    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6",
                    "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7",
                    "lpt8", "lpt9",
                ]
                .contains(
                    &part
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                )
        })
        || !Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        || !allowed(name, options)
    {
        return Err(format!("Unsafe or unsupported backup entry: {name}"));
    }
    Ok(PathBuf::from(name))
}

fn visit(
    root: &Path,
    directory: &Path,
    options: Options,
    files: &mut Vec<(String, u64)>,
) -> Result<(), String> {
    plain(directory)?;
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let name = relative
            .to_str()
            .ok_or("Profile filenames must be Unicode")?
            .replace('\\', "/");
        // Only descend into explicitly included profile directories.
        let top = name.split('/').next().unwrap_or("");
        let included = allowed(&format!("{top}/"), options) || allowed(&name, options);
        if !included
            || (name.starts_with("annotations/")
                || name.starts_with("positions/")
                || name.starts_with("reading/"))
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy().starts_with('.')
                        && name.to_string_lossy().ends_with(".tmp")
                })
        {
            continue;
        }
        let metadata = plain(&path)?;
        if metadata.is_dir() {
            visit(root, &path, options, files)?;
        } else if metadata.is_file() {
            checked_name(&name, options)?;
            if metadata.len() > MAX_FILE {
                return Err(format!("Backup file exceeds 512 MiB: {name}"));
            }
            files.push((name, metadata.len()));
            if files.len() > MAX_FILES {
                return Err("Backup contains too many files".into());
            }
        } else {
            return Err("Profile contains an unsupported file type".into());
        }
    }
    Ok(())
}

fn inventory(root: &Path, options: Options) -> Result<Vec<(String, u64)>, String> {
    let mut files = Vec::new();
    if root.exists() {
        visit(root, root, options, &mut files)?;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if files.iter().map(|f| f.1).sum::<u64>() > MAX_TOTAL {
        return Err("Backup exceeds the 16 GiB limit".into());
    }
    Ok(files)
}

pub fn estimate(options: Options) -> Result<Summary, String> {
    let files = inventory(&root(), options)?;
    Ok(Summary {
        files: files.len(),
        bytes: files.iter().map(|f| f.1).sum(),
        documents: options.documents,
        dictionaries: options.dictionaries,
    })
}

fn unique(parent: &Path, label: &str) -> PathBuf {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    parent.join(format!(
        ".simPl-{label}-{time}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn copy_hash(
    mut source: impl Read,
    mut destination: impl Write,
    expected: u64,
) -> Result<String, String> {
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > expected {
            return Err("Backup file grew or exceeds its declared size".into());
        }
        destination
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        digest.update(&buffer[..count]);
    }
    if bytes != expected {
        return Err("Backup file is truncated or changed during transfer".into());
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn create(destination: &Path, options: Options) -> Result<Summary, String> {
    create_from(&root(), destination, options)
}

fn create_from(root: &Path, destination: &Path, options: Options) -> Result<Summary, String> {
    let parent = destination
        .parent()
        .ok_or("Backup destination needs a folder")?;
    if root.exists() {
        plain(root)?;
    } else {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("Cannot read profile: {e}"))?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if parent.starts_with(&root) {
        return Err("Save the backup outside the simPl profile".into());
    }
    let files = inventory(&root, options)?;
    let summary = Summary {
        files: files.len(),
        bytes: files.iter().map(|f| f.1).sum(),
        documents: options.documents,
        dictionaries: options.dictionaries,
    };
    let temporary = unique(&parent, "backup");
    let result = (|| {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let mut zip = ZipWriter::new(file);
        let settings =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut manifest = Manifest {
            version: VERSION,
            source_root: root.clone(),
            options,
            files: Vec::new(),
        };
        for (name, bytes) in files {
            let path = root.join(&name);
            let before = plain(&path)?;
            zip.start_file(&name, settings).map_err(|e| e.to_string())?;
            let sha256 = copy_hash(
                File::open(&path).map_err(|e| e.to_string())?,
                &mut zip,
                bytes,
            )?;
            let after = plain(&path)?;
            if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
                return Err(format!(
                    "Profile changed while backing up {name}; try again"
                ));
            }
            manifest.files.push(Item {
                path: name,
                bytes,
                sha256,
            });
        }
        let data = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        if data.len() as u64 > MAX_MANIFEST {
            return Err("Backup manifest exceeds its size limit".into());
        }
        zip.start_file("manifest.json", settings)
            .map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
        zip.finish()
            .map_err(|e| e.to_string())?
            .sync_all()
            .map_err(|e| e.to_string())?;
        // Verify the completed ZIP before replacing an existing backup.
        validate_archive(&temporary)?;
        crate::position::replace_file(&temporary, destination, "backup")?;
        Ok(summary)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn manifest(zip: &mut ZipArchive<File>) -> Result<Manifest, String> {
    let file = zip
        .by_name("manifest.json")
        .map_err(|e| format!("Not a simPl backup: {e}"))?;
    if file.size() > MAX_MANIFEST {
        return Err("Backup manifest is oversized".into());
    }
    let record: Manifest =
        serde_json::from_reader(file.take(MAX_MANIFEST + 1)).map_err(|e| e.to_string())?;
    if record.version != VERSION
        || !record.source_root.to_str().is_some_and(portable_absolute)
        || record.files.len() > MAX_FILES
        || record
            .files
            .iter()
            .try_fold(0_u64, |total, f| total.checked_add(f.bytes))
            .is_none_or(|total| total > MAX_TOTAL)
        || record.files.iter().any(|f| {
            f.bytes > MAX_FILE
                || f.sha256.len() != 64
                || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        return Err("Unsupported or oversized backup manifest".into());
    }
    let mut names = HashSet::new();
    for item in &record.files {
        checked_name(&item.path, record.options)?;
        if !names.insert(item.path.to_lowercase()) {
            return Err("Backup contains duplicate filenames".into());
        }
    }
    if zip.len() != record.files.len() + 1 {
        return Err("Backup ZIP does not match its manifest".into());
    }
    let declared: HashMap<&str, u64> = record
        .files
        .iter()
        .map(|item| (item.path.as_str(), item.bytes))
        .collect();
    let mut actual = HashSet::new();
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|e| e.to_string())?;
        if !actual.insert(file.name().to_lowercase())
            || file.is_dir()
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Backup contains a duplicate, directory or linked ZIP entry".into());
        }
        if file.name() != "manifest.json" && declared.get(file.name()).copied() != Some(file.size())
        {
            return Err("Backup contains an undeclared entry or incorrect size".into());
        }
    }
    Ok(record)
}

fn validate_archive(path: &Path) -> Result<Manifest, String> {
    let mut zip =
        ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let record = manifest(&mut zip)?;
    for item in &record.files {
        let hash = copy_hash(
            zip.by_name(&item.path).map_err(|e| e.to_string())?,
            std::io::sink(),
            item.bytes,
        )?;
        if hash != item.sha256 {
            return Err(format!("Backup checksum mismatch: {}", item.path));
        }
    }
    Ok(record)
}

pub fn inspect(path: &Path) -> Result<Summary, String> {
    let record = validate_archive(path)?;
    Ok(Summary {
        files: record.files.len(),
        bytes: record.files.iter().map(|f| f.bytes).sum(),
        documents: record.options.documents,
        dictionaries: record.options.dictionaries,
    })
}

pub fn restore(path: &Path) -> Result<PathBuf, String> {
    restore_into(path, &root())
}

fn restore_into(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let parent = root.parent().ok_or("Profile needs a parent folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    let root = parent.join(root.file_name().ok_or("Invalid profile name")?);
    if root.exists() {
        plain(&root)?;
    }
    let record = validate_archive(path)?;
    let stage = unique(&parent, "restore");
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut zip = ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for item in &record.files {
            let destination = stage.join(checked_name(&item.path, record.options)?);
            fs::create_dir_all(destination.parent().ok_or("Invalid entry parent")?)
                .map_err(|e| e.to_string())?;
            let file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|e| e.to_string())?;
            let hash = copy_hash(
                zip.by_name(&item.path).map_err(|e| e.to_string())?,
                &file,
                item.bytes,
            )?;
            if hash != item.sha256 {
                return Err(format!(
                    "Backup changed or checksum mismatch: {}",
                    item.path
                ));
            }
            file.sync_all().map_err(|e| e.to_string())?;
        }
        drop(zip);
        remap_paths(&stage, &record.source_root, &root)?;
        validate_profile(&stage)?;
        // A lightweight backup retains the current local document/dictionary
        // files. Included directories replace theirs as part of the snapshot.
        for (folder, included) in [
            ("documents", record.options.documents),
            ("dictionaries", record.options.dictionaries),
        ] {
            if !included && root.join(folder).exists() {
                copy_tree(&root.join(folder), &stage.join(folder))?;
            }
        }
        let previous = unique(&parent, "before-restore");
        let had_profile = root.exists();
        if had_profile {
            fs::rename(&root, &previous)
                .map_err(|e| format!("Cannot retain the current profile: {e}"))?;
        }
        if let Err(error) = fs::rename(&stage, &root) {
            if had_profile && let Err(rollback) = fs::rename(&previous, &root) {
                return Err(format!(
                    "Restore failed: {error}; rollback failed: {rollback}. The previous profile is at {}",
                    previous.display()
                ));
            }
            return Err(format!(
                "Restore failed; the current profile was preserved: {error}"
            ));
        }
        Ok(if had_profile {
            previous
        } else {
            PathBuf::new()
        })
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if !plain(from)?.is_dir() {
        return Err("Expected a profile directory".into());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for item in fs::read_dir(from).map_err(|e| e.to_string())? {
        let path = item.map_err(|e| e.to_string())?.path();
        let target = to.join(path.file_name().ok_or("Invalid filename")?);
        let metadata = plain(&path)?;
        if metadata.is_dir() {
            copy_tree(&path, &target)?;
        } else if metadata.is_file() {
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        } else {
            return Err("Unsupported profile file".into());
        }
    }
    Ok(())
}

fn remap_paths(stage: &Path, from: &Path, to: &Path) -> Result<(), String> {
    let mut paths = Vec::new();
    let from = from.to_str().ok_or("Backup profile path is not Unicode")?;
    let windows = windows_path(from);
    let prefix = if windows {
        from.replace('\\', "/")
    } else {
        from.into()
    };
    let prefix = format!("{}/", prefix.trim_end_matches('/'));
    fn walk(
        value: &mut serde_json::Value,
        prefix: &str,
        windows: bool,
        to: &Path,
        paths: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), String> {
        match value {
            serde_json::Value::String(text) => {
                if !portable_absolute(text) || text.contains('\0') || text.len() > 4096 {
                    return Err("Restored document has an invalid source path".into());
                }
                let normalized = if windows {
                    text.replace('\\', "/")
                } else {
                    text.clone()
                };
                let relative = normalized.strip_prefix(prefix);
                let new = if let Some(relative) = relative {
                    to.join(checked_name(
                        relative,
                        Options {
                            documents: true,
                            dictionaries: true,
                        },
                    )?)
                } else if Path::new(text).is_absolute() && windows_path(text) == cfg!(windows) {
                    PathBuf::from(text.as_str())
                } else {
                    // Foreign external paths remain visibly missing until Locate
                    // imports matching content; never interpret them as local files.
                    to.join("documents/.missing")
                        .join(source_path_key(text, windows))
                        .join("book")
                };
                if new.to_string_lossy() != *text {
                    let old = text.clone();
                    *text = new.to_string_lossy().into_owned();
                    paths.push((old, new));
                }
            }
            serde_json::Value::Object(map) => {
                for (key, value) in map.iter_mut() {
                    if key == "path" || !value.is_string() {
                        walk(value, prefix, windows, to, paths)?;
                    }
                }
            }
            serde_json::Value::Array(array) => {
                for value in array {
                    walk(value, prefix, windows, to, paths)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for name in ["library.json", "recent.json"] {
        let path = stage.join(name);
        if !path.exists() {
            continue;
        }
        if plain(&path)?.len() > 4 * 1024 * 1024 {
            return Err("Restored library/history is oversized".into());
        }
        let mut data: serde_json::Value =
            serde_json::from_reader(File::open(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Invalid restored {name}: {e}"))?;
        walk(&mut data, &prefix, windows, to, &mut paths)?;
        fs::write(&path, serde_json::to_vec(&data).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    paths.sort();
    paths.dedup();
    for (old, new) in paths {
        let old_key = source_path_key(&old, windows);
        let new_key = crate::position::path_key(&new);
        for suffix in [
            "json",
            "epub.json",
            "pdf.json",
            "pdf-book.json",
            "pdf-mode.json",
        ] {
            let original = stage.join("positions").join(format!("{old_key}.{suffix}"));
            let target = stage.join("positions").join(format!("{new_key}.{suffix}"));
            if original.exists() && original != target {
                fs::copy(original, target).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Text,
    Json,
}
impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Markdown => "Markdown",
            Self::Text => "Text",
            Self::Json => "JSON",
        })
    }
}

pub fn export_annotations(
    path: &Path,
    title: &str,
    notes: &crate::annotations::Annotations,
    format: ExportFormat,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct Export<'a> {
        version: u32,
        title: &'a str,
        fingerprint: &'a str,
        bookmarks: &'a [crate::annotations::Bookmark],
        highlights: &'a [crate::annotations::Highlight],
    }
    let bytes = if format == ExportFormat::Json {
        serde_json::to_vec_pretty(&Export {
            version: VERSION,
            title,
            fingerprint: &notes.fingerprint,
            bookmarks: &notes.bookmarks,
            highlights: &notes.highlights,
        })
        .map_err(|e| e.to_string())?
    } else {
        let mut output = if format == ExportFormat::Markdown {
            format!("# {}\n\n", title.replace(['\r', '\n'], " "))
        } else {
            format!("{title}\n\n")
        };
        for mark in &notes.bookmarks {
            output.push_str(&format!(
                "{}Bookmark — page {}\n\n{}\n\n",
                if format == ExportFormat::Markdown {
                    "## "
                } else {
                    ""
                },
                mark.page,
                mark.excerpt
            ));
        }
        for highlight in &notes.highlights {
            let chapter = match &highlight.place {
                crate::annotations::Place::Reflow { chapter, .. } => {
                    chapter.as_deref().unwrap_or("")
                }
                _ => "",
            };
            output.push_str(&format!(
                "{}Page {} — {} {}\n\n",
                if format == ExportFormat::Markdown {
                    "## "
                } else {
                    ""
                },
                highlight.page,
                highlight.color.label(),
                chapter
            ));
            for line in highlight.quote.lines() {
                output.push_str(&format!(
                    "{}{line}\n",
                    if format == ExportFormat::Markdown {
                        "> "
                    } else {
                        ""
                    }
                ));
            }
            if let Some(note) = &highlight.note {
                output.push_str(&format!("\n{note}\n"));
            }
            output.push('\n');
        }
        output.into_bytes()
    };
    crate::position::atomic_write(
        path,
        &bytes,
        "annotation export",
        "annotation export",
        ".export",
    )
}

fn validate_profile(stage: &Path) -> Result<(), String> {
    crate::preferences::load_from(&stage.join("preferences.json"))?;
    crate::library::load_at(&stage.join("library.json"))?;
    crate::recent::load_at(&stage.join("recent.json"))?;
    crate::shelves::load_at(&stage.join("shelves.json"))?;
    for directory in ["annotations", "reading", "positions"] {
        let folder = stage.join(directory);
        if !folder.exists() {
            continue;
        }
        for entry in fs::read_dir(&folder).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if !path.is_file() || path.extension().is_none_or(|s| s != "json") {
                return Err(format!("Unsupported {directory} record"));
            }
            let key = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid record filename")?;
            if directory == "annotations" {
                crate::annotations::load_from(&folder, key)?;
            } else if directory == "reading" {
                crate::reading::load_from(&path, key)?;
            } else {
                crate::position::validate_backup_record(&path)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn windows_and_android_v1_archives_remap_documents_and_encoded_position_keys() {
        assert!(windows_path(r"\\?\UNC\server\share\simPl"));
        assert!(windows_path(r"\\server\share\simPl"));
        for bytes in [
            include_bytes!("../../../android/app/src/androidTest/assets/p3-windows.zip").as_slice(),
            include_bytes!("../../../android/app/src/androidTest/assets/p3-android.zip").as_slice(),
        ] {
            let dir = TestDir::new();
            let zip = dir.0.join("portable.zip");
            fs::write(&zip, bytes).unwrap();
            let root = dir.0.join("restored");
            restore_into(&zip, &root).unwrap();
            let books = crate::library::load_at(&root.join("library.json")).unwrap();
            let document = &books[0].document;
            assert!(document.path.is_file());
            assert_eq!(document.title, "Portable Harbour");
            let key = crate::position::path_key(&document.path);
            let position: serde_json::Value = serde_json::from_slice(
                &fs::read(root.join(format!("positions/{key}.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(position["position"]["within"], 0.5);
            let notes =
                crate::annotations::load_from(&root.join("annotations"), &document.fingerprint)
                    .unwrap();
            assert_eq!(
                notes.highlights[0].note.as_deref(),
                Some("Travel note — İstanbul 😀")
            );
        }
    }

    #[test]
    fn foreign_external_paths_become_missing_without_being_interpreted_locally() {
        let dir = TestDir::new();
        let source = if cfg!(windows) {
            "/home/reader/simPl"
        } else {
            "C:\\Users\\Reader\\simPl"
        };
        let path = if cfg!(windows) {
            "/home/reader/books/one.html"
        } else {
            "D:\\Books\\one.html"
        };
        dir.write(
            "stage/library.json",
            &serde_json::to_vec(&json!({"entries":[{"document":{"path":path,"title":"A title"}}]}))
                .unwrap(),
        );
        let root = dir.0.join("restored");
        remap_paths(&dir.0.join("stage"), Path::new(source), &root).unwrap();
        let data: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.0.join("stage/library.json")).unwrap()).unwrap();
        let mapped = PathBuf::from(data["entries"][0]["document"]["path"].as_str().unwrap());
        assert!(mapped.starts_with(root.join("documents/.missing")));
        assert!(!mapped.exists());
        assert_eq!(data["entries"][0]["document"]["title"], "A title");
    }
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = unique(&std::env::temp_dir(), "backup-test");
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn record(root: &TestDir) -> (PathBuf, String) {
        root.write(
            "source/documents/book/chapter.html",
            b"<p>Unicode: \xc3\x87in</p>",
        );
        root.write("source/documents/book/asset.png", b"resource");
        root.write("source/dictionaries/en-tr.bin", b"dictionary");
        root.write("source/page-maps/expensive.json", b"excluded cache");
        let path = root
            .0
            .join("source/documents/book/chapter.html")
            .canonicalize()
            .unwrap();
        let key = "a".repeat(64);
        let document = json!({"path": path, "title": path.to_string_lossy(), "fingerprint": key, "kind": "Html"});
        root.write(
            "source/recent.json",
            &serde_json::to_vec(&json!({"version":1,"entries":[document]})).unwrap(),
        );
        root.write("source/library.json", &serde_json::to_vec(&json!({"version":1,"entries":[{"document": document, "author": "Reader", "byte_len": 23,"opened_at": 1,"progress": 0.5,"current": 2,"total": 4,"cover": false,"favourite": true,"source_kind": "Markdown"}]})).unwrap());
        root.write("source/preferences.json", br#"{"version":1,"appearance":"dark","reading":{"font":"spectral","size":26,"spacing":180,"margin":64}}"#);
        let reflow =
            json!({"fingerprint":key,"item_id":"paragraph-0","within":0.5,"font_size":20.0});
        for suffix in ["json", "pdf-book.json"] {
            root.write(
                &format!(
                    "source/positions/{}.{}",
                    crate::position::path_key(&path),
                    suffix
                ),
                &serde_json::to_vec(&json!({"version":1,"position":reflow})).unwrap(),
            );
        }
        root.write(&format!("source/positions/{}.epub.json", crate::position::path_key(&path)), &serde_json::to_vec(&json!({"version":1,"position":{"fingerprint":key,"chapter":"chapter.xhtml","item_id":"paragraph-0","within":0.5,"font_size":20.0}})).unwrap());
        root.write(&format!("source/positions/{}.pdf.json", crate::position::path_key(&path)), &serde_json::to_vec(&json!({"version":1,"position":{"fingerprint":key,"page":1,"within":0.5,"horizontal":0.0,"zoom":"FitWidth"}})).unwrap());
        root.write(
            &format!(
                "source/positions/{}.pdf-mode.json",
                crate::position::path_key(&path)
            ),
            &serde_json::to_vec(&json!({"version":1,"position":{"fingerprint":key,"mode":"Book"}}))
                .unwrap(),
        );
        root.write(&format!("source/reading/{key}.json"), &serde_json::to_vec(&json!({"version":1,"fingerprint":key,"options":{"font":"fira_sans","size":24,"spacing":160,"margin":32}})).unwrap());
        root.write("source/shelves.json", &serde_json::to_vec(&json!({"version":1,"next_id":1,"shelves":[{"id":1,"name":"Okunacak — 日本語","books":[key]}]})).unwrap());
        root.write(&format!("source/annotations/{key}.json"), &serde_json::to_vec(&json!({
            "version":1,"fingerprint":key,"next_id":3,
            "bookmarks":[{"id":1,"created":1,"place":{"kind":"reflow","chapter":null,"item_id":"paragraph-0","within":0.0,"page_number":0},"page":"1","excerpt":"İstanbul — 日本語"}],
            "highlights":[{"id":2,"created":1,"color":"yellow","place":{"kind":"reflow","chapter":null,"from":{"item_id":"paragraph-0","byte":0},"to":{"item_id":"paragraph-0","byte":8}},"page":"1","quote":"Çin 😀","note":"Kendi notum — 한국어"}]
        })).unwrap());
        root.write(&format!("source/covers/{key}.png"), b"thumbnail bytes");
        (path, key)
    }
    #[test]
    fn full_backup_is_portable_rekeys_every_position_and_retains_previous_profile() {
        let dir = TestDir::new();
        let (old_path, key) = record(&dir);
        dir.write("destination/preferences.json", b"previous bytes");
        dir.write("destination/documents/old.html", b"old book");
        let backup = dir.0.join("snapshot.zip");
        let options = Options {
            documents: true,
            dictionaries: true,
        };
        let summary = create_from(&dir.0.join("source"), &backup, options).unwrap();
        assert!(summary.files >= 11);
        assert_eq!(inspect(&backup).unwrap().files, summary.files);
        let destination = dir.0.join("destination");
        let previous = restore_into(&backup, &destination).unwrap();
        assert_eq!(
            fs::read(previous.join("preferences.json")).unwrap(),
            b"previous bytes"
        );
        assert!(!destination.join("page-maps").exists());
        assert!(!destination.join("documents/old.html").exists());
        let library = crate::library::load_at(&destination.join("library.json")).unwrap();
        let new_path = destination
            .join("documents/book/chapter.html")
            .canonicalize()
            .unwrap();
        assert_eq!(library[0].document.path, new_path);
        assert_eq!(library[0].document.title, old_path.to_string_lossy());
        assert!(library[0].favourite);
        assert_eq!(library[0].progress, 0.5);
        assert_eq!(library[0].format(), crate::library::SourceFormat::Markdown);
        assert_eq!(
            crate::recent::load_at(&destination.join("recent.json")).unwrap()[0].path,
            new_path
        );
        for suffix in [
            "json",
            "epub.json",
            "pdf.json",
            "pdf-book.json",
            "pdf-mode.json",
        ] {
            assert_eq!(
                fs::read(destination.join(format!(
                    "positions/{}.{}",
                    crate::position::path_key(&new_path),
                    suffix
                )))
                .unwrap(),
                fs::read(dir.0.join(format!(
                    "source/positions/{}.{}",
                    crate::position::path_key(&old_path),
                    suffix
                )))
                .unwrap()
            );
        }
        assert_eq!(
            crate::preferences::load_from(&destination.join("preferences.json"))
                .unwrap()
                .reading
                .size,
            26
        );
        assert_eq!(
            crate::reading::load_from(&destination.join(format!("reading/{key}.json")), &key)
                .unwrap()
                .unwrap()
                .size,
            24
        );
        assert_eq!(
            fs::read(destination.join("dictionaries/en-tr.bin")).unwrap(),
            b"dictionary"
        );
        assert_eq!(
            fs::read(destination.join("documents/book/asset.png")).unwrap(),
            b"resource"
        );
        let annotations =
            crate::annotations::load_from(&destination.join("annotations"), &key).unwrap();
        assert_eq!(
            annotations,
            crate::annotations::load_from(&dir.0.join("source/annotations"), &key).unwrap()
        );
        assert_eq!(
            annotations.highlights[0].note.as_deref(),
            Some("Kendi notum — 한국어")
        );
        let shelves = crate::shelves::load_at(&destination.join("shelves.json")).unwrap();
        assert!(shelves.contains(1, &key));
        assert_eq!(shelves.get(1).unwrap().name, "Okunacak — 日本語");
        assert_eq!(
            fs::read(destination.join(format!("covers/{key}.png"))).unwrap(),
            b"thumbnail bytes"
        );
    }
    #[test]
    fn lightweight_snapshot_keeps_local_packs_and_books_but_restores_metadata() {
        let dir = TestDir::new();
        record(&dir);
        dir.write("destination/documents/local.html", b"local");
        dir.write("destination/dictionaries/local.bin", b"local pack");
        let backup = dir.0.join("snapshot.zip");
        create_from(&dir.0.join("source"), &backup, Options::default()).unwrap();
        let destination = dir.0.join("destination");
        restore_into(&backup, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("documents/local.html")).unwrap(),
            b"local"
        );
        assert_eq!(
            fs::read(destination.join("dictionaries/local.bin")).unwrap(),
            b"local pack"
        );
        assert!(!destination.join("documents/book/chapter.html").exists());
        assert_eq!(
            crate::library::load_at(&destination.join("library.json"))
                .unwrap()
                .len(),
            1
        );
    }
    fn archive(path: &Path, entries: &[(&str, &[u8])], change: impl FnOnce(&mut Manifest)) {
        let mut record = Manifest {
            version: 1,
            source_root: std::env::temp_dir(),
            options: Options {
                documents: true,
                dictionaries: true,
            },
            files: entries
                .iter()
                .map(|(name, data)| Item {
                    path: name.to_string(),
                    bytes: data.len() as u64,
                    sha256: format!("{:x}", Sha256::digest(data)),
                })
                .collect(),
        };
        change(&mut record);
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&record).unwrap())
            .unwrap();
        zip.finish().unwrap();
    }
    #[test]
    fn corrupt_unsafe_duplicate_and_overflowing_archives_never_change_profile() {
        let dir = TestDir::new();
        dir.write("destination/preferences.json", b"unchanged");
        let destination = dir.0.join("destination");
        let backup = dir.0.join("bad.zip");
        for name in [
            "../outside.txt",
            "documents/../outside.txt",
            "documents/C:evil",
            "documents/NUL.txt",
            "documents/test?.txt",
            "documents/link/../../escape",
            "documents/a.",
        ] {
            archive(&backup, &[(name, b"unsafe")], |_| {});
            assert!(restore_into(&backup, &destination).is_err(), "{name}");
        }
        archive(
            &backup,
            &[("documents/a", b"one"), ("documents/A", b"two")],
            |_| {},
        );
        assert!(restore_into(&backup, &destination).is_err());
        archive(&backup, &[("documents/a", b"payload")], |record| {
            record.files[0].sha256 = "0".repeat(64)
        });
        assert!(restore_into(&backup, &destination).is_err());
        archive(
            &backup,
            &[("documents/a", b"one"), ("documents/b", b"two")],
            |record| record.files.iter_mut().for_each(|f| f.bytes = u64::MAX),
        );
        assert!(restore_into(&backup, &destination).is_err());
        archive(&backup, &[("preferences.json", b"{broken")], |_| {});
        assert!(restore_into(&backup, &destination).is_err());
        assert_eq!(
            fs::read(destination.join("preferences.json")).unwrap(),
            b"unchanged"
        );
        assert!(!dir.0.join("outside.txt").exists());
        assert!(!fs::read_dir(&dir.0).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".simPl-restore-")
        }));
    }
    #[test]
    fn backup_must_be_outside_profile_and_can_atomically_replace_existing_zip() {
        let dir = TestDir::new();
        record(&dir);
        let source = dir.0.join("source");
        assert!(create_from(&source, &source.join("snapshot.zip"), Options::default()).is_err());
        let backup = dir.0.join("snapshot.zip");
        fs::write(&backup, b"previous").unwrap();
        create_from(&source, &backup, Options::default()).unwrap();
        assert!(inspect(&backup).is_ok());
    }
    #[test]
    fn exports_keep_unicode_quotes_notes_bookmarks_and_json_anchors() {
        use crate::annotations::{Annotations, BookmarkPlace, HighlightColor, Place, ReflowPoint};
        let dir = TestDir::new();
        let mut notes = Annotations::new(&"a".repeat(64)).unwrap();
        notes
            .add_bookmark(
                BookmarkPlace::Pdf {
                    page: 2,
                    within: 0.0,
                },
                "iii".into(),
                "Şiir — 日本語".into(),
            )
            .unwrap();
        let id = notes
            .add_highlight(
                Place::Reflow {
                    chapter: Some("chapter.xhtml".into()),
                    from: ReflowPoint {
                        item_id: "p-1".into(),
                        byte: 0,
                    },
                    to: ReflowPoint {
                        item_id: "p-1".into(),
                        byte: 8,
                    },
                },
                HighlightColor::Yellow,
                "12".into(),
                "Çin\n中文 한국어 😀".into(),
            )
            .unwrap();
        notes.set_note(id, "Kendi notum — français").unwrap();
        for (name, format) in [
            ("notes.md", ExportFormat::Markdown),
            ("notes.txt", ExportFormat::Text),
            ("notes.json", ExportFormat::Json),
        ] {
            let path = dir.0.join(name);
            export_annotations(&path, "Kitabım", &notes, format).unwrap();
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains("Çin") && text.contains("Kendi notum") && text.contains("Şiir"));
            if format == ExportFormat::Json {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(value["highlights"][0]["place"]["to"]["byte"], 8);
                assert_eq!(value["bookmarks"][0]["place"]["page"], 2);
            }
            if format == ExportFormat::Markdown {
                assert!(text.contains("> 中文 한국어 😀"));
            }
        }
    }
}
