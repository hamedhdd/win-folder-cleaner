use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use crate::core::types::LockingProcessInfo;

#[cfg(windows)]
use windows::core::{PCWSTR, PWSTR};
#[cfg(windows)]
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession,
    RM_PROCESS_INFO,
};

/// Queries the Windows Restart Manager to identify any running processes locking the specified file.
pub fn get_locking_processes(file_path: &Path) -> Vec<LockingProcessInfo> {
    let mut locking_apps = Vec::new();

    #[cfg(windows)]
    unsafe {
        let mut session_handle = 0u32;
        let mut session_key = [0u16; 33]; // CCH_RM_SESSION_KEY + 1 is 33

        let start_res = RmStartSession(&mut session_handle, 0, PWSTR(session_key.as_mut_ptr()));
        if start_res.is_err() {
            return locking_apps;
        }

        let mut path_wide: Vec<u16> = file_path.as_os_str().encode_wide().collect();
        path_wide.push(0);

        let files = [PCWSTR(path_wide.as_ptr())];
        let reg_res = RmRegisterResources(session_handle, Some(&files), None, None);
        if reg_res.is_ok() {
            let mut proc_info_needed = 0u32;
            let mut proc_info_count = 0u32;
            let mut reboot_reasons = 0u32;

            // First call with null buffer to query count needed
            let _ = RmGetList(
                session_handle,
                &mut proc_info_needed,
                &mut proc_info_count,
                None,
                &mut reboot_reasons,
            );

            if proc_info_needed > 0 {
                let mut proc_infos = vec![RM_PROCESS_INFO::default(); proc_info_needed as usize];
                proc_info_count = proc_info_needed;

                let get_list_res = RmGetList(
                    session_handle,
                    &mut proc_info_needed,
                    &mut proc_info_count,
                    Some(proc_infos.as_mut_ptr()),
                    &mut reboot_reasons,
                );

                if get_list_res.is_ok() {
                    for info in proc_infos.iter().take(proc_info_count as usize) {
                        let app_name = decode_wide_null_terminated(&info.strAppName);
                        let service_name = decode_wide_null_terminated(&info.strServiceShortName);

                        locking_apps.push(LockingProcessInfo {
                            pid: info.Process.dwProcessId,
                            app_name,
                            service_name,
                        });
                    }
                }
            }
        }

        let _ = RmEndSession(session_handle);
    }

    locking_apps
}

#[cfg(windows)]
fn decode_wide_null_terminated(wide_slice: &[u16]) -> String {
    let len = wide_slice.iter().position(|&c| c == 0).unwrap_or(wide_slice.len());
    String::from_utf16_lossy(&wide_slice[..len])
}
