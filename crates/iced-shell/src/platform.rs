use std::path::PathBuf;

/// Display the native Windows picker. Call this from an application worker task,
/// not from the UI update/view thread. Cancellation is not an error.
#[cfg(windows)]
pub fn open_document_dialog() -> Result<Option<PathBuf>, String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, OFN_DONTADDTORECENT, OFN_EXPLORER,
        OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };

    // The filter is pairs of UTF-16 strings terminated by an extra NUL.
    let filter: Vec<u16> =
        "Documents (*.html;*.htm;*.xhtml;*.pdf;*.epub)\0*.html;*.htm;*.xhtml;*.pdf;*.epub\0EPUB books (*.epub)\0*.epub\0PDF files (*.pdf)\0*.pdf\0HTML files (*.html;*.htm;*.xhtml)\0*.html;*.htm;*.xhtml\0All files (*.*)\0*.*\0\0"
            .encode_utf16()
            .collect();
    let title: Vec<u16> = "Open document\0".encode_utf16().collect();
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
pub fn open_document_dialog() -> Result<Option<PathBuf>, String> {
    Err("the native document picker requires Windows".into())
}
