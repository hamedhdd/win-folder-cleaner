#![windows_subsystem = "windows"]

use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use chrono::Local;
use eframe::egui::{Vec2, ViewportBuilder};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
use win_folder_cleaner::ui::app::WinFolderCleanerApp;

fn show_native_error_dialog(title: &str, message: &str) {
    let title_wide: Vec<u16> = std::ffi::OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let msg_wide: Vec<u16> = std::ffi::OsStr::new(message).encode_wide().chain(Some(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            HWND::default(),
            PCWSTR(msg_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn log_crash_to_file(context: &str, details: &str) {
    let temp_dir = std::env::temp_dir();
    let log_path = temp_dir.join("win_folder_cleaner_crash.log");
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");

    let entry = format!(
        "[{}] {}\n{}\n----------------------------------------\n\n",
        timestamp, context, details
    );

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

fn main() {
    // 1. Install global panic hook to guarantee no silent crash
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Internal application panic occurred.".to_string()
        };

        let err_details = format!("Location: {}\nPayload: {}", location, payload);
        log_crash_to_file("FATAL PANIC", &err_details);

        let user_msg = format!(
            "Folder Size Pro encountered an unexpected fatal error and must close.\n\n\
            Error: {}\n\
            Location: {}\n\n\
            A detailed diagnostic log was saved to:\n\
            %TEMP%\\win_folder_cleaner_crash.log",
            payload, location
        );

        show_native_error_dialog("Fatal Error - Folder Size Pro", &user_msg);
    }));

    // 2. Configure native viewport and launch eframe
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Folder Size Pro - Storage Analyzer & Cleaner")
            .with_inner_size(Vec2::new(1200.0, 780.0))
            .with_min_inner_size(Vec2::new(960.0, 620.0)),
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        "Folder Size Pro",
        native_options,
        Box::new(|cc| Ok(Box::new(WinFolderCleanerApp::new(cc)))),
    ) {
        let err_str = err.to_string();
        log_crash_to_file("STARTUP ERROR", &err_str);

        let user_msg = format!(
            "Failed to initialize the user interface window.\n\n\
            Error Details:\n\
            {}\n\n\
            Common Causes:\n\
            1. Graphics Driver: Missing DirectX 12, DirectX 11, or Vulkan hardware driver.\n\
            2. Remote Desktop (RDP) / Virtual Machine: Graphics hardware acceleration may be disabled; enable RemoteFX or WARP rasterizer.\n\
            3. Microsoft Basic Display Adapter: Standard VGA fallback driver is active.\n\n\
            Diagnostic log saved to:\n\
            %TEMP%\\win_folder_cleaner_crash.log",
            err_str
        );

        show_native_error_dialog("Startup Initialization Error - Folder Size Pro", &user_msg);
    }
}
