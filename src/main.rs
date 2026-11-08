use eframe::egui::{Vec2, ViewportBuilder};
use win_folder_cleaner::ui::app::WinFolderCleanerApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Windows Folder Analyzer & Advanced Cleaner")
            .with_inner_size(Vec2::new(1120.0, 740.0))
            .with_min_inner_size(Vec2::new(880.0, 580.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Windows Folder Analyzer & Advanced Cleaner",
        native_options,
        Box::new(|cc| Ok(Box::new(WinFolderCleanerApp::new(cc)))),
    )
}
