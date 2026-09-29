//! Bounded offline word lookup. Only the active language pair is decompressed.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    sync::{Arc, LazyLock, Mutex},
};
use unicode_normalization::UnicodeNormalization;

const DATA: &[u8] = include_bytes!("../../../assets/dictionaries/words.zip");
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

type Cache = Option<(Language, Language, Arc<Lexicon>)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(None));

fn lexicon(source: Language, target: Language) -> Result<Arc<Lexicon>, String> {
    let mut cache = CACHE
        .lock()
        .map_err(|_| "The dictionary cache is unavailable.")?;
    if let Some((from, to, lexicon)) = cache.as_ref()
        && *from == source
        && *to == target
    {
        return Ok(lexicon.clone());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(DATA)).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_reader(
        archive
            .by_name("manifest.json")
            .map_err(|e| e.to_string())?
            .take(64 * 1024),
    )
    .map_err(|e| e.to_string())?;
    if manifest.version != 1 {
        return Err("Unsupported dictionary package version.".into());
    }
    let pair = manifest
        .pairs
        .iter()
        .find(|pair| pair.source == source.code() && pair.target == target.code())
        .ok_or("No offline dictionary for this language pair.")?;
    let mut file = archive
        .by_name(&format!("{}-{}.tsv", pair.source, pair.target))
        .map_err(|e| e.to_string())?;
    if file.size() > MAX_DATA {
        return Err("Dictionary exceeds the local size limit.".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_DATA + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DATA || format!("{:x}", Sha256::digest(&bytes)) != pair.sha256 {
        return Err("The local dictionary failed its integrity check.".into());
    }
    let lexicon = Arc::new(Lexicon::parse(
        String::from_utf8(bytes).map_err(|e| e.to_string())?,
        pair.provider.clone(),
    )?);
    *cache = Some((source, target, lexicon.clone()));
    Ok(lexicon)
}

/// Run on a worker. Returns dictionary meanings, never generated sentence translations.
pub fn lookup(
    text: &str,
    source: Language,
    target: Language,
) -> Result<Option<Translation>, String> {
    let word = query(text, source).ok_or("Select a word or short phrase (up to 4 words).")?;
    if !supported(source, target) {
        return Err("No offline dictionary for this language pair.".into());
    }
    let lexicon = lexicon(source, target)?;
    if let Some(result) = lexicon.find(&word, false) {
        return Ok(Some(result));
    }
    if source == Language::English && !word.contains(' ') {
        for candidate in english_bases(&word) {
            if let Some(result) = lexicon.find(&candidate, true) {
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
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
    fn bundled_pairs_pass_integrity_and_return_real_words() {
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
            assert!(lookup(word, source, target).unwrap().is_some(), "{word}");
        }
        for target in Language::English.targets() {
            assert!(
                lookup("book", Language::English, target).unwrap().is_some(),
                "{target}"
            );
        }
    }

    #[test]
    fn fallbacks_are_labeled_and_missing_entries_are_not_invented() {
        let run = lookup("ran", Language::English, Language::Turkish)
            .unwrap()
            .unwrap();
        assert_eq!(run.headword, "run");
        assert!(run.base_form);
        assert!(
            lookup("zzzzzzzzz", Language::English, Language::Turkish)
                .unwrap()
                .is_none()
        );
        assert!(lookup("book", Language::English, Language::Korean).is_err());
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
