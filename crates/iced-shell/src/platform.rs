use std::path::PathBuf;

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

/// Display the native Windows picker. Call this from an application worker task,
/// not from the UI update/view thread. Cancellation is not an error.
#[cfg(windows)]
pub fn open_document_dialog(locate: bool) -> Result<Option<PathBuf>, String> {
    open_file_dialog(locate, false)
}

#[cfg(windows)]
pub fn open_dictionary_dialog() -> Result<Option<PathBuf>, String> {
    open_file_dialog(false, true)
}

/// Native backup/export picker, run on a worker so the reader stays responsive.
#[cfg(windows)]
pub fn transfer_dialog(
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
pub fn transfer_dialog(
    _save: bool,
    _format: Option<reader_document::backup::ExportFormat>,
) -> Result<Option<PathBuf>, String> {
    Err("The native backup/export picker requires Windows".into())
}

#[cfg(windows)]
fn open_file_dialog(locate: bool, dictionary: bool) -> Result<Option<PathBuf>, String> {
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
    let title: Vec<u16> = if dictionary {
        "Import dictionary package\0"
    } else if locate {
        "Locate moved document\0"
    } else {
        "Open document\0"
    }
    .encode_utf16()
    .collect();
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

#[cfg(not(windows))]
pub fn open_document_dialog(_locate: bool) -> Result<Option<PathBuf>, String> {
    Err("the native document picker requires Windows".into())
}

#[cfg(not(windows))]
pub fn open_dictionary_dialog() -> Result<Option<PathBuf>, String> {
    Err("the native dictionary picker requires Windows".into())
}
