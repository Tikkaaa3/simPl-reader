//! Bounded offline word lookup using separately installed data packages.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    sync::{Arc, LazyLock, Mutex},
};
use unicode_normalization::UnicodeNormalization;

#[path = "dictionary_packages.rs"]
mod packages;
pub use packages::{
    LookupError, Package, PackageId, PackageState, Store, data_version, package, package_id,
    packages,
};
const MAX_DATA: u64 = 16 * 1024 * 1024;
pub const MAX_QUERY_BYTES: usize = 256;
pub const MAX_QUERY_WORDS: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    English,
    Turkish,
    Spanish,
    German,
    French,
    Japanese,
    Korean,
    Chinese,
}

impl Language {
    pub const ALL: [Self; 8] = [
        Self::English,
        Self::Turkish,
        Self::Spanish,
        Self::German,
        Self::French,
        Self::Japanese,
        Self::Korean,
        Self::Chinese,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Turkish => "tr",
            Self::Spanish => "es",
            Self::German => "de",
            Self::French => "fr",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Chinese => "zh",
        }
    }
    pub fn targets(self) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|target| supported(self, *target))
            .collect()
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::English => "English",
            Self::Turkish => "Türkçe",
            Self::Spanish => "Español",
            Self::German => "Deutsch",
            Self::French => "Français",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Chinese => "中文",
        })
    }
}

pub fn supported(source: Language, target: Language) -> bool {
    source != target
        && (target == Language::English
            || source == Language::English && target != Language::Korean)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub automatic: bool,
    pub source: Language,
    pub target: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            automatic: true,
            source: Language::English,
            target: Language::Turkish,
        }
    }
}

impl Settings {
    pub fn validated(mut self) -> Self {
        if !supported(self.source, self.target) {
            self.target = self.source.targets()[0];
        }
        self
    }
}

/// Normalization matches the package builder; punctuation around a selection is not a word.
pub fn query(text: &str, language: Language) -> Option<String> {
    if text.len() > MAX_QUERY_BYTES {
        return None;
    }
    let normalized: String = text.nfkc().collect();
    let text = normalized.trim().trim_matches(|c: char| {
        !c.is_alphanumeric() && !unicode_normalization::char::is_combining_mark(c)
    });
    if !text.chars().any(char::is_alphanumeric) || text.split_whitespace().count() > MAX_QUERY_WORDS
    {
        return None;
    }
    let text = text.to_owned();
    let text = if language == Language::Turkish {
        text.replace('I', "ı").replace('İ', "i")
    } else {
        text
    };
    let text = text
        .to_lowercase()
        .replace('ß', "ss")
        .replace('’', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (text.len() <= MAX_QUERY_BYTES).then_some(text)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    pub headword: String,
    pub meanings: Vec<String>,
    pub provider: String,
    pub base_form: bool,
}

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    pairs: Vec<Pair>,
}
#[derive(Deserialize)]
struct Pair {
    source: String,
    target: String,
    provider: String,
    sha256: String,
}

struct Lexicon {
    text: String,
    lines: Vec<u32>,
    provider: String,
}

impl Lexicon {
    fn parse(text: String, provider: String) -> Result<Self, String> {
        let mut lines = Vec::new();
        let mut offset = 0;
        let mut previous = "";
        for line in text.split_inclusive('\n') {
            let fields: Vec<_> = line.trim_end_matches('\n').split('\t').collect();
            if fields.len() != 3
                || fields[0].is_empty()
                || fields[1].is_empty()
                || fields[2].is_empty()
                || fields[0] <= previous
                || line.len() > 24 * 1024
            {
                return Err("The local dictionary index is invalid.".into());
            }
            previous = fields[0];
            lines.push(offset as u32);
            offset += line.len();
        }
        if lines.is_empty() {
            return Err("The local dictionary is empty.".into());
        }
        Ok(Self {
            text,
            lines,
            provider,
        })
    }

    fn line(&self, index: usize) -> &str {
        &self.text[self.lines[index] as usize
            ..self
                .lines
                .get(index + 1)
                .map_or(self.text.len(), |offset| *offset as usize)]
    }

    fn find(&self, word: &str, base_form: bool) -> Option<Translation> {
        let index = self
            .lines
            .binary_search_by(|offset| {
                let tail = &self.text[*offset as usize..];
                tail[..tail.find('\t').unwrap()].cmp(word)
            })
            .ok()?;
        let mut fields = self.line(index).trim_end_matches('\n').split('\t');
        fields.next()?;
        Some(Translation {
            headword: fields.next()?.to_owned(),
            meanings: fields
                .next()?
                .split(" | ")
                .take(8)
                .map(str::to_owned)
                .collect(),
            provider: self.provider.clone(),
            base_form,
        })
    }
}

fn english_bases(word: &str) -> Vec<String> {
    let irregular = match word {
        "ran" => Some("run"),
        "went" | "gone" => Some("go"),
        "was" | "were" | "been" | "is" | "are" => Some("be"),
        "had" => Some("have"),
        "did" | "done" => Some("do"),
        "saw" | "seen" => Some("see"),
        "took" | "taken" => Some("take"),
        "came" => Some("come"),
        "made" => Some("make"),
        "children" => Some("child"),
        "men" => Some("man"),
        "women" => Some("woman"),
        "feet" => Some("foot"),
        "teeth" => Some("tooth"),
        "mice" => Some("mouse"),
        "better" | "best" => Some("good"),
        _ => None,
    };
    let mut bases = irregular.into_iter().map(str::to_owned).collect::<Vec<_>>();
    // Conservative, labeled fallbacks; an exact entry always wins.
    for (suffix, replacement) in [
        ("ies", "y"),
        ("ied", "y"),
        ("ing", ""),
        ("ed", ""),
        ("es", ""),
        ("s", ""),
    ] {
        if let Some(base) = word.strip_suffix(suffix).filter(|base| base.len() >= 3) {
            bases.push(format!("{base}{replacement}"));
            if matches!(suffix, "ing" | "ed") {
                bases.push(format!("{base}e"));
                if base.is_ascii()
                    && base.as_bytes()[base.len() - 1] == base.as_bytes()[base.len() - 2]
                {
                    bases.push(base[..base.len() - 1].to_owned());
                }
            }
        }
    }
    bases
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        store: Store,
        path: std::path::PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
    fn fixture() -> Fixture {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "simpl-dictionaries-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        Fixture {
            store: Store::new(path.clone()),
            path,
        }
    }
    fn bytes(id: PackageId) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/dictionaries/packs")
                .join(&package(id).unwrap().file),
        )
        .unwrap()
    }
    fn install(store: &Store, id: PackageId) {
        store
            .install(id, &bytes(id), &std::sync::atomic::AtomicBool::new(false))
            .unwrap();
    }

    #[test]
    fn normalization_preserves_language_and_bounds() {
        assert_eq!(
            query("  “BOOK,” ", Language::English).as_deref(),
            Some("book")
        );
        assert_eq!(query("IŞIK", Language::Turkish).as_deref(), Some("ışık"));
        assert_eq!(query("İYİ", Language::Turkish).as_deref(), Some("iyi"));
        assert_eq!(
            query("ＢＯＯＫ", Language::English).as_deref(),
            Some("book")
        );
        assert_eq!(query("Don't", Language::English).as_deref(), Some("don't"));
        assert_eq!(
            query("cafe\u{0301}", Language::French).as_deref(),
            Some("café")
        );
        assert!(query("one two three four five", Language::English).is_none());
        assert!(query(&"a".repeat(257), Language::English).is_none());
        assert!(query("...", Language::English).is_none());
    }

    #[test]
    fn downloadable_pairs_pass_integrity_and_return_real_words() {
        let fixture = fixture();
        let store = &fixture.store;
        for (i, p) in packages().iter().enumerate() {
            assert_eq!(package_id(p.source, p.target), Some(PackageId(i)));
            assert_eq!(
                p.file,
                format!(
                    "{}-{}-{}.zip",
                    p.source.code(),
                    p.target.code(),
                    data_version()
                )
            );
            assert!(p.bytes < 8 * 1024 * 1024 && p.data_bytes <= MAX_DATA);
            install(store, PackageId(i));
        }
        assert!(
            store
                .inventory()
                .iter()
                .all(|state| *state == PackageState::Installed)
        );
        for (source, word, target) in [
            (Language::English, "book", Language::Turkish),
            (Language::Turkish, "kitap", Language::English),
            (Language::Spanish, "libro", Language::English),
            (Language::German, "Buch", Language::English),
            (Language::French, "livre", Language::English),
            (Language::Japanese, "本", Language::English),
            (Language::Korean, "책", Language::English),
            (Language::Chinese, "书", Language::English),
            (Language::Chinese, "書", Language::English),
        ] {
            assert!(
                store.lookup(word, source, target).unwrap().is_some(),
                "{word}"
            );
        }
        for target in Language::English.targets() {
            assert!(
                store
                    .lookup("book", Language::English, target)
                    .unwrap()
                    .is_some(),
                "{target}"
            );
        }
    }

    #[test]
    fn fallbacks_are_labeled_and_missing_entries_are_not_invented() {
        let fixture = fixture();
        let store = &fixture.store;
        install(
            store,
            package_id(Language::English, Language::Turkish).unwrap(),
        );
        let run = store
            .lookup("ran", Language::English, Language::Turkish)
            .unwrap()
            .unwrap();
        assert_eq!(run.headword, "run");
        assert!(run.base_form);
        assert!(
            store
                .lookup("zzzzzzzzz", Language::English, Language::Turkish)
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .lookup("book", Language::English, Language::Korean)
                .is_err()
        );
        assert_eq!(
            Settings {
                source: Language::Korean,
                target: Language::Turkish,
                ..Settings::default()
            }
            .validated()
            .target,
            Language::English
        );
    }

    #[test]
    fn missing_corrupt_cancelled_and_removed_packages_preserve_storage_contract() {
        let fixture = fixture();
        let store = &fixture.store;
        let id = package_id(Language::English, Language::Turkish).unwrap();
        let lookup = || store.lookup("book", Language::English, Language::Turkish);
        assert_eq!(
            lookup(),
            Err(LookupError::Unavailable {
                package: id,
                error: None
            })
        );
        assert!(
            !fixture.path.exists(),
            "a lookup must not create or download a file"
        );
        install(store, id);
        assert!(lookup().unwrap().is_some());
        let valid = bytes(id);
        let cancel = std::sync::atomic::AtomicBool::new(false);
        assert!(
            store
                .install(id, &valid[..valid.len() - 1], &cancel)
                .is_err()
        );
        let mut corrupt = valid.clone();
        corrupt[0] ^= 1;
        assert!(store.install(id, &corrupt, &cancel).is_err());
        assert!(
            store
                .install(id, &valid, &std::sync::atomic::AtomicBool::new(true))
                .is_err()
        );
        assert_eq!(
            std::fs::read(fixture.path.join(&package(id).unwrap().file)).unwrap(),
            valid
        );
        store.remove(id).unwrap();
        assert_eq!(
            lookup(),
            Err(LookupError::Unavailable {
                package: id,
                error: None
            }),
            "removal must invalidate the cached lexicon"
        );
        std::fs::write(fixture.path.join(&package(id).unwrap().file), corrupt).unwrap();
        assert_eq!(store.inventory()[id.0], PackageState::Invalid);
        assert!(matches!(
            lookup(),
            Err(LookupError::Unavailable { error: Some(_), .. })
        ));
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/dictionaries/packs")
            .join(&package(id).unwrap().file);
        assert_eq!(store.import(&file, &cancel).unwrap(), id);
        assert!(lookup().unwrap().is_some());
        assert_eq!(
            std::fs::read_dir(&fixture.path).unwrap().count(),
            1,
            "no temporary files remain"
        );
    }

    #[test]
    fn invalid_indexes_are_rejected_before_lookup() {
        for text in [
            "",
            "word\tword\t\n",
            "word\tword\tmeaning\textra\n",
            "z\tz\tz\na\ta\ta\n",
            "a\ta\ta\na\ta\ta\n",
        ] {
            assert!(Lexicon::parse(text.into(), "test".into()).is_err());
        }
    }
}
