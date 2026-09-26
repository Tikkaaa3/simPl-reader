//! Suspended launch into a private kill-on-close Job Object.
//!
//! The target is created suspended with an explicit selected-stdio handle
//! allowlist, assigned to the private job, and only then resumed. Checked
//! teardown is the primary path; handle `Drop` is the abrupt-death fallback.

use crate::args::{StdioMode, ValidatedArgs};
use crate::cmdline::encode_os_command_line;
use crate::records::ApiError;
use crate::win;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess,
    InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

pub const HELPER_SNAPSHOT_LIMIT: usize = 256;
pub const HELPER_RUN_IDENTITY_LIMIT: usize = 256;
pub const CLEANUP_TIMEOUT_MS: u32 = 5_000;

fn io_error(api: &str) -> ApiError {
    win::api_error(api)
}

#[derive(Debug)]
pub struct LaunchIdentity {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
}

pub fn resolve_launch(args: &ValidatedArgs) -> Result<LaunchIdentity, String> {
    let executable = std::fs::canonicalize(&args.target_exe)
        .map_err(|e| format!("target executable cannot be resolved: {e}"))?;
    if !executable.is_file() {
        return Err("target executable is not a file".into());
    }
    let ext = executable.extension().and_then(OsStr::to_str).unwrap_or("");
    if ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat") {
        return Err(
            ".cmd/.bat wrappers are not executable targets; launch a local .exe directly".into(),
        );
    }
    let working_directory = match &args.target_cwd {
        Some(p) => std::fs::canonicalize(p)
            .map_err(|e| format!("target working directory cannot be resolved: {e}"))?,
        None => std::env::current_dir()
            .and_then(std::fs::canonicalize)
            .map_err(|e| format!("current directory cannot be resolved: {e}"))?,
    };
    if !working_directory.is_dir() {
        return Err("target working directory is not a directory".into());
    }
    Ok(LaunchIdentity {
        executable,
        working_directory,
    })
}

pub struct OwnedJob {
    handle: HANDLE,
}
unsafe impl Send for OwnedJob {}
impl Drop for OwnedJob {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub struct OwnedProcess {
    handle: HANDLE,
    pub pid: u32,
}
unsafe impl Send for OwnedProcess {}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
impl OwnedProcess {
    pub fn raw(&self) -> HANDLE {
        self.handle
    }
    pub fn wait(&self, timeout_ms: u32) -> Result<bool, ApiError> {
        match unsafe { WaitForSingleObject(self.handle, timeout_ms) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io_error("WaitForSingleObject")),
        }
    }
    pub fn exit_code(&self) -> Result<u32, ApiError> {
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.handle, &mut code) } == 0 {
            Err(io_error("GetExitCodeProcess"))
        } else {
            Ok(code)
        }
    }
}

#[derive(Debug, Clone)]
pub struct LaunchError {
    pub primary: ApiError,
    pub cleanup_errors: Vec<ApiError>,
}
impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} failed ({})", self.primary.api, self.primary.os_code)?;
        for e in &self.cleanup_errors {
            write!(f, "; cleanup {} failed ({})", e.api, e.os_code)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum LaunchFault {
    AfterCreate,
    AfterAssign,
}

#[derive(Debug)]
pub enum MemberSnapshot {
    Complete { pids: Vec<u32>, assigned: u32 },
    Truncated { assigned: u32, limit: usize },
    Failed(ApiError),
}

pub struct Launched {
    pub job: OwnedJob,
    pub process: OwnedProcess,
}
impl Launched {
    fn active_processes(&self) -> Result<u32, ApiError> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.job.handle,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        } == 0
        {
            Err(io_error("QueryInformationJobObject(accounting)"))
        } else {
            Ok(info.ActiveProcesses)
        }
    }
    pub fn terminate_and_wait(&self, code: u32, timeout_ms: u32) -> Result<(), Vec<ApiError>> {
        let mut errors = Vec::new();
        if unsafe { TerminateJobObject(self.job.handle, code) } == 0 {
            errors.push(io_error("TerminateJobObject"));
        }
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        match self.process.wait(timeout_ms) {
            Ok(true) => {}
            Ok(false) => errors.push(ApiError {
                api: "WaitForSingleObject(root)".into(),
                os_code: WAIT_TIMEOUT,
                reason: "cleanup_timeout".into(),
            }),
            Err(e) => errors.push(e),
        }
        loop {
            match self.active_processes() {
                Ok(0) => break,
                Ok(_) => {
                    if Instant::now() >= deadline {
                        errors.push(ApiError {
                            api: "QueryInformationJobObject(accounting)".into(),
                            os_code: WAIT_TIMEOUT,
                            reason: "owned_tree_cleanup_timeout".into(),
                        });
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    errors.push(e);
                    break;
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    pub fn member_snapshot(&self) -> MemberSnapshot {
        #[repr(C)]
        struct List {
            assigned: u32,
            count: u32,
            ids: [usize; HELPER_SNAPSHOT_LIMIT],
        }
        let mut list: List = unsafe { zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.job.handle,
                JobObjectBasicProcessIdList,
                (&mut list as *mut List).cast(),
                size_of::<List>() as u32,
                null_mut(),
            )
        };
        if ok == 0 {
            if list.assigned as usize > HELPER_SNAPSHOT_LIMIT {
                MemberSnapshot::Truncated {
                    assigned: list.assigned,
                    limit: HELPER_SNAPSHOT_LIMIT,
                }
            } else {
                MemberSnapshot::Failed(io_error("QueryInformationJobObject(process_ids)"))
            }
        } else {
            MemberSnapshot::Complete {
                pids: list.ids[..(list.count as usize).min(HELPER_SNAPSHOT_LIMIT)]
                    .iter()
                    .map(|v| *v as u32)
                    .collect(),
                assigned: list.assigned,
            }
        }
    }
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct StdioHandles {
    _files: Vec<File>,
    handles: [OwnedHandle; 3],
}
fn duplicate_inheritable(source: HANDLE) -> io::Result<OwnedHandle> {
    if source.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "selected stdio handle is null",
        ));
    }
    let mut target = null_mut();
    let current = unsafe { GetCurrentProcess() };
    if unsafe {
        DuplicateHandle(
            current,
            source,
            current,
            &mut target,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(OwnedHandle(target))
    }
}
fn selected_stdio(mode: StdioMode, out: &Path) -> io::Result<StdioHandles> {
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    let files = match mode {
        StdioMode::Inherit => Vec::new(),
        StdioMode::Null => vec![
            OpenOptions::new().read(true).open("NUL")?,
            OpenOptions::new().write(true).open("NUL")?,
            OpenOptions::new().write(true).open("NUL")?,
        ],
        StdioMode::File => vec![
            OpenOptions::new().read(true).open("NUL")?,
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(out.join("target.stdout"))?,
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(out.join("target.stderr"))?,
        ],
    };
    let raw = if mode == StdioMode::Inherit {
        [
            unsafe { GetStdHandle(STD_INPUT_HANDLE) },
            unsafe { GetStdHandle(STD_OUTPUT_HANDLE) },
            unsafe { GetStdHandle(STD_ERROR_HANDLE) },
        ]
    } else {
        [
            files[0].as_raw_handle() as HANDLE,
            files[1].as_raw_handle() as HANDLE,
            files[2].as_raw_handle() as HANDLE,
        ]
    };
    Ok(StdioHandles {
        _files: files,
        handles: [
            duplicate_inheritable(raw[0])?,
            duplicate_inheritable(raw[1])?,
            duplicate_inheritable(raw[2])?,
        ],
    })
}

struct AttributeList {
    storage: Vec<usize>,
    ptr: *mut std::ffi::c_void,
}
impl AttributeList {
    fn handles(handles: &[HANDLE]) -> Result<Self, ApiError> {
        let mut bytes = 0usize;
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0usize; words];
        let ptr = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(ptr, 1, 0, &mut bytes) } == 0 {
            return Err(io_error("InitializeProcThreadAttributeList"));
        }
        if unsafe {
            UpdateProcThreadAttribute(
                ptr,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                null_mut(),
                null(),
            )
        } == 0
        {
            let e = io_error("UpdateProcThreadAttribute");
            unsafe { DeleteProcThreadAttributeList(ptr) };
            return Err(e);
        }
        Ok(Self { storage, ptr })
    }
}
impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.ptr) };
        std::hint::black_box(&self.storage);
    }
}

pub fn launch(args: &ValidatedArgs, identity: &LaunchIdentity) -> Result<Launched, LaunchError> {
    launch_with_fault(args, identity, None)
}
#[doc(hidden)]
pub fn launch_with_fault(
    args: &ValidatedArgs,
    identity: &LaunchIdentity,
    fault: Option<LaunchFault>,
) -> Result<Launched, LaunchError> {
    let fail = |primary: ApiError| LaunchError {
        primary,
        cleanup_errors: vec![],
    };
    let job_handle = unsafe { CreateJobObjectW(null(), null()) };
    if job_handle.is_null() {
        return Err(fail(io_error("CreateJobObjectW")));
    }
    let job = OwnedJob { handle: job_handle };
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.handle,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(fail(io_error("SetInformationJobObject")));
    }
    let io = selected_stdio(args.stdio, &args.out_dir).map_err(|e| {
        fail(ApiError {
            api: "stdio_setup".into(),
            os_code: e.raw_os_error().unwrap_or(0) as u32,
            reason: e.to_string(),
        })
    })?;
    let inherit: [HANDLE; 3] = [io.handles[0].0, io.handles[1].0, io.handles[2].0];
    let attributes = AttributeList::handles(&inherit).map_err(fail)?;
    let argv: Vec<&OsStr> = std::iter::once(identity.executable.as_os_str())
        .chain(args.target_args.iter().map(OsStr::new))
        .collect();
    let mut command = encode_os_command_line(&argv).map_err(|_| {
        fail(ApiError {
            api: "command_line".into(),
            os_code: 0,
            reason: "embedded_nul".into(),
        })
    })?;
    command.push(0);
    let application: Vec<u16> = identity
        .executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let cwd: Vec<u16> = identity
        .working_directory
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = inherit[0];
    startup.StartupInfo.hStdOutput = inherit[1];
    startup.StartupInfo.hStdError = inherit[2];
    startup.lpAttributeList = attributes.ptr;
    let mut pi: PROCESS_INFORMATION = unsafe { zeroed() };
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            null(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut pi,
        )
    } == 0
    {
        return Err(fail(io_error("CreateProcessW")));
    }
    let process = OwnedProcess {
        handle: pi.hProcess,
        pid: pi.dwProcessId,
    };
    let thread = OwnedHandle(pi.hThread);
    if fault == Some(LaunchFault::AfterCreate) {
        return Err(cleanup_uncontained(
            ApiError {
                api: "injected_after_create".into(),
                os_code: 0,
                reason: "test_fault".into(),
            },
            &process,
        ));
    }
    if unsafe { AssignProcessToJobObject(job.handle, process.handle) } == 0 {
        let primary = io_error("AssignProcessToJobObject");
        return Err(cleanup_uncontained(primary, &process));
    }
    let launched = Launched { job, process };
    if fault == Some(LaunchFault::AfterAssign) {
        let primary = ApiError {
            api: "injected_after_assign".into(),
            os_code: 0,
            reason: "test_fault".into(),
        };
        let cleanup_errors = launched
            .terminate_and_wait(125, CLEANUP_TIMEOUT_MS)
            .err()
            .unwrap_or_default();
        return Err(LaunchError {
            primary,
            cleanup_errors,
        });
    }
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        let primary = io_error("ResumeThread");
        let cleanup_errors = launched
            .terminate_and_wait(125, CLEANUP_TIMEOUT_MS)
            .err()
            .unwrap_or_default();
        return Err(LaunchError {
            primary,
            cleanup_errors,
        });
    }
    Ok(launched)
}
fn cleanup_uncontained(primary: ApiError, process: &OwnedProcess) -> LaunchError {
    let mut errors = Vec::new();
    if unsafe { TerminateProcess(process.handle, 125) } == 0 {
        errors.push(io_error("TerminateProcess(setup_cleanup)"));
    }
    match process.wait(CLEANUP_TIMEOUT_MS) {
        Ok(true) => {}
        Ok(false) => errors.push(ApiError {
            api: "WaitForSingleObject(setup_cleanup)".into(),
            os_code: WAIT_TIMEOUT,
            reason: "cleanup_timeout".into(),
        }),
        Err(e) => errors.push(e),
    }
    LaunchError {
        primary,
        cleanup_errors: errors,
    }
}
