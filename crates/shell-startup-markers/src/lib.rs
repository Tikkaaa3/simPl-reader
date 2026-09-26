//! Bounded, opt-in QPC startup markers used by the Iced prototype.
//!
//! Format version `shell-startup-markers/v1`: one UTF-8 line per marker,
//! `shell-startup-markers/v1 <event> <qpc_ticks>\n`, where `qpc_ticks` is a
//! raw unsigned [`QueryPerformanceCounter`] value from the system-wide
//! monotonic QPC clock. UTC FILETIME never appears in a marker line, so the
//! stream can only be aligned on the QPC timeline. The stream is bounded by
//! construction: each event kind is emitted at most once per process, and
//! consumers cap what they read and parse.
//!
//! App-declared markers are supporting evidence, not proof of a displayed frame.

pub mod markers;
pub use markers::{
    Emitter, Event, FORMAT, InvalidLine, MarkerRecord, MarkerStream, emitter_from_env, encode_line,
    parse, qpc, qpc_frequency,
};
