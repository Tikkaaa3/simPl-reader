//! Independent typography defaults and small per-document overrides.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Font {
    #[default]
    Theme,
    Literata,
    Spectral,
    FiraSans,
}
impl std::fmt::Display for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Theme => "Theme font",
            Self::Literata => "Literata",
            Self::Spectral => "Spectral",
            Self::FiraSans => "Fira Sans",
        })
    }
}

/// A bundled reading theme, saved by the same id the preferences use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeId {
    Default,
    Soft,
    Clear,
    Compact,
}
impl ThemeId {
    pub const ALL: [Self; 4] = [Self::Default, Self::Soft, Self::Clear, Self::Compact];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Soft => "soft",
            Self::Clear => "clear",
            Self::Compact => "compact",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.as_str() == id)
    }
}

/// A theme id from a newer version, or anything else unreadable, means "follow
/// the default theme" rather than losing the rest of the book's settings.
fn lenient_theme<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ThemeId>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Known(ThemeId),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Known(theme) => Some(theme),
        Stored::Other(_) => None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub font: Font,
    pub size: u16,
    /// Zero follows the theme, otherwise percent of font size.
    pub spacing: u16,
    pub margin: u16,
    /// A book's own theme; `None` follows the default theme. The library
    /// defaults never carry one.
    #[serde(
        deserialize_with = "lenient_theme",
        skip_serializing_if = "Option::is_none"
    )]
    pub theme: Option<ThemeId>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            font: Font::Theme,
            size: 20,
            spacing: 0,
            margin: 48,
            theme: None,
        }
    }
}
impl Options {
    /// The typography alone, as the library defaults and the layout use it.
    pub fn without_theme(self) -> Self {
        Self {
            theme: None,
            ..self
        }
    }
    pub fn validated(self) -> Self {
        Self {
            size: self.size.clamp(12, 36),
            spacing: if self.spacing == 0 {
                0
            } else {
                self.spacing.clamp(110, 220)
            },
            margin: self.margin.clamp(16, 96),
            ..self
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    fingerprint: String,
    options: Options,
}
fn path(fingerprint: &str) -> Result<PathBuf, String> {
    valid_key(fingerprint)?;
    Ok(crate::position::storage_base()
        .join("simPl/reading")
        .join(format!("{}.json", fingerprint.to_ascii_lowercase())))
}
fn valid_key(fingerprint: &str) -> Result<(), String> {
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid reading settings fingerprint".into());
    }
    Ok(())
}
pub fn load(fingerprint: &str) -> Result<Option<Options>, String> {
    load_from(&path(fingerprint)?, fingerprint)
}
pub(crate) fn load_from(path: &Path, fingerprint: &str) -> Result<Option<Options>, String> {
    valid_key(fingerprint)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16_384 {
        return Err("Reading settings exceed size limit".into());
    }
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid reading settings: {e}"))?;
    if record.version != 1 || !record.fingerprint.eq_ignore_ascii_case(fingerprint) {
        return Err("Unsupported or mismatched reading settings".into());
    }
    Ok(Some(record.options.validated()))
}
pub fn save(fingerprint: &str, options: Option<Options>) -> Result<(), String> {
    save_to(&path(fingerprint)?, fingerprint, options)
}
fn save_to(path: &Path, fingerprint: &str, options: Option<Options>) -> Result<(), String> {
    if let Some(options) = options {
        let bytes = serde_json::to_vec(&Record {
            version: 1,
            fingerprint: fingerprint.to_ascii_lowercase(),
            options: options.validated(),
        })
        .map_err(|e| e.to_string())?;
        crate::position::atomic_write(
            path,
            &bytes,
            "reading settings",
            "reading settings",
            ".reading",
        )
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_overrides_roundtrip_and_reset() {
        let key = "a".repeat(64);
        let folder = std::env::temp_dir().join(format!("simpl-reading-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("reading.json");
        assert_eq!(load_from(&path, &key).unwrap(), None);
        let options = Options {
            size: u16::MAX,
            spacing: 1,
            margin: 0,
            font: Font::Spectral,
            theme: Some(ThemeId::Soft),
        };
        save_to(&path, &key, Some(options)).unwrap();
        assert_eq!(
            load_from(&path, &key).unwrap(),
            Some(Options {
                font: Font::Spectral,
                size: 36,
                spacing: 110,
                margin: 16,
                theme: Some(ThemeId::Soft),
            })
        );
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["options"]["theme"], "soft");
        assert!(load_from(&path, &"b".repeat(64)).is_err());
        fs::write(&path, b"{invalid").unwrap();
        assert!(load_from(&path, &key).is_err());
        save_to(&path, &key, None).unwrap();
        assert_eq!(load_from(&path, &key).unwrap(), None);
        fs::remove_dir(&folder).unwrap();
    }

    fn stored(options: &str) -> Option<Options> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let key = "c".repeat(64);
        let folder = std::env::temp_dir().join(format!(
            "simpl-reading-theme-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("reading.json");
        fs::write(
            &path,
            format!(r#"{{"version":1,"fingerprint":"{key}","options":{options}}}"#),
        )
        .unwrap();
        let loaded = load_from(&path, &key).unwrap();
        fs::remove_dir_all(&folder).unwrap();
        loaded
    }

    #[test]
    fn settings_saved_before_book_themes_load_and_follow_the_default_theme() {
        assert_eq!(
            stored(r#"{"font":"fira_sans","size":24,"spacing":160,"margin":32}"#),
            Some(Options {
                font: Font::FiraSans,
                size: 24,
                spacing: 160,
                margin: 32,
                theme: None,
            })
        );
    }

    #[test]
    fn an_unknown_book_theme_follows_the_default_and_keeps_the_typography() {
        let typography = Options {
            font: Font::Literata,
            size: 26,
            spacing: 180,
            margin: 64,
            theme: None,
        };
        for theme in [r#""from-a-newer-version""#, "7", "null", r#"{"id":"soft"}"#] {
            assert_eq!(
                stored(&format!(
                    r#"{{"font":"literata","size":26,"spacing":180,"margin":64,"theme":{theme}}}"#
                )),
                Some(typography),
                "{theme}"
            );
        }
        assert_eq!(
            stored(r#"{"font":"literata","size":26,"spacing":180,"margin":64,"theme":"clear"}"#),
            Some(Options {
                theme: Some(ThemeId::Clear),
                ..typography
            })
        );
    }
}
