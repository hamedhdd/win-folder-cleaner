use std::os::windows::ffi::OsStrExt;
use std::path::Path;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};

/// Schedules a file or directory for deletion during the next Windows reboot.
/// Uses the Win32 `MoveFileExW` API with `MOVEFILE_DELAY_UNTIL_REBOOT`.
pub fn schedule_deletion_on_reboot(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        let mut path_wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        path_wide.push(0);

        let res = MoveFileExW(
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            MOVEFILE_DELAY_UNTIL_REBOOT,
        );

        if res.is_ok() {
            Ok(())
        } else {
            let err = std::io::Error::last_os_error();
            Err(format!("MoveFileExW failed: {}", err))
        }
    }
    #[cfg(not(windows))]
    {
        Err("Schedule on reboot is only supported on Windows.".to_string())
    }
}
