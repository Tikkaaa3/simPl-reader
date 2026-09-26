//! Disposable controlled child for process-measure native tests.
use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
};

fn ms(v: Option<String>) -> Duration {
    Duration::from_millis(
        v.expect("milliseconds")
            .parse()
            .expect("integer milliseconds"),
    )
}
fn marker(path: Option<String>, text: &str) {
    if let Some(p) = path {
        let path = Path::new(&p);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text).expect("write marker");
        std::fs::rename(tmp, path).expect("publish marker");
    }
}

#[allow(clippy::zombie_processes)] // helper-root intentionally exits before its owned helper; the Job Object must reap it.
fn main() {
    let mut a = std::env::args().skip(1);
    let mode = a.next().unwrap_or_else(|| "sleep".into());
    match mode.as_str() {
        "sleep" => {
            let d = ms(a.next());
            marker(a.next(), "ready");
            std::thread::sleep(d);
        }
        "busy" => {
            let d = ms(a.next());
            marker(a.next(), "ready");
            let until = Instant::now() + d;
            let mut x = 1u64;
            while Instant::now() < until {
                x = black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
            }
            black_box(x);
        }
        "exit" => std::process::exit(a.next().expect("code").parse().expect("exit code")),
        "sleep-exit" => {
            std::thread::sleep(ms(a.next()));
            std::process::exit(a.next().expect("code").parse().expect("exit code"));
        }
        "echo" => {
            for (i, v) in a.enumerate() {
                println!("{i}:{v}");
            }
        }
        "memory" => {
            let kind = a.next().expect("reserve|commit|touch");
            let bytes: usize = a.next().expect("bytes").parse().expect("byte count");
            let hold = ms(a.next());
            let flags = if kind == "reserve" {
                MEM_RESERVE
            } else {
                MEM_RESERVE | MEM_COMMIT
            };
            let p = unsafe { VirtualAlloc(std::ptr::null(), bytes, flags, PAGE_READWRITE) };
            if p.is_null() {
                std::process::exit(70)
            }
            if kind == "touch" {
                let page = 4096;
                for i in (0..bytes).step_by(page) {
                    unsafe { (p as *mut u8).add(i).write_volatile(1) }
                }
            }
            marker(a.next(), "ready");
            std::thread::sleep(hold);
            unsafe { VirtualFree(p, 0, MEM_RELEASE) };
        }
        "check-handle" => {
            use windows_sys::Win32::Foundation::GetHandleInformation;
            use windows_sys::Win32::Storage::FileSystem::WriteFile;
            let raw: usize = a.next().expect("handle").parse().expect("numeric handle");
            let handle = raw as *mut std::ffi::c_void;
            let mut flags = 0u32;
            let valid = unsafe { GetHandleInformation(handle, &mut flags) } != 0;
            let probe = b"sentinel-inherited";
            let mut written = 0u32;
            let wrote = valid
                && unsafe {
                    WriteFile(
                        handle,
                        probe.as_ptr(),
                        probe.len() as u32,
                        &mut written,
                        std::ptr::null_mut(),
                    )
                } != 0;
            marker(
                a.next(),
                if wrote {
                    "wrote_sentinel"
                } else {
                    "not_inherited"
                },
            );
            std::thread::sleep(ms(a.next()));
        }
        "helper-root" => {
            let helper_ms = a.next().expect("helper ms");
            let root_hold = ms(a.next());
            let marker_path = a.next();
            let exe = std::env::current_exe().unwrap();
            let child = std::process::Command::new(exe)
                .args(["sleep", &helper_ms])
                .spawn()
                .expect("spawn helper");
            marker(marker_path, &child.id().to_string());
            std::thread::sleep(root_hold);
        }
        _ => {
            eprintln!("unknown mode");
            std::process::exit(64)
        }
    }
}
