#![windows_subsystem = "windows"]

use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use chrono::Local;
use eframe::egui::{Vec2, ViewportBuilder};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK};
use win_folder_cleaner::core::security::{enable_rdp_hardware_acceleration, is_rdp_session};
use win_folder_cleaner::ui::app::WinFolderCleanerApp;

fn show_native_dialog(title: &str, message: &str, is_error: bool) {
    let title_wide: Vec<u16> = std::ffi::OsStr::new(title).encode_wide().chain(Some(0)).collect();
    let msg_wide: Vec<u16> = std::ffi::OsStr::new(message).encode_wide().chain(Some(0)).collect();
    let icon = if is_error { MB_ICONERROR } else { MB_ICONINFORMATION };
    unsafe {
        let _ = MessageBoxW(
            HWND::default(),
            PCWSTR(msg_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_OK | icon,
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
    // 0. Handle CLI command to configure Remote Desktop hardware acceleration
    if std::env::args().any(|a| a == "--fix-rdp" || a == "--enable-rdp-gpu") {
        match enable_rdp_hardware_acceleration() {
            Ok(_) => {
                show_native_dialog(
                    "RDP GPU Acceleration Enabled - Folder Size Pro",
                    "Hardware graphics acceleration has been successfully enabled in the Windows Registry for Remote Desktop Services.\n\n\
                    Registry keys updated:\n\
                    - bUseHWGraphics = 1\n\
                    - bEnumerateHWUsingDirectX = 1\n\n\
                    Please disconnect and reconnect your Remote Desktop (RDP) session for the changes to take full effect.",
                    false,
                );
            }
            Err(e) => {
                show_native_dialog(
                    "RDP GPU Acceleration Error - Folder Size Pro",
                    &format!(
                        "Failed to update registry settings:\n{}\n\n\
                        Please ensure you run this command in a terminal or shortcut with Administrator privileges.",
                        e
                    ),
                    true,
                );
            }
        }
        return;
    }

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

        show_native_dialog("Fatal Error - Folder Size Pro", &user_msg, true);
    }));

    // 2. Configure environment defaults for maximum driver and software adapter compatibility
    if std::env::var("WGPU_POWER_PREF").is_err() {
        std::env::set_var("WGPU_POWER_PREF", "none");
    }

    let is_rdp = is_rdp_session();

    // 3. Configure primary viewport
    let viewport = ViewportBuilder::default()
        .with_title("Folder Size Pro - Storage Analyzer & Cleaner")
        .with_inner_size(Vec2::new(1200.0, 780.0))
        .with_min_inner_size(Vec2::new(960.0, 620.0));

    // 4. Attempt 1: DirectX / WGPU Engine (All backends, Low/None power pref allowing WARP)
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    wgpu_options.supported_backends = eframe::wgpu::Backends::all();
    wgpu_options.power_preference = eframe::wgpu::PowerPreference::None;

    let native_options_wgpu = eframe::NativeOptions {
        viewport: viewport.clone(),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        ..Default::default()
    };

    let wgpu_res = eframe::run_native(
        "Folder Size Pro",
        native_options_wgpu,
        Box::new(|cc| Ok(Box::new(WinFolderCleanerApp::new(cc)))),
    );

    // If WGPU succeeded, we are done
    let wgpu_err = match wgpu_res {
        Ok(()) => return,
        Err(e) => e,
    };

    let wgpu_err_str = wgpu_err.to_string();
    log_crash_to_file("WGPU STARTUP FAILED - ATTEMPTING OPENGL/GLOW FALLBACK", &wgpu_err_str);

    // 5. Attempt 2: OpenGL / Glow Engine Fallback
    let native_options_glow = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    let glow_res = eframe::run_native(
        "Folder Size Pro",
        native_options_glow,
        Box::new(|cc| Ok(Box::new(WinFolderCleanerApp::new(cc)))),
    );

    let glow_err = match glow_res {
        Ok(()) => return,
        Err(e) => e,
    };

    let glow_err_str = glow_err.to_string();
    let combined_err = format!(
        "Primary Engine (DirectX/WGPU) Error:\n{}\n\nSecondary Engine (OpenGL/Glow) Error:\n{}",
        wgpu_err_str, glow_err_str
    );
    log_crash_to_file("STARTUP ERROR - ALL GRAPHICS ENGINES FAILED", &combined_err);

    // 6. Present tailored, actionable resolution dialog
    let user_msg = if is_rdp {
        format!(
            "Failed to initialize the user interface window over Remote Desktop (RDP).\n\n\
            Windows Remote Desktop disables hardware graphics acceleration by default, \
            preventing both DirectX and OpenGL from acquiring an adapter.\n\n\
            Quick Resolution:\n\
            1. Open Command Prompt / PowerShell as Administrator and run:\n\
               win-folder-cleaner.exe --fix-rdp\n\
            2. Disconnect and reconnect your Remote Desktop session.\n\n\
            Alternatively, enable Group Policy:\n\
            Computer Configuration > Administrative Templates > Windows Components > \
            Remote Desktop Services > Remote Desktop Session Host > Remote Session Environment > \
            'Use the hardware default graphics adapter for all Remote Desktop Services sessions'.\n\n\
            Diagnostic log saved to:\n\
            %TEMP%\\win_folder_cleaner_crash.log"
        )
    } else {
        format!(
            "Failed to initialize the user interface window.\n\n\
            Error Details:\n\
            {}\n\n\
            Common Causes:\n\
            1. Graphics Driver: Missing DirectX 12, DirectX 11, or OpenGL 2.0+ hardware driver.\n\
            2. Microsoft Basic Display Adapter: Standard VGA fallback driver is active without hardware acceleration.\n\
            3. Virtual Machine: 3D hardware acceleration is disabled in VM settings.\n\n\
            Diagnostic log saved to:\n\
            %TEMP%\\win_folder_cleaner_crash.log",
            combined_err
        )
    };

    show_native_dialog("Startup Initialization Error - Folder Size Pro", &user_msg, true);
}
