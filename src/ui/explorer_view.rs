use std::path::PathBuf;
use eframe::egui::{
    self, Color32, Frame, Margin, ProgressBar, RichText, Rounding, ScrollArea, Stroke, Ui,
};

use crate::core::types::{
    DirScanSummary, FileInfo, SortColumn, SortDirection, format_bytes,
};
use crate::ui::theme::ThemeColors;

pub struct ExplorerViewState {
    pub selected_path: String,
    pub is_scanning: bool,
    pub scan_summary: Option<DirScanSummary>,
    pub search_query: String,
    pub active_tab: ExplorerSubTab,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerSubTab {
    LargestFiles,
    LargestSubdirs,
    Extensions,
}

impl Default for ExplorerViewState {
    fn default() -> Self {
        Self {
            selected_path: String::from("C:\\Windows\\Temp"),
            is_scanning: false,
            scan_summary: None,
            search_query: String::new(),
            active_tab: ExplorerSubTab::LargestFiles,
            sort_column: SortColumn::Size,
            sort_direction: SortDirection::Descending,
        }
    }
}

pub fn render_explorer_view(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    current_scan_path: &str,
    current_files_count: usize,
    current_bytes_count: u64,
    on_start_scan: &mut impl FnMut(PathBuf),
    on_cancel_scan: &mut impl FnMut(),
    on_inspect_file: &mut impl FnMut(&FileInfo),
    on_delete_single_file: &mut impl FnMut(&FileInfo),
) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

    // Folder selection & action bar
    render_path_selector_bar(ui, state, on_start_scan, on_cancel_scan);

    // Real-time progress bar if scanning is in progress
    if state.is_scanning {
        render_scanning_indicator(ui, current_scan_path, current_files_count, current_bytes_count, on_cancel_scan);
    }

    // Results summary header
    let summary_opt = state.scan_summary.clone();
    if let Some(summary) = &summary_opt {
        render_summary_banner(ui, summary);

        // Subtabs: Largest Files | Largest Subdirectories | File Types (Extensions)
        ui.horizontal(|ui| {
            if ui.selectable_label(state.active_tab == ExplorerSubTab::LargestFiles, "Largest Files (Top 100)").clicked() {
                state.active_tab = ExplorerSubTab::LargestFiles;
            }
            if ui.selectable_label(state.active_tab == ExplorerSubTab::LargestSubdirs, "Largest Subdirectories").clicked() {
                state.active_tab = ExplorerSubTab::LargestSubdirs;
            }
            if ui.selectable_label(state.active_tab == ExplorerSubTab::Extensions, "File Extensions Distribution").clicked() {
                state.active_tab = ExplorerSubTab::Extensions;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text("Filter by name or extension...")
                        .desired_width(220.0),
                );
            });
        });

        ui.separator();

        match state.active_tab {
            ExplorerSubTab::LargestFiles => {
                render_largest_files_table(ui, state, summary, on_inspect_file, on_delete_single_file);
            }
            ExplorerSubTab::LargestSubdirs => {
                render_largest_subdirs_table(ui, state, summary);
            }
            ExplorerSubTab::Extensions => {
                render_extensions_table(ui, summary);
            }
        }
    } else if !state.is_scanning {
        Frame::none()
            .fill(ThemeColors::BG_CARD)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
            .inner_margin(Margin::same(32.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No directory scanned yet").color(ThemeColors::TEXT_MUTED).size(16.0));
                    ui.label(RichText::new("Select a directory above and click 'Analyze Folder' to inspect file and folder sizes.").color(ThemeColors::TEXT_SECONDARY));
                });
            });
    }
}

fn render_path_selector_bar(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    on_start_scan: &mut impl FnMut(PathBuf),
    on_cancel_scan: &mut impl FnMut(),
) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Target Directory:").color(ThemeColors::TEXT_MUTED).strong());

                ui.add(
                    egui::TextEdit::singleline(&mut state.selected_path)
                        .desired_width(450.0),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        state.selected_path = folder.to_string_lossy().to_string();
                    }
                }

                if state.is_scanning {
                    if ui.button(RichText::new("Cancel Scan").color(ThemeColors::STATUS_ERROR)).clicked() {
                        on_cancel_scan();
                    }
                } else {
                    if ui.button(RichText::new("Analyze Folder").color(Color32::WHITE).strong()).clicked() {
                        let path = PathBuf::from(&state.selected_path);
                        if path.exists() {
                            on_start_scan(path);
                        }
                    }
                }
            });
        });
}

fn render_scanning_indicator(
    ui: &mut Ui,
    current_scan_path: &str,
    current_files_count: usize,
    current_bytes_count: u64,
    on_cancel_scan: &mut impl FnMut(),
) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Scanning:").color(ThemeColors::ACCENT).strong());
                    ui.label(
                        RichText::new(current_scan_path)
                            .color(ThemeColors::TEXT_SECONDARY)
                            .monospace(),
                    );
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Files scanned: {}", current_files_count)).color(ThemeColors::TEXT_PRIMARY));
                    ui.add_space(16.0);
                    ui.label(RichText::new(format!("Bytes processed: {}", format_bytes(current_bytes_count))).color(ThemeColors::STATUS_INFO).strong());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Stop").color(ThemeColors::STATUS_ERROR)).clicked() {
                            on_cancel_scan();
                        }
                    });
                });
            });
        });
}

fn render_summary_banner(ui: &mut Ui, summary: &DirScanSummary) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Total Size:").color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new(summary.formatted_total_size()).color(ThemeColors::ACCENT).strong());

                ui.add_space(16.0);
                ui.label(RichText::new("Accessible:").color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new(summary.formatted_accessible_size()).color(ThemeColors::STATUS_SUCCESS));

                if summary.inaccessible_files > 0 {
                    ui.add_space(16.0);
                    ui.label(RichText::new("Inaccessible Items:").color(ThemeColors::TEXT_MUTED));
                    ui.label(RichText::new(format!("{}", summary.inaccessible_files)).color(ThemeColors::STATUS_WARNING).strong());
                }

                ui.add_space(16.0);
                ui.label(RichText::new(format!("Files: {}", summary.total_files)).color(ThemeColors::TEXT_SECONDARY));

                ui.add_space(8.0);
                ui.label(RichText::new(format!("Folders: {}", summary.total_dirs)).color(ThemeColors::TEXT_SECONDARY));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Scan time: {} ms", summary.duration_ms)).color(ThemeColors::TEXT_MUTED));
                });
            });
        });
}

fn render_largest_files_table(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    summary: &DirScanSummary,
    on_inspect_file: &mut impl FnMut(&FileInfo),
    on_delete_single_file: &mut impl FnMut(&FileInfo),
) {
    let query = state.search_query.to_lowercase();
    let mut filtered_files: Vec<&FileInfo> = summary
        .largest_files
        .iter()
        .filter(|f| {
            if query.is_empty() {
                true
            } else {
                f.name.to_lowercase().contains(&query) || f.extension.to_lowercase().contains(&query)
            }
        })
        .collect();

    // Sort files according to selected column and direction
    let dir = state.sort_direction;
    match state.sort_column {
        SortColumn::Name => {
            filtered_files.sort_by(|a, b| {
                let res = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if dir == SortDirection::Descending { res.reverse() } else { res }
            });
        }
        SortColumn::Size => {
            filtered_files.sort_by(|a, b| {
                let res = a.size.cmp(&b.size);
                if dir == SortDirection::Descending { res.reverse() } else { res }
            });
        }
        SortColumn::DateModified => {
            filtered_files.sort_by(|a, b| {
                let res = a.modified.cmp(&b.modified);
                if dir == SortDirection::Descending { res.reverse() } else { res }
            });
        }
        SortColumn::Extension => {
            filtered_files.sort_by(|a, b| {
                let res = a.extension.cmp(&b.extension);
                if dir == SortDirection::Descending { res.reverse() } else { res }
            });
        }
    }

    let sort_col = state.sort_column;
    let sort_dir = state.sort_direction;

    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("largest_files_grid")
            .striped(true)
            .spacing([12.0, 6.0])
            .min_col_width(80.0)
            .show(ui, |ui| {
                // Clickable Header for Name
                let name_arrow = if sort_col == SortColumn::Name { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Name{}", name_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::Name);
                }

                // Clickable Header for Extension
                let ext_arrow = if sort_col == SortColumn::Extension { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Extension{}", ext_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::Extension);
                }

                // Clickable Header for Size
                let size_arrow = if sort_col == SortColumn::Size { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Size{}", size_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::Size);
                }

                // Clickable Header for Date Modified
                let date_arrow = if sort_col == SortColumn::DateModified { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Last Modified{}", date_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::DateModified);
                }

                ui.label(RichText::new("Attributes").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Actions").strong().color(ThemeColors::TEXT_MUTED));
                ui.end_row();

                for file in filtered_files {
                    ui.label(RichText::new(&file.name).color(ThemeColors::TEXT_PRIMARY));
                    ui.label(RichText::new(&file.extension).color(ThemeColors::TEXT_SECONDARY));
                    ui.label(RichText::new(file.formatted_size()).color(ThemeColors::ACCENT).strong());
                    ui.label(RichText::new(file.formatted_modified()).color(ThemeColors::TEXT_MUTED));

                    let attr = if file.is_readonly { "Read-Only" } else { "Normal" };
                    ui.label(RichText::new(attr).color(if file.is_readonly { ThemeColors::STATUS_WARNING } else { ThemeColors::TEXT_MUTED }));

                    ui.horizontal(|ui| {
                        if ui.small_button("Inspect").clicked() {
                            on_inspect_file(file);
                        }
                        if ui.small_button(RichText::new("Delete").color(ThemeColors::STATUS_ERROR)).clicked() {
                            on_delete_single_file(file);
                        }
                    });
                    ui.end_row();
                }
            });
    });
}

fn toggle_sort(state: &mut ExplorerViewState, col: SortColumn) {
    if state.sort_column == col {
        state.sort_direction = match state.sort_direction {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        };
    } else {
        state.sort_column = col;
        state.sort_direction = if col == SortColumn::Size || col == SortColumn::DateModified {
            SortDirection::Descending
        } else {
            SortDirection::Ascending
        };
    }
}

fn render_largest_subdirs_table(
    ui: &mut Ui,
    state: &ExplorerViewState,
    summary: &DirScanSummary,
) {
    let query = state.search_query.to_lowercase();
    let filtered_subdirs: Vec<&crate::core::types::DirEntrySummary> = summary
        .largest_subdirs
        .iter()
        .filter(|d| {
            if query.is_empty() {
                true
            } else {
                d.name.to_lowercase().contains(&query)
            }
        })
        .collect();

    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("largest_subdirs_grid")
            .striped(true)
            .spacing([12.0, 6.0])
            .min_col_width(100.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Folder Name").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Cumulative Size").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Files Count").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Subdirectories").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Path").strong().color(ThemeColors::TEXT_MUTED));
                ui.end_row();

                for dir in filtered_subdirs {
                    ui.label(RichText::new(&dir.name).color(ThemeColors::TEXT_PRIMARY).strong());
                    ui.label(RichText::new(dir.formatted_size()).color(ThemeColors::ACCENT).strong());
                    ui.label(RichText::new(format!("{}", dir.file_count)).color(ThemeColors::TEXT_SECONDARY));
                    ui.label(RichText::new(format!("{}", dir.dir_count)).color(ThemeColors::TEXT_MUTED));
                    ui.label(RichText::new(dir.path.to_string_lossy()).color(ThemeColors::TEXT_MUTED).size(11.0));
                    ui.end_row();
                }
            });
    });
}

fn render_extensions_table(ui: &mut Ui, summary: &DirScanSummary) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("extensions_grid")
            .striped(true)
            .spacing([12.0, 6.0])
            .min_col_width(120.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Extension").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Files Count").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Total Size").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Percentage of Folder").strong().color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new("Proportion").strong().color(ThemeColors::TEXT_MUTED));
                ui.end_row();

                for ext in &summary.extensions {
                    ui.label(RichText::new(&ext.ext).color(ThemeColors::TEXT_PRIMARY).strong());
                    ui.label(RichText::new(format!("{}", ext.count)).color(ThemeColors::TEXT_SECONDARY));
                    ui.label(RichText::new(ext.formatted_size()).color(ThemeColors::ACCENT));
                    ui.label(RichText::new(format!("{:.1}%", ext.percentage)).color(ThemeColors::TEXT_MUTED));

                    let progress_fraction = (ext.percentage / 100.0).clamp(0.0, 1.0);
                    ui.add(ProgressBar::new(progress_fraction).desired_width(150.0));

                    ui.end_row();
                }
            });
    });
}
