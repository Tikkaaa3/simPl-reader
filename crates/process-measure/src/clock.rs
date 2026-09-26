//! Clock helpers. QPC is used only for monotonic sampling; FILETIME is used
//! only for wall-clock run identity, and the two domains are never subtracted.

use crate::records::ApiError;
use crate::win;
use std::mem::zeroed;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

pub fn utc_filetime_now() -> u64 {
    let mut value: FILETIME = unsafe { zeroed() };
    unsafe {
        GetSystemTimeAsFileTime(&mut value);
    }
    win::filetime_value(value)
}

pub fn qpc() -> Result<u64, ApiError> {
    win::qpc()
}
pub fn frequency() -> Result<u64, ApiError> {
    win::qpc_frequency()
}
