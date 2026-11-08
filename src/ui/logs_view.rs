use std::fs::File;
use std::io::Write;
use eframe::egui::{
    self, ComboBox, Frame, Margin, RichText, Rounding, ScrollArea, Stroke, Ui,
};

use crate::core::types::{LogEntry, LogStatus};
use crate::ui::theme::ThemeColors;

pub struct LogsViewState {
    pub search_query: String,
    pub status_filter: Option<LogStatus>,
    pub export_message: Option<String>,
}

impl Default for LogsViewState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            status_filter: None,
            export_message: None,
        }
    }
}

pub fn render_logs_view(
    ui: &mut Ui,
    state: &mut LogsViewState,
    logs: &mut Vec<LogEntry>,
) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

    // Toolbar: Filter, Search, Export buttons
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Filter:").color(ThemeColors::TEXT_MUTED));

                let current_label = match state.status_filter {
                    None => "All Events",
                    Some(LogStatus::Success) => "Success",
                    Some(LogStatus::Warning) => "Warning",
                    Some(LogStatus::Error) => "Error",
                    Some(LogStatus::DryRun) => "Dry Run",
                    Some(LogStatus::Skipped) => "Skipped",
                };

                ComboBox::from_id_salt("log_status_filter_combo")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.status_filter, None, "All Events");
                        ui.selectable_value(&mut state.status_filter, Some(LogStatus::Success), "Success");
                        ui.selectable_value(&mut state.status_filter, Some(LogStatus::Warning), "Warning");
                        ui.selectable_value(&mut state.status_filter, Some(LogStatus::Error), "Error");
                        ui.selectable_value(&mut state.status_filter, Some(LogStatus::DryRun), "Dry Run");
                        ui.selectable_value(&mut state.status_filter, Some(LogStatus::Skipped), "Skipped");
                    });

                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text("Search path, action, or details...")
                        .desired_width(260.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear Log").clicked() {
                        logs.clear();
                        state.export_message = None;
                    }

                    if ui.button("Export to TXT").clicked() {
                        export_logs_txt(logs, state);
                    }

                    if ui.button("Export to CSV").clicked() {
                        export_logs_csv(logs, state);
                    }
                });
            });
        });

    if let Some(msg) = &state.export_message {
        ui.label(RichText::new(msg).color(ThemeColors::STATUS_SUCCESS));
    }

    // Logs Table
    let search = state.search_query.to_lowercase();
    let filter = state.status_filter;

    let filtered_logs: Vec<&LogEntry> = logs
        .iter()
        .rev() // Most recent first
        .filter(|entry| {
            if let Some(st) = filter {
                if entry.status != st {
                    return false;
                }
            }
            if search.is_empty() {
                true
            } else {
                entry.target_path.to_lowercase().contains(&search)
                    || entry.action.to_lowercase().contains(&search)
                    || entry.details.to_lowercase().contains(&search)
            }
        })
        .collect();

    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("logs_table_grid")
            .striped(true)
            .spacing([12.0, 6.0])
            .min_col_width(90.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Timestamp").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Status").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Action").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Target Path").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Details").strong().color(ThemeColors::TEXT_MUTED));
                ui.end_row();

                for entry in filtered_logs {
                    ui.label(
                        RichText::new(entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string())
                            .color(ThemeColors::TEXT_MUTED)
                            .monospace(),
                    );

                    let (status_str, status_color) = match entry.status {
                        LogStatus::Success => ("SUCCESS", ThemeColors::STATUS_SUCCESS),
                        LogStatus::Warning => ("WARNING", ThemeColors::STATUS_WARNING),
                        LogStatus::Error => ("ERROR", ThemeColors::STATUS_ERROR),
                        LogStatus::DryRun => ("DRY-RUN", ThemeColors::STATUS_INFO),
                        LogStatus::Skipped => ("SKIPPED", ThemeColors::TEXT_MUTED),
                    };

                    ui.label(RichText::new(status_str).color(status_color).strong());
                    ui.label(RichText::new(&entry.action).color(ThemeColors::TEXT_PRIMARY));
                    ui.label(RichText::new(&entry.target_path).color(ThemeColors::TEXT_SECONDARY).monospace().size(11.0));
                    ui.label(RichText::new(&entry.details).color(ThemeColors::TEXT_PRIMARY));
                    ui.end_row();
                }
            });
    });
}

fn export_logs_csv(logs: &[LogEntry], state: &mut LogsViewState) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV Files (*.csv)", &["csv"])
        .set_file_name("win_folder_cleaner_log.csv")
        .save_file()
    {
        if let Ok(mut file) = File::create(&path) {
            let _ = writeln!(file, "Timestamp,Status,Action,TargetPath,Details");
            for entry in logs {
                let status_str = match entry.status {
                    LogStatus::Success => "Success",
                    LogStatus::Warning => "Warning",
                    LogStatus::Error => "Error",
                    LogStatus::DryRun => "DryRun",
                    LogStatus::Skipped => "Skipped",
                };
                let escaped_path = entry.target_path.replace('"', "\"\"");
                let escaped_details = entry.details.replace('"', "\"\"");
                let _ = writeln!(
                    file,
                    "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"",
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    status_str,
                    entry.action,
                    escaped_path,
                    escaped_details
                );
            }
            state.export_message = Some(format!("Exported {} log entries to {:?}", logs.len(), path));
        }
    }
}

fn export_logs_txt(logs: &[LogEntry], state: &mut LogsViewState) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Text Files (*.txt)", &["txt"])
        .set_file_name("win_folder_cleaner_log.txt")
        .save_file()
    {
        if let Ok(mut file) = File::create(&path) {
            let _ = writeln!(file, "=== Windows Folder Analyzer & Cleaner Audit Log ===");
            let _ = writeln!(file, "Generated: {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
            for entry in logs {
                let status_str = match entry.status {
                    LogStatus::Success => "SUCCESS",
                    LogStatus::Warning => "WARNING",
                    LogStatus::Error => "ERROR",
                    LogStatus::DryRun => "DRY-RUN",
                    LogStatus::Skipped => "SKIPPED",
                };
                let _ = writeln!(
                    file,
                    "[{}] [{}] {}: {} | {}",
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    status_str,
                    entry.action,
                    entry.target_path,
                    entry.details
                );
            }
            state.export_message = Some(format!("Exported {} log entries to {:?}", logs.len(), path));
        }
    }
}
