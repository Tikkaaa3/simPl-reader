//! Versioned, checksummed dictionary files; no network requests in this crate.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageId(pub usize);

#[derive(Debug, Deserialize)]
pub struct Package {
    pub source: Language,
    pub target: Language,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
    pub data_bytes: u64,
    pub data_sha256: String,
    pub provider: String,
    pub entries: usize,
}

impl Package {
    pub fn label(&self) -> String {
        format!("{} → {}", self.source, self.target)
    }
    pub fn size_label(&self) -> String {
        format!("{:.2} MB", self.bytes as f64 / 1_000_000.0)
    }
    pub fn url(&self) -> String {
        format!("{}{}", CATALOG.base_url, self.file)
    }
}

#[derive(Deserialize)]
struct Catalog {
    version: u32,
    data_version: String,
    base_url: String,
    packages: Vec<Package>,
}
static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../assets/dictionaries/catalog.json"))
            .expect("verified dictionary catalog");
    assert_eq!(catalog.version, 1);
    catalog
});
pub fn packages() -> &'static [Package] {
    &CATALOG.packages
}
pub fn package(id: PackageId) -> Option<&'static Package> {
    packages().get(id.0)
}
pub fn package_id(source: Language, target: Language) -> Option<PackageId> {
    packages()
        .iter()
        .position(|p| p.source == source && p.target == target)
        .map(PackageId)
}
pub fn data_version() -> &'static str {
    &CATALOG.data_version
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageState {
    Missing,
    Installed,
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupError {
    Unavailable {
        package: PackageId,
        error: Option<String>,
    },
    Failed(String),
}
impl From<String> for LookupError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}
impl From<&str> for LookupError {
    fn from(value: &str) -> Self {
        Self::Failed(value.into())
    }
}
impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable {
                error: Some(error), ..
            }
            | Self::Failed(error) => f.write_str(error),
            Self::Unavailable { .. } => {
                f.write_str("Download this dictionary to translate offline.")
            }
        }
    }
}

pub struct Store {
    directory: PathBuf,
    cache: Mutex<Option<(PackageId, Arc<Lexicon>)>>,
}
impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DictionaryStore")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}
impl Default for Store {
    fn default() -> Self {
        Self::new(reader_profile::storage_base().join("simPl/dictionaries"))
    }
}

impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            cache: Mutex::new(None),
        }
    }
    fn path(&self, id: PackageId) -> Result<PathBuf, String> {
        let p = package(id).ok_or("Unknown dictionary package.")?;
        Ok(self.directory.join(&p.file))
    }
    pub fn inventory(&self) -> Vec<PackageState> {
        packages()
            .iter()
            .map(|p| match read_package(&self.directory.join(&p.file), p) {
                Ok(_) => PackageState::Installed,
                Err(_) if !self.directory.join(&p.file).exists() => PackageState::Missing,
                Err(_) => PackageState::Invalid,
            })
            .collect()
    }
    /// Verify all bytes before atomically replacing a package. The cancellation
    /// check is immediately before the commit; an already committed install wins.
    pub fn install(&self, id: PackageId, bytes: &[u8], cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled.".into());
        }
        let p = package(id).ok_or("Unknown dictionary package.")?;
        check_bytes(bytes, p)?;
        decode(bytes, p)?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "The dictionary cache is unavailable.")?;
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled.".into());
        }
        crate::position::atomic_write(
            &self.path(id)?,
            bytes,
            "dictionary",
            "dictionary",
            ".dictionary",
        )?;
        if cache.as_ref().is_some_and(|(cached, _)| *cached == id) {
            *cache = None;
        }
        Ok(())
    }
    /// Import the same release ZIPs without an Internet connection.
    pub fn import(&self, path: &Path, cancel: &AtomicBool) -> Result<PackageId, String> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let id = packages()
            .iter()
            .position(|p| p.bytes == bytes.len() as u64 && p.sha256 == hash)
            .map(PackageId)
            .ok_or("This file is not a supported dictionary package for this version.")?;
        self.install(id, &bytes, cancel)?;
        Ok(id)
    }
    pub fn remove(&self, id: PackageId) -> Result<(), String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "The dictionary cache is unavailable.")?;
        match fs::remove_file(self.path(id)?) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("Could not remove dictionary: {e}")),
        }
        if cache.as_ref().is_some_and(|(cached, _)| *cached == id) {
            *cache = None;
        }
        Ok(())
    }
    /// Read attribution from the same verified archive; never extract ZIP paths.
    pub fn notices(&self, id: PackageId) -> Result<String, String> {
        let p = package(id).ok_or("Unknown dictionary package.")?;
        let bytes = read_package(&self.path(id)?, p)?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let mut notices = String::new();
        for name in ["README.md", "LICENSE.txt"] {
            let mut file = archive.by_name(name).map_err(|e| e.to_string())?;
            if file.size() > 128 * 1024 {
                return Err("Dictionary notices exceed the size limit.".into());
            }
            file.read_to_string(&mut notices)
                .map_err(|e| e.to_string())?;
            notices.push_str("\n\n");
        }
        Ok(notices)
    }
    /// Run on a worker; only one active direction is decompressed and indexed.
    pub fn lookup(
        &self,
        text: &str,
        source: Language,
        target: Language,
    ) -> Result<Option<Translation>, LookupError> {
        let word = query(text, source).ok_or("Select a word or short phrase (up to 4 words).")?;
        let id =
            package_id(source, target).ok_or("No offline dictionary for this language pair.")?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "The dictionary cache is unavailable.")?;
        if cache.as_ref().is_none_or(|(cached, _)| *cached != id) {
            let p = package(id).unwrap();
            let path = self.path(id)?;
            let unavailable = |error| LookupError::Unavailable { package: id, error };
            let bytes = read_package(&path, p)
                .map_err(|error| unavailable(path.exists().then_some(error)))?;
            let lexicon = decode(&bytes, p).map_err(|error| unavailable(Some(error)))?;
            *cache = Some((id, Arc::new(lexicon)));
        }
        let lexicon = &cache.as_ref().unwrap().1;
        Ok(lexicon.lookup(&word, source))
    }
}

fn check_bytes(bytes: &[u8], p: &Package) -> Result<(), String> {
    if bytes.len() as u64 != p.bytes || format!("{:x}", Sha256::digest(bytes)) != p.sha256 {
        return Err(
            "The dictionary package failed its integrity check. Try downloading it again.".into(),
        );
    }
    Ok(())
}
fn read_package(path: &Path, p: &Package) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != p.bytes {
        return Err("The dictionary file is incomplete or damaged.".into());
    }
    let mut bytes = Vec::with_capacity(p.bytes as usize);
    file.take(p.bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    check_bytes(&bytes, p)?;
    Ok(bytes)
}
fn decode(bytes: &[u8], p: &Package) -> Result<Lexicon, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_reader(
        archive
            .by_name("manifest.json")
            .map_err(|e| e.to_string())?
            .take(64 * 1024),
    )
    .map_err(|e| e.to_string())?;
    if manifest.version != 1 || manifest.pairs.len() != 1 {
        return Err("Unsupported dictionary package format.".into());
    }
    let pair = &manifest.pairs[0];
    if pair.source != p.source.code()
        || pair.target != p.target.code()
        || pair.sha256 != p.data_sha256
        || pair.provider != p.provider
    {
        return Err("The dictionary package metadata is invalid.".into());
    }
    let mut file = archive
        .by_name(&format!("{}-{}.tsv", p.source.code(), p.target.code()))
        .map_err(|e| e.to_string())?;
    if file.size() != p.data_bytes || file.size() > MAX_DATA {
        return Err("Dictionary exceeds the local size limit.".into());
    }
    let mut data = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_DATA + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_DATA || format!("{:x}", Sha256::digest(&data)) != p.data_sha256 {
        return Err("The dictionary index failed its integrity check.".into());
    }
    Lexicon::parse(
        String::from_utf8(data).map_err(|e| e.to_string())?,
        p.provider.clone(),
    )
}
