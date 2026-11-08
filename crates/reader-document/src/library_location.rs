//! Where imported copies live. `library-location.json` in the profile names the
//! library root; only a missing file means the default `<profile>\documents`.
//! Settings, positions and history always stay in the profile.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
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
