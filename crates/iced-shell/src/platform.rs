use std::path::{Path, PathBuf};

/// Let Setup/Uninstall ask for a normal close before replacing the reader or
/// removing its profile. This is a lifetime marker, not a single-instance lock.
pub fn mark_reader_running() {
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Local\\simPl.Reader.Running\0".encode_utf16().collect();
    // SAFETY: a terminated UTF-16 name remains valid for this call, with no
    // security attributes and no ownership requested. Windows releases the
    // deliberately retained handle when the process terminates, including exit().
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        eprintln!("Could not register the installer running-app marker");
    }
}

/// Runs blocking `work` on a thread of its own and resolves with its result.
///
/// iced polls a new task on the UI thread before the executor gets it, and
/// polls it again there while a batch it belongs to still has ready work. So
/// blocking work inside a task's future can run in the window's event handler
/// and freeze the window. On its own thread it cannot, however the task is polled.
pub async fn on_worker_thread<T: Send + 'static>(
    name: &'static str,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _ = sender.send(work());
        })
        .map_err(|error| format!("Cannot start {name}: {error}"))?;
    receiver
        .await
        .map_err(|_| format!("{name} stopped unexpectedly."))
}

/// Runs a native dialog on a thread of its own and resolves when it closes.
///
/// iced polls a new task once on the UI thread before the executor gets it, so
/// a dialog called straight from a task ran its modal loop inside the window's
/// event handler. There the shell never finished listing the Downloads folder
/// when it was chosen from the dialog's side list. The thread gets a COM
/// apartment of its own, which the shell dialogs need.
#[cfg(windows)]
async fn on_dialog_thread<T: Send + 'static>(
    name: &'static str,
    dialog: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    /// Leaves the apartment when the dialog returns or panics.
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // SAFETY: balances the successful CoInitializeEx that made this value.
            unsafe { CoUninitialize() };
        }
    }
    on_worker_thread(name, move || {
        // SAFETY: initializes this new thread's apartment once.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
            .ok()
            .map_err(|error| format!("Cannot open the dialog: {error}"))
            .and_then(|()| {
                let _apartment = Apartment;
                dialog()
            })
    })
    .await?
}

/// Display the native Windows picker. Cancellation is not an error.
#[cfg(windows)]
pub async fn open_document_dialog(title: &'static str) -> Result<Option<PathBuf>, String> {
    on_dialog_thread("document-dialog", move || open_file_dialog(title, false)).await
}

/// Blocks until the picker closes. Call it from a worker thread, never from a task.
#[cfg(windows)]
pub fn open_dictionary_dialog() -> Result<Option<PathBuf>, String> {
    open_file_dialog("Import dictionary package", true)
}

/// Native backup/export picker.
#[cfg(windows)]
pub async fn transfer_dialog(
    save: bool,
    format: Option<reader_document::backup::ExportFormat>,
) -> Result<Option<PathBuf>, String> {
    on_dialog_thread("transfer-dialog", move || {
        show_transfer_dialog(save, format)
    })
    .await
}

#[cfg(windows)]
fn show_transfer_dialog(
    save: bool,
    format: Option<reader_document::backup::ExportFormat>,
) -> Result<Option<PathBuf>, String> {
    use reader_document::backup::ExportFormat;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_DONTADDTORECENT,
        OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
        OPENFILENAMEW,
    };
    let (filter, extension, name, title) = match format {
        Some(ExportFormat::Markdown) => (
            "Markdown (*.md)\0*.md\0\0",
            "md\0",
            "Reading notes.md",
            "Export reading notes\0",
        ),
        Some(ExportFormat::Text) => (
            "Text (*.txt)\0*.txt\0\0",
            "txt\0",
            "Reading notes.txt",
            "Export reading notes\0",
        ),
        Some(ExportFormat::Json) => (
            "JSON (*.json)\0*.json\0\0",
            "json\0",
            "Reading notes.json",
            "Export reading notes\0",
        ),
        None => (
            "simPl backup (*.zip)\0*.zip\0\0",
            "zip\0",
            "simPl-backup.zip",
            if save {
                "Create library backup\0"
            } else {
                "Restore library backup\0"
            },
        ),
    };
    let filter: Vec<u16> = filter.encode_utf16().collect();
    let extension: Vec<u16> = extension.encode_utf16().collect();
    let title: Vec<u16> = title.encode_utf16().collect();
    let mut filename = vec![0_u16; 32_768];
    if save {
        for (index, unit) in name.encode_utf16().enumerate() {
            filename[index] = unit;
        }
    }
    let mut options = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: filter.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: filename.as_mut_ptr(),
        nMaxFile: filename.len() as u32,
        lpstrTitle: title.as_ptr(),
        lpstrDefExt: extension.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_DONTADDTORECENT
            | OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..OPENFILENAMEW::default()
    };
    // SAFETY: all referenced UTF-16 buffers stay alive through the synchronous dialog.
    let chosen = unsafe {
        if save {
            GetSaveFileNameW(&mut options)
        } else {
            GetOpenFileNameW(&mut options)
        }
    };
    if chosen == 0 {
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 {
            Ok(None)
        } else {
            Err(format!("File dialog failed (0x{error:04X})"))
        };
    }
    let end = filename
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("Unterminated dialog filename")?;
    Ok(Some(OsString::from_wide(&filename[..end]).into()))
}

#[cfg(not(windows))]
pub async fn transfer_dialog(
    _save: bool,
    _format: Option<reader_document::backup::ExportFormat>,
) -> Result<Option<PathBuf>, String> {
    Err("The native backup/export picker requires Windows".into())
}

#[cfg(windows)]
fn open_file_dialog(title: &str, dictionary: bool) -> Result<Option<PathBuf>, String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, OFN_DONTADDTORECENT, OFN_EXPLORER,
        OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };

    // The filter is pairs of UTF-16 strings terminated by an extra NUL.
    let document_filter = "Documents (*.html;*.htm;*.xhtml;*.pdf;*.epub;*.txt;*.md)\0*.html;*.htm;*.xhtml;*.pdf;*.epub;*.txt;*.text;*.md;*.markdown\0EPUB books (*.epub)\0*.epub\0PDF files (*.pdf)\0*.pdf\0HTML files (*.html;*.htm;*.xhtml)\0*.html;*.htm;*.xhtml\0Text and Markdown (*.txt;*.md)\0*.txt;*.text;*.md;*.markdown\0All files (*.*)\0*.*\0\0";
    let filter: Vec<u16> = if dictionary {
        "simPl dictionary packages (*.zip)\0*.zip\0\0"
    } else {
        document_filter
    }
    .encode_utf16()
    .collect();
    let title: Vec<u16> = title.encode_utf16().chain([0]).collect();
    let mut filename = vec![0_u16; 32_768];
    let mut options = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: filter.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: filename.as_mut_ptr(),
        nMaxFile: filename.len() as u32,
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_NOCHANGEDIR
            | OFN_DONTADDTORECENT,
        ..OPENFILENAMEW::default()
    };
    // SAFETY: the struct and all referenced UTF-16 buffers remain alive and
    // writable as required throughout this synchronous Win32 call.
    if unsafe { GetOpenFileNameW(&mut options) } == 0 {
        // Per common-dialog API, zero means the user cancelled; nonzero means
        // the dialog itself failed (including an undersized filename buffer).
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 {
            Ok(None)
        } else {
            Err(format!(
                "cannot open native file dialog (Win32 error 0x{error:04X})"
            ))
        };
    }
    let end = filename
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("native file dialog returned an unterminated path")?;
    if end == 0 {
        return Err("native file dialog returned an empty path".into());
    }
    Ok(Some(OsString::from_wide(&filename[..end]).into()))
}

#[cfg(windows)]
pub async fn pick_folder(
    title: &'static str,
    initial: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
        IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::HSTRING;

    let title = HSTRING::from(title);
    let initial = initial.map(|path| {
        use std::os::windows::ffi::OsStrExt;
        HSTRING::from_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
    });
    let picker = move || -> windows::core::Result<Option<PathBuf>> {
        // SAFETY: every COM call below runs on the dialog thread, whose
        // apartment is initialized, and the strings they borrow outlive the calls.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST,
            )?;
            dialog.SetTitle(&title)?;
            if let Some(initial) = &initial
                && let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(initial, None)
            {
                dialog.SetFolder(&folder)?;
            }
            match dialog.Show(None) {
                Ok(()) => {}
                Err(error)
                    if error.code() == windows::Win32::Foundation::ERROR_CANCELLED.to_hresult() =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
            let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = std::ffi::OsString::from_wide(name.as_wide());
            CoTaskMemFree(Some(name.0.cast()));
            Ok(Some(path.into()))
        }
    };
    on_dialog_thread("folder-picker", move || {
        picker().map_err(|error| format!("Folder picker failed: {error}"))
    })
    .await
}

#[cfg(not(windows))]
pub async fn pick_folder(
    _title: &'static str,
    _initial: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    Err("the native folder picker requires Windows".into())
}

pub fn open_in_explorer(folder: &Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(reader_document::library_location::display_path(folder))
        .spawn()
        .map(drop)
        .map_err(|error| format!("Cannot open Explorer: {error}"))
}

#[cfg(not(windows))]
pub async fn open_document_dialog(_title: &'static str) -> Result<Option<PathBuf>, String> {
    Err("the native document picker requires Windows".into())
}

#[cfg(not(windows))]
pub fn open_dictionary_dialog() -> Result<Option<PathBuf>, String> {
    Err("the native dictionary picker requires Windows".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use iced::futures::{FutureExt, executor::block_on, task::noop_waker_ref};
    use std::task::{Context, Poll};

    #[test]
    fn work_on_a_worker_thread_never_runs_on_the_thread_that_polls_it() {
        let polling = std::thread::current().id();
        let mut work = on_worker_thread("test-worker", || std::thread::current().id()).boxed();
        // iced can poll a task many times on the UI thread while it drains a
        // batch. However often this thread polls, the work runs elsewhere.
        let ran_on = loop {
            if let Poll::Ready(ran_on) = work.poll_unpin(&mut Context::from_waker(noop_waker_ref()))
            {
                break ran_on;
            }
            std::thread::yield_now();
        };
        assert_ne!(ran_on, Ok(polling));
        assert!(ran_on.is_ok());
    }

    #[test]
    fn a_dialog_never_runs_on_the_thread_that_polls_its_task() {
        let polling = std::thread::current().id();
        let mut dialog =
            on_dialog_thread("test-dialog", || Ok(std::thread::current().id())).boxed();
        // iced polls a new task once on the UI thread, as this poll does.
        let ran_on = match dialog.poll_unpin(&mut Context::from_waker(noop_waker_ref())) {
            Poll::Ready(ran_on) => ran_on,
            Poll::Pending => block_on(dialog),
        };
        assert_ne!(ran_on, Ok(polling));
        assert!(ran_on.is_ok());
    }

    #[test]
    fn a_dialog_that_fails_reports_its_error() {
        let failed: Result<(), String> =
            block_on(on_dialog_thread("test-dialog", || Err("no picker".into())));
        assert_eq!(failed, Err("no picker".into()));
    }
}
