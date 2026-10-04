//! Small, versioned application preferences, independent of document positions.
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

impl Appearance {
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    pub fn toggle_label(self) -> &'static str {
        match self {
            Self::Light => "Switch to dark mode",
            Self::Dark => "Switch to light mode",
        }
    }
}

/// Placement and style of the minimize, maximize and close buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowControls {
    /// Round buttons at the left of the title bar.
    #[default]
    Mac,
    /// Minimize, maximize and close glyphs at the right of the title bar.
    Windows,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub appearance: Appearance,
    pub window_controls: WindowControls,
    /// Id of the chosen reading theme; empty or unknown ids mean the default theme.
    pub theme: String,
    /// Read-aloud voice token id; empty means choose automatically.
    pub voice: String,
    /// Read-aloud speed step, -10 (slowest) to 10 (fastest); 0 is normal.
    pub speech_rate: i8,
    pub dictionary: crate::dictionary::Settings,
    pub reading: crate::reading::Options,
}

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    version: u32,
    #[serde(default)]
    appearance: Appearance,
    #[serde(default)]
    window_controls: WindowControls,
    #[serde(default)]
    theme: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    voice: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    speech_rate: i8,
    #[serde(default)]
    dictionary: crate::dictionary::Settings,
    #[serde(default)]
    reading: crate::reading::Options,
}

fn is_zero(value: &i8) -> bool {
    *value == 0
}

fn path() -> PathBuf {
    reader_profile::storage_base().join("simPl/preferences.json")
}

pub fn load() -> Result<Preferences, String> {
    load_from(&path())
}

pub fn save(preferences: Preferences) -> Result<(), String> {
    save_to(&path(), preferences)
}

pub(crate) fn load_from(path: &Path) -> Result<Preferences, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(format!("Cannot open preferences: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read preferences: {error}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Preferences exceed size limit".into());
    }
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid preferences: {error}"))?;
    if record.version != VERSION {
        return Err(format!(
            "Unsupported preferences version {}",
            record.version
        ));
    }
    Ok(Preferences {
        appearance: record.appearance,
        window_controls: record.window_controls,
        theme: record.theme.chars().take(64).collect(),
        voice: if record.voice.len() > 512 {
            String::new()
        } else {
            record.voice
        },
        speech_rate: record.speech_rate.clamp(-10, 10),
        dictionary: record.dictionary.validated(),
        reading: record.reading.validated(),
    })
}

fn save_to(path: &Path, preferences: Preferences) -> Result<(), String> {
    let bytes = serde_json::to_vec(&Record {
        version: VERSION,
        appearance: preferences.appearance,
        window_controls: preferences.window_controls,
        theme: preferences.theme,
        voice: preferences.voice,
        speech_rate: preferences.speech_rate,
        dictionary: preferences.dictionary.validated(),
        reading: preferences.reading.validated(),
    })
    .map_err(|error| format!("Cannot encode preferences: {error}"))?;
    crate::position::atomic_write(path, &bytes, "preferences", "preferences", ".preferences")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn test_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "simpl-preferences-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json")
    }

    #[test]
    fn missing_preferences_default_to_light_and_saved_choices_round_trip() {
        let path = test_path();
        assert_eq!(load_from(&path).unwrap(), Preferences::default());
        for appearance in [Appearance::Dark, Appearance::Light] {
            for window_controls in [WindowControls::Windows, WindowControls::Mac] {
                let preferences = Preferences {
                    appearance,
                    window_controls,
                    theme: "soft".into(),
                    voice: r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Speech\Voices\Tokens\X".into(),
                    speech_rate: -3,
                    reading: crate::reading::Options {
                        size: 26,
                        ..Default::default()
                    },
                    dictionary: crate::dictionary::Settings {
                        automatic: false,
                        source: crate::dictionary::Language::Spanish,
                        target: crate::dictionary::Language::English,
                    },
                };
                save_to(&path, preferences.clone()).unwrap();
                assert_eq!(load_from(&path).unwrap(), preferences);
            }
        }
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn old_records_default_missing_appearance_and_invalid_records_stay_intact() {
        let path = test_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"version":1}"#).unwrap();
        assert_eq!(load_from(&path).unwrap(), Preferences::default());
        fs::write(&path, br#"{"version":1,"appearance":"dark"}"#).unwrap();
        assert_eq!(
            load_from(&path).unwrap(),
            Preferences {
                appearance: Appearance::Dark,
                window_controls: WindowControls::Mac,
                ..Preferences::default()
            }
        );
        // A newer record with a theme loads, and older readers would ignore the extra field.
        fs::write(
            &path,
            br#"{"version":1,"appearance":"dark","theme":"clear"}"#,
        )
        .unwrap();
        assert_eq!(load_from(&path).unwrap().theme, "clear");
        fs::write(&path, br#"{"version":1,"speech_rate":40}"#).unwrap();
        assert_eq!(load_from(&path).unwrap().speech_rate, 10);
        fs::write(&path, br#"{"version":1,"dictionary":{"automatic":false,"source":"korean","target":"turkish"}}"#).unwrap();
        let dictionary = load_from(&path).unwrap().dictionary;
        assert!(!dictionary.automatic);
        assert_eq!(dictionary.source, crate::dictionary::Language::Korean);
        assert_eq!(dictionary.target, crate::dictionary::Language::English);
        for bytes in [
            b"broken".to_vec(),
            br#"{"version":2,"appearance":"dark"}"#.to_vec(),
            br#"{"version":1,"appearance":"unknown"}"#.to_vec(),
            vec![b' '; MAX_BYTES as usize + 1],
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(load_from(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
