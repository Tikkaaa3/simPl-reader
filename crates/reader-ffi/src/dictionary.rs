//! Local dictionaries: the network adapter only receives pinned package URLs.
use crate::CoreError;
use reader_document::dictionary::{self, Language, LookupError, PackageId, Store};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug, uniffi::Record)]
pub struct DictionaryLanguage {
    pub code: String,
    pub label: String,
    pub targets: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DictionaryPackage {
    pub id: u32,
    pub source: String,
    pub target: String,
    pub label: String,
    pub url: String,
    pub bytes: u64,
    pub provider: String,
    pub entries: u32,
    pub state: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DictionaryResult {
    pub headword: Option<String>,
    pub meanings: Vec<String>,
    pub provider: Option<String>,
    pub base_form: bool,
    pub missing_package: Option<u32>,
    pub message: Option<String>,
}

pub(crate) fn language(code: &str) -> Result<Language, CoreError> {
    Language::ALL
        .into_iter()
        .find(|language| language.code() == code)
        .ok_or_else(|| "Unknown dictionary language.".to_owned().into())
}

#[uniffi::export]
pub fn dictionary_languages() -> Vec<DictionaryLanguage> {
    Language::ALL
        .into_iter()
        .map(|language| DictionaryLanguage {
            code: language.code().into(),
            label: language.to_string(),
            targets: language
                .targets()
                .into_iter()
                .map(|target| target.code().into())
                .collect(),
        })
        .collect()
}

#[uniffi::export]
pub fn dictionary_query(text: String, source: String) -> Result<Option<String>, CoreError> {
    Ok(dictionary::query(&text, language(&source)?))
}

/// A worker can signal cancellation while verification runs on another thread.
#[derive(Debug, uniffi::Object)]
pub struct DictionaryCancellation {
    cancelled: AtomicBool,
}
#[uniffi::export]
impl DictionaryCancellation {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
        })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Debug, uniffi::Object)]
pub struct DictionaryStore {
    store: Store,
}
#[uniffi::export]
impl DictionaryStore {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            store: Store::default(),
        })
    }

    pub fn inventory(&self) -> Vec<DictionaryPackage> {
        let states = self.store.inventory();
        dictionary::packages()
            .iter()
            .enumerate()
            .map(|(index, package)| DictionaryPackage {
                id: index as u32,
                source: package.source.code().into(),
                target: package.target.code().into(),
                label: package.label(),
                url: package.url(),
                bytes: package.bytes,
                provider: package.provider.clone(),
                entries: crate::count(package.entries),
                state: match states[index] {
                    dictionary::PackageState::Missing => "missing",
                    dictionary::PackageState::Installed => "installed",
                    dictionary::PackageState::Invalid => "invalid",
                }
                .into(),
            })
            .collect()
    }

    pub fn lookup(
        &self,
        text: String,
        source: String,
        target: String,
    ) -> Result<DictionaryResult, CoreError> {
        let mut result = DictionaryResult {
            headword: None,
            meanings: vec![],
            provider: None,
            base_form: false,
            missing_package: None,
            message: None,
        };
        match self
            .store
            .lookup(&text, language(&source)?, language(&target)?)
        {
            Ok(Some(translation)) => {
                result.headword = Some(translation.headword);
                result.meanings = translation.meanings;
                result.provider = Some(translation.provider);
                result.base_form = translation.base_form;
            }
            Ok(None) => result.message = Some("No entry in this offline dictionary.".into()),
            Err(error @ LookupError::Unavailable { package, .. }) => {
                result.missing_package = Some(package.0 as u32);
                result.message = Some(error.to_string());
            }
            Err(error) => return Err(error.to_string().into()),
        }
        Ok(result)
    }

    pub fn import_package(
        &self,
        path: String,
        cancel: Arc<DictionaryCancellation>,
    ) -> Result<u32, CoreError> {
        let _guard = crate::backup::read()?;
        Ok(self.store.import(Path::new(&path), &cancel.cancelled)?.0 as u32)
    }

    pub fn install_package(
        &self,
        id: u32,
        path: String,
        cancel: Arc<DictionaryCancellation>,
    ) -> Result<(), CoreError> {
        let _guard = crate::backup::read()?;
        use std::io::Read;
        let package = dictionary::package(PackageId(id as usize))
            .ok_or_else(|| "Unknown dictionary package.".to_owned())?;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(package.bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        self.store
            .install(PackageId(id as usize), &bytes, &cancel.cancelled)?;
        Ok(())
    }

    pub fn remove_package(&self, id: u32) -> Result<(), CoreError> {
        let _guard = crate::backup::read()?;
        Ok(self.store.remove(PackageId(id as usize))?)
    }
    pub fn package_notices(&self, id: u32) -> Result<String, CoreError> {
        Ok(self.store.notices(PackageId(id as usize))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_catalog_languages_and_supported_pairs_cross_the_boundary() {
        assert_eq!(dictionary_languages().len(), 8);
        assert_eq!(
            dictionary_query("İYİ".into(), "tr".into())
                .unwrap()
                .as_deref(),
            Some("iyi")
        );
        assert!(dictionary_query("book".into(), "bogus".into()).is_err());
        assert_eq!(dictionary_languages().last().unwrap().targets, ["en"]);
        let token = DictionaryCancellation::new();
        token.cancel();
        assert!(token.cancelled.load(Ordering::Acquire));
    }
}
