//! Read-aloud through the Windows speech engine (SAPI 5), fully offline.
//!
//! The engine speaks asynchronously on its own audio thread; this wrapper only
//! queues text, pauses, stops and reports progress. It is created and used on
//! the UI thread, which is a COM single-threaded apartment.

use std::fmt;

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Globalization::LCIDToLocaleName;
use windows::Win32::Media::Speech::{
    IEnumSpObjectTokens, ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SPCAT_VOICES, SPF_ASYNC,
    SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK, SPRS_DONE, SPVOICESTATUS, SpObjectTokenCategory, SpVoice,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::{PCWSTR, PWSTR, w};

/// Voices installed for the modern speech platform; SAPI can speak with them too.
const ONECORE_VOICES: PCWSTR =
    w!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech_OneCore\\Voices");

/// The slowest and fastest SAPI rate steps.
pub const MIN_RATE: i8 = -10;
pub const MAX_RATE: i8 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voice {
    /// The registry token id, stable across runs.
    pub id: String,
    pub name: String,
    /// BCP-47 tags such as "tr-TR"; a voice may list several.
    pub languages: Vec<String>,
}

impl Voice {
    pub fn speaks(&self, language: &str) -> bool {
        self.languages.iter().any(|tag| {
            tag.split('-')
                .next()
                .is_some_and(|primary| primary.eq_ignore_ascii_case(language))
        })
    }
}

impl fmt::Display for Voice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.languages.first() {
            Some(language) => write!(f, "{} ({language})", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// Where the engine is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// The stream being spoken; streams are numbered in the order they were queued.
    pub stream: u32,
    /// Whether every queued stream has been spoken.
    pub done: bool,
    /// UTF-16 offset of the word being spoken, in its stream's text.
    pub word: u32,
    /// UTF-16 length of the word being processed.
    pub word_len: u32,
}

pub struct Speaker {
    voice: ISpVoice,
    voices: Vec<(Voice, ISpObjectToken)>,
    chosen: Option<String>,
    paused: bool,
    // Drop the COM interfaces before balancing this thread's initialization.
    _apartment: Option<Apartment>,
}

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances one successful CoInitializeEx on this same UI thread.
        unsafe { CoUninitialize() };
    }
}

impl fmt::Debug for Speaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Speaker")
            .field("voices", &self.voices.len())
            .field("chosen", &self.chosen)
            .field("paused", &self.paused)
            .finish()
    }
}

impl Speaker {
    pub fn new() -> Result<Self, String> {
        // SAFETY: plain COM initialization for this thread. An apartment that
        // winit already set up is reported as success or RPC_E_CHANGED_MODE,
        // either of which leaves COM usable here.
        let apartment = match unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
            Ok(()) => Some(Apartment),
            Err(error) if error.code() == RPC_E_CHANGED_MODE => None,
            Err(error) => {
                return Err(format!(
                    "Cannot initialize Windows speech: {}",
                    error.message()
                ));
            }
        };
        // SAFETY: SpVoice is a registered in-process COM class on Windows.
        let voice: ISpVoice = unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_ALL) }
            .map_err(|error| format!("Windows speech is not available: {}", error.message()))?;
        let mut voices: Vec<(Voice, ISpObjectToken)> = Vec::new();
        for category in [SPCAT_VOICES, ONECORE_VOICES] {
            for (found, token) in enumerate(category) {
                if !voices.iter().any(|(known, _)| known.name == found.name) {
                    voices.push((found, token));
                }
            }
        }
        voices.sort_by_cached_key(|(voice, _)| voice.to_string());
        Ok(Self {
            voice,
            voices,
            chosen: None,
            paused: false,
            _apartment: apartment,
        })
    }

    pub fn voices(&self) -> impl Iterator<Item = &Voice> {
        self.voices.iter().map(|(voice, _)| voice)
    }

    /// Speaks with this voice from the next queued text on; `None` is the Windows default.
    pub fn use_voice(&mut self, id: Option<&str>) -> Result<(), String> {
        if self.chosen.as_deref() == id {
            return Ok(());
        }
        let token = match id {
            Some(id) => Some(
                self.voices
                    .iter()
                    .find(|(voice, _)| voice.id == id)
                    .map(|(_, token)| token)
                    .ok_or("That voice is no longer installed.")?,
            ),
            None => None,
        };
        // SAFETY: the token belongs to a live category enumeration; None restores the default.
        unsafe { self.voice.SetVoice(token) }
            .map_err(|error| format!("Cannot use that voice: {}", error.message()))?;
        self.chosen = id.map(str::to_owned);
        Ok(())
    }

    pub fn set_rate(&self, rate: i8) {
        // SAFETY: SetRate accepts any value and clamps to its own range.
        let _ = unsafe {
            self.voice
                .SetRate(i32::from(rate.clamp(MIN_RATE, MAX_RATE)))
        };
    }

    /// Queues plain text after anything already queued; returns its stream number.
    pub fn speak(&self, text: &str) -> Result<u32, String> {
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let mut stream = 0_u32;
        // SAFETY: the terminated UTF-16 buffer outlives the call; SAPI copies it
        // before an asynchronous Speak returns.
        unsafe {
            self.voice.Speak(
                PCWSTR(wide.as_ptr()),
                (SPF_ASYNC.0 | SPF_IS_NOT_XML.0) as u32,
                Some(&mut stream),
            )
        }
        .map_err(|error| format!("Windows speech failed: {}", error.message()))?;
        Ok(stream)
    }

    /// Stops speaking and forgets everything queued.
    pub fn stop(&mut self) {
        // SAFETY: a null string with PURGEBEFORESPEAK only clears the queue.
        let _ = unsafe {
            self.voice.Speak(
                PCWSTR::null(),
                (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32,
                None,
            )
        };
        if self.paused {
            // A paused voice stays paused for the next text; release it.
            let _ = unsafe { self.voice.Resume() };
            self.paused = false;
        }
    }

    pub fn pause(&mut self) {
        if !self.paused {
            // SAFETY: Pause and Resume are counted; `paused` keeps them balanced.
            let _ = unsafe { self.voice.Pause() };
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        if self.paused {
            // SAFETY: balanced with the single Pause above.
            let _ = unsafe { self.voice.Resume() };
            self.paused = false;
        }
    }

    pub fn status(&self) -> Result<Status, String> {
        let mut status = SPVOICESTATUS::default();
        // SAFETY: SAPI fills the caller-owned struct; a null bookmark pointer
        // asks for no bookmark string.
        unsafe { self.voice.GetStatus(&mut status, std::ptr::null_mut()) }
            .map_err(|error| format!("Cannot check Windows speech: {}", error.message()))?;
        status
            .hrLastResult
            .ok()
            .map_err(|error| format!("Windows speech failed: {}", error.message()))?;
        Ok(Status {
            stream: status.ulCurrentStream,
            done: status.dwRunningState == SPRS_DONE.0 as u32
                && status.ulCurrentStream >= status.ulLastStreamQueued,
            word: status.ulInputWordPos,
            word_len: status.ulInputWordLen,
        })
    }

    #[cfg(test)]
    pub(crate) fn mute(&self) {
        // SAFETY: suppresses output during native integration checks.
        unsafe { self.voice.SetVolume(0) }.unwrap();
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The voices of one registry category, with their display names and languages.
fn enumerate(category: PCWSTR) -> Vec<(Voice, ISpObjectToken)> {
    let mut found = Vec::new();
    // SAFETY: every call below is a documented SAPI token-category method on
    // live interfaces; returned strings are freed with CoTaskMemFree.
    unsafe {
        let Ok(list): Result<ISpObjectTokenCategory, _> =
            CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL)
        else {
            return found;
        };
        if list.SetId(category, false).is_err() {
            return found;
        }
        let Ok(tokens): Result<IEnumSpObjectTokens, _> = list.EnumTokens(None, None) else {
            return found;
        };
        let mut count = 0_u32;
        if tokens.GetCount(&mut count).is_err() {
            return found;
        }
        for index in 0..count {
            let Ok(token) = tokens.Item(index) else {
                continue;
            };
            let (Some(id), Some(name)) = (
                take(token.GetId()),
                take(token.GetStringValue(PCWSTR::null())),
            ) else {
                continue;
            };
            let languages = token
                .OpenKey(w!("Attributes"))
                .ok()
                .and_then(|attributes| take(attributes.GetStringValue(w!("Language"))))
                .map(|codes| {
                    codes
                        .split(';')
                        .filter_map(|code| u32::from_str_radix(code.trim(), 16).ok())
                        .filter_map(locale_name)
                        .collect()
                })
                .unwrap_or_default();
            found.push((
                Voice {
                    id,
                    name,
                    languages,
                },
                token,
            ));
        }
    }
    found
}

/// Copies and frees a string SAPI allocated for the caller.
unsafe fn take(value: windows::core::Result<PWSTR>) -> Option<String> {
    let value = value.ok()?;
    if value.is_null() {
        return None;
    }
    // SAFETY: SAPI returned a terminated CoTaskMem string owned by the caller.
    let text = unsafe { value.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(value.0 as *const _)) };
    text.filter(|text| !text.trim().is_empty())
}

fn locale_name(lcid: u32) -> Option<String> {
    let mut buffer = [0_u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH wide units.
    let length = unsafe { LCIDToLocaleName(lcid, Some(&mut buffer), 0) };
    (length > 1).then(|| String::from_utf16_lossy(&buffer[..length as usize - 1]))
}

pub use reader_core::read_aloud::{guess_language, speed, word_bytes};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sapi_word_ranges_preserve_unicode_and_surrogate_pairs() {
        let text = "A 😀 Çin 日本語 café";
        for word in ["A", "😀", "Çin", "日本語", "café"] {
            let byte = text.find(word).unwrap();
            let start = text[..byte].encode_utf16().count() as u32;
            let length = word.encode_utf16().count() as u32;
            assert_eq!(&text[word_bytes(text, start, length)], word);
        }
        assert_eq!(&text[word_bytes(text, 3, 1)], "😀");
        assert!(word_bytes(text, u32::MAX, u32::MAX).is_empty());
    }

    #[test]
    fn languages_are_guessed_from_their_letters() {
        assert_eq!(
            guess_language(
                "Bilincini öyle gizli bilgilendirir ki sadece kendi bilir, haliyle kod da mesajı ifşaya karşı korumuş olur."
            ),
            Some("tr")
        );
        assert_eq!(
            guess_language(
                "It is a truth universally acknowledged, that a single man in possession of a good fortune."
            ),
            Some("en")
        );
        assert_eq!(
            guess_language(
                "¿Dónde está la biblioteca? Mañana leeremos el capítulo siguiente juntos."
            ),
            Some("es")
        );
        assert_eq!(guess_language("Short."), None);
        assert_eq!(
            guess_language("静かな頁です。静かな頁です。静かな頁です。静かな頁です。"),
            None
        );
    }

    #[test]
    fn speed_steps_span_a_third_to_three_times() {
        assert!((speed(0) - 1.0).abs() < 1e-6);
        assert!((speed(10) - 3.0).abs() < 1e-4);
        assert!((speed(-10) - 1.0 / 3.0).abs() < 1e-4);
        assert!(speed(1) > speed(0));
    }

    #[test]
    fn voices_match_primary_language_tags() {
        let voice = Voice {
            id: "x".into(),
            name: "Microsoft Tolga".into(),
            languages: vec!["tr-TR".into()],
        };
        assert!(voice.speaks("tr"));
        assert!(!voice.speaks("en"));
        assert_eq!(voice.to_string(), "Microsoft Tolga (tr-TR)");
    }

    /// Speaks silently through every installed voice. Needs Windows speech.
    #[test]
    #[ignore = "Native speech QA: uses the installed Windows voices"]
    fn installed_voices_speak() {
        let mut speaker = Speaker::new().unwrap();
        let voices: Vec<Voice> = speaker.voices().cloned().collect();
        println!("{} voices", voices.len());
        speaker.mute();
        for voice in &voices {
            println!("{voice}  [{}]", voice.id);
            speaker.use_voice(Some(&voice.id)).unwrap();
            let first = speaker.speak("One.").unwrap();
            let second = speaker.speak("Two.").unwrap();
            assert!(second > first);
            // SAFETY: synchronous wait with a timeout.
            unsafe { speaker.voice.WaitUntilDone(10_000) }.unwrap();
            let status = speaker.status().unwrap();
            assert!(status.done, "{voice}: {status:?}");
        }
        speaker
            .speak("A longer sentence to stop before it ends.")
            .unwrap();
        speaker.pause();
        speaker.stop();
        let first = speaker.speak("Again.").unwrap();
        unsafe { speaker.voice.WaitUntilDone(10_000) }.unwrap();
        let status = speaker.status().unwrap();
        assert!(status.done && status.stream >= first);
    }
}
