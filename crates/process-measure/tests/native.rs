#![cfg(windows)]

use process_measure::args::{StdioMode, ValidatedArgs};
use process_measure::job::{LaunchFault, launch_with_fault, resolve_launch};
use process_measure::records::{Manifest, SampleRecord};
use process_measure::run::{TestHooks, run_collection_with_hooks};
use process_measure::sampler::SampleFault;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct OwnedTemp {
    root: PathBuf,
}
impl OwnedTemp {
    fn new(name: &str) -> Self {
        loop {
            let root = std::env::temp_dir().join(format!(
                "process-measure-owned-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Self { root },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("allocate temp: {e}"),
            }
        }
    }
    fn run(&self) -> PathBuf {
        self.root.join("run")
    }
    fn marker(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}
impl Drop for OwnedTemp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
fn tool() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_process-measure"))
}
fn child() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_process-measure-testchild"))
}
fn manifest(dir: &Path) -> Manifest {
    serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}
fn samples(dir: &Path) -> Vec<SampleRecord> {
    fs::read_to_string(dir.join("samples.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn args(dir: &Path, interval: u64, duration: u64, child_args: &[&str]) -> ValidatedArgs {
    ValidatedArgs {
        out_dir: dir.into(),
        target_exe: child().into_os_string(),
        target_args: child_args.iter().map(OsString::from).collect(),
        target_cwd: None,
        interval_ms: interval,
        duration_ms: duration,
        stdio: StdioMode::Null,
        declarations: vec![],
    }
}
fn wait_manifest_pid(dir: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let pid = fs::read(dir.join("manifest.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
            .and_then(|m| m.target_pid);
        if let Some(p) = pid {
            return p;
        }
        assert!(Instant::now() < deadline, "target pid not published");
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn read_u32_when_complete(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(s) = fs::read_to_string(path)
            && let Ok(v) = s.parse()
        {
            return v;
        }
        assert!(
            Instant::now() < deadline,
            "marker was not atomically published"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn natural_and_abnormal_exit_keep_collection_validity_independent() {
    let t = OwnedTemp::new("outcomes");
    let d = t.run();
    let out = Command::new(tool())
        .args(["--out"])
        .arg(&d)
        .args(["--interval-ms", "50", "--duration-ms", "1000", "--"])
        .arg(child())
        .args(["sleep-exit", "350", "23"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let m = manifest(&d);
    assert!(m.collection_valid);
    assert!(!m.tool_exit_success);
    assert_eq!(m.target_exit_code, Some(23));
    assert!(m.harness_error.is_none());
    assert_eq!(m.collection_stop_reason.as_deref(), Some("natural_exit"));
}

#[test]
fn nondivisible_duration_and_root_exit_between_ticks_are_prompt() {
    let t = OwnedTemp::new("deadline");
    let d = t.run();
    let start = Instant::now();
    let out = Command::new(tool())
        .args(["--out"])
        .arg(&d)
        .args(["--interval-ms", "1000", "--duration-ms", "1100", "--"])
        .arg(child())
        .args(["sleep", "10000"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        start.elapsed() < Duration::from_millis(1800),
        "duration waited for the 2 s sample tick"
    );
    assert_eq!(
        manifest(&d).collection_stop_reason.as_deref(),
        Some("duration_limit")
    );
    let t2 = OwnedTemp::new("exit-between");
    let d2 = t2.run();
    let start = Instant::now();
    let out = Command::new(tool())
        .args(["--out"])
        .arg(&d2)
        .args(["--interval-ms", "2000", "--duration-ms", "4000", "--"])
        .arg(child())
        .args(["sleep", "100"])
        .output()
        .unwrap();
    let _ = out;
    assert!(
        start.elapsed() < Duration::from_millis(900),
        "root exit waited for sample tick"
    );
    assert_eq!(
        manifest(&d2).collection_stop_reason.as_deref(),
        Some("natural_exit")
    );
}

#[test]
fn delayed_sampler_skips_nominal_slots_instead_of_catching_up() {
    let t = OwnedTemp::new("late");
    let d = t.run();
    let r = run_collection_with_hooks(
        args(&d, 100, 700, &["sleep", "10000"]),
        TestHooks {
            delay_after_sample_ms: Some(260),
            ..Default::default()
        },
    );
    assert_eq!(r.exit_code, 0, "{}", r.message);
    let rows = samples(&d);
    let interval_ticks = manifest(&d).qpc_frequency_hz / 10;
    assert!(
        rows.len() <= 5,
        "late sampler emitted catch-up burst: {}",
        rows.len()
    );
    for pair in rows.windows(2).filter(|p| !p[1].terminal_sample) {
        assert!(
            pair[1].scheduled_qpc - pair[0].scheduled_qpc >= 2 * interval_ticks,
            "nominal slot was not skipped"
        );
    }
}

#[test]
fn explicit_handle_list_excludes_ambient_inheritable_sentinel_for_all_stdio_modes() {
    for mode in ["null", "file", "inherit"] {
        let t = OwnedTemp::new(mode);
        let d = t.run();
        let sentinel_path = t.marker("sentinel");
        let sentinel = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&sentinel_path)
            .unwrap();
        let raw = sentinel.as_raw_handle() as HANDLE;
        assert_ne!(
            unsafe { SetHandleInformation(raw, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
            0
        );
        let marker = t.marker("handle-result");
        let out = Command::new(tool())
            .args(["--out"])
            .arg(&d)
            .args([
                "--stdio",
                mode,
                "--interval-ms",
                "50",
                "--duration-ms",
                "1000",
                "--",
            ])
            .arg(child())
            .arg("check-handle")
            .arg(format!("{}", raw as usize))
            .arg(&marker)
            .arg("300")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "mode {mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let diagnostic = fs::read_to_string(marker).unwrap();
        assert_ne!(diagnostic, "wrote_sentinel", "stdio mode {mode}");
        assert_eq!(
            fs::metadata(&sentinel_path).unwrap().len(),
            0,
            "ambient sentinel was inherited in mode {mode}"
        );
        drop(sentinel);
    }
}

#[test]
fn existing_output_with_valid_target_and_missing_target_never_launch() {
    let t = OwnedTemp::new("existing");
    let d = t.run();
    fs::create_dir(&d).unwrap();
    let marker = t.marker("launch-marker");
    let out = Command::new(tool())
        .args(["--out"])
        .arg(&d)
        .args(["--"])
        .arg(child())
        .args(["sleep", "100"])
        .arg(&marker)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!marker.exists());
    assert!(!d.join("manifest.json").exists());
    let t2 = OwnedTemp::new("missing");
    let d2 = t2.run();
    let out = Command::new(tool())
        .args(["--out"])
        .arg(&d2)
        .args(["--", r"Z:\definitely-missing.exe"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!d2.exists());
}

#[test]
fn setup_failures_before_and_after_assignment_are_checked_and_child_never_runs() {
    for fault in [LaunchFault::AfterCreate, LaunchFault::AfterAssign] {
        let t = OwnedTemp::new("setup-fault");
        let d = t.run();
        fs::create_dir(&d).unwrap();
        let marker = t.marker("started");
        let a = args(&d, 50, 500, &["sleep", "1000", marker.to_str().unwrap()]);
        let id = resolve_launch(&a).unwrap();
        let e = launch_with_fault(&a, &id, Some(fault))
            .err()
            .expect("injected failure");
        assert!(e.cleanup_errors.is_empty(), "{e}");
        assert!(
            !marker.exists(),
            "suspended target ran before containment completed"
        );
    }
}

#[test]
fn production_output_and_query_faults_finalize_and_cleanup() {
    let t = OwnedTemp::new("output-fault");
    let d = t.run();
    let run_args = args(&d, 50, 1000, &["sleep", "10000"]);
    let d_thread = d.clone();
    let join = std::thread::spawn(move || {
        run_collection_with_hooks(
            run_args,
            TestHooks {
                fail_sample_write_after_rows: Some(1),
                ..Default::default()
            },
        )
    });
    let pid = wait_manifest_pid(&d_thread);
    let handle = HandleGuard(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) });
    assert!(!handle.0.is_null());
    let r = join.join().unwrap();
    assert_ne!(r.exit_code, 0);
    assert_eq!(
        unsafe { WaitForSingleObject(handle.0, 5000) },
        WAIT_OBJECT_0
    );
    let m = manifest(&d);
    assert_eq!(m.state, "complete");
    assert_eq!(m.collection_stop_reason.as_deref(), Some("output_error"));
    assert!(m.harness_error.as_deref().unwrap().contains("injected"));
    assert_eq!(
        samples(&d).len(),
        1,
        "complete rows before failure were not preserved"
    );
    let t2 = OwnedTemp::new("query-fault");
    let d2 = t2.run();
    let r = run_collection_with_hooks(
        args(&d2, 50, 500, &["sleep", "10000"]),
        TestHooks {
            sample_fault_at: Some((
                2,
                SampleFault {
                    required_memory: true,
                    ..Default::default()
                },
            )),
            ..Default::default()
        },
    );
    assert_ne!(r.exit_code, 0);
    let m = manifest(&d2);
    assert!(m.required_live_query_failures > 0);
    assert!(!m.collection_valid);
    assert_eq!(m.state, "complete");
    let t3 = OwnedTemp::new("fatal-fault");
    let d3 = t3.run();
    let r = run_collection_with_hooks(
        args(&d3, 50, 500, &["sleep", "10000"]),
        TestHooks {
            sample_fault_at: Some((
                2,
                SampleFault {
                    fatal: true,
                    ..Default::default()
                },
            )),
            ..Default::default()
        },
    );
    assert_ne!(r.exit_code, 0);
    let m = manifest(&d3);
    assert_eq!(m.state, "complete");
    assert_eq!(
        m.collection_stop_reason.as_deref(),
        Some("fatal_collection_error")
    );
    assert!(m.harness_error.is_some());
    let t4 = OwnedTemp::new("cleanup-fault");
    let d4 = t4.run();
    let r = run_collection_with_hooks(
        args(&d4, 50, 300, &["sleep", "10000"]),
        TestHooks {
            fail_cleanup_after_real_cleanup: true,
            ..Default::default()
        },
    );
    assert_ne!(r.exit_code, 0);
    assert!(
        manifest(&d4)
            .cleanup_error
            .as_deref()
            .unwrap()
            .contains("injected")
    );
    let t5 = OwnedTemp::new("control-fault");
    let d5 = t5.run();
    let r = run_collection_with_hooks(
        args(&d5, 50, 300, &["sleep", "10000"]),
        TestHooks {
            fail_control_registration: true,
            ..Default::default()
        },
    );
    assert_ne!(r.exit_code, 0);
    let m = manifest(&d5);
    assert_eq!(m.state, "complete");
    assert_eq!(
        m.collection_stop_reason.as_deref(),
        Some("control_handler_setup_failure")
    );
    let t6 = OwnedTemp::new("sampler-query-fault");
    let d6 = t6.run();
    let r = run_collection_with_hooks(
        args(&d6, 50, 350, &["sleep", "10000"]),
        TestHooks {
            sample_fault_at: Some((
                2,
                SampleFault {
                    sampler_cpu: true,
                    ..Default::default()
                },
            )),
            ..Default::default()
        },
    );
    assert_eq!(
        r.exit_code, 0,
        "optional sampler query loss should stay explicit but not invalidate target collection"
    );
    let rows = samples(&d6);
    assert!(matches!(
        rows[2].sampler_delta_user_cpu_100ns,
        process_measure::records::Metric::Unavailable { .. }
    ));
    if rows.len() > 3 {
        match &rows[3].sampler_delta_user_cpu_100ns {
            process_measure::records::Metric::Unavailable { error } => {
                assert_eq!(error.reason, "first_observation")
            }
            other => panic!("sampler failure was silently bridged: {other:?}"),
        }
    }
    let t7 = OwnedTemp::new("clock-fault");
    let d7 = t7.run();
    let r = run_collection_with_hooks(
        args(&d7, 50, 350, &["sleep", "10000"]),
        TestHooks {
            fail_post_launch_clock: true,
            ..Default::default()
        },
    );
    assert_ne!(r.exit_code, 0);
    let m = manifest(&d7);
    assert_eq!(m.state, "complete");
    assert_eq!(
        m.collection_stop_reason.as_deref(),
        Some("fatal_collection_error")
    );
    assert!(m.harness_error.as_deref().unwrap().contains("QPC"));
}

#[test]
fn helper_cleanup_is_bounded_and_does_not_touch_unrelated_process() {
    let t = OwnedTemp::new("helper");
    let d = t.run();
    let marker = t.marker("helper-pid");
    let mut unrelated = ChildGuard(
        Command::new(child())
            .args(["sleep", "10000"])
            .spawn()
            .unwrap(),
    );
    let mut sampler = ChildGuard(
        Command::new(tool())
            .args(["--out"])
            .arg(&d)
            .args(["--interval-ms", "50", "--duration-ms", "2000", "--"])
            .arg(child())
            .args(["helper-root", "10000", "300"])
            .arg(&marker)
            .spawn()
            .unwrap(),
    );
    let helper_pid = read_u32_when_complete(&marker);
    let helper = HandleGuard(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, helper_pid) });
    assert!(!helper.0.is_null());
    let status = sampler.0.wait().unwrap();
    assert!(status.success());
    assert_eq!(
        unsafe { WaitForSingleObject(helper.0, 5000) },
        WAIT_OBJECT_0
    );
    assert!(unrelated.0.try_wait().unwrap().is_none());
    let m = manifest(&d);
    assert!(m.observed_helper_pids.contains(&helper_pid));
}

#[test]
fn interruption_and_abrupt_sampler_death_both_clean_owned_target() {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
    let t = OwnedTemp::new("interrupt");
    let d = t.run();
    let mut sampler = ChildGuard(
        Command::new(tool())
            .args(["--out"])
            .arg(&d)
            .args(["--interval-ms", "100", "--duration-ms", "10000", "--"])
            .arg(child())
            .args(["sleep", "10000"])
            .creation_flags(CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .unwrap(),
    );
    let pid = wait_manifest_pid(&d);
    let target = HandleGuard(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) });
    assert_ne!(
        unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, sampler.0.id()) },
        0
    );
    let status = sampler.0.wait().unwrap();
    assert!(!status.success());
    assert_eq!(
        unsafe { WaitForSingleObject(target.0, 5000) },
        WAIT_OBJECT_0
    );
    let m = manifest(&d);
    assert_eq!(m.collection_stop_reason.as_deref(), Some("interrupted"));
    assert!(!m.tool_exit_success);
    let t2 = OwnedTemp::new("abrupt");
    let d2 = t2.run();
    let mut sampler = ChildGuard(
        Command::new(tool())
            .args(["--out"])
            .arg(&d2)
            .args(["--interval-ms", "100", "--duration-ms", "10000", "--"])
            .arg(child())
            .args(["sleep", "10000"])
            .spawn()
            .unwrap(),
    );
    let pid = wait_manifest_pid(&d2);
    let target = HandleGuard(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) });
    sampler.0.kill().unwrap();
    sampler.0.wait().unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(target.0, 5000) },
        WAIT_OBJECT_0
    );
}

#[test]
fn file_stdio_preserves_empty_quotes_spaces_and_unicode_arguments() {
    let t = OwnedTemp::new("argv");
    let d = t.run();
    let out = Command::new(tool())
        .args(["--out"])
        .arg(&d)
        .args([
            "--stdio",
            "file",
            "--interval-ms",
            "50",
            "--duration-ms",
            "500",
            "--",
        ])
        .arg(child())
        .arg("echo")
        .arg("")
        .arg("a b")
        .arg("he said \"hi\"")
        .arg("trailing\\")
        .arg("café 🙂")
        .output()
        .unwrap();
    let text = fs::read_to_string(d.join("target.stdout")).unwrap();
    assert!(
        text.contains("0:\n")
            && text.contains("1:a b")
            && text.contains("2:he said \"hi\"")
            && text.contains("3:trailing\\")
            && text.contains("4:café 🙂")
    );
    assert_eq!(manifest(&d).target_exit_code, Some(0));
    drop(out);
}
