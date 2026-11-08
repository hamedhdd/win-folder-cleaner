use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use eframe::egui::{
    self, Color32, Frame, Margin, ProgressBar, RichText, Rounding, ScrollArea, Stroke, Ui,
};

use crate::core::security::get_available_drives;
use crate::core::types::{
    DirScanSummary, FileInfo, FolderTreeNode, SortColumn, SortDirection, format_bytes,
};
use crate::ui::theme::ThemeColors;

pub struct ExplorerViewState {
    pub selected_path: String,
    pub available_drives: Vec<String>,
    pub is_scanning: bool,
    pub scan_summary: Option<DirScanSummary>,
    pub search_query: String,
    pub tree_search_query: String,
    pub active_tab: ExplorerSubTab,
    pub expanded_paths: HashSet<PathBuf>,
    pub selected_node: Option<FolderTreeNode>,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerSubTab {
    FolderTree,
    LargestFiles,
    LargestSubdirs,
    Extensions,
}

impl Default for ExplorerViewState {
    fn default() -> Self {
        let drives = get_available_drives();
        let default_path = drives.first().cloned().unwrap_or_else(|| "C:\\".to_string());

        Self {
            selected_path: default_path,
            available_drives: drives,
            is_scanning: false,
            scan_summary: None,
            search_query: String::new(),
            tree_search_query: String::new(),
            active_tab: ExplorerSubTab::FolderTree,
            expanded_paths: HashSet::new(),
            selected_node: None,
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
    on_clean_folder_contents: &mut impl FnMut(&PathBuf),
) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

    // Folder selection & action bar with drive quick-select
    render_path_selector_bar(ui, state, on_start_scan, on_cancel_scan);

    // Real-time progress bar if scanning is in progress
    if state.is_scanning {
        render_scanning_indicator(ui, current_scan_path, current_files_count, current_bytes_count, on_cancel_scan);
    }

    // Results summary header
    let summary_opt = state.scan_summary.clone();
    if let Some(summary) = &summary_opt {
        render_summary_banner(ui, summary);

        // Subtabs: Folder Tree View | Largest Files | Largest Subdirectories | File Extensions
        ui.horizontal(|ui| {
            if ui.selectable_label(state.active_tab == ExplorerSubTab::FolderTree, "Folder Tree View").clicked() {
                state.active_tab = ExplorerSubTab::FolderTree;
            }
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
                if state.active_tab == ExplorerSubTab::FolderTree {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.tree_search_query)
                            .hint_text("Filter folders in tree...")
                            .desired_width(200.0),
                    );
                } else {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.search_query)
                            .hint_text("Filter by name or extension...")
                            .desired_width(200.0),
                    );
                }
            });
        });

        ui.separator();

        match state.active_tab {
            ExplorerSubTab::FolderTree => {
                render_folder_tree_subtab(
                    ui,
                    state,
                    summary,
                    on_start_scan,
                    on_inspect_file,
                    on_delete_single_file,
                    on_clean_folder_contents,
                );
            }
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
                    ui.add_space(4.0);
                    ui.label(RichText::new("Select a drive or folder above and click 'Analyze Folder' to view the full folder tree and calculate sizes.").color(ThemeColors::TEXT_SECONDARY));
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
                ui.label(RichText::new("Drives:").color(ThemeColors::TEXT_MUTED).strong());

                for drive in &state.available_drives {
                    let is_active = state.selected_path.starts_with(drive);
                    let btn = egui::Button::new(
                        RichText::new(drive).color(if is_active { ThemeColors::ACCENT } else { ThemeColors::TEXT_PRIMARY }),
                    );
                    if ui.add(btn).clicked() {
                        state.selected_path = drive.clone();
                    }
                }

                ui.separator();
                ui.label(RichText::new("Target:").color(ThemeColors::TEXT_MUTED).strong());

                ui.add(
                    egui::TextEdit::singleline(&mut state.selected_path)
                        .desired_width(360.0),
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

// ------------------------------------------------------------------------------------------------
// Folder Tree View Implementation
// ------------------------------------------------------------------------------------------------

fn render_folder_tree_subtab(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    summary: &DirScanSummary,
    on_start_scan: &mut impl FnMut(PathBuf),
    on_inspect_file: &mut impl FnMut(&FileInfo),
    on_delete_single_file: &mut impl FnMut(&FileInfo),
    on_clean_folder_contents: &mut impl FnMut(&PathBuf),
) {
    if let Some(root_node) = &summary.root_tree {
        // Expand root by default if empty
        if state.expanded_paths.is_empty() {
            state.expanded_paths.insert(root_node.path.clone());
            state.selected_node = Some(root_node.clone());
        }

        // Toolbar for tree controls
        ui.horizontal(|ui| {
            if ui.small_button("Expand All Top Levels").clicked() {
                expand_top_levels(root_node, &mut state.expanded_paths, 0, 2);
            }
            if ui.small_button("Collapse All").clicked() {
                state.expanded_paths.clear();
                state.expanded_paths.insert(root_node.path.clone());
            }

            ui.label(RichText::new(format!("Root: {}", root_node.path.display())).color(ThemeColors::TEXT_MUTED));
        });

        ui.add_space(4.0);

        // Split view: Left = Scrollable Tree, Right = Selected Folder Inspector
        ui.columns(2, |cols| {
            // LEFT COLUMN: Hierarchical Folder Tree
            cols[0].group(|ui| {
                ui.label(RichText::new("Folder Hierarchy (Sorted by Size)").strong().color(ThemeColors::TEXT_PRIMARY));
                ui.separator();

                let mut node_to_select: Option<FolderTreeNode> = None;
                let root_total_size = root_node.total_size.max(1);
                let query = state.tree_search_query.to_lowercase();

                ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    render_tree_node_row(
                        ui,
                        root_node,
                        root_total_size,
                        0,
                        &mut state.expanded_paths,
                        &mut node_to_select,
                        &query,
                    );
                });

                if let Some(node) = node_to_select {
                    state.selected_node = Some(node);
                }
            });

            // RIGHT COLUMN: Selected Folder Inspector
            cols[1].group(|ui| {
                render_selected_node_inspector(
                    ui,
                    state,
                    on_start_scan,
                    on_inspect_file,
                    on_delete_single_file,
                    on_clean_folder_contents,
                );
            });
        });
    } else {
        ui.label(RichText::new("Folder tree information is not available.").color(ThemeColors::TEXT_MUTED));
    }
}

fn expand_top_levels(node: &FolderTreeNode, expanded: &mut HashSet<PathBuf>, current_depth: usize, max_depth: usize) {
    if current_depth <= max_depth {
        expanded.insert(node.path.clone());
        for child in &node.children {
            expand_top_levels(child, expanded, current_depth + 1, max_depth);
        }
    }
}

fn render_tree_node_row(
    ui: &mut Ui,
    node: &FolderTreeNode,
    root_total_size: u64,
    depth: usize,
    expanded_paths: &mut HashSet<PathBuf>,
    node_to_select: &mut Option<FolderTreeNode>,
    search_query: &str,
) {
    let is_expanded = expanded_paths.contains(&node.path);
    let has_children = !node.children.is_empty();
    let matches_search = search_query.is_empty() || node.name.to_lowercase().contains(search_query);

    if matches_search || is_expanded {
        ui.horizontal(|ui| {
            // Indentation
            if depth > 0 {
                ui.add_space((depth as f32) * 14.0);
            }

            // Expander button
            if has_children {
                let icon = if is_expanded { "▼" } else { "▶" };
                if ui.small_button(icon).clicked() {
                    if is_expanded {
                        expanded_paths.remove(&node.path);
                    } else {
                        expanded_paths.insert(node.path.clone());
                    }
                }
            } else {
                ui.add_space(18.0);
            }

            // Folder label (selectable)
            let name_color = if matches_search && !search_query.is_empty() {
                ThemeColors::STATUS_INFO
            } else if node.is_inaccessible {
                ThemeColors::STATUS_WARNING
            } else {
                ThemeColors::TEXT_PRIMARY
            };

            let label_btn = ui.selectable_label(
                false,
                RichText::new(&node.name).color(name_color).strong(),
            );

            if label_btn.clicked() {
                *node_to_select = Some(node.clone());
            }

            // Proportion bar
            let fraction = (node.total_size as f32 / root_total_size as f32).clamp(0.0, 1.0);
            ui.add(ProgressBar::new(fraction).desired_width(60.0));

            // Formatted size
            ui.label(RichText::new(node.formatted_size()).color(ThemeColors::ACCENT).strong());

            // Child count summary
            ui.label(
                RichText::new(format!("({} files, {} subfolders)", node.file_count, node.dir_count))
                    .color(ThemeColors::TEXT_MUTED)
                    .size(11.0),
            );
        });
    }

    // Render children recursively if expanded
    if is_expanded && has_children {
        for child in &node.children {
            render_tree_node_row(
                ui,
                child,
                root_total_size,
                depth + 1,
                expanded_paths,
                node_to_select,
                search_query,
            );
        }
    }
}

fn render_selected_node_inspector(
    ui: &mut Ui,
    state: &ExplorerViewState,
    on_start_scan: &mut impl FnMut(PathBuf),
    on_inspect_file: &mut impl FnMut(&FileInfo),
    on_delete_single_file: &mut impl FnMut(&FileInfo),
    on_clean_folder_contents: &mut impl FnMut(&PathBuf),
) {
    if let Some(node) = &state.selected_node {
        ui.heading(RichText::new(&node.name).color(ThemeColors::TEXT_PRIMARY).strong());
        ui.label(RichText::new(node.path.to_string_lossy()).color(ThemeColors::TEXT_MUTED).size(11.0));

        ui.separator();

        egui::Grid::new("selected_node_stats_grid")
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Total Tree Size:").color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new(node.formatted_size()).color(ThemeColors::ACCENT).strong());
                ui.end_row();

                ui.label(RichText::new("Direct Files Size:").color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new(node.formatted_immediate_size()).color(ThemeColors::TEXT_PRIMARY));
                ui.end_row();

                ui.label(RichText::new("Total Subtree Files:").color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new(format!("{}", node.file_count)).color(ThemeColors::TEXT_SECONDARY));
                ui.end_row();

                ui.label(RichText::new("Total Subdirectories:").color(ThemeColors::TEXT_MUTED));
                ui.label(RichText::new(format!("{}", node.dir_count)).color(ThemeColors::TEXT_SECONDARY));
                ui.end_row();
            });

        ui.add_space(6.0);

        // Action buttons
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Analyze from here").color(ThemeColors::ACCENT)).clicked() {
                on_start_scan(node.path.clone());
            }

            if ui.button("Open in Explorer").clicked() {
                open_in_file_explorer(&node.path);
            }

            if ui.button(RichText::new("Clean contents").color(ThemeColors::STATUS_ERROR)).clicked() {
                on_clean_folder_contents(&node.path);
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new(format!("Direct Files in this Folder ({})", node.direct_files.len())).color(ThemeColors::TEXT_PRIMARY).strong());

        if node.direct_files.is_empty() {
            ui.label(RichText::new("No direct files in this folder (only subdirectories).").color(ThemeColors::TEXT_MUTED));
        } else {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("selected_node_direct_files_grid")
                    .striped(true)
                    .spacing([8.0, 4.0])
                    .min_col_width(70.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("File Name").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Size").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Modified").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Actions").strong().color(ThemeColors::TEXT_MUTED));
                        ui.end_row();

                        for file in &node.direct_files {
                            ui.label(RichText::new(&file.name).color(ThemeColors::TEXT_PRIMARY));
                            ui.label(RichText::new(file.formatted_size()).color(ThemeColors::ACCENT));
                            ui.label(RichText::new(file.formatted_modified()).color(ThemeColors::TEXT_MUTED).size(11.0));

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
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(RichText::new("Select a folder in the tree to inspect details.").color(ThemeColors::TEXT_MUTED));
        });
    }
}

fn open_in_file_explorer(path: &PathBuf) {
    let _ = Command::new("explorer.exe")
        .arg(path.to_string_lossy().to_string())
        .spawn();
}

// ------------------------------------------------------------------------------------------------
// Other Subtabs (Largest Files, Largest Subdirectories, Extensions)
// ------------------------------------------------------------------------------------------------

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
                let name_arrow = if sort_col == SortColumn::Name { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Name{}", name_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::Name);
                }

                let ext_arrow = if sort_col == SortColumn::Extension { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Extension{}", ext_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::Extension);
                }

                let size_arrow = if sort_col == SortColumn::Size { if sort_dir == SortDirection::Ascending { " ▲" } else { " ▼" } } else { "" };
                if ui.button(RichText::new(format!("Size{}", size_arrow)).strong().color(ThemeColors::TEXT_MUTED)).clicked() {
                    toggle_sort(state, SortColumn::Size);
                }

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
