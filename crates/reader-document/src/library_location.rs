//! Where imported copies live. `library-location.json` in the profile names the
//! library root; only a missing file means the default `<profile>\documents`.
//! Settings, positions and history always stay in the profile.

use crate::managed::MARKER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf, Prefix},
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const FILE: &str = "library-location.json";
const LOCK: &str = "library-location.lock";
const STAGING: &str = ".incoming";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 64 * 1024;
const CHUNK: usize = 64 * 1024;
const BUSY: &str = "Another simPl window is changing the library.";
/// The folder simPl creates inside the folder the user picks.
pub const FOLDER: &str = "simPl Library";

/// `Default` is `<profile>\documents` and is never stored as a path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Root {
    Default,
    /// The canonical `…\simPl Library`.
    Folder(PathBuf),
}

impl Root {
    fn path(&self) -> PathBuf {
        match self {
            Self::Default => default_root(),
            Self::Folder(path) => path.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Move {
    Copying { to: Root },
    Cleaning { from: Root, positions_copied: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    root: Root,
    moving: Option<Move>,
}

impl Record {
    fn new(root: Root, moving: Option<Move>) -> Self {
        Self {
            version: VERSION,
            root,
            moving,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub root: PathBuf,
    pub custom: bool,
    pub available: bool,
    /// A move journal is pending: only recovery may change the library.
    pub moving: bool,
}

fn profile() -> PathBuf {
    crate::position::storage_base().join("simPl")
}

fn default_root() -> PathBuf {
    profile().join("documents")
}

fn read() -> Result<Record, String> {
    let path = profile().join(FILE);
    let unreadable = |error: String| format!("Library settings could not be read: {error}");
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Record::new(Root::Default, None));
        }
        Err(error) => return Err(unreadable(error.to_string())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| unreadable(error.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(unreadable("the file is too large".into()));
    }
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|error| unreadable(error.to_string()))?;
    if record.version != VERSION {
        return Err(unreadable(format!(
            "unsupported version {}",
            record.version
        )));
    }
    let other = match &record.moving {
        Some(Move::Copying { to }) => Some(to),
        Some(Move::Cleaning { from, .. }) => Some(from),
        None => None,
    };
    for root in std::iter::once(&record.root).chain(other) {
        if let Root::Folder(path) = root
            && !(path.is_absolute() && path.to_str().is_some())
        {
            return Err(unreadable("the folder is not an absolute path".into()));
        }
    }
    Ok(record)
}

fn write(record: &Record) -> Result<(), String> {
    injected_fault()?;
    let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
    crate::position::atomic_write(
        &profile().join(FILE),
        &bytes,
        "library settings",
        "profile",
        ".library-location",
    )
}

const ERROR_SHARING_VIOLATION: i32 = 32;

struct MoveLock(#[allow(dead_code)] fs::File);

impl MoveLock {
    fn take() -> Result<Option<Self>, String> {
        fs::create_dir_all(profile()).map_err(|error| error.to_string())?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        match options.open(profile().join(LOCK)) {
            Ok(file) => Ok(Some(Self(file))),
            Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => Ok(None),
            Err(error) => Err(format!("Cannot lock the library settings: {error}")),
        }
    }
}

pub(crate) fn current_root() -> Result<PathBuf, String> {
    Ok(read()?.root.path())
}

pub(crate) fn import_root() -> Result<PathBuf, String> {
    let record = read()?;
    let root = record.root.path();
    if matches!(record.root, Root::Folder(_)) && !root.is_dir() {
        return Err(format!("Library folder not found: {}", display_path(&root)));
    }
    Ok(root)
}

pub fn location() -> Result<Location, String> {
    let record = read()?;
    let root = record.root.path();
    let custom = matches!(record.root, Root::Folder(_));
    Ok(Location {
        available: !custom || root.is_dir(),
        root,
        custom,
        moving: record.moving.is_some(),
    })
}

/// "Stop using this folder": an unavailable custom folder is replaced by the
/// default one. Nothing is moved or deleted.
pub fn forget() -> Result<(), String> {
    let _lock = MoveLock::take()?.ok_or(BUSY)?;
    let record = read()?;
    let Root::Folder(root) = &record.root else {
        return Err("The library already uses the default folder.".into());
    };
    if root.is_dir() {
        return Err(format!(
            "The library folder {} is connected.",
            display_path(root)
        ));
    }
    if matches!(record.moving, Some(Move::Copying { .. })) {
        return Err("Finish the library move first.".into());
    }
    write(&Record::new(Root::Default, None))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The folder the user picked; the library goes into its `simPl Library`.
    Folder(PathBuf),
    Default,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub books: usize,
    pub bytes: u64,
}

/// A validated move. Only [`plan`] builds one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    from: Root,
    to: Root,
    to_path: PathBuf,
    summary: Summary,
}

impl Plan {
    pub fn to(&self) -> &Path {
        &self.to_path
    }

    pub fn summary(&self) -> Summary {
        self.summary
    }
}

/// The imported books in the current library folder.
pub fn summary() -> Result<Summary, String> {
    count(&current_root()?)
}

fn count(root: &Path) -> Result<Summary, String> {
    let mut summary = Summary { books: 0, bytes: 0 };
    for folder in owned_folders(root)? {
        summary.books += 1;
        summary.bytes += tree_bytes(&folder)?;
    }
    Ok(summary)
}

fn owned_folders(root: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(crate::managed::owned_copies(root)?
        .into_iter()
        .filter_map(|copy| copy.parent().map(Path::to_path_buf))
        .collect())
}

fn tree_bytes(folder: &Path) -> Result<u64, String> {
    let mut bytes = 0;
    for entry in fs::read_dir(folder).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let metadata = crate::managed::regular(&path)?;
        bytes += if metadata.is_dir() {
            tree_bytes(&path)?
        } else {
            metadata.len()
        };
    }
    Ok(bytes)
}

/// Checks a move from the current library folder to `target`.
pub fn plan(target: Target) -> Result<Plan, String> {
    let record = read()?;
    if record.moving.is_some() {
        return Err("Finish the library move first.".into());
    }
    let from_path = record.root.path();
    if matches!(record.root, Root::Folder(_)) && !from_path.is_dir() {
        return Err(format!(
            "Library folder not found: {}",
            display_path(&from_path)
        ));
    }
    let to = match target {
        Target::Default if record.root == Root::Default => {
            return Err("This is already your library folder.".into());
        }
        Target::Default => Root::Default,
        Target::Folder(picked) => Root::Folder(picked_library(&picked)?),
    };
    let to_path = to.path();
    let to_resolved = resolved(&to_path);
    let from_resolved = resolved(&from_path);
    if within(&to_resolved, &from_resolved) && within(&from_resolved, &to_resolved) {
        return Err("This is already your library folder.".into());
    }
    if overlaps(&to_resolved, &from_resolved) {
        return Err("The new library folder can't be inside the current one or contain it.".into());
    }
    if matches!(to, Root::Folder(_)) && overlaps(&to_resolved, &resolved(&profile())) {
        return Err("Choose a folder outside simPl's own data folder.".into());
    }
    if let Some(install) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(resolved))
        && overlaps(&to_resolved, &install)
    {
        return Err("Choose a folder outside the folder simPl is installed in.".into());
    }
    if to_path.exists() {
        if !crate::managed::regular(&to_path)?.is_dir() {
            return Err(format!("{} is not a folder.", display_path(&to_path)));
        }
        refuse_existing_copies(&to_path)?;
    }
    let summary = count(&from_path)?;
    check_free_space(&to_resolved, summary.bytes)?;
    Ok(Plan {
        from: record.root,
        to,
        to_path,
        summary,
    })
}

/// Rollback and staging delete what simPl put in `to`, so it must start
/// with nothing simPl would treat as its own.
fn refuse_existing_copies(to: &Path) -> Result<(), String> {
    if !crate::managed::owned_copies(to)?.is_empty() {
        return Err(format!(
            "{} already has simPl books in it. Choose another folder.",
            display_path(to)
        ));
    }
    if fs::symlink_metadata(to.join(STAGING)).is_ok() {
        return Err(format!(
            "{} already has a {STAGING} folder. Choose another folder.",
            display_path(to)
        ));
    }
    Ok(())
}

fn picked_library(picked: &Path) -> Result<PathBuf, String> {
    let shown = display_path(picked);
    let metadata =
        fs::symlink_metadata(picked).map_err(|_| format!("The folder {shown} doesn't exist."))?;
    if crate::managed::regular(picked).is_err() {
        return Err(format!(
            "{shown} is a link or junction. Choose the folder it points to."
        ));
    }
    if !metadata.is_dir() {
        return Err(format!("{shown} is not a folder."));
    }
    let canonical = fs::canonicalize(picked).map_err(|error| error.to_string())?;
    if network(&canonical) {
        return Err("Network folders can't hold the library. Choose a folder on this PC.".into());
    }
    Ok(canonical.join(FOLDER))
}

#[cfg(windows)]
fn network(canonical: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_REMOTE: u32 = 4;
    let drive = match canonical.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(drive) | Prefix::Disk(drive) => drive,
            _ => return true,
        },
        _ => return true,
    };
    let root: Vec<u16> = std::ffi::OsStr::new(&format!("{}:\\", drive as char))
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
}

#[cfg(not(windows))]
fn network(_canonical: &Path) -> bool {
    false
}

#[cfg(windows)]
fn check_free_space(target: &Path, bytes: u64) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let existing = target
        .ancestors()
        .find(|path| path.is_dir())
        .ok_or("The library folder's drive is not available.")?;
    let name: Vec<u16> = existing.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0_u64;
    // SAFETY: `name` is NUL-terminated and `free` is a valid output location.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            name.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!(
            "Cannot read the free space on {}: {}",
            display_path(existing),
            std::io::Error::last_os_error()
        ));
    }
    let needed = bytes + (bytes / 50).max(64 * 1024 * 1024);
    if free < needed {
        let mb = |bytes: u64| bytes.div_ceil(1024 * 1024);
        return Err(format!(
            "Not enough free space: the move needs {} MB and {} has {} MB free.",
            mb(needed),
            display_path(existing),
            mb(free)
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn check_free_space(_target: &Path, _bytes: u64) -> Result<(), String> {
    Ok(())
}

fn resolved(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(mut canonical) = fs::canonicalize(current) {
            canonical.extend(missing.iter().rev());
            return canonical;
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_owned());
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Path components compared the way Windows does, with `\\?\` and plain
/// prefixes treated alike.
fn parts(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) | Prefix::Disk(drive) => {
                    format!("{}:", drive.to_ascii_uppercase() as char)
                }
                Prefix::VerbatimUNC(server, share) | Prefix::UNC(server, share) => format!(
                    r"\\{}\{}",
                    server.to_string_lossy(),
                    share.to_string_lossy()
                ),
                _ => prefix.as_os_str().to_string_lossy().into_owned(),
            },
            other => other.as_os_str().to_string_lossy().into_owned(),
        })
        .collect()
}

fn within(path: &Path, base: &Path) -> bool {
    let (path, base) = (parts(path), parts(base));
    base.len() <= path.len()
        && base
            .iter()
            .zip(&path)
            .all(|(a, b)| crate::recent::same_path(Path::new(a), Path::new(b)))
}

fn overlaps(first: &Path, second: &Path) -> bool {
    within(first, second) || within(second, first)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub books: usize,
    pub books_total: usize,
    pub bytes: u64,
    pub bytes_total: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Moved {
        to: PathBuf,
        books: usize,
    },
    /// Nothing changed; the books are still in `kept`.
    Cancelled {
        kept: PathBuf,
    },
    /// The library is now `to`, but cleaning up the old folder failed.
    Committed {
        to: PathBuf,
        error: String,
    },
}

enum Stop {
    Cancelled,
    Failed(String),
}

impl From<String> for Stop {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

pub(crate) struct Job<'a> {
    progress: Progress,
    report: &'a mut dyn FnMut(Progress),
    cancel: &'a AtomicBool,
}

impl Job<'_> {
    fn check(&self) -> Result<(), Stop> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Moves every imported copy to the plan's target. The source is not touched
/// until the record names the target, so cancel, failure and crash before
/// that point all leave the library where it was.
pub fn run_move(
    plan: &Plan,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<Outcome, String> {
    let _lock = MoveLock::take()?.ok_or(BUSY)?;
    let record = read()?;
    if record.root != plan.from || record.moving.is_some() {
        return Err("The library changed since the move was prepared. Try again.".into());
    }
    let from = plan.from.path();
    let to = &plan.to_path;
    match plan.to {
        Root::Default => fs::create_dir_all(to).map_err(io)?,
        Root::Folder(_) if !to.exists() => fs::create_dir(to).map_err(io)?,
        Root::Folder(_) => {}
    }
    let mut job = Job {
        progress: Progress {
            books_total: plan.summary.books,
            bytes_total: plan.summary.bytes,
            ..Progress::default()
        },
        report: progress,
        cancel,
    };
    (job.report)(job.progress);
    refuse_existing_copies(to)?;
    let copied = (|| -> Result<(), Stop> {
        remove_staging(to)?;
        write(&Record::new(
            plan.from.clone(),
            Some(Move::Copying {
                to: plan.to.clone(),
            }),
        ))?;
        for folder in owned_folders(&from)? {
            job.check()?;
            place(&folder, to, &mut job)?;
            job.progress.books += 1;
            (job.report)(job.progress);
        }
        remove_staging(to)?;
        job.check()?;
        write(&Record::new(
            plan.to.clone(),
            Some(Move::Cleaning {
                from: plan.from.clone(),
                positions_copied: false,
            }),
        ))?;
        Ok(())
    })();
    if let Err(stop) = copied {
        let undone = rollback(to).and(write(&Record::new(plan.from.clone(), None)));
        return match (stop, undone) {
            (Stop::Cancelled, Ok(())) => Ok(Outcome::Cancelled { kept: from }),
            (Stop::Cancelled, Err(error)) => Err(format!("The move was cancelled. {error}")),
            (Stop::Failed(error), Ok(())) => Err(error),
            (Stop::Failed(error), Err(undo)) => Err(format!("{error} {undo}")),
        };
    }
    Ok(match finish(&plan.from, &plan.to, false) {
        Ok(()) => Outcome::Moved {
            to: to.clone(),
            books: plan.summary.books,
        },
        Err(error) => Outcome::Committed {
            to: to.clone(),
            error,
        },
    })
}

pub(crate) fn placed_id(staged: &Path, library: &Path) -> Result<String, String> {
    let id = staged
        .file_name()
        .and_then(|id| id.to_str())
        .ok_or("Invalid book folder")?;
    let digest = tree_digest(staged)?;
    let mut fresh = Sha256::new();
    fresh.update(id.as_bytes());
    fresh.update(digest.as_bytes());
    let fresh = format!("{:x}", fresh.finalize());
    for candidate in [id, fresh.as_str()] {
        let folder = library.join(candidate);
        if !folder.exists() || tree_digest(&folder)? == digest {
            return Ok(candidate.to_owned());
        }
    }
    Err(format!(
        "{} already holds a different copy of a restored book.",
        display_path(library)
    ))
}

pub(crate) fn place_book(staged: &Path, library: &Path, id: &str) -> Result<(), String> {
    if library.join(id).exists() {
        return Ok(());
    }
    place_quietly(staged, library, id).map(drop)
}

fn tree_digest(folder: &Path) -> Result<String, String> {
    fn visit(folder: &Path, prefix: &str, digest: &mut Sha256) -> Result<(), String> {
        let mut entries = fs::read_dir(folder)
            .map_err(io)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(io)?;
        entries.sort();
        for path in entries {
            let name = format!(
                "{prefix}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            if crate::managed::regular(&path)?.is_dir() {
                visit(&path, &name, digest)?;
            } else {
                let (len, content) = hash_file(&path)?;
                digest.update(name.as_bytes());
                digest.update([0]);
                digest.update(len.to_le_bytes());
                digest.update(&content);
            }
        }
        Ok(())
    }
    let mut digest = Sha256::new();
    visit(folder, "", &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn place_quietly(source: &Path, root: &Path, id: &str) -> Result<PathBuf, String> {
    let cancel = AtomicBool::new(false);
    let mut report = |_| {};
    let mut job = Job {
        progress: Progress::default(),
        report: &mut report,
        cancel: &cancel,
    };
    let result = place_as(source, root, id, &mut job).map_err(|stop| match stop {
        Stop::Failed(error) => error,
        Stop::Cancelled => "Cancelled".into(),
    });
    remove_staging(root)?;
    result
}

fn place(source: &Path, root: &Path, job: &mut Job) -> Result<PathBuf, Stop> {
    let id = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid library folder name".to_string())?;
    place_as(source, root, id, job)
}

fn place_as(source: &Path, root: &Path, id: &str, job: &mut Job) -> Result<PathBuf, Stop> {
    crate::managed::check_tree(source)?;
    let incoming = root.join(STAGING).join(id);
    fs::create_dir_all(root.join(STAGING)).map_err(io)?;
    if incoming.exists() {
        crate::managed::check_tree(&incoming)?;
        fs::remove_dir_all(&incoming).map_err(io)?;
    }
    fs::create_dir(&incoming).map_err(io)?;
    copy_tree(source, &incoming, job)?;
    injected_fault()?;
    let target = root.join(id);
    fs::rename(&incoming, &target)
        .map_err(|error| format!("Cannot place {}: {error}", display_path(&target)))?;
    Ok(target)
}

fn copy_tree(source: &Path, target: &Path, job: &mut Job) -> Result<(), Stop> {
    let mut entries = fs::read_dir(source)
        .map_err(io)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    entries.sort_by_key(|path| {
        (
            path.file_name() == Some(std::ffi::OsStr::new(MARKER)),
            path.clone(),
        )
    });
    for path in entries {
        let metadata = crate::managed::regular(&path)?;
        let name = path
            .file_name()
            .ok_or_else(|| "Invalid file name".to_string())?;
        let destination = target.join(name);
        if metadata.is_dir() {
            fs::create_dir(&destination).map_err(io)?;
            copy_tree(&path, &destination, job)?;
        } else if metadata.is_file() {
            copy_file(&path, &destination, metadata.len(), job)?;
        } else {
            return Err(Stop::Failed(format!(
                "{} is not a regular file.",
                display_path(&path)
            )));
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path, len: u64, job: &mut Job) -> Result<(), Stop> {
    let mut input = fs::File::open(source).map_err(io)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(io)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    let mut copied = 0_u64;
    loop {
        job.check()?;
        let count = input.read(&mut buffer).map_err(io)?;
        if count == 0 {
            break;
        }
        copied += count as u64;
        if copied > len {
            break;
        }
        output.write_all(&buffer[..count]).map_err(io)?;
        digest.update(&buffer[..count]);
        job.progress.bytes += count as u64;
        (job.report)(job.progress);
    }
    if copied != len {
        return Err(Stop::Failed(format!(
            "{} changed while it was being copied.",
            display_path(source)
        )));
    }
    output.sync_all().map_err(io)?;
    drop(output);
    injected_fault()?;
    if hash_file(target)? != (len, digest.finalize().to_vec()) {
        return Err(Stop::Failed(format!(
            "The copy of {} doesn't match the original.",
            display_path(source)
        )));
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<(u64, Vec<u8>), String> {
    let mut file = fs::File::open(path).map_err(io)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    let mut len = 0;
    loop {
        let count = file.read(&mut buffer).map_err(io)?;
        if count == 0 {
            return Ok((len, digest.finalize().to_vec()));
        }
        len += count as u64;
        digest.update(&buffer[..count]);
    }
}

fn remove_staging(root: &Path) -> Result<(), String> {
    let staging = root.join(STAGING);
    if staging.exists() {
        crate::managed::check_tree(&staging)?;
        fs::remove_dir_all(&staging).map_err(io)?;
    }
    Ok(())
}

fn rollback(to: &Path) -> Result<(), String> {
    let undo = || -> Result<(), String> {
        if !to.exists() {
            return Ok(());
        }
        for folder in owned_folders(to)? {
            injected_fault()?;
            crate::managed::check_tree(&folder)?;
            fs::remove_dir_all(&folder).map_err(io)?;
        }
        remove_staging(to)?;
        let _ = fs::remove_dir(to);
        Ok(())
    };
    undo().map_err(|error| format!("Some copies may be left in {}: {error}", display_path(to)))
}

fn moved(path: &Path, from: &[Vec<String>], to: &Path) -> Option<PathBuf> {
    let id = id_under(path, from)?;
    fs::canonicalize(to.join(id).join(path.file_name()?)).ok()
}

fn id_under<'a>(path: &'a Path, from: &[Vec<String>]) -> Option<&'a str> {
    let folder = path.parent()?;
    let id = folder.file_name()?.to_str()?;
    let root = parts(folder.parent()?);
    let same = |base: &Vec<String>| {
        base.len() == root.len()
            && base
                .iter()
                .zip(&root)
                .all(|(a, b)| crate::recent::same_path(Path::new(a), Path::new(b)))
    };
    (id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()) && from.iter().any(same))
        .then_some(id)
}

pub(crate) fn rebase(path: &Path, library: &Path, to: &Path) -> Option<PathBuf> {
    let id = id_under(path, &root_keys(library))?;
    Some(to.join(id).join(path.file_name()?))
}

fn root_keys(root: &Path) -> Vec<Vec<String>> {
    vec![parts(root), parts(&resolved(root))]
}

fn dedupe<T>(entries: &mut Vec<T>, path: impl Fn(&T) -> &Path) {
    let mut kept: Vec<PathBuf> = Vec::new();
    entries.retain(|entry| {
        let path = path(entry);
        let first = !kept.iter().any(|seen| crate::recent::same_path(seen, path));
        if first {
            kept.push(path.to_path_buf());
        }
        first
    });
}

fn finish(from: &Root, to: &Root, positions_copied: bool) -> Result<(), String> {
    let from_path = from.path();
    let to_path = to.path();
    if !to_path.is_dir() {
        return Err(format!(
            "Library folder not found: {}",
            display_path(&to_path)
        ));
    }
    let from_keys = root_keys(&from_path);
    let library_file = crate::library::library_path();
    let recent_file = crate::recent::history_path();
    let mut library = crate::library::load_at(&library_file)?;
    let mut recent = crate::recent::load_at(&recent_file)?;
    if !positions_copied {
        let paths = library
            .iter()
            .map(|entry| &entry.document.path)
            .chain(recent.iter().map(|entry| &entry.path));
        for old in paths {
            if let Some(new) = moved(old, &from_keys, &to_path) {
                injected_fault()?;
                crate::position::copy_saved_records(old, &new)?;
            }
        }
        write(&Record::new(
            to.clone(),
            Some(Move::Cleaning {
                from: from.clone(),
                positions_copied: true,
            }),
        ))?;
    }
    let mut changed = false;
    for entry in &mut library {
        if let Some(new) = moved(&entry.document.path, &from_keys, &to_path) {
            entry.document.path = new;
            changed = true;
        }
    }
    if changed {
        dedupe(&mut library, |entry| &entry.document.path);
        injected_fault()?;
        crate::library::save_at(&library_file, &library)?;
    }
    changed = false;
    for entry in &mut recent {
        if let Some(new) = moved(&entry.path, &from_keys, &to_path) {
            entry.path = new;
            changed = true;
        }
    }
    if changed {
        dedupe(&mut recent, |entry| &entry.path);
        injected_fault()?;
        crate::recent::save_at(&recent_file, &recent)?;
    }
    let mut in_use = false;
    if from_path.is_dir() {
        let library = crate::library::load_at(&library_file)?;
        let recent = crate::recent::load_at(&recent_file)?;
        let referenced = library
            .iter()
            .map(|entry| entry.document.path.as_path())
            .chain(recent.iter().map(|entry| entry.path.as_path()))
            .collect::<Vec<_>>();
        for copy in crate::managed::owned_copies(&from_path)? {
            let folder = copy.parent().ok_or("Invalid library folder")?;
            let Some(id) = folder.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let target = to_path.join(id).join(copy.file_name().unwrap_or_default());
            let mut still_used = referenced
                .iter()
                .filter(|path| id_under(path, &from_keys) == Some(id));
            if let Some(path) = still_used.next() {
                in_use |= moved(path, &from_keys, &to_path).is_some();
                continue;
            }
            if !crate::managed::is_owned_at(&to_path, &target) {
                continue;
            }
            injected_fault()?;
            crate::managed::check_tree(folder)?;
            fs::remove_dir_all(folder).map_err(io)?;
        }
        let _ = fs::remove_dir(&from_path);
    }
    if in_use {
        return Err(format!(
            "Some books in {} were still in use. Retry to finish.",
            display_path(&from_path)
        ));
    }
    write(&Record::new(to.clone(), None))
}

/// Finishes or undoes a restore or a move a crash interrupted. Called once
/// at startup, before the library loads, and by Retry. Returns a notice for
/// the user.
pub fn recover() -> Option<String> {
    // The setting lives inside the profile, so the profile comes back first.
    let notices = [crate::backup::recover_restore(), recover_move()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    (!notices.is_empty()).then(|| notices.join(" "))
}

fn recover_move() -> Option<String> {
    match read() {
        Err(error) => return Some(error),
        Ok(record) if record.moving.is_none() => return None,
        Ok(_) => {}
    }
    let _lock = match MoveLock::take() {
        Ok(Some(lock)) => lock,
        Ok(None) => return None,
        Err(error) => return Some(error),
    };
    let record = match read() {
        Ok(record) => record,
        Err(error) => return Some(error),
    };
    let root = display_path(&record.root.path());
    match &record.moving {
        None => None,
        Some(Move::Copying { to }) => {
            let undone = rollback(&to.path());
            let cleared = write(&Record::new(record.root.clone(), None));
            Some(match (undone, cleared) {
                (_, Err(error)) => format!(
                    "A library move was interrupted and couldn't be undone: {error}. Retry in Settings."
                ),
                (Err(error), Ok(())) => format!(
                    "A library move was interrupted. Your books are still in {root}. {error}"
                ),
                (Ok(()), Ok(())) => {
                    format!("A library move was interrupted. Your books are still in {root}.")
                }
            })
        }
        Some(Move::Cleaning {
            from,
            positions_copied,
        }) => Some(match finish(from, &record.root, *positions_copied) {
            Ok(()) => format!("Finished moving your library to {root}."),
            Err(error) => {
                format!("Couldn't finish moving the library to {root}: {error} Retry in Settings.")
            }
        }),
    }
}

/// "Find it…": the unavailable library folder is now at `picked` (another
/// drive letter, a renamed parent). Accepted only when it holds a copy for
/// every card and Recent entry; nothing is copied.
pub fn relink(picked: &Path) -> Result<usize, String> {
    let _lock = MoveLock::take()?.ok_or(BUSY)?;
    let record = read()?;
    let Root::Folder(old) = &record.root else {
        return Err("The library uses the default folder.".into());
    };
    if record.moving.is_some() {
        return Err("Finish the library move first.".into());
    }
    if old.is_dir() {
        return Err(format!(
            "The library folder {} is connected.",
            display_path(old)
        ));
    }
    let named = picked
        .file_name()
        .is_some_and(|name| crate::recent::same_path(Path::new(name), Path::new(FOLDER)));
    let candidate = if named {
        picked.to_path_buf()
    } else {
        picked.join(FOLDER)
    };
    if !candidate.is_dir() || crate::managed::regular(&candidate).is_err() {
        return Err(format!(
            "{} has no simPl Library folder.",
            display_path(picked)
        ));
    }
    let new = fs::canonicalize(&candidate).map_err(io)?;
    if network(&new) {
        return Err("Network folders can't hold the library. Choose a folder on this PC.".into());
    }
    let resolved_new = resolved(&new);
    if overlaps(&resolved_new, &resolved(&profile())) {
        return Err("Choose a folder outside simPl's own data folder.".into());
    }
    let keys = root_keys(old);
    let library = crate::library::load_at(&crate::library::library_path())?;
    let recent = crate::recent::load_at(&crate::recent::history_path())?;
    let mut books: Vec<PathBuf> = Vec::new();
    for path in library
        .iter()
        .map(|entry| &entry.document.path)
        .chain(recent.iter().map(|entry| &entry.path))
    {
        if let Some(id) = id_under(path, &keys) {
            let book = PathBuf::from(id).join(path.file_name().unwrap_or_default());
            if !books
                .iter()
                .any(|known| crate::recent::same_path(known, &book))
            {
                books.push(book);
            }
        }
    }
    let missing = books
        .iter()
        .filter(|book| {
            let copy = new.join(book);
            !(crate::managed::is_owned_at(&new, &copy)
                && crate::managed::regular(&copy).is_ok_and(|file| file.is_file()))
        })
        .count();
    if missing > 0 {
        return Err(format!(
            "{missing} of {} books are not in {}. Choose the folder that holds your simPl Library.",
            books.len(),
            display_path(&new)
        ));
    }
    let to = Root::Folder(new);
    let from = record.root.clone();
    write(&Record::new(
        to.clone(),
        Some(Move::Cleaning {
            from: from.clone(),
            positions_copied: false,
        }),
    ))?;
    finish(&from, &to, false)?;
    Ok(books.len())
}

fn injected_fault() -> Result<(), String> {
    #[cfg(test)]
    tests::fault()?;
    Ok(())
}

/// The path without the `\\?\` prefix, for the UI and Explorer.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::process::Command;

    const STORE: &str = "SIMPL_LIBRARY_LOCATION_STORE";
    const PREFIX: &str = "simpl-location-";

    fn run_isolated(name: &str) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "{PREFIX}{}-{}-{nanos}",
            name.rsplit("::").nth(1).unwrap_or(name),
            std::process::id()
        ));
        fs::create_dir_all(root.join("profile")).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                name,
                "--test-threads=1",
                "--nocapture",
            ])
            .env("LOCALAPPDATA", root.join("profile"))
            .env(STORE, &root)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{name} failed; files kept at {}\n{stdout}\n{}",
            root.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_dir_all(&root).unwrap();
    }

    pub(crate) fn store() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(std::env::var_os(STORE).expect("run through run_isolated"));
        assert!(
            root.parent() == Some(std::env::temp_dir().as_path())
                && root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(PREFIX)),
            "isolated store must be a fresh temp folder, not {}",
            root.display()
        );
        let base = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
        assert!(base.starts_with(&root), "{}", base.display());
        let scratch = root.join("scratch");
        fs::create_dir_all(&scratch).unwrap();
        (base, scratch)
    }

    macro_rules! isolated {
        ($name:ident) => {
            mod $name {
                #[test]
                fn run() {
                    let path = module_path!().split_once("::").unwrap().1;
                    super::run_isolated(&format!("{path}::child"));
                }

                #[test]
                #[ignore = "runs in an isolated profile through run()"]
                fn child() {
                    super::$name();
                }
            }
        };
    }

    fn point_at(root: &Root) {
        fs::create_dir_all(profile()).unwrap();
        fs::write(
            profile().join(FILE),
            serde_json::to_vec(&Record::new(root.clone(), None)).unwrap(),
        )
        .unwrap();
    }

    isolated!(no_location_file_keeps_the_default_library);
    fn no_location_file_keeps_the_default_library() {
        let (base, scratch) = store();
        let documents = base.join("simPl").join("documents");
        assert_eq!(crate::managed::root().unwrap(), documents);
        let source = scratch.join("Notes.md");
        fs::write(&source, "# Notes\n").unwrap();
        let copy = crate::managed::import(&source).unwrap();
        assert_eq!(copy.parent().unwrap().parent().unwrap(), documents);
        assert_eq!(copy.file_name().unwrap(), "Notes.html");
        assert_eq!(
            crate::managed::source_kind(&copy),
            Some(crate::library::SourceFormat::Markdown)
        );
        assert_eq!(
            location().unwrap(),
            Location {
                root: documents.clone(),
                custom: false,
                available: true,
                moving: false
            }
        );
        crate::managed::remove(&copy).unwrap();
        assert!(!copy.parent().unwrap().exists());
        assert!(source.is_file());
        assert!(!base.join("simPl").join(FILE).exists());
    }

    isolated!(a_missing_custom_folder_is_never_recreated);
    fn a_missing_custom_folder_is_never_recreated() {
        let (base, scratch) = store();
        let missing = scratch.join("E").join(FOLDER);
        point_at(&Root::Folder(missing.clone()));
        let source = scratch.join("book.pdf");
        fs::write(&source, b"%PDF-1.4 bytes").unwrap();

        assert_eq!(
            crate::managed::import(&source).unwrap_err(),
            format!("Library folder not found: {}", missing.display())
        );
        assert!(!scratch.join("E").exists());
        assert!(!base.join("simPl").join("documents").exists());
        assert_eq!(crate::managed::root().unwrap(), missing);
        let location = location().unwrap();
        assert_eq!((location.custom, location.available), (true, false));

        forget().unwrap();
        assert_eq!(
            crate::managed::root().unwrap(),
            base.join("simPl").join("documents")
        );
        assert_eq!(
            forget().unwrap_err(),
            "The library already uses the default folder."
        );
        assert!(!scratch.join("E").exists());
    }

    isolated!(an_unreadable_location_fails_closed);
    fn an_unreadable_location_fails_closed() {
        let (base, scratch) = store();
        let documents = base.join("simPl").join("documents");
        fs::create_dir_all(&documents).unwrap();
        let source = scratch.join("Story.txt");
        fs::write(&source, "Once.\n").unwrap();
        let copy = crate::managed::import(&source).unwrap();
        fs::write(base.join("simPl").join(FILE), b"{\"version\":1,\"root\"").unwrap();

        let error = crate::managed::import(&source).unwrap_err();
        assert!(
            error.starts_with("Library settings could not be read: "),
            "{error}"
        );
        assert_eq!(crate::managed::remove(&copy).unwrap_err(), error);
        assert!(copy.is_file());
        assert_eq!(crate::managed::source_kind(&copy), None);
        assert_eq!(location().unwrap_err(), error);
        assert_eq!(forget().unwrap_err(), error);
        assert_eq!(
            fs::read(base.join("simPl").join(FILE)).unwrap(),
            b"{\"version\":1,\"root\""
        );
    }

    pub(crate) fn import(scratch: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let source = scratch.join("sources").join(name);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, bytes).unwrap();
        fs::canonicalize(crate::managed::import(&source).unwrap()).unwrap()
    }

    pub(crate) fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let target = to.join(path.file_name().unwrap());
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                fs::copy(&path, &target).unwrap();
            }
        }
    }

    fn junction(link: &Path, target: &Path) {
        let status = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status;
        assert!(status.success());
    }

    isolated!(plan_counts_books_and_refuses_unsafe_targets);
    fn plan_counts_books_and_refuses_unsafe_targets() {
        let (base, scratch) = store();
        let profile = base.join("simPl");
        import(&scratch, "one.pdf", b"%PDF one");
        import(&scratch, "two.epub", b"epub two bytes");
        let picked = scratch.join("E");
        fs::create_dir(&picked).unwrap();
        let refused = |target: Target| plan(target).unwrap_err();

        let plan_e = plan(Target::Folder(picked.clone())).unwrap();
        let library = fs::canonicalize(&picked).unwrap().join(FOLDER);
        assert_eq!(plan_e.to(), library);
        assert_eq!(
            plan_e.summary(),
            Summary {
                books: 2,
                bytes: (8 + 7) + (14 + 8)
            }
        );
        assert!(!library.exists());

        assert_eq!(
            refused(Target::Default),
            "This is already your library folder."
        );
        assert_eq!(
            refused(Target::Folder(scratch.join("missing"))),
            format!(
                "The folder {} doesn't exist.",
                scratch.join("missing").display()
            )
        );
        assert_eq!(
            refused(Target::Folder(profile.join("documents"))),
            "The new library folder can't be inside the current one or contain it."
        );
        assert_eq!(
            refused(Target::Folder(profile.clone())),
            "Choose a folder outside simPl's own data folder."
        );
        fs::create_dir_all(profile.join("covers")).unwrap();
        assert_eq!(
            refused(Target::Folder(profile.join("covers"))),
            "Choose a folder outside simPl's own data folder."
        );
        let install = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        assert_eq!(
            refused(Target::Folder(install)),
            "Choose a folder outside the folder simPl is installed in."
        );
        let link = scratch.join("link");
        junction(&link, &picked);
        assert_eq!(
            refused(Target::Folder(link.clone())),
            format!(
                "{} is a link or junction. Choose the folder it points to.",
                link.display()
            )
        );
        let file = scratch.join("file.txt");
        fs::write(&file, b"x").unwrap();
        assert_eq!(
            refused(Target::Folder(file.clone())),
            format!("{} is not a folder.", file.display())
        );

        let staged = scratch.join("staged");
        let notes = staged.join(FOLDER).join(STAGING).join("notes.txt");
        fs::create_dir_all(notes.parent().unwrap()).unwrap();
        fs::write(&notes, b"mine").unwrap();
        assert_eq!(
            refused(Target::Folder(staged.clone())),
            format!(
                "{} already has a .incoming folder. Choose another folder.",
                display_path(&fs::canonicalize(staged.join(FOLDER)).unwrap())
            )
        );
        assert_eq!(fs::read(&notes).unwrap(), b"mine");

        let used = scratch.join("used");
        let copy = import(&scratch, "three.pdf", b"%PDF three");
        let id = copy.parent().unwrap().file_name().unwrap();
        copy_dir(copy.parent().unwrap(), &used.join(FOLDER).join(id));
        assert_eq!(
            refused(Target::Folder(used.clone())),
            format!(
                "{} already has simPl books in it. Choose another folder.",
                display_path(&fs::canonicalize(&used).unwrap().join(FOLDER))
            )
        );
        let unowned = scratch.join("unowned");
        fs::create_dir_all(unowned.join(FOLDER).join("a".repeat(64))).unwrap();
        fs::write(unowned.join(FOLDER).join("notes.txt"), b"mine").unwrap();
        assert_eq!(plan(Target::Folder(unowned)).unwrap().summary().books, 3);

        point_at(&Root::Folder(library.clone()));
        fs::create_dir(&library).unwrap();
        assert_eq!(
            refused(Target::Folder(picked.clone())),
            "This is already your library folder."
        );
        fs::create_dir(library.join("inner")).unwrap();
        assert_eq!(
            refused(Target::Folder(library.join("inner"))),
            "The new library folder can't be inside the current one or contain it."
        );
        assert_eq!(
            refused(Target::Default),
            format!(
                "{} already has simPl books in it. Choose another folder.",
                profile.join("documents").display()
            )
        );

        fs::remove_dir_all(&library).unwrap();
        assert_eq!(
            refused(Target::Default),
            format!("Library folder not found: {}", display_path(&library))
        );
        fs::write(
            profile.join(FILE),
            serde_json::to_vec(&Record::new(
                Root::Default,
                Some(Move::Copying {
                    to: Root::Folder(library),
                }),
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            refused(Target::Folder(picked)),
            "Finish the library move first."
        );
    }

    #[cfg(windows)]
    #[test]
    fn network_paths_and_nesting_compare_like_windows() {
        assert!(network(Path::new(r"\\?\UNC\server\share\Books")));
        assert!(!network(Path::new(r"\\?\C:\Books")));
        assert!(within(
            Path::new(r"\\?\C:\Users\A\simPl\documents"),
            Path::new(r"c:\users\a\SIMPL")
        ));
        assert!(!within(
            Path::new(r"C:\Users\A\simPl-old"),
            Path::new(r"C:\Users\A\simPl")
        ));
        assert!(!within(Path::new(r"C:\Books"), Path::new(r"D:\Books")));
    }
    #[derive(Clone, Copy, Debug)]
    enum Mode {
        Crash,
        Error,
    }

    thread_local! {
        static STEP: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        static ARMED: std::cell::Cell<Option<(usize, Mode)>> = const { std::cell::Cell::new(None) };
        static FIRED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    pub(super) fn fault() -> Result<(), String> {
        let step = STEP.get();
        STEP.set(step + 1);
        match ARMED.get() {
            Some((at, mode)) if at == step => {
                ARMED.set(None);
                FIRED.set(true);
                match mode {
                    Mode::Crash => panic!("simulated crash at step {step}"),
                    Mode::Error => Err(format!("simulated failure at step {step}")),
                }
            }
            _ => Ok(()),
        }
    }

    fn arm(step: usize, mode: Mode) {
        STEP.set(0);
        FIRED.set(false);
        ARMED.set(Some((step, mode)));
    }

    fn disarm() -> bool {
        ARMED.set(None);
        FIRED.get()
    }

    const FINGERPRINT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct Seeded {
        books: Vec<(&'static str, Vec<u8>)>,
        legacy: PathBuf,
        in_place: PathBuf,
    }

    fn entry(path: &Path, title: &str) -> crate::recent::Entry {
        crate::recent::Entry {
            path: path.to_path_buf(),
            title: title.into(),
            fingerprint: FINGERPRINT.into(),
            kind: crate::recent::DocumentKind::from_path(path).unwrap(),
        }
    }

    fn card(path: &Path, title: &str) -> crate::library::Entry {
        crate::library::Entry {
            document: entry(path, title),
            author: None,
            byte_len: 1,
            opened_at: 1,
            progress: 0.5,
            current: 1,
            total: 2,
            cover: false,
            favourite: title == "Story",
            source_kind: None,
        }
    }

    fn reflow(item: &str) -> crate::position::ReadingPosition {
        crate::position::ReadingPosition {
            fingerprint: FINGERPRINT.into(),
            item_id: item.into(),
            within: 0.25,
            font_size: 20.0,
        }
    }

    fn seed(scratch: &Path) -> Seeded {
        let story = b"<title>Story</title><p>Once upon a time.</p>".to_vec();
        let novel = b"PK novel bytes".to_vec();
        let big = (0..200 * 1024).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let html = import(scratch, "story.html", &story);
        let epub = import(scratch, "novel.epub", &novel);
        let pdf = import(scratch, "big.pdf", &big);
        let legacy = scratch.join("legacy.pdf");
        fs::write(&legacy, b"%PDF legacy").unwrap();
        let legacy = fs::canonicalize(legacy).unwrap();
        let in_place = scratch.join("open.html");
        fs::write(&in_place, b"<p>in place</p>").unwrap();
        let in_place = fs::canonicalize(in_place).unwrap();
        crate::library::save(&[
            card(&html, "Story"),
            card(&epub, "Novel"),
            card(&pdf, "Big"),
            card(&legacy, "Legacy"),
        ])
        .unwrap();
        crate::recent::save(&[
            entry(&pdf, "Big"),
            entry(&in_place, "Open"),
            entry(&html, "Story"),
        ])
        .unwrap();
        crate::position::save(&html, &reflow("html-item")).unwrap();
        crate::position::save(&in_place, &reflow("in-place-item")).unwrap();
        crate::position::save_epub(
            &epub,
            &crate::position::EpubReadingPosition {
                fingerprint: FINGERPRINT.into(),
                chapter: "OPS/ch2.xhtml".into(),
                item_id: "epub-item".into(),
                within: 0.5,
                font_size: 18.0,
            },
        )
        .unwrap();
        crate::position::save_pdf(
            &pdf,
            &crate::position::PdfReadingPosition {
                fingerprint: FINGERPRINT.into(),
                page: 7,
                within: 0.5,
                horizontal: 0.0,
                zoom: crate::position::PdfZoom::FitWidth,
            },
        )
        .unwrap();
        Seeded {
            books: vec![("Story", story), ("Novel", novel), ("Big", big)],
            legacy,
            in_place,
        }
    }

    fn cards() -> Vec<crate::library::Entry> {
        crate::library::load_at(&crate::library::library_path()).unwrap()
    }

    fn history() -> Vec<crate::recent::Entry> {
        crate::recent::load_at(&crate::recent::history_path()).unwrap()
    }

    fn path_of(title: &str) -> PathBuf {
        cards()
            .into_iter()
            .find(|card| card.document.title == title)
            .unwrap()
            .document
            .path
    }

    fn assert_converged(seeded: &Seeded, other: &Path, story_item: &str) {
        let location = location().unwrap();
        assert!(!location.moving);
        let root = fs::canonicalize(&location.root).unwrap();
        let cards = cards();
        assert_eq!(cards.len(), 4);
        for card in &cards {
            assert!(
                card.document.path.is_file(),
                "{}",
                card.document.path.display()
            );
        }
        for entry in history() {
            assert!(entry.path.is_file(), "{}", entry.path.display());
        }
        for (title, bytes) in &seeded.books {
            let path = path_of(title);
            assert_eq!(&fs::read(&path).unwrap(), bytes, "{title}");
            assert_eq!(path.parent().unwrap().parent().unwrap(), root, "{title}");
        }
        assert_eq!(path_of("Legacy"), seeded.legacy);
        assert!(history().iter().any(|entry| entry.path == seeded.in_place));
        assert_eq!(crate::managed::owned_copies(&root).unwrap().len(), 3);
        assert_eq!(
            crate::managed::owned_copies(other).unwrap(),
            Vec::<PathBuf>::new()
        );
        assert!(!root.join(STAGING).exists());
        assert_eq!(
            crate::position::load(&path_of("Story"))
                .unwrap()
                .unwrap()
                .item_id,
            story_item
        );
        assert_eq!(
            crate::position::load_pdf(&path_of("Big"))
                .unwrap()
                .unwrap()
                .page,
            7
        );
    }

    fn quiet() -> AtomicBool {
        AtomicBool::new(false)
    }

    isolated!(a_move_copies_verifies_and_rewrites_every_path);
    fn a_move_copies_verifies_and_rewrites_every_path() {
        let (base, scratch) = store();
        let documents = base.join("simPl").join("documents");
        let seeded = seed(&scratch);
        let picked = scratch.join("E");
        fs::create_dir(&picked).unwrap();
        let plan = plan(Target::Folder(picked.clone())).unwrap();
        let total = plan.summary();
        let mut reports = Vec::new();

        let outcome = run_move(&plan, &mut |progress| reports.push(progress), &quiet()).unwrap();

        let library = fs::canonicalize(&picked).unwrap().join(FOLDER);
        assert_eq!(
            outcome,
            Outcome::Moved {
                to: library.clone(),
                books: 3
            }
        );
        assert_eq!(
            reports.first(),
            Some(&Progress {
                books: 0,
                books_total: 3,
                bytes: 0,
                bytes_total: total.bytes
            })
        );
        assert_eq!(
            reports.last(),
            Some(&Progress {
                books: 3,
                books_total: 3,
                bytes: total.bytes,
                bytes_total: total.bytes
            })
        );
        assert_eq!(
            location().unwrap(),
            Location {
                root: library.clone(),
                custom: true,
                available: true,
                moving: false
            }
        );
        assert_converged(&seeded, &documents, "html-item");
        assert!(!documents.exists());
        let epub = path_of("Novel");
        assert_eq!(
            crate::position::load_epub(&epub).unwrap().unwrap().chapter,
            "OPS/ch2.xhtml"
        );
        let recent = history();
        assert_eq!(
            recent
                .iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
            [path_of("Big"), seeded.in_place.clone(), path_of("Story")]
        );
        assert_eq!(
            crate::position::load(&seeded.in_place)
                .unwrap()
                .unwrap()
                .item_id,
            "in-place-item"
        );
        assert!(
            cards()
                .iter()
                .any(|card| card.favourite && card.document.title == "Story")
        );
        assert_eq!(summary().unwrap(), total);
    }

    isolated!(a_round_trip_keeps_the_latest_position);
    fn a_round_trip_keeps_the_latest_position() {
        let (base, scratch) = store();
        let documents = base.join("simPl").join("documents");
        let seeded = seed(&scratch);
        let picked = scratch.join("E");
        fs::create_dir(&picked).unwrap();
        let away = plan(Target::Folder(picked.clone())).unwrap();
        run_move(&away, &mut |_| {}, &quiet()).unwrap();
        crate::position::save(&path_of("Story"), &reflow("read-later")).unwrap();

        let back = plan(Target::Default).unwrap();
        assert_eq!(back.to(), documents);
        let outcome = run_move(&back, &mut |_| {}, &quiet()).unwrap();

        assert_eq!(
            outcome,
            Outcome::Moved {
                to: documents.clone(),
                books: 3
            }
        );
        let story = path_of("Story");
        assert_eq!(
            story.parent().unwrap().parent().unwrap(),
            fs::canonicalize(&documents).unwrap()
        );
        assert_eq!(
            crate::position::load(&story).unwrap().unwrap().item_id,
            "read-later"
        );
        assert!(!location().unwrap().custom);
        assert!(!picked.join(FOLDER).exists());
        assert_eq!(fs::read(path_of("Big")).unwrap(), seeded.books[2].1);
    }

    isolated!(cancel_leaves_the_library_where_it_was);
    fn cancel_leaves_the_library_where_it_was() {
        let (base, scratch) = store();
        let documents = base.join("simPl").join("documents");
        seed(&scratch);
        let library_json = fs::read(crate::library::library_path()).unwrap();
        let recent_json = fs::read(crate::recent::history_path()).unwrap();
        let copies = crate::managed::owned_copies(&documents).unwrap();
        let picked = scratch.join("E");
        fs::create_dir(&picked).unwrap();
        type When = fn(&Progress) -> bool;
        let cases: [(&str, When); 3] = [
            ("before the first file", |_| true),
            ("mid-book", |progress| progress.bytes >= CHUNK as u64),
            ("after the last book", |progress| {
                progress.books == progress.books_total
            }),
        ];
        for (case, when) in cases {
            let plan = plan(Target::Folder(picked.clone())).unwrap();
            let cancel = quiet();
            let mut at = None;
            let outcome = run_move(
                &plan,
                &mut |progress| {
                    if at.is_none() && when(&progress) {
                        at = Some(progress);
                        cancel.store(true, Ordering::Relaxed);
                    }
                },
                &cancel,
            )
            .unwrap();
            assert_eq!(
                outcome,
                Outcome::Cancelled {
                    kept: documents.clone()
                },
                "{case}"
            );
            let at = at.unwrap();
            match case {
                "before the first file" => assert_eq!(at.bytes, 0),
                "mid-book" => assert!(at.books < 3 && at.bytes < at.bytes_total, "{at:?}"),
                _ => assert_eq!(at.bytes, at.bytes_total),
            }
            assert_eq!(
                location().unwrap(),
                Location {
                    root: documents.clone(),
                    custom: false,
                    available: true,
                    moving: false
                },
                "{case}"
            );
            assert_eq!(crate::managed::owned_copies(&documents).unwrap(), copies);
            assert_eq!(
                fs::read(crate::library::library_path()).unwrap(),
                library_json
            );
            assert_eq!(
                fs::read(crate::recent::history_path()).unwrap(),
                recent_json
            );
            assert!(!picked.join(FOLDER).exists(), "{case}");
            assert!(picked.is_dir());
        }
    }

    isolated!(every_interrupted_step_converges);
    fn every_interrupted_step_converges() {
        let (_, scratch) = store();
        std::panic::set_hook(Box::new(|_| {}));
        for back in [false, true] {
            for mode in [Mode::Crash, Mode::Error] {
                for step in 0.. {
                    let run = scratch.join(format!("{back}-{mode:?}-{step}"));
                    // SAFETY: the isolated child runs this one test on one thread.
                    unsafe { std::env::set_var("LOCALAPPDATA", run.join("profile")) };
                    let documents = default_root();
                    let seeded = seed(&run.join("books"));
                    let picked = run.join("E");
                    fs::create_dir(&picked).unwrap();
                    let away = plan(Target::Folder(picked.clone())).unwrap();
                    let target = if back {
                        run_move(&away, &mut |_| {}, &quiet()).unwrap();
                        plan(Target::Default).unwrap()
                    } else {
                        away
                    };
                    let custom = fs::canonicalize(&picked).unwrap().join(FOLDER);
                    let (from, to) = if back {
                        (custom, documents.clone())
                    } else {
                        (documents.clone(), custom)
                    };

                    arm(step, mode);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_move(&target, &mut |_| {}, &quiet())
                    }));
                    let fired = disarm();
                    recover();

                    assert_eq!(recover(), None, "{back} {mode:?} {step} {result:?}");
                    let root = location().unwrap().root;
                    let other = if root == from { &to } else { &from };
                    assert!(root == from || root == to, "{}", root.display());
                    assert_converged(&seeded, other, "html-item");
                    if !fired {
                        assert!(step > 20, "only {step} steps");
                        assert_eq!(root, to);
                        break;
                    }
                }
            }
        }
    }

    fn journal(record: &Record) {
        fs::write(profile().join(FILE), serde_json::to_vec(record).unwrap()).unwrap();
    }

    isolated!(recovery_undoes_copying_and_finishes_cleaning);
    fn recovery_undoes_copying_and_finishes_cleaning() {
        let (base, scratch) = store();
        let documents = base.join("simPl").join("documents");
        let seeded = seed(&scratch);
        let library = scratch.join("E").join(FOLDER);
        let gone = scratch.join("gone").join(FOLDER);
        let copies = crate::managed::owned_copies(&documents).unwrap();
        let copy_all = || {
            for copy in &copies {
                let folder = copy.parent().unwrap();
                copy_dir(folder, &library.join(folder.file_name().unwrap()));
            }
        };

        copy_all();
        fs::create_dir_all(library.join(STAGING).join("partial")).unwrap();
        fs::write(library.join("mine.txt"), b"user file").unwrap();
        journal(&Record::new(
            Root::Default,
            Some(Move::Copying {
                to: Root::Folder(library.clone()),
            }),
        ));
        assert_eq!(
            recover().unwrap(),
            format!(
                "A library move was interrupted. Your books are still in {}.",
                documents.display()
            )
        );
        assert_eq!(recover(), None);
        assert_eq!(names(&library), ["mine.txt"]);
        assert_converged(&seeded, &library, "html-item");
        fs::remove_file(library.join("mine.txt")).unwrap();

        journal(&Record::new(
            Root::Default,
            Some(Move::Copying {
                to: Root::Folder(gone.clone()),
            }),
        ));
        assert_eq!(
            recover().unwrap(),
            format!(
                "A library move was interrupted. Your books are still in {}.",
                documents.display()
            )
        );
        assert!(!location().unwrap().moving);

        copy_all();
        let library = fs::canonicalize(&library).unwrap();
        journal(&Record::new(
            Root::Folder(library.clone()),
            Some(Move::Cleaning {
                from: Root::Default,
                positions_copied: false,
            }),
        ));
        assert_eq!(
            recover().unwrap(),
            format!(
                "Finished moving your library to {}.",
                display_path(&library)
            )
        );
        assert_eq!(recover(), None);
        assert_converged(&seeded, &documents, "html-item");

        let before = (
            fs::read(crate::library::library_path()).unwrap(),
            fs::read(crate::recent::history_path()).unwrap(),
        );
        let stuck = Record::new(
            Root::Folder(gone.clone()),
            Some(Move::Cleaning {
                from: Root::Folder(library.clone()),
                positions_copied: false,
            }),
        );
        journal(&stuck);
        for _ in 0..2 {
            assert_eq!(
                recover().unwrap(),
                format!(
                    "Couldn't finish moving the library to {}: Library folder not found: {} Retry in Settings.",
                    gone.display(),
                    gone.display()
                )
            );
            assert_eq!(read().unwrap(), stuck);
            assert_eq!(
                (
                    fs::read(crate::library::library_path()).unwrap(),
                    fs::read(crate::recent::history_path()).unwrap(),
                ),
                before
            );
        }
        assert_eq!(
            plan(Target::Default).unwrap_err(),
            "Finish the library move first."
        );
        forget().unwrap();
        assert_eq!(read().unwrap(), Record::new(Root::Default, None));
        assert!(path_of("Story").is_file());
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    isolated!(find_it_repoints_a_moved_library_folder);
    fn find_it_repoints_a_moved_library_folder() {
        let (_, scratch) = store();
        let seeded = seed(&scratch);
        let picked = scratch.join("E");
        fs::create_dir(&picked).unwrap();
        let away = plan(Target::Folder(picked.clone())).unwrap();
        run_move(&away, &mut |_| {}, &quiet()).unwrap();
        crate::position::save(&path_of("Story"), &reflow("on-e")).unwrap();
        let renamed = scratch.join("F");
        fs::rename(&picked, &renamed).unwrap();
        assert!(!location().unwrap().available);

        let story = path_of("Story");
        let id = story.parent().unwrap().file_name().unwrap().to_owned();
        let aside = scratch.join("aside");
        fs::rename(renamed.join(FOLDER).join(&id), &aside).unwrap();
        assert_eq!(
            relink(&renamed).unwrap_err(),
            format!(
                "1 of 3 books are not in {}. Choose the folder that holds your simPl Library.",
                display_path(&fs::canonicalize(renamed.join(FOLDER)).unwrap())
            )
        );
        fs::rename(&aside, renamed.join(FOLDER).join(&id)).unwrap();

        let book = renamed
            .join(FOLDER)
            .join(&id)
            .join(story.file_name().unwrap());
        let kept = scratch.join("kept");
        fs::rename(&book, &kept).unwrap();
        assert_eq!(
            relink(&renamed).unwrap_err(),
            format!(
                "1 of 3 books are not in {}. Choose the folder that holds your simPl Library.",
                display_path(&fs::canonicalize(renamed.join(FOLDER)).unwrap())
            )
        );
        fs::rename(&kept, &book).unwrap();

        assert_eq!(relink(&renamed).unwrap(), 3);

        let library = fs::canonicalize(renamed.join(FOLDER)).unwrap();
        assert_eq!(location().unwrap().root, library);
        assert_converged(&seeded, &picked, "on-e");
    }

    fn archived(zip: &Path, name: &str) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(fs::File::open(zip).unwrap()).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name(name)
            .unwrap_or_else(|_| panic!("{name} is not in the backup"))
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    }

    fn archived_names(zip: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(fs::File::open(zip).unwrap()).unwrap();
        let mut names = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn custom_library(scratch: &Path) -> (Seeded, PathBuf) {
        let seeded = seed(scratch);
        let picked = scratch.join("E");
        fs::create_dir(&picked).unwrap();
        let plan = plan(Target::Folder(picked)).unwrap();
        run_move(&plan, &mut |_| {}, &quiet()).unwrap();
        (seeded, plan.to().to_path_buf())
    }

    const BOOKS: crate::backup::Options = crate::backup::Options {
        documents: true,
        dictionaries: false,
    };

    isolated!(a_custom_library_backs_up_as_a_default_layout_archive);
    fn a_custom_library_backs_up_as_a_default_layout_archive() {
        let (base, scratch) = store();
        let (seeded, library) = custom_library(&scratch);
        crate::position::save(&path_of("Story"), &reflow("on-e")).unwrap();
        fs::write(library.join("user notes.txt"), b"not a book").unwrap();
        let zip = scratch.join("backup.zip");

        let estimate = crate::backup::estimate(BOOKS).unwrap();
        let created = crate::backup::create(&zip, BOOKS).unwrap();

        assert_eq!(
            (created.files, created.bytes),
            (estimate.files, estimate.bytes)
        );
        let source_root = fs::canonicalize(base.join("simPl")).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&archived(&zip, "manifest.json")).unwrap();
        assert_eq!(manifest["version"], 1);
        assert_eq!(manifest["source_root"], source_root.to_str().unwrap());
        let virtual_path = |title: &str| {
            let path = path_of(title);
            let folder = path.parent().unwrap();
            source_root
                .join("documents")
                .join(folder.file_name().unwrap())
                .join(path.file_name().unwrap())
        };
        let books = archived_names(&zip)
            .into_iter()
            .filter(|name| name.starts_with("documents/"))
            .collect::<Vec<_>>();
        let mut expected = Vec::new();
        for (title, bytes) in &seeded.books {
            let path = path_of(title);
            let id = path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
            let name = path.file_name().unwrap().to_str().unwrap();
            assert_eq!(&archived(&zip, &format!("documents/{id}/{name}")), bytes);
            expected.push(format!("documents/{id}/{name}"));
            expected.push(format!("documents/{id}/{MARKER}"));
        }
        expected.sort();
        assert_eq!(books, expected);
        let cards: serde_json::Value =
            serde_json::from_slice(&archived(&zip, "library.json")).unwrap();
        let paths = cards["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|card| PathBuf::from(card["document"]["path"].as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                virtual_path("Story"),
                virtual_path("Novel"),
                virtual_path("Big"),
                seeded.legacy.clone()
            ]
        );
        let recent: serde_json::Value =
            serde_json::from_slice(&archived(&zip, "recent.json")).unwrap();
        assert_eq!(
            recent["entries"][1]["path"].as_str().unwrap(),
            seeded.in_place.to_str().unwrap()
        );
        let story_key = crate::position::path_key(&virtual_path("Story"));
        let position: serde_json::Value =
            serde_json::from_slice(&archived(&zip, &format!("positions/{story_key}.json")))
                .unwrap();
        assert_eq!(position["position"]["item_id"], "on-e");

        // SAFETY: the isolated child runs this one test on one thread.
        unsafe { std::env::set_var("LOCALAPPDATA", scratch.join("other")) };
        crate::backup::restore(&zip).unwrap();
        let documents = fs::canonicalize(default_root()).unwrap();
        for (title, bytes) in &seeded.books {
            let path = path_of(title);
            assert_eq!(path.parent().unwrap().parent().unwrap(), documents);
            assert_eq!(&fs::read(&path).unwrap(), bytes);
        }
        assert_eq!(
            crate::position::load(&path_of("Story"))
                .unwrap()
                .unwrap()
                .item_id,
            "on-e"
        );
        assert!(!location().unwrap().custom);
    }

    isolated!(including_books_needs_the_library_folder_connected);
    fn including_books_needs_the_library_folder_connected() {
        let (_, scratch) = store();
        let (_, library) = custom_library(&scratch);
        fs::rename(scratch.join("E"), scratch.join("unplugged")).unwrap();
        let zip = scratch.join("backup.zip");
        let error = format!(
            "The library folder {} isn't connected. Connect it or turn off Include book files.",
            display_path(&library)
        );

        assert_eq!(crate::backup::estimate(BOOKS).unwrap_err(), error);
        assert_eq!(crate::backup::create(&zip, BOOKS).unwrap_err(), error);
        assert!(!zip.exists());

        let light = crate::backup::create(&zip, crate::backup::Options::default()).unwrap();
        assert!(!light.documents);
        assert!(
            !archived_names(&zip)
                .iter()
                .any(|name| name.starts_with("documents/"))
        );
    }

    isolated!(restore_adds_books_to_a_custom_library_and_keeps_the_setting);
    fn restore_adds_books_to_a_custom_library_and_keeps_the_setting() {
        let (_, scratch) = store();
        let (seeded, library) = custom_library(&scratch);
        let full = scratch.join("full.zip");
        let light = scratch.join("light.zip");
        crate::backup::create(&full, BOOKS).unwrap();
        crate::backup::create(&light, crate::backup::Options::default()).unwrap();
        let setting = fs::read(profile().join(FILE)).unwrap();
        let folder_of = |title: &str| path_of(title).parent().unwrap().to_path_buf();
        let (story, novel, big) = (folder_of("Story"), folder_of("Novel"), folder_of("Big"));
        let changed = b"%PDF edited after the backup".to_vec();
        fs::remove_dir_all(&novel).unwrap();
        fs::write(path_of("Big"), &changed).unwrap();
        crate::position::save(&path_of("Story"), &reflow("after-backup")).unwrap();

        crate::backup::restore(&full).unwrap();

        assert_eq!(fs::read(profile().join(FILE)).unwrap(), setting);
        assert_eq!(location().unwrap().root, library);
        assert_eq!(folder_of("Story"), story);
        assert_eq!(folder_of("Novel"), novel);
        let placed = folder_of("Big");
        assert_eq!(placed.parent().unwrap(), library);
        assert_ne!(placed, big);
        for (title, bytes) in &seeded.books {
            assert_eq!(&fs::read(path_of(title)).unwrap(), bytes, "{title}");
        }
        assert_eq!(fs::read(big.join("big.pdf")).unwrap(), changed);
        assert_eq!(crate::managed::owned_copies(&library).unwrap().len(), 4);
        assert!(!library.join(STAGING).exists());
        assert!(!default_root().exists());
        assert_eq!(
            crate::position::load(&path_of("Story"))
                .unwrap()
                .unwrap()
                .item_id,
            "html-item"
        );
        assert_eq!(
            crate::position::load_pdf(&path_of("Big"))
                .unwrap()
                .unwrap()
                .page,
            7
        );
        assert_eq!(path_of("Legacy"), seeded.legacy);

        crate::backup::restore(&light).unwrap();

        assert_eq!(fs::read(profile().join(FILE)).unwrap(), setting);
        assert_eq!(folder_of("Story"), story);
        assert_eq!(folder_of("Big"), big);
        assert_eq!(
            crate::position::load(&path_of("Story"))
                .unwrap()
                .unwrap()
                .item_id,
            "html-item"
        );
        assert!(!default_root().exists());

        let before = fs::read(crate::library::library_path()).unwrap();
        fs::rename(scratch.join("E"), scratch.join("unplugged")).unwrap();
        assert_eq!(
            crate::backup::restore(&full).unwrap_err(),
            format!(
                "The library folder {} isn't connected. Connect it before restoring a backup with book files.",
                display_path(&library)
            )
        );
        assert_eq!(fs::read(crate::library::library_path()).unwrap(), before);
        fs::rename(scratch.join("unplugged"), scratch.join("E")).unwrap();

        let pending = Record::new(
            Root::Folder(library.clone()),
            Some(Move::Cleaning {
                from: Root::Default,
                positions_copied: true,
            }),
        );
        journal(&pending);
        assert_eq!(
            crate::backup::restore(&light).unwrap_err(),
            "Finish the library move first."
        );
        assert_eq!(
            crate::backup::create(&scratch.join("again.zip"), BOOKS).unwrap_err(),
            "Finish the library move first."
        );
    }

    #[test]
    fn display_path_strips_the_verbatim_prefix() {
        assert_eq!(
            display_path(Path::new(r"\\?\E:\Books\simPl Library")),
            r"E:\Books\simPl Library"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\x")),
            r"\\server\share\x"
        );
        assert_eq!(display_path(Path::new(r"C:\simPl")), r"C:\simPl");
    }
}
