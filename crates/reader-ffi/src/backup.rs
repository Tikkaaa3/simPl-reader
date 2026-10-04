//! Portable snapshots use the desktop archive, verification and atomic restore.
use crate::CoreError;
use reader_document::{backup, preferences};
use std::{
    path::Path,
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

pub(crate) static PROFILE: RwLock<()> = RwLock::new(());
static EPOCH: AtomicU64 = AtomicU64::new(0);
pub(crate) fn epoch() -> u64 {
    EPOCH.load(Ordering::Acquire)
}
pub(crate) fn current(value: u64) -> Result<(), CoreError> {
    if value == epoch() {
        Ok(())
    } else {
        Err("The profile was restored. Reopen this book."
            .to_owned()
            .into())
    }
}
pub(crate) fn read() -> Result<std::sync::RwLockReadGuard<'static, ()>, CoreError> {
    PROFILE
        .read()
        .map_err(|_| "Profile storage is unavailable".to_owned().into())
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct BackupSummary {
    pub files: u32,
    pub bytes: u64,
    pub documents: bool,
    pub dictionaries: bool,
}
impl From<backup::Summary> for BackupSummary {
    fn from(s: backup::Summary) -> Self {
        Self {
            files: crate::count(s.files),
            bytes: s.bytes,
            documents: s.documents,
            dictionaries: s.dictionaries,
        }
    }
}
#[uniffi::export]
pub fn estimate_backup(documents: bool, dictionaries: bool) -> Result<BackupSummary, CoreError> {
    let _guard = read()?;
    Ok(backup::estimate(backup::Options {
        documents,
        dictionaries,
    })?
    .into())
}
#[uniffi::export]
pub fn create_backup(
    path: String,
    documents: bool,
    dictionaries: bool,
) -> Result<BackupSummary, CoreError> {
    let _guard = PROFILE
        .write()
        .map_err(|_| "Profile storage is unavailable".to_owned())?;
    Ok(backup::create(
        Path::new(&path),
        backup::Options {
            documents,
            dictionaries,
        },
    )?
    .into())
}
#[uniffi::export]
pub fn inspect_backup(path: String) -> Result<BackupSummary, CoreError> {
    Ok(backup::inspect(Path::new(&path))?.into())
}
#[uniffi::export]
pub fn restore_backup(path: String) -> Result<String, CoreError> {
    let _guard = PROFILE
        .write()
        .map_err(|_| "Profile storage is unavailable".to_owned())?;
    let previous = backup::restore(Path::new(&path))?;
    EPOCH.fetch_add(1, Ordering::AcqRel);
    Ok(previous.to_string_lossy().into_owned())
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum NotesFormat {
    Markdown,
    Text,
    Json,
}
#[uniffi::export]
pub fn export_notes(
    path: String,
    fingerprint: String,
    title: String,
    format: NotesFormat,
) -> Result<(), CoreError> {
    let _guard = read()?;
    let notes = reader_document::annotations::load(&fingerprint)?;
    backup::export_annotations(
        Path::new(&path),
        &title,
        &notes,
        match format {
            NotesFormat::Markdown => backup::ExportFormat::Markdown,
            NotesFormat::Text => backup::ExportFormat::Text,
            NotesFormat::Json => backup::ExportFormat::Json,
        },
    )?;
    Ok(())
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PortablePreferences {
    pub dark: bool,
    pub theme: String,
    pub source: String,
    pub target: String,
    pub automatic: bool,
}
#[uniffi::export]
pub fn load_portable_preferences() -> Result<PortablePreferences, CoreError> {
    let _guard = read()?;
    let p = preferences::load()?;
    Ok(PortablePreferences {
        dark: p.appearance == preferences::Appearance::Dark,
        theme: p.theme,
        source: p.dictionary.source.code().into(),
        target: p.dictionary.target.code().into(),
        automatic: p.dictionary.automatic,
    })
}
#[uniffi::export]
pub fn save_portable_preferences(value: PortablePreferences) -> Result<(), CoreError> {
    let _guard = read()?;
    let mut p = preferences::load()?;
    p.appearance = if value.dark {
        preferences::Appearance::Dark
    } else {
        preferences::Appearance::Light
    };
    p.theme = value.theme;
    p.dictionary = reader_document::dictionary::Settings {
        source: crate::dictionary::language(&value.source)?,
        target: crate::dictionary::language(&value.target)?,
        automatic: value.automatic,
    }
    .validated();
    preferences::save(p)?;
    Ok(())
}
