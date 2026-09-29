//! Run orchestration and durable artifact writing.

use crate::args::{DeclaredKey, ValidatedArgs};
use crate::job::resolve_launch;
use crate::records::*;
use crate::{clock, win};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::mem::{size_of, zeroed};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Security::Cryptography::*;
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};
use windows_sys::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows_sys::Win32::System::SystemInformation::{
    GetNativeSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, PROCESSOR_ARCHITECTURE_AMD64,
    PROCESSOR_ARCHITECTURE_ARM64, PROCESSOR_ARCHITECTURE_INTEL, SYSTEM_INFO,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
unsafe extern "system" fn ctrl_handler(kind: u32) -> i32 {
    if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
        INTERRUPTED.store(true, Ordering::SeqCst);
        1
    } else {
        0
    }
}

#[derive(Debug)]
pub struct RunResult {
    pub exit_code: i32,
    pub message: String,
}

fn unavailable<T>(api: &str, reason: &str) -> Metric<T> {
    Metric::Unavailable {
        error: ApiError {
            api: api.into(),
            os_code: 0,
            reason: reason.into(),
        },
    }
}
fn path_text(p: &Path) -> String {
    p.as_os_str().to_string_lossy().into_owned()
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut algorithm: BCRYPT_ALG_HANDLE = null_mut();
    let status =
        unsafe { BCryptOpenAlgorithmProvider(&mut algorithm, BCRYPT_SHA256_ALGORITHM, null(), 0) };
    if status < 0 {
        return Err(format!("BCryptOpenAlgorithmProvider NTSTATUS={status:#x}"));
    }
    let result = (|| {
        let mut object_length = 0u32;
        let mut got = 0u32;
        let status = unsafe {
            BCryptGetProperty(
                algorithm,
                BCRYPT_OBJECT_LENGTH,
                (&mut object_length as *mut u32).cast(),
                size_of::<u32>() as u32,
                &mut got,
                0,
            )
        };
        if status < 0 {
            return Err(format!("BCryptGetProperty NTSTATUS={status:#x}"));
        }
        let mut object = vec![0u8; object_length as usize];
        let mut hash: BCRYPT_HASH_HANDLE = null_mut();
        let status = unsafe {
            BCryptCreateHash(
                algorithm,
                &mut hash,
                object.as_mut_ptr(),
                object.len() as u32,
                null(),
                0,
                0,
            )
        };
        if status < 0 {
            return Err(format!("BCryptCreateHash NTSTATUS={status:#x}"));
        }
        let hash_result = (|| {
            let mut file = File::open(path).map_err(|e| e.to_string())?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                let status = unsafe { BCryptHashData(hash, buffer.as_ptr(), n as u32, 0) };
                if status < 0 {
                    return Err(format!("BCryptHashData NTSTATUS={status:#x}"));
                }
            }
            let mut digest = [0u8; 32];
            let status =
                unsafe { BCryptFinishHash(hash, digest.as_mut_ptr(), digest.len() as u32, 0) };
            if status < 0 {
                return Err(format!("BCryptFinishHash NTSTATUS={status:#x}"));
            }
            Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
        })();
        unsafe {
            BCryptDestroyHash(hash);
        }
        hash_result
    })();
    unsafe {
        BCryptCloseAlgorithmProvider(algorithm, 0);
    }
    result
}

fn identity(path: &Path) -> BinaryIdentity {
    BinaryIdentity {
        canonical_path: path_text(path),
        sha256: match sha256(path) {
            Ok(v) => Metric::Valid { value: v },
            Err(e) => unavailable("BCrypt SHA-256", &e),
        },
        size_bytes: match std::fs::metadata(path) {
            Ok(m) => Metric::Valid { value: m.len() },
            Err(e) => unavailable("metadata", &e.to_string()),
        },
    }
}

fn registry_string(value: &[u16]) -> Option<String> {
    let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\0"
        .encode_utf16()
        .collect();
    let mut data = vec![0u16; 256];
    let mut bytes = (data.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            data.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = data
        .iter()
        .position(|c| *c == 0)
        .unwrap_or((bytes as usize / 2).min(data.len()));
    Some(String::from_utf16_lossy(&data[..len]))
}
fn registry_dword(value: &[u16]) -> Option<u32> {
    let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\0"
        .encode_utf16()
        .collect();
    let mut data = 0u32;
    let mut bytes = 4u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut data as *mut u32).cast(),
            &mut bytes,
        )
    };
    (rc == 0).then_some(data)
}
fn os_version() -> (Metric<String>, bool) {
    let build_name: Vec<u16> = "CurrentBuildNumber\0".encode_utf16().collect();
    let ubr_name: Vec<u16> = "UBR\0".encode_utf16().collect();
    let build = registry_string(&build_name).and_then(|v| v.parse::<u32>().ok());
    let ubr = registry_dword(&ubr_name);
    match (build, ubr) {
        (Some(b), Some(u)) => {
            let supported = (b == 19045 && u >= 3448) || (b == 22621 && u >= 2283) || b > 22621;
            (
                Metric::Valid {
                    value: format!("Windows build {b}.{u}"),
                },
                supported,
            )
        }
        _ => (
            unavailable("RegGetValueW", "Windows build/UBR unavailable"),
            false,
        ),
    }
}

fn environment(logical: Option<u32>, os: Metric<String>) -> EnvironmentRecord {
    let mut system: SYSTEM_INFO = unsafe { zeroed() };
    unsafe { GetNativeSystemInfo(&mut system) };
    let architecture = unsafe { system.Anonymous.Anonymous.wProcessorArchitecture };
    let arch = match architecture {
        PROCESSOR_ARCHITECTURE_AMD64 => "x86_64",
        PROCESSOR_ARCHITECTURE_ARM64 => "arm64",
        PROCESSOR_ARCHITECTURE_INTEL => "x86",
        _ => "unknown",
    }
    .to_owned();
    let mut memory: MEMORYSTATUSEX = unsafe { zeroed() };
    memory.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    let installed_ram_bytes = if unsafe { GlobalMemoryStatusEx(&mut memory) } != 0 {
        Metric::Valid {
            value: memory.ullTotalPhys,
        }
    } else {
        unavailable("GlobalMemoryStatusEx", "call_failed")
    };
    let cpu_identity = std::env::var("PROCESSOR_IDENTIFIER")
        .ok()
        .filter(|s| !s.is_empty())
        .map_or_else(
            || unavailable("PROCESSOR_IDENTIFIER", "not_available"),
            |value| Metric::Valid { value },
        );
    EnvironmentRecord {
        os_version: os,
        architecture: Metric::Valid { value: arch },
        architecture_provenance: "GetNativeSystemInfo.wProcessorArchitecture".into(),
        logical_processor_count: logical.map_or_else(
            || unavailable("GetActiveProcessorCount", "call_failed"),
            |v| Metric::Valid { value: v },
        ),
        installed_ram_bytes,
        cpu_identity,
        cpu_identity_provenance:
            "PROCESSOR_IDENTIFIER environment variable; may be absent or caller-overridden".into(),
    }
}

fn declarations(items: &[(DeclaredKey, String)]) -> BTreeMap<String, String> {
    items
        .iter()
        .map(|(k, v)| (k.as_str().to_owned(), v.clone()))
        .collect()
}
fn optional_run_context(declared: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    [
        "gpu",
        "backend",
        "driver",
        "power_mode",
        "refresh_rate",
        "dpi",
    ]
    .into_iter()
    .map(|key| {
        (
            key.to_owned(),
            declared
                .get(key)
                .cloned()
                .unwrap_or_else(|| "not_recorded".into()),
        )
    })
    .collect()
}
fn write_manifest(path: &Path, manifest: &impl serde::Serialize) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let tmp = path.with_extension("json.tmp");
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    // Only this invocation owns the new temporary file. A failed write or
    // replacement must not leave it blocking the final manifest update.
    let result = (|| {
        serde_json::to_writer_pretty(&mut f, manifest).map_err(|e| e.to_string())?;
        f.write_all(b"\n").map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        for attempt in 0..10 {
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } != 0
            {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            // File scanners or concurrent observers can hold a short Windows lock.
            if attempt == 9 || !matches!(error.raw_os_error(), Some(5 | 32 | 33)) {
                return Err(format!("manifest replacement failed: {error}"));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        unreachable!()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

mod orchestrator;
pub use orchestrator::{TestHooks, run_collection_with_hooks};

pub fn run_collection(args: ValidatedArgs) -> RunResult {
    orchestrator::run(args, TestHooks::default())
}

impl From<ApiError> for String {
    fn from(e: ApiError) -> Self {
        format!("{} failed ({})", e.api, e.os_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_manifest_replacement_preserves_old_data_and_can_be_retried() {
        use std::os::windows::fs::OpenOptionsExt;
        let name = format!(
            "process-measure-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("manifest.json");
        write_manifest(&path, &serde_json::json!({"state":"old"})).unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(write_manifest(&path, &serde_json::json!({"state":"new"})).is_err());
        assert!(!path.with_extension("json.tmp").exists());
        assert!(std::fs::read_to_string(&path).unwrap().contains("old"));
        drop(locked);
        write_manifest(&path, &serde_json::json!({"state":"new"})).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("new"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn absent_optional_machine_context_is_explicitly_not_recorded() {
        let empty = BTreeMap::new();
        let context = optional_run_context(&empty);
        assert_eq!(context.len(), 6);
        assert!(context.values().all(|value| value == "not_recorded"));
        let declared = BTreeMap::from([("gpu".to_owned(), "operator value".to_owned())]);
        let context = optional_run_context(&declared);
        assert_eq!(context["gpu"], "operator value");
        assert_eq!(context["backend"], "not_recorded");
    }
}
