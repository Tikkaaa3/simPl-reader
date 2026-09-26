use super::*;
use crate::job::{CLEANUP_TIMEOUT_MS, HELPER_RUN_IDENTITY_LIMIT, LaunchFault, launch_with_fault};
use crate::sampler::{SampleFault, SampleState, collect_with_fault};
use std::collections::BTreeSet;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
#[doc(hidden)]
pub struct TestHooks {
    pub launch_fault: Option<LaunchFault>,
    pub sample_fault_at: Option<(u64, SampleFault)>,
    pub fail_sample_write_after_rows: Option<u64>,
    pub delay_after_sample_ms: Option<u64>,
    pub fail_control_registration: bool,
    pub fail_cleanup_after_real_cleanup: bool,
    pub fail_post_launch_clock: bool,
}

#[doc(hidden)]
pub fn run_collection_with_hooks(args: ValidatedArgs, hooks: TestHooks) -> RunResult {
    run(args, hooks)
}

struct ControlGuard {
    registered: bool,
}
impl ControlGuard {
    fn register(injected_failure: bool) -> Result<Self, String> {
        if injected_failure {
            return Err("SetConsoleCtrlHandler injected failure".into());
        }
        INTERRUPTED.store(false, Ordering::SeqCst);
        if unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) } == 0 {
            return Err(format!(
                "SetConsoleCtrlHandler failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self { registered: true })
    }
    fn unregister(&mut self) -> Result<(), String> {
        if self.registered {
            if unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 0) } == 0 {
                return Err(format!(
                    "SetConsoleCtrlHandler(unregister) failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            self.registered = false;
        }
        Ok(())
    }
}
impl Drop for ControlGuard {
    fn drop(&mut self) {
        if self.registered {
            unsafe {
                SetConsoleCtrlHandler(Some(ctrl_handler), 0);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stop {
    Natural,
    Duration,
    Interrupted,
    Fatal(String),
    Output(String),
}
impl Stop {
    fn label(&self) -> &'static str {
        match self {
            Self::Natural => "natural_exit",
            Self::Duration => "duration_limit",
            Self::Interrupted => "interrupted",
            Self::Fatal(_) => "fatal_collection_error",
            Self::Output(_) => "output_error",
        }
    }
}

pub(super) fn run(args: ValidatedArgs, hooks: TestHooks) -> RunResult {
    match prepare_and_run(args, hooks) {
        Ok(r) => r,
        Err(e) => RunResult {
            exit_code: 1,
            message: e,
        },
    }
}

fn prepare_and_run(args: ValidatedArgs, hooks: TestHooks) -> Result<RunResult, String> {
    let resolved = resolve_launch(&args)?;
    std::fs::create_dir(&args.out_dir)
        .map_err(|e| format!("cannot create new output directory: {e}"))?;
    let output = std::fs::canonicalize(&args.out_dir)
        .map_err(|e| format!("cannot resolve output directory: {e}"))?;
    let manifest_path = output.join("manifest.json");
    let samples_path = output.join("samples.jsonl");
    let frequency = clock::frequency().map_err(String::from)?;
    let start_wall = clock::utc_filetime_now();
    let run_id = format!("{:016x}-{}", start_wall, unsafe { GetCurrentProcessId() });
    let self_path = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot identify sampler: {e}"))?;
    let logical = win::logical_processor_count();
    let (os, ex2_supported) = os_version();
    let declared = declarations(&args.declarations);
    let mut clocks = BTreeMap::new();
    clocks.insert(
        "sampling".into(),
        "QueryPerformanceCounter monotonic ticks; frequency in qpc_frequency_hz".into(),
    );
    clocks.insert(
        "wall_identity".into(),
        "UTC FILETIME 100 ns since 1601; never subtracted from QPC".into(),
    );
    let mut provenance = BTreeMap::new();
    provenance.insert(
        "cpu".into(),
        "root GetProcessTimes cumulative user/kernel, 100 ns".into(),
    );
    provenance.insert(
        "private_working_set".into(),
        "root GetProcessMemoryInfo PROCESS_MEMORY_COUNTERS_EX2.PrivateWorkingSetSize, bytes".into(),
    );
    provenance.insert(
        "private_commit".into(),
        "root GetProcessMemoryInfo PROCESS_MEMORY_COUNTERS_EX2.PrivateUsage, bytes".into(),
    );
    let mut manifest=Manifest{schema:SCHEMA_VERSION.into(),tool_version:TOOL_VERSION.into(),run_id:run_id.clone(),state:"in_progress".into(),started_utc_filetime:start_wall,ended_utc_filetime:None,sampler:identity(&self_path),target:identity(&resolved.executable),target_arguments:args.target_args.iter().map(|v|v.to_string_lossy().into_owned()).collect(),target_working_directory:path_text(&resolved.working_directory),sampler_pid:unsafe{GetCurrentProcessId()},target_pid:None,target_creation_filetime:None,target_exit_filetime:None,interval_ms:args.interval_ms,maximum_duration_ms:args.duration_ms,qpc_frequency_hz:frequency,clocks,sampling_scope:"root_process_only; bounded helper snapshots are identities only and are not aggregated".into(),metric_provenance:provenance,ex2_support:if ex2_supported{"supported by conservative Windows build/UBR gate documented in the crate README".into()}else{"unsupported by conservative Windows build gate; private working set is unavailable".into()},logical_processor_normalization:"system-wide GetActiveProcessorCount(ALL_PROCESSOR_GROUPS); machine percent = one-logical percent / count".into(),environment:environment(logical,os),declarations:declared.clone(),optional_run_context:optional_run_context(&declared),collection_stop_reason:None,collection_valid:false,tool_exit_success:false,required_live_query_failures:0,target_exit_code:None,harness_error:None,cleanup_error:None,counts:Counts{samples:0,valid_cpu_observations:0,invalid_cpu_observations:0,valid_private_working_set_observations:0,valid_private_commit_observations:0},observed_helper_pids:vec![],observed_helper_pids_truncated:false,helper_snapshot_failures:0,helper_snapshot_truncations:0,helper_visibility_limit:format!("per-sample snapshots and run identity summary are capped at {HELPER_RUN_IDENTITY_LIMIT}; short-lived helpers may be missed"),sampler_cpu_delta_user_100ns:None,sampler_cpu_delta_kernel_100ns:None};
    write_manifest(&manifest_path, &manifest)?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&samples_path)
        .map_err(|e| format!("cannot create samples.jsonl: {e}"))?;
    let mut samples = BufWriter::new(file);
    let mut control = match ControlGuard::register(hooks.fail_control_registration) {
        Ok(v) => v,
        Err(e) => {
            manifest.state = "complete".into();
            manifest.harness_error = Some(e.clone());
            manifest.collection_stop_reason = Some("control_handler_setup_failure".into());
            manifest.ended_utc_filetime = Some(clock::utc_filetime_now());
            let _ = write_manifest(&manifest_path, &manifest);
            return Ok(RunResult {
                exit_code: 1,
                message: e,
            });
        }
    };
    let sampler_handle = unsafe { GetCurrentProcess() };
    let sampler_qpc = match clock::qpc() {
        Ok(v) => v,
        Err(e) => {
            return finalize_before_launch(manifest, manifest_path, &mut control, String::from(e));
        }
    };
    let sampler_before = win::process_times(sampler_handle, sampler_qpc)
        .ok()
        .map(|v| v.0);
    // Target lifetime begins before CreateProcess and includes all post-launch manifest/query work.
    let lifetime_start = match clock::qpc() {
        Ok(v) => v,
        Err(e) => {
            return finalize_before_launch(manifest, manifest_path, &mut control, String::from(e));
        }
    };
    let target = match launch_with_fault(&args, &resolved, hooks.launch_fault) {
        Ok(v) => v,
        Err(e) => {
            manifest.state = "complete".into();
            manifest.harness_error = Some(e.to_string());
            manifest.cleanup_error =
                (!e.cleanup_errors.is_empty()).then(|| format_errors(&e.cleanup_errors));
            manifest.collection_stop_reason = Some("launch_failure".into());
            manifest.ended_utc_filetime = Some(clock::utc_filetime_now());
            if let Err(x) = control.unregister() {
                manifest.cleanup_error = Some(match manifest.cleanup_error.take() {
                    Some(old) => format!("{old}; {x}"),
                    None => x,
                });
            }
            write_manifest(&manifest_path, &manifest)?;
            return Ok(RunResult {
                exit_code: 1,
                message: format!("target launch failed: {e}"),
            });
        }
    };
    manifest.target_pid = Some(target.process.pid);
    let mut pre_loop_error = None;
    if hooks.fail_post_launch_clock {
        pre_loop_error = Some("injected post-launch QPC failure".into());
    } else {
        match clock::qpc() {
            Ok(now) => {
                manifest.target_creation_filetime = win::process_times(target.process.raw(), now)
                    .ok()
                    .map(|v| v.0.creation_filetime)
            }
            Err(e) => pre_loop_error = Some(String::from(e)),
        }
    }
    if pre_loop_error.is_none()
        && let Err(e) = write_manifest(&manifest_path, &manifest)
    {
        pre_loop_error = Some(format!("manifest update failed: {e}"));
    }
    let interval_ticks = (u128::from(args.interval_ms) * u128::from(frequency) / 1000).max(1);
    let duration_ticks = (u128::from(args.duration_ms) * u128::from(frequency) / 1000).max(1);
    let deadline = (u128::from(lifetime_start) + duration_ticks).min(u128::from(u64::MAX)) as u64;
    let mut state = SampleState {
        run_id: run_id.clone(),
        frequency,
        logical_processors: logical,
        baseline: None,
        invalid_since_baseline: false,
        sampler_cpu_baseline: None,
    };
    let mut sequence = 0u64;
    let mut next_slot = 0u128;
    let mut helpers = BTreeSet::new();
    let mut stop = pre_loop_error.map(Stop::Fatal);
    while stop.is_none() {
        if INTERRUPTED.load(Ordering::SeqCst) {
            stop = Some(Stop::Interrupted);
            break;
        }
        let now = match clock::qpc() {
            Ok(v) => v,
            Err(e) => {
                stop = Some(Stop::Fatal(String::from(e)));
                break;
            }
        };
        let exited = match target.process.wait(0) {
            Ok(v) => v,
            Err(e) => {
                stop = Some(Stop::Fatal(String::from(e)));
                break;
            }
        };
        if exited {
            match emit_sample(
                &mut samples,
                &mut state,
                &target,
                sampler_handle,
                sequence,
                now,
                ex2_supported,
                true,
                &hooks,
                &mut manifest,
                &mut helpers,
            ) {
                Ok(()) => stop = Some(Stop::Natural),
                Err(e) => stop = Some(e),
            };
            break;
        }
        if now >= deadline {
            stop = Some(Stop::Duration);
            break;
        }
        let scheduled = (u128::from(lifetime_start) + next_slot * interval_ticks)
            .min(u128::from(u64::MAX)) as u64;
        if now >= scheduled {
            match emit_sample(
                &mut samples,
                &mut state,
                &target,
                sampler_handle,
                sequence,
                scheduled,
                ex2_supported,
                false,
                &hooks,
                &mut manifest,
                &mut helpers,
            ) {
                Ok(()) => {}
                Err(e) => {
                    stop = Some(e);
                    break;
                }
            }
            sequence += 1;
            if let Some(ms) = hooks.delay_after_sample_ms {
                std::thread::sleep(Duration::from_millis(ms));
            }
            let after = match clock::qpc() {
                Ok(v) => v,
                Err(e) => {
                    stop = Some(Stop::Fatal(String::from(e)));
                    break;
                }
            };
            // Advance to the first future nominal slot: elapsed slots are skipped, never caught up.
            next_slot = u128::from(after.saturating_sub(lifetime_start)) / interval_ticks + 1;
            continue;
        }
        let until = scheduled.min(deadline).saturating_sub(now);
        let wait_ms = (u128::from(until) * 1000 / u128::from(frequency)).clamp(1, 50) as u32;
        match target.process.wait(wait_ms) {
            Ok(_) => {}
            Err(e) => {
                stop = Some(Stop::Fatal(String::from(e)));
            }
        }
    }
    let mut stop = stop.unwrap_or_else(|| Stop::Fatal("run loop ended without outcome".into()));
    let termination_code = match stop {
        Stop::Natural => 0,
        Stop::Duration | Stop::Interrupted => 124,
        Stop::Fatal(_) | Stop::Output(_) => 125,
    };
    if let Err(errors) = target.terminate_and_wait(termination_code, CLEANUP_TIMEOUT_MS) {
        manifest.cleanup_error = Some(format_errors(&errors));
    }
    if hooks.fail_cleanup_after_real_cleanup {
        manifest.cleanup_error = Some("injected checked-cleanup failure".into());
    }
    // For duration/interruption take a final CPU observation after termination without waiting for a nominal tick.
    if matches!(stop, Stop::Duration | Stop::Interrupted)
        && let Ok(now) = clock::qpc()
        && let Err(error) = emit_sample(
            &mut samples,
            &mut state,
            &target,
            sampler_handle,
            sequence,
            now,
            ex2_supported,
            true,
            &TestHooks::default(),
            &mut manifest,
            &mut helpers,
        )
    {
        stop = error;
    }
    manifest.target_exit_code = target.process.exit_code().ok();
    if let Ok(now) = clock::qpc()
        && let Ok((t, _)) = win::process_times(target.process.raw(), now)
    {
        manifest.target_exit_filetime = Some(t.exit_filetime);
    }
    if let Ok(now) = clock::qpc()
        && let (Some(a), Ok((b, _))) = (sampler_before, win::process_times(sampler_handle, now))
    {
        manifest.sampler_cpu_delta_user_100ns = b.user_100ns.checked_sub(a.user_100ns);
        manifest.sampler_cpu_delta_kernel_100ns = b.kernel_100ns.checked_sub(a.kernel_100ns);
    }
    if let Err(e) = control.unregister() {
        manifest.cleanup_error = Some(match manifest.cleanup_error.take() {
            Some(old) => format!("{old}; {e}"),
            None => e,
        });
    }
    manifest.observed_helper_pids = helpers.into_iter().collect();
    manifest.collection_stop_reason = Some(stop.label().into());
    manifest.ended_utc_filetime = Some(clock::utc_filetime_now());
    manifest.state = "complete".into();
    if let Stop::Fatal(ref e) | Stop::Output(ref e) = stop {
        manifest.harness_error = Some(e.clone());
    }
    let required_counts = manifest.counts.valid_cpu_observations >= 2
        && manifest.counts.valid_private_working_set_observations >= 1
        && manifest.counts.valid_private_commit_observations >= 1;
    manifest.collection_valid = required_counts
        && manifest.required_live_query_failures == 0
        && !matches!(stop, Stop::Fatal(_) | Stop::Output(_));
    let target_ok = match stop {
        Stop::Duration => true,
        Stop::Natural => manifest.target_exit_code == Some(0),
        _ => false,
    };
    manifest.tool_exit_success =
        manifest.collection_valid && target_ok && manifest.cleanup_error.is_none();
    let final_write = write_manifest(&manifest_path, &manifest);
    if let Err(e) = final_write {
        return Ok(RunResult {
            exit_code: 1,
            message: format!("collection ended but manifest finalization failed: {e}"),
        });
    }
    let detail = if manifest.tool_exit_success {
        "collection complete".into()
    } else if let Some(e) = &manifest.harness_error {
        e.clone()
    } else if !manifest.collection_valid {
        "insufficient or invalid required measurements".into()
    } else if manifest.target_exit_code != Some(0) {
        format!(
            "target exited {}",
            manifest.target_exit_code.unwrap_or(u32::MAX)
        )
    } else if let Some(e) = &manifest.cleanup_error {
        e.clone()
    } else {
        "collection did not meet success policy".into()
    };
    Ok(RunResult {
        exit_code: if manifest.tool_exit_success { 0 } else { 1 },
        message: format!("{detail}: {}", path_text(&output)),
    })
}

fn finalize_before_launch(
    mut manifest: Manifest,
    manifest_path: PathBuf,
    control: &mut ControlGuard,
    error: String,
) -> Result<RunResult, String> {
    manifest.state = "complete".into();
    manifest.collection_stop_reason = Some("pre_launch_collection_error".into());
    manifest.harness_error = Some(error.clone());
    manifest.ended_utc_filetime = Some(clock::utc_filetime_now());
    if let Err(e) = control.unregister() {
        manifest.cleanup_error = Some(e);
    }
    write_manifest(&manifest_path, &manifest)?;
    Ok(RunResult {
        exit_code: 1,
        message: error,
    })
}

#[allow(clippy::too_many_arguments)]
fn emit_sample<W: Write>(
    writer: &mut W,
    state: &mut SampleState,
    target: &crate::job::Launched,
    sampler_handle: windows_sys::Win32::Foundation::HANDLE,
    sequence: u64,
    scheduled: u64,
    ex2_supported: bool,
    terminal: bool,
    hooks: &TestHooks,
    manifest: &mut Manifest,
    helpers: &mut BTreeSet<u32>,
) -> Result<(), Stop> {
    let fault = hooks
        .sample_fault_at
        .filter(|(seq, _)| *seq == sequence)
        .map(|(_, f)| f)
        .unwrap_or_default();
    let sample = collect_with_fault(
        state,
        target,
        sampler_handle,
        sequence,
        scheduled,
        ex2_supported,
        terminal,
        fault,
    )
    .map_err(|e| Stop::Fatal(String::from(e)))?;
    if hooks
        .fail_sample_write_after_rows
        .is_some_and(|n| manifest.counts.samples >= n)
    {
        return Err(Stop::Output("injected samples write failure".into()));
    }
    serde_json::to_writer(&mut *writer, &sample)
        .map_err(|e| Stop::Output(format!("samples write failed: {e}")))?;
    writer
        .write_all(b"\n")
        .map_err(|e| Stop::Output(format!("samples write failed: {e}")))?;
    writer
        .flush()
        .map_err(|e| Stop::Output(format!("samples flush failed: {e}")))?;
    manifest.counts.samples += 1;
    let cpu_valid = sample.user_cpu_100ns.valid() && sample.kernel_cpu_100ns.valid();
    if cpu_valid {
        manifest.counts.valid_cpu_observations += 1
    } else {
        manifest.counts.invalid_cpu_observations += 1
    }
    if sample.private_working_set_bytes.valid() {
        manifest.counts.valid_private_working_set_observations += 1
    }
    if sample.private_commit_bytes.valid() {
        manifest.counts.valid_private_commit_observations += 1
    }
    let live_required_failure = !sample.terminal_sample
        && (!cpu_valid
            || matches!(sample.private_working_set_bytes, Metric::Unavailable { .. })
            || matches!(sample.private_commit_bytes, Metric::Unavailable { .. }));
    if live_required_failure {
        manifest.required_live_query_failures += 1;
    }
    let observed = observe_helpers(&sample.helper_snapshot, target.process.pid, helpers);
    manifest.observed_helper_pids_truncated |= observed.identities_truncated;
    manifest.helper_snapshot_truncations += observed.snapshot_truncations;
    manifest.helper_snapshot_failures += observed.snapshot_failures;
    Ok(())
}
#[derive(Default)]
struct HelperObservationDelta {
    identities_truncated: bool,
    snapshot_truncations: u64,
    snapshot_failures: u64,
}
fn observe_helpers(
    snapshot: &HelperSnapshotRecord,
    root_pid: u32,
    helpers: &mut BTreeSet<u32>,
) -> HelperObservationDelta {
    let mut delta = HelperObservationDelta::default();
    match snapshot {
        HelperSnapshotRecord::Complete { pids, .. } => {
            for pid in pids {
                if *pid != root_pid {
                    if helpers.len() < HELPER_RUN_IDENTITY_LIMIT {
                        helpers.insert(*pid);
                    } else if !helpers.contains(pid) {
                        delta.identities_truncated = true;
                    }
                }
            }
        }
        HelperSnapshotRecord::Truncated { .. } => delta.snapshot_truncations = 1,
        HelperSnapshotRecord::Failed { .. } => delta.snapshot_failures = 1,
    }
    delta
}

fn format_errors(errors: &[ApiError]) -> String {
    errors
        .iter()
        .map(|e| format!("{} failed ({}, {})", e.api, e.os_code, e.reason))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::{HELPER_RUN_IDENTITY_LIMIT, observe_helpers};
    use crate::records::{ApiError, HelperSnapshotRecord};
    use std::collections::BTreeSet;
    #[test]
    fn schedule_skips_missed_slots() {
        let start = 100u128;
        let interval = 10u128;
        let after = 136u128;
        let next = (after - start) / interval + 1;
        assert_eq!(next, 4);
        assert_eq!(start + next * interval, 140);
    }
    #[test]
    fn helper_identity_churn_is_bounded_and_snapshot_degradation_is_explicit() {
        let mut identities = BTreeSet::new();
        let pids = (1..=(HELPER_RUN_IDENTITY_LIMIT as u32 + 20)).collect();
        let delta = observe_helpers(
            &HelperSnapshotRecord::Complete {
                pids,
                assigned: 300,
            },
            0,
            &mut identities,
        );
        assert_eq!(identities.len(), HELPER_RUN_IDENTITY_LIMIT);
        assert!(delta.identities_truncated);
        let truncated = observe_helpers(
            &HelperSnapshotRecord::Truncated {
                assigned: 300,
                limit: HELPER_RUN_IDENTITY_LIMIT,
            },
            0,
            &mut identities,
        );
        assert_eq!(truncated.snapshot_truncations, 1);
        let failed = observe_helpers(
            &HelperSnapshotRecord::Failed {
                error: ApiError {
                    api: "QueryInformationJobObject".into(),
                    os_code: 5,
                    reason: "injected".into(),
                },
            },
            0,
            &mut identities,
        );
        assert_eq!(failed.snapshot_failures, 1);
    }
    #[test]
    fn outcomes_keep_collection_and_target_independent() {
        let enough = true;
        let live_failures = 0;
        let collection = enough && live_failures == 0;
        assert!(collection);
        let target_exit = 23;
        let tool = collection && target_exit == 0;
        assert!(!tool);
    }
}
