use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use windows::core::{w, PCWSTR};
#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, HWND};
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
    FO_DELETE, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW, SHFILEOPSTRUCTW, ShellExecuteExW,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

/// Moves a file or directory safely to the Windows Recycle Bin (reversible soft-delete).
pub fn move_to_recycle_bin(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        if !path.exists() {
            return Err(format!("Path does not exist: {:?}", path));
        }

        // SHFileOperation requires a double-null-terminated UTF-16 wide string
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        wide.push(0);

        let mut op = SHFILEOPSTRUCTW {
            hwnd: HWND::default(),
            wFunc: FO_DELETE,
            pFrom: PCWSTR(wide.as_ptr()),
            pTo: PCWSTR::null(),
            fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0 | FOF_NOERRORUI.0) as u16,
            fAnyOperationsAborted: BOOL(0),
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: PCWSTR::null(),
        };

        let res = unsafe { SHFileOperationW(&mut op) };
        if res == 0 {
            if op.fAnyOperationsAborted.as_bool() {
                Err("Recycle bin operation was aborted.".to_string())
            } else {
                Ok(())
            }
        } else {
            Err(format!("SHFileOperationW failed with error code: 0x{:X}", res))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

/// Reveals a file or directory in Windows File Explorer with the item selected.
pub fn reveal_in_explorer(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new("explorer.exe");
        if path.is_dir() {
            cmd.arg(path);
        } else {
            cmd.arg(format!("/select,{}", path.to_string_lossy()));
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn().map_err(|e| format!("Failed to spawn explorer.exe: {}", e))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

/// Opens a file or folder with the system default application.
pub fn open_with_default_app(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: windows::Win32::UI::Shell::SEE_MASK_DEFAULT,
                hwnd: HWND::default(),
                lpVerb: w!("open"),
                lpFile: PCWSTR(wide.as_ptr()),
                lpParameters: PCWSTR::null(),
                lpDirectory: PCWSTR::null(),
                nShow: SW_SHOW.0,
                ..Default::default()
            };
            if ShellExecuteExW(&mut info).is_ok() {
                Ok(())
            } else {
                Err(format!("Failed to open item with default application: {:?}", path))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

/// Displays the native Windows File/Folder Properties dialog sheet.
pub fn show_file_properties(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_INVOKEIDLIST,
                hwnd: HWND::default(),
                lpVerb: w!("properties"),
                lpFile: PCWSTR(wide.as_ptr()),
                lpParameters: PCWSTR::null(),
                lpDirectory: PCWSTR::null(),
                nShow: SW_SHOW.0,
                ..Default::default()
            };
            if ShellExecuteExW(&mut info).is_ok() {
                Ok(())
            } else {
                Err(format!("Failed to open properties dialog for: {:?}", path))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}
