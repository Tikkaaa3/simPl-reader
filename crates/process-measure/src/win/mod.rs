//! Compact Win32 query seam. Unsafe calls are localized here: every handle is
//! owned by the caller for the duration of a call, every output struct is
//! zero-initialized, and all buffers are sized to their concrete type.

use crate::metrics::CpuObservation;
use crate::records::{ApiError, Metric};
use std::mem::{size_of, zeroed};
use windows_sys::Win32::Foundation::{FILETIME, GetLastError, HANDLE};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
};
use windows_sys::Win32::System::Threading::{
    ALL_PROCESSOR_GROUPS, GetActiveProcessorCount, GetProcessTimes,
};

fn error(api: &str) -> ApiError {
    ApiError {
        api: api.into(),
        os_code: unsafe { GetLastError() },
        reason: "win32_call_failed".into(),
    }
}

pub fn filetime_value(v: FILETIME) -> u64 {
    (u64::from(v.dwHighDateTime) << 32) | u64::from(v.dwLowDateTime)
}

pub fn qpc() -> Result<u64, ApiError> {
    let mut value = 0i64;
    if unsafe { QueryPerformanceCounter(&mut value) } == 0 || value < 0 {
        Err(error("QueryPerformanceCounter"))
    } else {
        Ok(value as u64)
    }
}

pub fn qpc_frequency() -> Result<u64, ApiError> {
    let mut value = 0i64;
    if unsafe { QueryPerformanceFrequency(&mut value) } == 0 || value <= 0 {
        Err(error("QueryPerformanceFrequency"))
    } else {
        Ok(value as u64)
    }
}

pub fn logical_processor_count() -> Option<u32> {
    let n = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    (n > 0).then_some(n)
}

pub(crate) struct ProcessTimes {
    pub creation_filetime: u64,
    pub exit_filetime: u64,
    pub kernel_100ns: u64,
    pub user_100ns: u64,
}

pub(crate) fn process_times(
    handle: HANDLE,
    observation_qpc: u64,
) -> Result<(ProcessTimes, CpuObservation), ApiError> {
    let (mut creation, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
        unsafe { zeroed() };
    if unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(error("GetProcessTimes"));
    }
    let times = ProcessTimes {
        creation_filetime: filetime_value(creation),
        exit_filetime: filetime_value(exit),
        kernel_100ns: filetime_value(kernel),
        user_100ns: filetime_value(user),
    };
    let obs = CpuObservation {
        user_100ns: times.user_100ns,
        kernel_100ns: times.kernel_100ns,
        qpc_after: observation_qpc,
    };
    Ok((times, obs))
}

pub(crate) struct MemoryMetrics {
    pub private_working_set_bytes: Metric<u64>,
    pub private_commit_bytes: Metric<u64>,
    pub total_working_set_bytes: Metric<u64>,
}

pub(crate) fn memory_metrics(handle: HANDLE, ex2_supported: bool, terminal: bool) -> MemoryMetrics {
    let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = unsafe { zeroed() };
    counters.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(
            handle,
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2).cast::<PROCESS_MEMORY_COUNTERS>(),
            size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
        )
    } != 0;
    if !ok {
        let e = error("GetProcessMemoryInfo");
        let metric = if terminal {
            Metric::TerminalUnavailable {
                reason: "process_has_exited".into(),
            }
        } else {
            Metric::Unavailable { error: e.clone() }
        };
        return MemoryMetrics {
            private_working_set_bytes: metric.clone(),
            private_commit_bytes: metric.clone(),
            total_working_set_bytes: metric,
        };
    }
    let private_ws = if ex2_supported {
        Metric::Valid {
            value: counters.PrivateWorkingSetSize as u64,
        }
    } else {
        Metric::Unsupported { reason: "PROCESS_MEMORY_COUNTERS_EX2 PrivateWorkingSetSize requires Windows 10/11 22H2 September 2023 update or newer".into() }
    };
    MemoryMetrics {
        private_working_set_bytes: private_ws,
        private_commit_bytes: Metric::Valid {
            value: counters.PrivateUsage as u64,
        },
        total_working_set_bytes: Metric::Valid {
            value: counters.WorkingSetSize as u64,
        },
    }
}

pub fn api_error(api: &str) -> ApiError {
    error(api)
}
