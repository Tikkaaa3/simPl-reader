//! Pure local lookup rules shared by both reader adapters.
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

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

pub struct Lexicon {
    text: String,
    lines: Vec<u32>,
    provider: String,
}

impl Lexicon {
    pub fn parse(text: String, provider: String) -> Result<Self, String> {
        let mut lines = Vec::new();
        let mut offset = 0;
        let mut previous = "";
        for line in text.split_inclusive('\n') {
            let mut fields = line.trim_end_matches('\n').split('\t');
            let key = fields.next().unwrap_or_default();
            let headword = fields.next().unwrap_or_default();
            let meanings = fields.next().unwrap_or_default();
            if fields.next().is_some()
                || key.is_empty()
                || headword.is_empty()
                || meanings.is_empty()
                || key <= previous
                || line.len() > 24 * 1024
            {
                return Err("The local dictionary index is invalid.".into());
            }
            previous = key;
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

    /// Exact entries precede conservative, explicitly labeled English bases.
    pub fn lookup(&self, text: &str, source: Language) -> Option<Translation> {
        let word = query(text, source)?;
        self.find(&word, false).or_else(|| {
            if source != Language::English || word.contains(' ') {
                return None;
            }
            english_bases(&word)
                .into_iter()
                .find_map(|candidate| self.find(&candidate, true))
        })
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

pub fn word_range(text: &str, byte: usize) -> Option<std::ops::Range<usize>> {
    let mut ending = None;
    for (start, word) in text.unicode_word_indices() {
        let end = start + word.len();
        if start <= byte && byte < end {
            return Some(start..end);
        }
        // Native paragraph hit tests may return the caret just after a final glyph.
        if byte == end {
            ending = Some(start..end);
        }
    }
    ending
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
    fn exact_entries_win_and_conservative_bases_are_labeled() {
        let lexicon = Lexicon::parse(
            "book\tbook\tkitap\nran\tran\tkoştu\nrun\trun\tkoşmak\n".into(),
            "Fixture".into(),
        )
        .unwrap();
        assert!(!lexicon.lookup("RAN", Language::English).unwrap().base_form);
        assert!(
            lexicon
                .lookup("books", Language::English)
                .unwrap()
                .base_form
        );
        assert!(lexicon.lookup("missing", Language::English).is_none());
        assert_eq!(word_range("İstanbul book", 2), Some(0..9));
        assert_eq!(word_range("İstanbul book", 14), Some(10..14));
        assert_eq!(
            Settings {
                source: Language::Korean,
                target: Language::Turkish,
                automatic: false
            }
            .validated()
            .target,
            Language::English
        );
    }
}
