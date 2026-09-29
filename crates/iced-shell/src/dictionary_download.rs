//! Bounded Windows HTTPS downloads. Called only after an explicit UI action.
use reader_document::dictionary::Package;
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::Networking::WinHttp::*;

struct Internet(*mut c_void);
impl Internet {
    fn new(handle: *mut c_void) -> Result<Self, String> {
        if handle.is_null() {
            Err(network_error())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for Internet {
    fn drop(&mut self) {
        // SAFETY: exclusively owned WinHTTP handle, closed exactly once.
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn network_error() -> String {
    format!(
        "Could not download dictionary ({}). Check your connection and try again.",
        std::io::Error::last_os_error()
    )
}
fn checked(result: i32) -> Result<(), String> {
    if result == 0 {
        Err(network_error())
    } else {
        Ok(())
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn cancelled(cancel: &AtomicBool, started: Instant) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Download cancelled.".into());
    }
    if started.elapsed() > Duration::from_secs(180) {
        return Err("Download timed out. Try again.".into());
    }
    Ok(())
}

pub fn download(
    package: &Package,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<Vec<u8>, String> {
    download_url(&package.url(), package.bytes, cancel, &mut progress)
}

fn download_url(
    url: &str,
    expected: u64,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64),
) -> Result<Vec<u8>, String> {
    let (host, path) = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split_once('/'))
        .ok_or("Invalid dictionary download address.")?;
    if expected > 8 * 1024 * 1024 {
        return Err("Dictionary package exceeds download limit.".into());
    }
    let started = Instant::now();
    cancelled(cancel, started)?;
    let agent = wide("simPl dictionary downloader/1");
    let server = wide(host);
    let resource = wide(&format!("/{path}"));
    // SAFETY: all UTF-16 buffers are terminated and live for these synchronous
    // calls. Handles are RAII owned; this function runs on an application worker.
    let session = Internet::new(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    })?;
    checked(unsafe { WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000) })?;
    let connection = Internet::new(unsafe { WinHttpConnect(session.0, server.as_ptr(), 443, 0) })?;
    let request = Internet::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            std::ptr::null(),
            resource.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    })?;
    // Never forward Windows credentials to the release host or its redirect.
    let policy = WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH;
    checked(unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_AUTOLOGON_POLICY,
            (&policy as *const u32).cast(),
            4,
        )
    })?;
    let redirects = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
    checked(unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&redirects as *const u32).cast(),
            4,
        )
    })?;
    cancelled(cancel, started)?;
    checked(unsafe {
        WinHttpSendRequest(request.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0)
    })?;
    checked(unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) })?;
    let mut status = 0_u32;
    let mut size = 4_u32;
    checked(unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
        )
    })?;
    if status != 200 {
        return Err(format!(
            "Dictionary download returned HTTP {status}. Try again later."
        ));
    }
    let mut data = Vec::with_capacity(expected as usize);
    let mut buffer = [0_u8; 32 * 1024];
    let mut last_progress = 0;
    loop {
        cancelled(cancel, started)?;
        let mut read = 0_u32;
        checked(unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        })?;
        if read == 0 {
            break;
        }
        if data.len() as u64 + u64::from(read) > expected {
            return Err("Downloaded dictionary is larger than expected.".into());
        }
        data.extend_from_slice(&buffer[..read as usize]);
        let percentage = data.len() as u64 * 100 / expected.max(1);
        if percentage != last_progress {
            progress(data.len() as u64);
            last_progress = percentage;
        }
    }
    cancelled(cancel, started)?;
    if data.len() as u64 != expected {
        return Err("Dictionary download was incomplete. Try again.".into());
    }
    progress(expected);
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_document::dictionary::{self, Language, PackageId, Store};

    #[test]
    fn cancelled_download_never_opens_a_connection() {
        let cancelled = AtomicBool::new(true);
        let error = download(
            dictionary::package(PackageId(0)).unwrap(),
            &cancelled,
            |_| panic!("no progress before a cancelled request"),
        )
        .unwrap_err();
        assert_eq!(error, "Download cancelled.");
    }

    #[test]
    #[ignore = "Published data QA: fetches all 13 GitHub packages into an owned temporary store"]
    fn published_packages_download_install_and_work_offline() {
        let root =
            std::env::temp_dir().join(format!("simpl-live-dictionaries-{}", std::process::id()));
        struct Owned(std::path::PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        assert!(!root.exists(), "use a fresh owned directory");
        let owned = Owned(root.clone());
        let cancel = AtomicBool::new(false);
        let store = Store::new(root.clone());
        for (i, package) in dictionary::packages().iter().enumerate() {
            let mut last = 0;
            let bytes = download(package, &cancel, |received| {
                assert!(received >= last);
                last = received;
            })
            .unwrap();
            assert_eq!(last, package.bytes);
            store.install(PackageId(i), &bytes, &cancel).unwrap();
            println!(
                "Downloaded and verified {} ({} bytes)",
                package.label(),
                bytes.len()
            );
        }
        drop(store);
        let reopened = Store::new(root);
        assert!(
            reopened
                .inventory()
                .iter()
                .all(|state| *state == dictionary::PackageState::Installed)
        );
        for (source, word) in [
            (Language::Turkish, "kitap"),
            (Language::Spanish, "libro"),
            (Language::German, "Buch"),
            (Language::French, "livre"),
            (Language::Japanese, "本"),
            (Language::Korean, "책"),
            (Language::Chinese, "書"),
        ] {
            assert!(
                reopened
                    .lookup(word, source, Language::English)
                    .unwrap()
                    .is_some()
            );
        }
        for target in Language::English.targets() {
            assert!(
                reopened
                    .lookup("book", Language::English, target)
                    .unwrap()
                    .is_some()
            );
        }
        drop(reopened);
        drop(owned);
    }
}
