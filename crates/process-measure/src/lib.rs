//! `process-measure` — Windows process resource measurement library (I-001/W4).
//!
//! The crate is a thin CLI (`process-measure`) plus this library so that the
//! measurement core can be exercised with injected query/clock/output
//! failures in tests. The Win32 process/job/queries backends live behind a
//! compact internal seam; the arithmetic (CPU deltas and normalizations),
//! argument validation, command-line encoding, and artifact schema are pure
//! and independently testable.
//!
//! Measurement semantics are defined by task T-004 and documented in
//! `crates/process-measure/README.md`. Short version: raw cumulative user and
//! kernel CPU time in 100 ns units from `GetProcessTimes`, private working
//! set (`PrivateWorkingSetSize`) and private commit (`PrivateUsage`) bytes
//! from `GetProcessMemoryInfo` with `PROCESS_MEMORY_COUNTERS_EX2`, explicit
//! validity per field (a genuine zero is valid, a failed query is never
//! written as zero), and all metrics explicitly root-process-only.

pub mod args;
pub mod clock;
pub mod cmdline;
pub mod job;
pub mod metrics;
pub mod records;
pub mod sampler;
pub mod win;

pub mod run;
