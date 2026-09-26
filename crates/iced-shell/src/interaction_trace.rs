//! Opt-in W14 app-event evidence; no display/present endpoint is exposed here.

use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
};

pub const VARIABLE: &str = "ICED_SHELL_INTERACTION_TRACE";
pub const LIMIT: usize = 200_000;

#[derive(Clone, Copy, Debug)]
pub struct Record {
    pub ticks: u64,
    pub event: &'static str,
    pub value: f32,
    pub duration_ticks: u64,
}

pub struct Summary {
    pub scroll_count: usize,
    pub view_intervals_ticks: Vec<u64>,
    pub present_intervals_ticks: Option<Vec<u64>>,
}

pub fn nearest_rank(sorted: &[u64], percentile: usize) -> Option<u64> {
    if sorted.is_empty() || !(1..=100).contains(&percentile) {
        return None;
    }
    sorted
        .get((sorted.len() * percentile).div_ceil(100) - 1)
        .copied()
}

/// Validates app records, not a frame/presentation trace. Zero and duplicate ticks fail closed.
pub fn summarize(records: &[Record], frequency: u64) -> Result<Summary, &'static str> {
    if records.is_empty() || frequency == 0 {
        return Err("missing trace or frequency");
    }
    let mut previous = 0;
    let mut last_view = None;
    let mut intervals = Vec::new();
    let mut scroll_count = 0;
    for record in records {
        if record.ticks <= previous
            || !record.value.is_finite()
            || record.duration_ticks > record.ticks
        {
            return Err("invalid or nonmonotonic QPC event");
        }
        match record.event {
            "wheel" | "scroll" | "view" | "redraw" | "resize" | "viewport" | "maximum"
            | "width" => {}
            _ => return Err("unknown event or unobserved presentation"),
        }
        if record.event == "scroll" {
            scroll_count += 1;
        }
        if record.event == "view" {
            if let Some(last) = last_view {
                intervals.push(record.ticks - last);
            }
            last_view = Some(record.ticks);
        }
        previous = record.ticks;
    }
    Ok(Summary {
        scroll_count,
        view_intervals_ticks: intervals,
        present_intervals_ticks: None,
    })
}

/// Only the explicitly selected reader child creates this stream. Every write is flushed
/// to make missing/truncated process evidence observable; write overhead is recorded separately.
#[derive(Debug)]
pub struct Trace {
    file: std::fs::File,
    count: usize,
    frequency: u64,
    write_ticks: u64,
    peak_write_ticks: u64,
}
impl Trace {
    pub fn create(path: &Path) -> io::Result<Self> {
        let frequency = shell_startup_markers::qpc_frequency()
            .ok_or_else(|| io::Error::other("QPC frequency unavailable"))?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        writeln!(file, "w14/v1,{frequency},app,qpc")?;
        file.flush()?;
        Ok(Self {
            file,
            count: 0,
            frequency,
            write_ticks: 0,
            peak_write_ticks: 0,
        })
    }
    pub fn frequency(&self) -> u64 {
        self.frequency
    }
    pub fn record(
        &mut self,
        event: &'static str,
        value: f32,
        start: Option<u64>,
    ) -> io::Result<()> {
        if self.count >= LIMIT {
            return Err(io::Error::other("W14 trace event limit"));
        }
        let ticks =
            shell_startup_markers::qpc().ok_or_else(|| io::Error::other("QPC unavailable"))?;
        let duration = start.map_or(0, |begin| ticks.saturating_sub(begin));
        writeln!(self.file, "{event},{ticks},{value:.3},{duration}")?;
        self.file.flush()?;
        let end = shell_startup_markers::qpc()
            .ok_or_else(|| io::Error::other("QPC unavailable after write"))?;
        let elapsed = end.saturating_sub(ticks);
        self.write_ticks = self.write_ticks.saturating_add(elapsed);
        self.peak_write_ticks = self.peak_write_ticks.max(elapsed);
        self.count += 1;
        Ok(())
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        // Drop may also run during panic unwind. Consumers require an independently
        // verified intentional close and exit 0; this footer excludes its own write.
        let _ = writeln!(
            self.file,
            "overhead,{},{},{}",
            self.write_ticks, self.peak_write_ticks, self.count
        );
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_repeated_and_unattributed_events() {
        assert!(summarize(&[], 10_000).is_err());
        assert!(summarize(&[sample(10, "wheel"), sample(10, "scroll")], 10_000).is_err());
        assert!(summarize(&[sample(10, "present")], 10_000).is_err());
    }

    #[test]
    fn nearest_rank_uses_raw_intervals_without_idle_as_frames() {
        let events = [
            sample(10, "wheel"),
            sample(20, "scroll"),
            sample(50, "view"),
            sample(70, "scroll"),
            sample(110, "view"),
        ];
        let result = summarize(&events, 10_000).unwrap();
        assert_eq!(result.scroll_count, 2);
        assert_eq!(result.view_intervals_ticks, vec![60]);
        assert_eq!(nearest_rank(&[2, 5, 9, 11, 40], 95), Some(40));
        assert_eq!(nearest_rank(&[], 50), None);
        assert_eq!(result.present_intervals_ticks, None);
    }

    fn sample(ticks: u64, event: &'static str) -> Record {
        Record {
            ticks,
            event,
            value: 0.0,
            duration_ticks: 0,
        }
    }
}
