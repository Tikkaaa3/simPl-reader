//! Pure CPU delta and normalization arithmetic (task T-004 "Raw measurements
//! and time semantics").
//!
//! No Win32 calls here: the sampler feeds cumulative `GetProcessTimes`
//! observations (user and kernel time, preserved separately in 100 ns units)
//! plus QPC elapsed ticks, and this module computes the documented derived
//! values. Rules:
//!
//! - Deltas are computed between consecutive *valid* CPU observations, never
//!   against the requested sampling interval.
//! - One-logical percentages may exceed 100 and are never clamped.
//! - Machine normalization divides by the system-wide logical-processor count.
//! - First observations, failed queries, counter regression, non-positive
//!   elapsed time, invalid frequency/denominator, and overflow produce an
//!   explicitly unavailable derived value with a stable reason string.
//! - Delta CPU durations and elapsed durations are preserved so the
//!   percentages can be independently reconstructed:
//!   `cpu_percent_one_logical = 100 * delta_cpu_100ns / delta_wall_100ns`.

/// Cumulative CPU counters for one process at one observation instant.
///
/// `user_100ns` / `kernel_100ns` are the raw `GetProcessTimes` values in
/// 100 ns units (valid zero values included); `qpc_after` is the monotonic
/// QPC counter captured right after the query returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuObservation {
    pub user_100ns: u64,
    pub kernel_100ns: u64,
    pub qpc_after: u64,
}

/// A failed CPU observation (the API call itself failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationError {
    pub api: &'static str,
    pub os_code: u32,
}

/// A completed delta between two valid CPU observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuDelta {
    /// Sequence number of the baseline (earlier) observation.
    pub baseline_seq: u64,
    /// True when at least one invalid observation lies between the baseline
    /// and the current observation; the delta is still computed between the
    /// two valid observations, but the span is labeled so no invalid
    /// observation is bridged silently.
    pub spans_invalid: bool,
    pub user_100ns: Option<u64>,
    pub kernel_100ns: Option<u64>,
    /// Raw QPC distance between the two observation instants.
    pub wall_qpc: Option<u64>,
    /// The same elapsed distance converted to 100 ns units (the denominator
    /// of the percentages; reconstructed deterministically from `wall_qpc`
    /// and the manifest-recorded QPC frequency).
    pub wall_100ns: Option<u64>,
}

/// The derived-CPU record for one sample.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedCpu {
    pub delta: CpuDelta,
    pub cpu_percent_one_logical: Option<f64>,
    pub cpu_percent_machine: Option<f64>,
    /// Stable reason strings explaining every unavailability in this record
    /// (never empty strings; absent when everything is available).
    pub reasons: Vec<&'static str>,
}

/// Convert a QPC tick distance to 100 ns units with deterministic rounding.
/// Returns `None` for a zero frequency (invalid clock).
pub fn qpc_ticks_to_100ns(ticks: u64, frequency: u64) -> Option<u64> {
    if frequency == 0 {
        return None;
    }
    let ticks = u128::from(ticks);
    let freq = u128::from(frequency);
    // Deterministic round-to-nearest, ties away from zero.
    u64::try_from((ticks * 10_000_000 + freq / 2) / freq).ok()
}

/// Derive the CPU delta and normalized percentages for one sample.
///
/// `baseline` is the last valid CPU observation (`None` before the second
/// valid observation). `current` is the fresh observation or its query
/// error. `spans_invalid` must be set by the caller when at least one
/// invalid observation lies between the baseline and this sample.
pub fn derive_cpu_delta(
    baseline: Option<(u64, CpuObservation)>,
    current: Result<CpuObservation, ObservationError>,
    frequency: u64,
    logical_processors: Option<u64>,
    spans_invalid: bool,
) -> DerivedCpu {
    let mut reasons: Vec<&'static str> = Vec::new();

    let Some((baseline_seq, base)) = baseline else {
        reasons.push("first_observation");
        return DerivedCpu {
            delta: CpuDelta {
                baseline_seq: 0,
                spans_invalid,
                user_100ns: None,
                kernel_100ns: None,
                wall_qpc: None,
                wall_100ns: None,
            },
            cpu_percent_one_logical: None,
            cpu_percent_machine: None,
            reasons,
        };
    };

    let Ok(cur) = current else {
        reasons.push("query_failed");
        return DerivedCpu {
            delta: CpuDelta {
                baseline_seq,
                spans_invalid,
                user_100ns: None,
                kernel_100ns: None,
                wall_qpc: None,
                wall_100ns: None,
            },
            cpu_percent_one_logical: None,
            cpu_percent_machine: None,
            reasons,
        };
    };

    if cur.user_100ns < base.user_100ns || cur.kernel_100ns < base.kernel_100ns {
        reasons.push("counter_regression");
        return DerivedCpu {
            delta: CpuDelta {
                baseline_seq,
                spans_invalid,
                user_100ns: None,
                kernel_100ns: None,
                wall_qpc: None,
                wall_100ns: None,
            },
            cpu_percent_one_logical: None,
            cpu_percent_machine: None,
            reasons,
        };
    }

    let user_delta = cur.user_100ns - base.user_100ns;
    let kernel_delta = cur.kernel_100ns - base.kernel_100ns;

    let wall_qpc = cur.qpc_after.checked_sub(base.qpc_after).filter(|w| *w > 0);
    let Some(wall_qpc) = wall_qpc else {
        reasons.push("nonpositive_elapsed");
        return DerivedCpu {
            delta: CpuDelta {
                baseline_seq,
                spans_invalid,
                user_100ns: Some(user_delta),
                kernel_100ns: Some(kernel_delta),
                wall_qpc: None,
                wall_100ns: None,
            },
            cpu_percent_one_logical: None,
            cpu_percent_machine: None,
            reasons,
        };
    };

    let Some(wall_100ns) = qpc_ticks_to_100ns(wall_qpc, frequency) else {
        reasons.push("invalid_frequency");
        return DerivedCpu {
            delta: CpuDelta {
                baseline_seq,
                spans_invalid,
                user_100ns: Some(user_delta),
                kernel_100ns: Some(kernel_delta),
                wall_qpc: Some(wall_qpc),
                wall_100ns: None,
            },
            cpu_percent_one_logical: None,
            cpu_percent_machine: None,
            reasons,
        };
    };

    // The summed CPU delta overflows u64 only with more than ~584 CPU-years
    // per interval; guarded anyway so the record can never carry a wrapped 0.
    let cpu_total = u128::from(user_delta) + u128::from(kernel_delta);
    let percent_one = if cpu_total > u128::from(u64::MAX) {
        reasons.push("counter_overflow");
        None
    } else {
        let one = 100.0 * cpu_total as u64 as f64 / wall_100ns as f64;
        if one.is_finite() {
            Some(one)
        } else {
            reasons.push("nonfinite_result");
            None
        }
    };

    let percent_machine = match (percent_one, logical_processors) {
        (Some(one), Some(lp)) if lp > 0 => Some(one / lp as f64),
        (Some(_), Some(0)) => {
            reasons.push("invalid_denominator");
            None
        }
        (Some(_), None) => {
            reasons.push("denominator_unknown");
            None
        }
        (None, _) => None,
        // Unreachable by the guards above; kept exhaustive defensively.
        (_, Some(_)) => None,
    };

    DerivedCpu {
        delta: CpuDelta {
            baseline_seq,
            spans_invalid,
            user_100ns: Some(user_delta),
            kernel_100ns: Some(kernel_delta),
            wall_qpc: Some(wall_qpc),
            wall_100ns: Some(wall_100ns),
        },
        cpu_percent_one_logical: percent_one,
        cpu_percent_machine: percent_machine,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREQ_100NS: u64 = 10_000_000; // QPC that ticks 100 ns units

    fn obs(user: u64, kernel: u64, qpc: u64) -> CpuObservation {
        CpuObservation {
            user_100ns: user,
            kernel_100ns: kernel,
            qpc_after: qpc,
        }
    }

    #[test]
    fn delta_with_multi_thread_work_exceeds_100_percent_one_logical() {
        // 5 s wall during which cumulative user CPU advanced by 10 s and
        // kernel CPU by 5 s: three logical processors' worth of work.
        let base = (7, obs(100_000_000, 50_000_000, 0));
        let cur = obs(200_000_000, 100_000_000, 50_000_000);
        let d = derive_cpu_delta(Some(base), Ok(cur), FREQ_100NS, Some(8), false);
        assert_eq!(d.delta.user_100ns, Some(100_000_000));
        assert_eq!(d.delta.kernel_100ns, Some(50_000_000));
        assert_eq!(d.delta.wall_qpc, Some(50_000_000));
        assert_eq!(d.delta.wall_100ns, Some(50_000_000));
        assert_eq!(d.delta.baseline_seq, 7);
        // 15 s CPU / 5 s wall = 300% of one logical processor, never clamped.
        let one = d.cpu_percent_one_logical.expect("one-logical percent");
        assert!((one - 300.0).abs() < 1e-9, "got {one}");
        let machine = d.cpu_percent_machine.expect("machine percent");
        assert!((machine - 37.5).abs() < 1e-9, "got {machine}");
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn first_observation_has_no_derived_delta() {
        let cur = obs(1_000, 2_000, 1_000_000);
        let d = derive_cpu_delta(None, Ok(cur), FREQ_100NS, Some(4), false);
        assert_eq!(d.cpu_percent_one_logical, None);
        assert_eq!(d.cpu_percent_machine, None);
        assert_eq!(d.delta.user_100ns, None);
        assert_eq!(d.delta.baseline_seq, 0);
        assert!(d.reasons.contains(&"first_observation"));
    }

    #[test]
    fn failed_query_is_unavailable_and_not_zero() {
        let base = (3, obs(100, 100, 10));
        let d = derive_cpu_delta(
            Some(base),
            Err(ObservationError {
                api: "GetProcessTimes",
                os_code: 5,
            }),
            FREQ_100NS,
            Some(8),
            false,
        );
        assert_eq!(d.cpu_percent_one_logical, None);
        assert_eq!(d.delta.user_100ns, None);
        assert!(d.reasons.contains(&"query_failed"));
    }

    #[test]
    fn counter_regression_is_explicit() {
        let base = (1, obs(500, 500, 100));
        let cur = obs(499, 500, 200); // user time went backwards
        let d = derive_cpu_delta(Some(base), Ok(cur), FREQ_100NS, Some(8), false);
        assert_eq!(d.cpu_percent_one_logical, None);
        assert_eq!(d.delta.user_100ns, None);
        assert!(d.reasons.contains(&"counter_regression"));
    }

    #[test]
    fn nonpositive_elapsed_time_is_explicit() {
        let base = (1, obs(100, 100, 1_000));
        let cur = obs(200, 200, 1_000); // no wall clock progress
        let d = derive_cpu_delta(Some(base), Ok(cur), FREQ_100NS, Some(8), false);
        assert_eq!(d.cpu_percent_one_logical, None);
        assert!(d.reasons.contains(&"nonpositive_elapsed"));
    }

    #[test]
    fn zero_qpc_frequency_is_invalid() {
        let base = (1, obs(100, 100, 1_000));
        let cur = obs(200, 200, 2_000);
        let d = derive_cpu_delta(Some(base), Ok(cur), 0, Some(8), false);
        assert_eq!(d.cpu_percent_one_logical, None);
        assert_eq!(d.delta.wall_100ns, None);
        assert!(d.reasons.contains(&"invalid_frequency"));
    }

    #[test]
    fn unknown_denominator_keeps_one_logical_percent() {
        let base = (1, obs(0, 0, 0));
        let cur = obs(5_000_000, 0, 10_000_000); // 0.5 s CPU in 1 s wall
        let d = derive_cpu_delta(Some(base), Ok(cur), FREQ_100NS, None, false);
        let one = d
            .cpu_percent_one_logical
            .expect("one-logical still computable");
        assert!((one - 50.0).abs() < 1e-9);
        assert_eq!(d.cpu_percent_machine, None);
        assert!(d.reasons.contains(&"denominator_unknown"));
    }

    #[test]
    fn zero_logical_processors_is_invalid_denominator() {
        let base = (1, obs(0, 0, 0));
        let cur = obs(1_000_000, 0, 1_000_000);
        let d = derive_cpu_delta(Some(base), Ok(cur), FREQ_100NS, Some(0), false);
        assert_eq!(d.cpu_percent_machine, None);
        assert!(d.reasons.contains(&"invalid_denominator"));
    }

    #[test]
    fn delta_across_an_invalid_observation_is_labeled() {
        let base = (1, obs(0, 0, 0));
        let cur = obs(2_500_000, 2_500_000, 20_000_000); // 0.5 s CPU in 2 s wall
        let d = derive_cpu_delta(Some(base), Ok(cur), FREQ_100NS, Some(4), true);
        assert!(d.delta.spans_invalid);
        assert_eq!(d.delta.baseline_seq, 1);
        let one = d.cpu_percent_one_logical.expect("valid pair still derives");
        assert!((one - 25.0).abs() < 1e-9);
        let machine = d.cpu_percent_machine.expect("machine percent");
        assert!((machine - 6.25).abs() < 1e-9);
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn conversion_and_summed_counter_overflow_are_unavailable() {
        assert_eq!(qpc_ticks_to_100ns(u64::MAX, 1), None);
        let base = (1, obs(0, 0, 0));
        let current = obs(u64::MAX, 1, 10_000_000);
        let d = derive_cpu_delta(Some(base), Ok(current), FREQ_100NS, Some(4), false);
        assert_eq!(d.cpu_percent_one_logical, None);
        assert!(d.reasons.contains(&"counter_overflow"));
    }

    #[test]
    fn tick_conversion_rounds_deterministically() {
        assert_eq!(qpc_ticks_to_100ns(1, FREQ_100NS), Some(1));
        assert_eq!(qpc_ticks_to_100ns(50_000_000, FREQ_100NS), Some(50_000_000));
        // 1 tick of a 3 MHz clock = 0.333... us = 3.333... *100ns; the
        // conversion must be deterministic for independent reconstruction.
        assert_eq!(qpc_ticks_to_100ns(1, 3_000_000), Some(3));
        assert_eq!(qpc_ticks_to_100ns(3, 3_000_000), Some(10));
        assert_eq!(qpc_ticks_to_100ns(1, 0), None);
    }
}
