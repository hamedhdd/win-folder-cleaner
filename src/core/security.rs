use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use crate::core::types::DriveSpaceInfo;

#[cfg(windows)]
use windows::core::{PCWSTR, w};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
#[cfg(windows)]
use windows::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
#[cfg(windows)]
use windows::Win32::UI::Shell::{SEE_MASK_DEFAULT, SHELLEXECUTEINFOW, ShellExecuteExW};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SW_NORMAL;

/// Discovers all available logical drives on the system (e.g. ["C:\\", "D:\\"]).
pub fn get_available_drives() -> Vec<String> {
    let mut drives = Vec::new();
    #[cfg(windows)]
    unsafe {
        let mask = windows::Win32::Storage::FileSystem::GetLogicalDrives();
        for i in 0..26 {
            if (mask & (1 << i)) != 0 {
                let letter = (b'A' + i) as char;
                drives.push(format!("{}:\\", letter));
            }
        }
    }
    if drives.is_empty() {
        drives.push("C:\\".to_string());
    }
    drives
}

/// Queries capacity, used bytes, and free bytes for all available Windows drives.
pub fn get_all_drives_space_info() -> Vec<DriveSpaceInfo> {
    let drives = get_available_drives();
    let mut infos = Vec::new();

    for d in drives {
        #[cfg(windows)]
        {
            let mut root_wide: Vec<u16> = d.encode_utf16().collect();
            root_wide.push(0);

            let mut free_available = 0u64;
            let mut total = 0u64;
            let mut total_free = 0u64;

            unsafe {
                let res = windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                    windows::core::PCWSTR(root_wide.as_ptr()),
                    Some(&mut free_available),
                    Some(&mut total),
                    Some(&mut total_free),
                );
                if res.is_ok() && total > 0 {
                    let used = total.saturating_sub(total_free);
                    infos.push(DriveSpaceInfo {
                        letter: d,
                        total_bytes: total,
                        used_bytes: used,
                        free_bytes: total_free,
                    });
                }
            }
        }
    }

    infos
}

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Checks whether the current process is running with Administrator (elevated) privileges.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
            let mut elevation = TOKEN_ELEVATION::default();
            let mut return_len = 0u32;
            let result = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut return_len,
            );
            let _ = CloseHandle(token);
            if result.is_ok() {
                return elevation.TokenIsElevated != 0;
            }
        }
        false
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Relaunches the current executable as Administrator using Windows ShellExecuteEx with the "runas" verb.
/// Preserves command-line arguments.
pub fn relaunch_as_admin() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe_path = std::env::current_exe().map_err(|e| format!("Failed to find current executable: {}", e))?;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let args_str = args.join(" ");

        let mut exe_wide: Vec<u16> = exe_path.as_os_str().encode_wide().collect();
        exe_wide.push(0);

        let mut args_wide: Vec<u16> = OsStr::new(&args_str).encode_wide().collect();
        args_wide.push(0);

        unsafe {
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_DEFAULT,
                hwnd: HWND::default(),
                lpVerb: w!("runas"),
                lpFile: PCWSTR(exe_wide.as_ptr()),
                lpParameters: if args.is_empty() {
                    PCWSTR::null()
                } else {
                    PCWSTR(args_wide.as_ptr())
                },
                lpDirectory: PCWSTR::null(),
                nShow: SW_NORMAL.0,
                ..Default::default()
            };

            if ShellExecuteExW(&mut info).is_ok() {
                // Exit current process so the elevated one takes over
                std::process::exit(0);
            } else {
                Err("Administrator elevation prompt was cancelled or denied by user.".to_string())
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("Elevation relaunch is only supported on Windows.".to_string())
    }
}

/// Validates whether a path is a critical protected Windows directory that should never be deleted.
///
/// Protected locations include:
/// - Root drives (e.g., C:\, D:\)
/// - C:\Windows and subdirectories (except specific safe temp folders like C:\Windows\Temp root preservation)
/// - C:\Windows\System32, SysWOW64, WinSxS
/// - C:\Program Files and C:\Program Files (x86)
/// - C:\ProgramData root
/// - C:\Users and individual user profile root folders (e.g., C:\Users\Alice)
pub fn is_critical_system_path(path: &Path) -> bool {
    let normalized = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    };

    let path_str = normalized.to_string_lossy().to_ascii_uppercase();
    let trimmed = path_str.trim_start_matches(r"\\?\");

    // Root directory protection (e.g. "C:\", "D:\", "/")
    if trimmed.len() <= 3 && trimmed.ends_with(r":\") {
        return true;
    }
    if trimmed == r"\" || trimmed == "/" || trimmed.is_empty() {
        return true;
    }

    let exact_protected = [
        r"C:\WINDOWS",
        r"C:\WINDOWS\SYSTEM32",
        r"C:\WINDOWS\SYSWOW64",
        r"C:\WINDOWS\WINSXS",
        r"C:\PROGRAM FILES",
        r"C:\PROGRAM FILES (X86)",
        r"C:\PROGRAMDATA",
        r"C:\USERS",
    ];

    for prot in exact_protected {
        if trimmed == prot {
            return true;
        }
    }

    // Protect C:\Windows subtrees except Temp
    if trimmed.starts_with(r"C:\WINDOWS\") && !trimmed.starts_with(r"C:\WINDOWS\TEMP") {
        return true;
    }

    // Protect User Profile root folders (e.g. C:\Users\Username)
    if trimmed.starts_with(r"C:\USERS\") {
        let parts: Vec<&str> = trimmed.split('\\').filter(|s| !s.is_empty()).collect();
        // C:, USERS, <USERNAME> -> length 3 is user root folder
        if parts.len() <= 3 {
            return true;
        }
    }

    false
}

/// Removes the read-only attribute from a file or folder.
pub fn remove_readonly(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        let mut perms = metadata.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms)
                .map_err(|e| format!("Failed to remove read-only attribute on {:?}: {}", path, e))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn enable_take_ownership_privilege() -> Result<(), String> {
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, TOKEN_ADJUST_PRIVILEGES,
        TOKEN_PRIVILEGES, TOKEN_QUERY, SE_PRIVILEGE_ENABLED,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::Foundation::HANDLE;
    use windows::core::w;

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token).is_err() {
            return Err("Failed to open process token for privilege adjustment.".to_string());
        }

        let mut luid = windows::Win32::Foundation::LUID::default();
        if LookupPrivilegeValueW(None, w!("SeTakeOwnershipPrivilege"), &mut luid).is_err() {
            let _ = windows::Win32::Foundation::CloseHandle(token);
            return Err("Failed to lookup SeTakeOwnershipPrivilege LUID.".to_string());
        }

        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };

        let res = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
        let _ = windows::Win32::Foundation::CloseHandle(token);
        if res.is_ok() {
            Ok(())
        } else {
            Err("AdjustTokenPrivileges failed to enable SeTakeOwnershipPrivilege.".to_string())
        }
    }
}

#[cfg(not(windows))]
pub fn enable_take_ownership_privilege() -> Result<(), String> {
    Ok(())
}

/// Takes administrative ownership of a path equivalent to:
/// `takeown /F "<path>" /A /R /D Y`
///
/// Uses direct `std::process::Command` without cmd.exe or PowerShell.
pub fn take_ownership(path: &Path) -> Result<(), String> {
    let _ = enable_take_ownership_privilege();
    let path_str = path.to_string_lossy().to_string();
    let mut cmd = Command::new("takeown.exe");
    cmd.args(&["/F", &path_str, "/A", "/R", "/D", "Y"]);
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().map_err(|e| format!("Failed to execute takeown.exe: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        Err(format!("takeown failed (code {:?}): {}", output.status.code(), msg))
    }
}

/// Grants the Administrators group (*S-1-5-32-544) Full Control permissions equivalent to:
/// `icacls "<path>" /grant *S-1-5-32-544:(OI)(CI)F /T /C`
///
/// Preserves existing ACL entries and uses SID instead of localized names.
/// Uses direct `std::process::Command` without cmd.exe or PowerShell.
pub fn grant_administrators_acl(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy().to_string();
    let mut cmd = Command::new("icacls.exe");
    cmd.args(&[&path_str, "/grant", "*S-1-5-32-544:(OI)(CI)F", "/T", "/C"]);
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().map_err(|e| format!("Failed to execute icacls.exe: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        Err(format!("icacls failed (code {:?}): {}", output.status.code(), msg))
    }
}
