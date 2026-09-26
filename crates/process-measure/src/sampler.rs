//! One-sample collection and the narrow production-used query fault seam.

use crate::job::{Launched, MemberSnapshot};
use crate::metrics::{CpuObservation, ObservationError, derive_cpu_delta};
use crate::records::{ApiError, CpuDerivedRecord, HelperSnapshotRecord, Metric, SampleRecord};
use crate::{clock, win};
use windows_sys::Win32::Foundation::HANDLE;

pub struct SampleState {
    pub run_id: String,
    pub frequency: u64,
    pub logical_processors: Option<u32>,
    pub baseline: Option<(u64, CpuObservation)>,
    pub invalid_since_baseline: bool,
    pub sampler_cpu_baseline: Option<(u64, u64)>,
}

#[derive(Debug, Clone, Copy, Default)]
#[doc(hidden)]
pub struct SampleFault {
    pub target_cpu: bool,
    pub required_memory: bool,
    pub sampler_cpu: bool,
    pub fatal: bool,
}

fn failed<T>(e: &ApiError) -> Metric<T> {
    Metric::Unavailable { error: e.clone() }
}
fn injected(api: &str) -> ApiError {
    ApiError {
        api: api.into(),
        os_code: 0,
        reason: "injected_test_failure".into(),
    }
}
fn terminalize<T>(metric: Metric<T>, terminal: bool) -> Metric<T> {
    if terminal && matches!(metric, Metric::Unavailable { .. }) {
        Metric::TerminalUnavailable {
            reason: "process_has_exited_during_query".into(),
        }
    } else {
        metric
    }
}

pub fn collect(
    state: &mut SampleState,
    target: &Launched,
    sampler_handle: HANDLE,
    sequence: u64,
    scheduled_qpc: u64,
    ex2_supported: bool,
    terminal: bool,
) -> Result<SampleRecord, ApiError> {
    collect_with_fault(
        state,
        target,
        sampler_handle,
        sequence,
        scheduled_qpc,
        ex2_supported,
        terminal,
        SampleFault::default(),
    )
}

#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn collect_with_fault(
    state: &mut SampleState,
    target: &Launched,
    sampler_handle: HANDLE,
    sequence: u64,
    scheduled_qpc: u64,
    ex2_supported: bool,
    terminal: bool,
    fault: SampleFault,
) -> Result<SampleRecord, ApiError> {
    if fault.fatal {
        return Err(injected("injected_fatal_collect"));
    }
    let start = clock::qpc()?;
    let target_time_result = if fault.target_cpu {
        Err(injected("GetProcessTimes(target)"))
    } else {
        win::process_times(target.process.raw(), start)
    };
    let cpu_point = clock::qpc()?;
    let mut memory = win::memory_metrics(target.process.raw(), ex2_supported, terminal);
    if fault.required_memory {
        let e = injected("GetProcessMemoryInfo");
        memory.private_working_set_bytes = failed(&e);
        memory.private_commit_bytes = failed(&e);
    }
    let sampler_times = if fault.sampler_cpu {
        Err(injected("GetProcessTimes(sampler)"))
    } else {
        win::process_times(sampler_handle, cpu_point)
    };
    let member_snapshot = target.member_snapshot();
    let exited_after = target.process.wait(0).unwrap_or(false);
    let terminal_effective = terminal || exited_after;
    memory.private_working_set_bytes =
        terminalize(memory.private_working_set_bytes, terminal_effective);
    memory.private_commit_bytes = terminalize(memory.private_commit_bytes, terminal_effective);
    memory.total_working_set_bytes =
        terminalize(memory.total_working_set_bytes, terminal_effective);
    let end = clock::qpc()?;

    let (user, kernel, observation) = match &target_time_result {
        Ok((t, _)) => (
            Metric::Valid {
                value: t.user_100ns,
            },
            Metric::Valid {
                value: t.kernel_100ns,
            },
            Ok(CpuObservation {
                user_100ns: t.user_100ns,
                kernel_100ns: t.kernel_100ns,
                qpc_after: cpu_point,
            }),
        ),
        Err(e) if terminal_effective => (
            Metric::TerminalUnavailable {
                reason: "process_has_exited".into(),
            },
            Metric::TerminalUnavailable {
                reason: "process_has_exited".into(),
            },
            Err(ObservationError {
                api: "GetProcessTimes",
                os_code: e.os_code,
            }),
        ),
        Err(e) => (
            failed(e),
            failed(e),
            Err(ObservationError {
                api: "GetProcessTimes",
                os_code: e.os_code,
            }),
        ),
    };
    let had_baseline = state.baseline.is_some();
    let derived = derive_cpu_delta(
        state.baseline,
        observation,
        state.frequency,
        state.logical_processors.map(u64::from),
        state.invalid_since_baseline,
    );
    if let Ok(obs) = observation {
        state.baseline = Some((sequence, obs));
        state.invalid_since_baseline = false;
    } else if state.baseline.is_some() {
        state.invalid_since_baseline = true;
    }

    let sampler_pair = match sampler_times {
        Ok((t, _)) => {
            let current = (t.user_100ns, t.kernel_100ns);
            let pair = match state.sampler_cpu_baseline {
                Some((u, k)) => match (current.0.checked_sub(u), current.1.checked_sub(k)) {
                    (Some(du), Some(dk)) => {
                        (Metric::Valid { value: du }, Metric::Valid { value: dk })
                    }
                    _ => (
                        failed(&ApiError {
                            api: "GetProcessTimes(sampler)".into(),
                            os_code: 0,
                            reason: "counter_regression".into(),
                        }),
                        failed(&ApiError {
                            api: "GetProcessTimes(sampler)".into(),
                            os_code: 0,
                            reason: "counter_regression".into(),
                        }),
                    ),
                },
                None => (
                    failed(&ApiError {
                        api: "GetProcessTimes(sampler)".into(),
                        os_code: 0,
                        reason: "first_observation".into(),
                    }),
                    failed(&ApiError {
                        api: "GetProcessTimes(sampler)".into(),
                        os_code: 0,
                        reason: "first_observation".into(),
                    }),
                ),
            };
            state.sampler_cpu_baseline = Some(current);
            pair
        }
        Err(e) => {
            state.sampler_cpu_baseline = None;
            (failed(&e), failed(&e))
        }
    };
    let helper_snapshot = match member_snapshot {
        MemberSnapshot::Complete { pids, assigned } => {
            HelperSnapshotRecord::Complete { pids, assigned }
        }
        MemberSnapshot::Truncated { assigned, limit } => {
            HelperSnapshotRecord::Truncated { assigned, limit }
        }
        MemberSnapshot::Failed(error) => HelperSnapshotRecord::Failed { error },
    };
    Ok(SampleRecord {
        schema: crate::records::SCHEMA_VERSION.into(),
        run_id: state.run_id.clone(),
        sequence,
        scheduled_qpc,
        sample_start_qpc: start,
        cpu_observation_qpc: cpu_point,
        sample_end_qpc: end,
        schedule_slip_qpc_ticks: i128::from(start)
            .saturating_sub(i128::from(scheduled_qpc))
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
            as i64,
        user_cpu_100ns: user,
        kernel_cpu_100ns: kernel,
        private_working_set_bytes: memory.private_working_set_bytes,
        private_commit_bytes: memory.private_commit_bytes,
        total_working_set_bytes: memory.total_working_set_bytes,
        derived_cpu: CpuDerivedRecord {
            baseline_sequence: had_baseline.then_some(derived.delta.baseline_seq),
            spans_invalid_observation: derived.delta.spans_invalid,
            delta_user_100ns: derived.delta.user_100ns,
            delta_kernel_100ns: derived.delta.kernel_100ns,
            elapsed_qpc_ticks: derived.delta.wall_qpc,
            elapsed_100ns: derived.delta.wall_100ns,
            cpu_percent_one_logical: derived.cpu_percent_one_logical,
            cpu_percent_machine: derived.cpu_percent_machine,
            unavailable_reasons: derived.reasons.into_iter().map(str::to_owned).collect(),
        },
        sampler_delta_user_cpu_100ns: sampler_pair.0,
        sampler_delta_kernel_cpu_100ns: sampler_pair.1,
        helper_snapshot,
        terminal_sample: terminal_effective,
    })
}
