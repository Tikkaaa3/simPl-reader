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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub font: Font,
    pub size: u16,
    /// Zero follows the theme, otherwise percent of font size.
    pub spacing: u16,
    pub margin: u16,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            font: Font::Theme,
            size: 20,
            spacing: 0,
            margin: 48,
        }
    }
}
impl Options {
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
        };
        save_to(&path, &key, Some(options)).unwrap();
        assert_eq!(load_from(&path, &key).unwrap(), Some(options.validated()));
        assert!(load_from(&path, &"b".repeat(64)).is_err());
        fs::write(&path, b"{invalid").unwrap();
        assert!(load_from(&path, &key).is_err());
        save_to(&path, &key, None).unwrap();
        assert_eq!(load_from(&path, &key).unwrap(), None);
        fs::remove_dir(&folder).unwrap();
    }
}
