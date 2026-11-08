#![windows_subsystem = "windows"]

use eframe::egui::{Vec2, ViewportBuilder};
use win_folder_cleaner::ui::app::WinFolderCleanerApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Folder Size Pro - Storage Analyzer & Cleaner")
            .with_inner_size(Vec2::new(1200.0, 780.0))
            .with_min_inner_size(Vec2::new(960.0, 620.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Folder Size Pro",
        native_options,
        Box::new(|cc| Ok(Box::new(WinFolderCleanerApp::new(cc)))),
    )
}
