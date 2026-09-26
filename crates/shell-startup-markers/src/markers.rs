//! Marker stream format, opt-in emitter, and validating parser.
//!
//! The parser never fabricates values: malformed or regressing lines are
//! preserved as labeled [`InvalidLine`]s (raw text kept, byte-capped), never
//! silently dropped and never turned into numeric records.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

/// Format tag written at the start of every marker line.
pub const FORMAT: &str = "shell-startup-markers/v1";

/// Upper bound for one marker line, kept so an emitter bug can never grow
/// the file without bound. Valid lines are far below this limit.
pub const MAX_LINE_BYTES: usize = 256;

/// One marker event kind. Each kind is emitted at most once per process.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Event {
    /// Shell `main` entry, stamped before any framework work.
    ProcessEntry,
    /// The shell's window was created by the framework.
    WindowCreated,
    /// First shell-side render submission the framework exposes
    /// (app-declared; never the displayed-frame endpoint).
    RenderSubmitted,
    /// First framework redraw request observed by the shell (app-declared).
    RedrawRequested,
}

impl Event {
    pub const ALL: [Event; 4] = [
        Event::ProcessEntry,
        Event::WindowCreated,
        Event::RenderSubmitted,
        Event::RedrawRequested,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Event::ProcessEntry => "process_entry",
            Event::WindowCreated => "window_created",
            Event::RenderSubmitted => "render_submitted",
            Event::RedrawRequested => "redraw_requested",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Event> {
        Event::ALL
            .iter()
            .copied()
            .find(|event| event.name() == name)
    }
}

/// One successfully parsed marker record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkerRecord {
    pub event: Event,
    pub qpc: u64,
}

/// One rejected input line, retained verbatim for evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidLine {
    pub sequence: usize,
    pub reason: &'static str,
    /// Raw line content, capped at [`MAX_LINE_BYTES`].
    pub raw: String,
}

/// Result of parsing one captured marker stream.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct MarkerStream {
    pub records: Vec<MarkerRecord>,
    pub invalid_lines: Vec<InvalidLine>,
    /// True when the consumer applied a read/size cap and content may be
    /// missing from the end of the stream.
    pub truncated: bool,
}

impl MarkerStream {
    #[must_use]
    pub fn valid(&self) -> bool {
        !self.truncated && self.invalid_lines.is_empty()
    }
}

/// Encode one marker line, including the trailing newline.
#[must_use]
pub fn encode_line(event: Event, qpc: u64) -> String {
    format!("{FORMAT} {} {qpc}\n", event.name())
}

/// Parse a captured marker stream (a bounded read of the marker file).
#[must_use]
pub fn parse(text: &str) -> MarkerStream {
    let mut stream = MarkerStream::default();
    let mut previous_qpc: Option<u64> = None;
    for (sequence, line) in text.lines().enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let rejected = |reason: &'static str, stream: &mut MarkerStream| {
            stream.invalid_lines.push(InvalidLine {
                sequence,
                reason,
                raw: line.chars().take(MAX_LINE_BYTES).collect(),
            });
        };
        let mut fields = line.splitn(4, ' ');
        let (tag, event, qpc) = (fields.next(), fields.next(), fields.next());
        if tag.is_none() || event.is_none() || qpc.is_none() || fields.next().is_some() {
            rejected("malformed_line", &mut stream);
            continue;
        }
        let (tag, event, qpc) = (tag.unwrap(), event.unwrap(), qpc.unwrap());
        if tag != FORMAT {
            rejected("unknown_format", &mut stream);
            continue;
        }
        let Some(event) = Event::parse(event) else {
            rejected("unknown_event", &mut stream);
            continue;
        };
        let Ok(qpc) = qpc.parse::<u64>() else {
            rejected("invalid_qpc", &mut stream);
            continue;
        };
        if previous_qpc.is_some_and(|previous| qpc < previous) {
            rejected("qpc_regression", &mut stream);
            continue;
        }
        previous_qpc = Some(qpc);
        stream.records.push(MarkerRecord { event, qpc });
    }
    stream
}

/// Best-effort raw `QueryPerformanceCounter` value, or `None` when the API
/// fails; markers are supporting evidence, so callers degrade gracefully.
#[must_use]
pub fn qpc() -> Option<u64> {
    let mut value = 0i64;
    if unsafe { QueryPerformanceCounter(&mut value) } == 0 || value < 0 {
        None
    } else {
        Some(value as u64)
    }
}

/// Best-effort raw `QueryPerformanceFrequency` value.
#[must_use]
pub fn qpc_frequency() -> Option<u64> {
    let mut value = 0i64;
    if unsafe { QueryPerformanceFrequency(&mut value) } == 0 || value <= 0 {
        None
    } else {
        Some(value as u64)
    }
}

struct Inner {
    file: Mutex<Option<File>>,
    emitted: [std::sync::atomic::AtomicBool; Event::ALL.len()],
}

/// Best-effort, process-scoped marker emitter.
///
/// One event kind is appended at most once; every line is flushed before
/// `emit` returns. Write failures are swallowed: markers are supporting
/// evidence, so a failed append degrades the run to "marker absent" instead
/// of changing normal shell behavior. The emitter is cheap to clone and
/// thread-safe.
#[derive(Clone)]
pub struct Emitter {
    inner: std::sync::Arc<Inner>,
}

impl std::fmt::Debug for Emitter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Emitter")
            .field("usable", &self.usable())
            .finish_non_exhaustive()
    }
}

impl Emitter {
    /// Opens (or creates) the marker file for appending. Open failure is
    /// reported as an unusable emitter: all `emit` calls become no-ops and
    /// the observer will label the marker stream absent.
    #[must_use]
    pub fn open(path: &Path) -> Emitter {
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        let inner = std::sync::Arc::new(Inner {
            file: Mutex::new(file),
            emitted: Default::default(),
        });
        Emitter { inner }
    }

    /// True when this event kind was already emitted, so a caller can skip
    /// any work (including the QPC read) for a bounded stream.
    #[must_use]
    pub fn has_emitted(&self, event: Event) -> bool {
        match Event::ALL.iter().position(|candidate| *candidate == event) {
            Some(index) => self.inner.emitted[index].load(Ordering::Acquire),
            None => true,
        }
    }

    /// True when the marker file was opened successfully.
    #[must_use]
    pub fn usable(&self) -> bool {
        self.inner
            .file
            .lock()
            .map(|file| file.is_some())
            .unwrap_or(false)
    }

    /// Appends one marker at most once per event kind. Best effort.
    pub fn emit(&self, event: Event, qpc: u64) {
        use std::sync::atomic::{AtomicBool, Ordering};
        let index = Event::ALL.iter().position(|candidate| *candidate == event);
        let Some(index) = index else { return };
        let AtomicBool { .. } = &self.inner.emitted[index];
        if self.inner.emitted[index].swap(true, Ordering::AcqRel) {
            return; // bounded stream: one line per event kind
        }
        let mut guard = match self.inner.file.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if let Some(file) = guard.as_mut() {
            let line = encode_line(event, qpc);
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

/// Reads the shell-side environment gate and opens the emitter when the
/// variable is present. This is the only process opt-in: the gate variable
/// must be set for this specific process by its launcher.
///
/// Unknown paths open unusable emitters rather than failing the shell.
#[must_use]
pub fn emitter_from_env(variable: &str) -> Option<Emitter> {
    let path = std::env::var_os(variable)?;
    Some(Emitter::open(Path::new(&path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn temp_marker_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "shell-startup-markers-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn line_format_is_tag_event_qpc_with_trailing_newline() {
        assert_eq!(
            encode_line(Event::ProcessEntry, 1234),
            "shell-startup-markers/v1 process_entry 1234\n"
        );
        assert_eq!(
            encode_line(Event::RenderSubmitted, u64::MAX),
            "shell-startup-markers/v1 render_submitted 18446744073709551615\n"
        );
    }

    #[test]
    fn parse_round_trips_all_event_names() {
        for event in Event::ALL {
            let stream = parse(&encode_line(event, 7));
            assert!(stream.valid());
            assert_eq!(stream.records.len(), 1);
            assert_eq!(stream.records[0], MarkerRecord { event, qpc: 7 });
            assert!(stream.invalid_lines.is_empty());
            assert!(!stream.truncated);
        }
    }

    #[test]
    fn parse_preserves_malformed_and_regression_lines_as_labeled_evidence() {
        let text = concat!(
            "shell-startup-markers/v1 process_entry 500\n",
            "garbage line\n",
            "shell-startup-markers/v1 not_an_event 600\n",
            "shell-startup-markers/v1 window_created notanumber\n",
            "other-format/v1 window_created 700\n",
            "shell-startup-markers/v1 render_submitted 900\n",
            "shell-startup-markers/v1 redraw_requested 800\n",
        );
        let stream = parse(text);
        assert_eq!(
            stream
                .records
                .iter()
                .map(|record| record.event)
                .collect::<Vec<_>>(),
            vec![Event::ProcessEntry, Event::RenderSubmitted]
        );
        let reasons: Vec<_> = stream
            .invalid_lines
            .iter()
            .map(|invalid| (invalid.sequence, invalid.reason))
            .collect();
        assert_eq!(
            reasons,
            vec![
                (1, "malformed_line"),
                (2, "unknown_event"),
                (3, "invalid_qpc"),
                (4, "unknown_format"),
                (6, "qpc_regression"),
            ]
        );
        assert_eq!(stream.invalid_lines[0].raw, "garbage line");
        assert!(!stream.valid());
    }

    #[test]
    fn parse_ignores_blank_lines_and_caps_raw_evidence_bytes() {
        let stream = parse("shell-startup-markers/v1 process_entry 10\n\n");
        assert_eq!(stream.records.len(), 1);

        let long = format!("{}\n", "x".repeat(MAX_LINE_BYTES * 4));
        let stream = parse(&long);
        assert_eq!(stream.records.len(), 0);
        assert_eq!(stream.invalid_lines.len(), 1);
        assert_eq!(stream.invalid_lines[0].raw.chars().count(), MAX_LINE_BYTES);
    }

    #[test]
    fn emitter_writes_each_event_kind_once_and_fails_open() {
        let path = temp_marker_path("emit-once");
        let emitter = Emitter::open(&path);
        assert!(emitter.usable());
        emitter.emit(Event::ProcessEntry, 10);
        emitter.emit(Event::WindowCreated, 20);
        // Second emission of the same kind must be dropped (bounded stream).
        emitter.emit(Event::ProcessEntry, 999);

        let text = std::fs::read_to_string(&path).unwrap();
        let stream = parse(&text);
        assert!(stream.valid());
        assert_eq!(
            stream
                .records
                .iter()
                .map(|record| record.event)
                .collect::<Vec<_>>(),
            vec![Event::ProcessEntry, Event::WindowCreated]
        );
        let _ = std::fs::remove_file(&path);

        // Unusable paths (e.g. a directory) degrade to an unusable emitter.
        let unusable = Emitter::open(&std::env::temp_dir());
        assert!(!unusable.usable());
        unusable.emit(Event::ProcessEntry, 5); // must not panic
    }

    #[test]
    fn emitter_from_env_follows_strict_process_scoped_gate() {
        let key = format!("SHELL_STARTUP_MARKERS_TEST_GATE_{}", std::process::id());
        assert!(emitter_from_env(&key).is_none());

        let path = temp_marker_path("env-gate");
        // SAFETY: test-only single-threaded process environment mutation of
        // a variable this test owns.
        unsafe {
            std::env::set_var(&key, OsString::from(&path));
        }
        let emitter = emitter_from_env(&key).expect("gate present opens emitter");
        assert!(emitter.usable());
        // SAFETY: as above.
        unsafe {
            std::env::remove_var(&key);
        }
        assert!(emitter_from_env(&key).is_none());

        emitter.emit(Event::ProcessEntry, 42);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("shell-startup-markers/v1 process_entry 42"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn qpc_helpers_return_monotonic_raw_values() {
        let first = qpc().expect("QPC is available on Windows");
        let second = qpc().expect("QPC is available on Windows");
        assert!(second >= first);
        let frequency = qpc_frequency().expect("QPC frequency is available");
        assert!(frequency > 0);
    }
}

#[cfg(test)]
mod has_emitted_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn has_emitted_reports_each_kind_only_after_its_emission() {
        let path = std::env::temp_dir().join(format!(
            "shell-startup-markers-has-emitted-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let emitter = Emitter::open(Path::new(&path));
        assert!(!emitter.has_emitted(Event::ProcessEntry));
        assert!(!emitter.has_emitted(Event::WindowCreated));
        emitter.emit(Event::ProcessEntry, 10);
        assert!(emitter.has_emitted(Event::ProcessEntry));
        assert!(!emitter.has_emitted(Event::WindowCreated));
        // A skipped duplicate emission must not flip the other kinds.
        emitter.emit(Event::ProcessEntry, 20);
        assert!(emitter.has_emitted(Event::ProcessEntry));
        assert!(!emitter.has_emitted(Event::WindowCreated));
        let _ = std::fs::remove_file(&path);
    }
}
