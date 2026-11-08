//! Where imported copies live. `library-location.json` in the profile names the
//! library root; only a missing file means the default `<profile>\documents`.
//! Settings, positions and history always stay in the profile.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf, Prefix},
};

const FILE: &str = "library-location.json";
const LOCK: &str = "library-location.lock";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 64 * 1024;
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

/// The move journal. One record write moves between states.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Move {
    /// `root` is still the source; verified copies are being placed in `to`.
    Copying { to: Root },
    /// `root` is already the target; paths are rewritten, then `from`'s copies deleted.
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
    let folders = [Some(&record.root)]
        .into_iter()
        .chain(match &record.moving {
            Some(Move::Copying { to }) => [Some(to)],
            Some(Move::Cleaning { from, .. }) => [Some(from)],
            None => [None],
        })
        .flatten();
    for root in folders {
        if let Root::Folder(path) = root
            && !(path.is_absolute() && path.to_str().is_some())
        {
            return Err(unreadable("the folder is not an absolute path".into()));
        }
    }
    Ok(record)
}

fn write(record: &Record) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
    crate::position::atomic_write(
        &profile().join(FILE),
        &bytes,
        "library settings",
        "profile",
        ".library-location",
    )
}

/// Held while a move or recovery changes the library, so two simPl processes
/// never run one at the same time.
struct Lock(#[allow(dead_code)] fs::File);

impl Lock {
    fn take() -> Result<Self, String> {
        fs::create_dir_all(profile()).map_err(|error| error.to_string())?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        options
            .open(profile().join(LOCK))
            .map(Self)
            .map_err(|error| {
                if error.raw_os_error() == Some(32) {
                    "Another simPl window is changing the library.".into()
                } else {
                    format!("Cannot lock the library settings: {error}")
                }
            })
    }
}

/// What `managed::root()` returns.
pub(crate) fn current_root() -> Result<PathBuf, String> {
    Ok(read()?.root.path())
}

/// Imports never recreate a custom folder that went missing.
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
    let _lock = Lock::take()?;
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
        if !crate::managed::owned_copies(&to_path)?.is_empty() {
            return Err(format!(
                "{} already has simPl books in it. Choose another folder.",
                display_path(&to_path)
            ));
        }
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

/// `<picked>\simPl Library`, canonical, on a local drive.
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

/// `path` with its longest existing ancestor canonicalized.
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

/// Whether `path` is `base` or inside it.
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

    /// Runs `name` in a child test process whose `LOCALAPPDATA` is a fresh
    /// temp folder, so nothing here can reach the real profile.
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

    /// The isolated store: `(profile base, scratch folder for books and targets)`.
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

    /// Imports `name` with `bytes` into the current library and returns the copy.
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
