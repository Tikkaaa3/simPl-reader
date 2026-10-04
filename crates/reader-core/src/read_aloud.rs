//! Engine-independent speech offsets, language hints and bounded text chunks.

/// A source position, independent of pages and the selected reading theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub section: usize,
    pub row: usize,
    pub byte: usize,
}

impl Cursor {
    /// Skip an image/empty row or continue into the following source section.
    pub fn advance_row(&mut self, rows: usize) {
        self.byte = 0;
        self.row += 1;
        if self.row >= rows {
            self.section += 1;
            self.row = 0;
        }
    }
}

/// Bound an utterance in UTF-16 units, retaining exact source bytes. Prefer a
/// sentence/word boundary; even unbroken text and supplementary characters fit.
pub fn chunk_end(text: &str, start: usize, max_units: usize) -> usize {
    let start = start.min(text.len());
    if !text.is_char_boundary(start) || max_units < 2 {
        return start;
    }
    let mut units = 0;
    let mut end = start;
    let mut space = None;
    let mut sentence = None;
    for (relative, c) in text[start..].char_indices() {
        if units + c.len_utf16() > max_units {
            break;
        }
        units += c.len_utf16();
        end = start + relative + c.len_utf8();
        if c.is_whitespace() {
            space = Some(end);
        }
        if matches!(c, '.' | '!' | '?' | '…') {
            sentence = Some(end);
        }
    }
    if end == text.len() {
        return end;
    }
    sentence
        .filter(|boundary| *boundary > start + (end - start) / 2)
        .or(space.filter(|boundary| *boundary > start + (end - start) / 2))
        .unwrap_or(end)
}

/// Return the visible sheet containing a word's line. Sheets can divide a row
/// and publisher pages can continue across sections; keep that decision shared.
pub fn follow_page(line: usize, lines: usize, sheets: &[(u32, usize, usize)]) -> Option<u32> {
    let line = line.min(lines.saturating_sub(1));
    sheets
        .iter()
        .find(|(_, start, end)| line >= *start && line < *end)
        .map(|s| s.0)
}

/// Locate a source offset in half-open page spans. Desktop supplies retained
/// glyph coordinates; Android supplies its paragraph line coordinates.
pub fn follow_offset(offset: f32, sheets: impl IntoIterator<Item = (f32, f32)>) -> Option<usize> {
    sheets
        .into_iter()
        .position(|(start, end)| start <= offset && offset < end)
}

/// Converts an engine's UTF-16 character offsets to safe UTF-8 source coordinates.
pub fn word_bytes(text: &str, start: u32, length: u32) -> std::ops::Range<usize> {
    let end = start.saturating_add(length.max(1));
    let mut units = 0_u32;
    let mut first = None;
    for (byte, character) in text.char_indices() {
        let next = units.saturating_add(character.len_utf16() as u32);
        if first.is_none() && start < next {
            first = Some(byte);
        }
        if units >= end {
            return first.unwrap_or(byte)..byte;
        }
        units = next;
    }
    first.unwrap_or(text.len())..text.len()
}

/// A guess at the language of `text` from letters only some languages use, so
/// "Automatic" can pick an installed voice that can pronounce it.
pub fn guess_language(text: &str) -> Option<&'static str> {
    let mut letters = 0_usize;
    let mut counts = [0_usize; 5];
    for c in text.chars().take(4000) {
        if c.is_alphabetic() {
            letters += 1;
        }
        let index = match c {
            'ğ' | 'Ğ' | 'ş' | 'Ş' | 'ı' | 'İ' => 0,
            'ß' | 'ä' | 'Ä' => 1,
            'é' | 'è' | 'ê' | 'à' | 'œ' | 'ë' | 'ù' => 2,
            'ñ' | 'Ñ' | '¿' | '¡' | 'á' | 'í' | 'ó' | 'ú' => 3,
            'a'..='z' | 'A'..='Z' => 4,
            _ => continue,
        };
        counts[index] += 1;
    }
    if letters < 20 {
        return None;
    }
    let (best, count) = counts[..4]
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| **count)
        .map(|(index, count)| (index, *count))?;
    // Distinctive letters are rare even in their own language: a few per hundred.
    if count * 200 >= letters {
        return Some(["tr", "de", "fr", "es"][best]);
    }
    (counts[4] * 10 >= letters * 9).then_some("en")
}

/// The speed a SAPI rate step roughly gives, for display: each step is about
/// a tenth of a tripling.
pub fn speed(rate: i8) -> f32 {
    3.0_f32.powf(f32::from(rate) / 10.0)
}

/// The start of the sentence holding byte `at`, or the next word start if the
/// sentence began far back.
pub fn sentence_start(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    if at == 0 {
        return 0;
    }
    let before = &text[..at];
    let sentence = before
        .char_indices()
        .rev()
        .find(|(index, c)| {
            matches!(c, '.' | '!' | '?' | '…' | '"' | '”')
                && before[index + c.len_utf8()..].starts_with(char::is_whitespace)
        })
        .map(|(index, c)| {
            let after = index + c.len_utf8();
            after + (before[after..].len() - before[after..].trim_start().len())
        });
    match sentence {
        Some(start) if at - start < 400 => start,
        _ => before
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(space, c)| space + c.len_utf8()),
    }
}

#[cfg(test)]
mod planning_tests {
    use super::*;

    #[test]
    fn chunks_fit_utf16_and_preserve_every_source_byte() {
        for text in [
            "😀 İstanbul 日本語. Next sentence. ".repeat(500),
            "界".repeat(9000),
        ] {
            let mut start = 0;
            let mut reconstructed = String::new();
            while start < text.len() {
                let end = chunk_end(&text, start, 3500);
                assert!(end > start && text[start..end].encode_utf16().count() <= 3500);
                reconstructed.push_str(&text[start..end]);
                start = end;
            }
            assert_eq!(reconstructed, text);
        }
    }

    #[test]
    fn follows_split_rows_without_repeating_the_previous_sheet() {
        let sheets = [(1, 0, 7), (2, 7, 15), (3, 15, 20)];
        assert_eq!(follow_page(7, 20, &sheets), Some(2));
        assert_eq!(follow_page(19, 20, &sheets), Some(3));
        assert_eq!(follow_offset(7.0, [(0.0, 7.0), (7.0, 15.0)]), Some(1));
        assert_eq!(follow_offset(15.0, [(0.0, 7.0), (7.0, 15.0)]), None);
        let mut cursor = Cursor {
            section: 2,
            row: 4,
            byte: 12,
        };
        cursor.advance_row(5);
        assert_eq!(
            cursor,
            Cursor {
                section: 3,
                row: 0,
                byte: 0
            }
        );
    }

    #[test]
    fn offsets_are_safe_inside_a_surrogate_pair_and_outside_text() {
        let text = "A 😀 Çin 日本語 café";
        assert_eq!(&text[word_bytes(text, 3, 1)], "😀");
        assert!(word_bytes(text, u32::MAX, u32::MAX).is_empty());
        assert_eq!(guess_language("Short."), None);
        assert_eq!(
            guess_language("A long English sentence with enough letters to choose a voice."),
            Some("en")
        );
        assert_eq!(
            sentence_start("First sentence. Next sentence here.", 24),
            16
        );
    }
}
