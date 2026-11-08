use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{
    self, Color32, ComboBox, Frame, Margin, ProgressBar, RichText, Rounding, ScrollArea, Stroke, Ui, Vec2,
};

use crate::core::security::get_all_drives_space_info;
use crate::core::types::{
    DirScanSummary, DriveSpaceInfo, FileInfo, FolderTreeNode, NodeScanResult, SizeUnit, SortColumn,
    SortDirection,
};
use crate::ui::chart_view::{render_pro_chart, ChartMode};
use crate::ui::theme::ThemeColors;

pub struct ExplorerViewState {
    pub selected_path: String,
    pub available_drives: Vec<DriveSpaceInfo>,
    pub selected_unit: SizeUnit,
    pub chart_mode: ChartMode,
    pub is_scanning: bool,
    pub scan_summary: Option<DirScanSummary>,
    pub expanded_paths: HashSet<PathBuf>,
    pub selected_node: Option<FolderTreeNode>,
    pub search_query: String,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub pending_expansions: HashSet<PathBuf>,
    pub node_expand_tx: Sender<NodeScanResult>,
    pub node_expand_rx: Receiver<NodeScanResult>,
}

impl Default for ExplorerViewState {
    fn default() -> Self {
        let drives = get_all_drives_space_info();
        let default_path = drives
            .first()
            .map(|d| d.letter.clone())
            .unwrap_or_else(|| "C:\\".to_string());
        let (tx, rx) = crossbeam_channel::unbounded();

        Self {
            selected_path: default_path,
            available_drives: drives,
            selected_unit: SizeUnit::Auto,
            chart_mode: ChartMode::Pie,
            is_scanning: false,
            scan_summary: None,
            expanded_paths: HashSet::new(),
            selected_node: None,
            search_query: String::new(),
            sort_column: SortColumn::Size,
            sort_direction: SortDirection::Descending,
            pending_expansions: HashSet::new(),
            node_expand_tx: tx,
            node_expand_rx: rx,
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
    // 0. Process completed background node expansions without ever blocking the UI
    while let Ok(res) = state.node_expand_rx.try_recv() {
        state.pending_expansions.remove(&res.path);
        if let Some(summary) = &mut state.scan_summary {
            if let Some(root_node) = &mut summary.root_tree {
                update_tree_node_recursive(root_node, &res);
            }
        }
        if let Some(sel) = &mut state.selected_node {
            if sel.path == res.path {
                sel.children = res.children.clone();
                sel.direct_files = res.direct_files.clone();
                sel.total_size = res.total_size;
                sel.immediate_files_size = res.immediate_files_size;
                sel.file_count = res.file_count;
                sel.dir_count = res.dir_count;
                sel.inaccessible_count = res.inaccessible_count;
                sel.is_inaccessible = res.inaccessible_count > 0 && res.total_size == 0;
                sel.is_deeper_scanned = true;
                sel.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
            }
        }
    }

    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);

    // 1. Pro Toolbar with Drive Dropdown, Path textfield, Scan/Stop, and Unit Selectors
    render_pro_toolbar(ui, state, on_start_scan, on_cancel_scan);

    // 2. Real-time scanning indicator banner if active
    if state.is_scanning {
        render_scanning_indicator(ui, current_scan_path, current_files_count, current_bytes_count, on_cancel_scan);
    }

    // 3. Four-Quadrant Main Workspace matching the Folder Size Professional layout
    let total_avail_height = (ui.available_height() - 24.0).max(350.0);
    let top_quadrant_height = total_avail_height * 0.54;
    let bottom_quadrant_height = total_avail_height * 0.46;

    // TOP HALF: [Top-Left: Directory Tree] | [Top-Right: Contents Detailed List]
    ui.allocate_ui(Vec2::new(ui.available_width(), top_quadrant_height), |ui| {
        ui.columns(2, |cols| {
            cols[0].set_height(top_quadrant_height);
            cols[1].set_height(top_quadrant_height);

            // Top-Left: Directory Tree View (Folder | Size)
            render_top_left_folder_tree(&mut cols[0], state);

            // Top-Right: Detailed Contents Table (Name | Size | % | Files | Folders | Date | Actions)
            render_top_right_contents_table(
                &mut cols[1],
                state,
                on_inspect_file,
                on_delete_single_file,
                on_clean_folder_contents,
            );
        });
    });

    ui.separator();

    // BOTTOM HALF: [Bottom-Left: Drives Overview] | [Bottom-Right: Interactive Pie Chart]
    ui.allocate_ui(Vec2::new(ui.available_width(), bottom_quadrant_height), |ui| {
        ui.columns(2, |cols| {
            cols[0].set_height(bottom_quadrant_height);
            cols[1].set_height(bottom_quadrant_height);

            // Bottom-Left: Drives Overview (Drive | Size | Used | Free)
            render_bottom_left_drives_table(&mut cols[0], state, on_start_scan);

            // Bottom-Right: Interactive Graphical Pie Chart with Hover Callout
            render_bottom_right_chart(&mut cols[1], state);
        });
    });
}

// ------------------------------------------------------------------------------------------------
// 1. Pro Toolbar
// ------------------------------------------------------------------------------------------------

fn render_pro_toolbar(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    on_start_scan: &mut impl FnMut(PathBuf),
    on_cancel_scan: &mut impl FnMut(),
) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Scan / Stop button
                if state.is_scanning {
                    if ui.button(RichText::new("Stop").color(ThemeColors::STATUS_ERROR).strong()).clicked() {
                        on_cancel_scan();
                    }
                } else {
                    if ui.button(RichText::new("Scan").color(Color32::WHITE).strong()).clicked() {
                        let path = PathBuf::from(&state.selected_path);
                        if path.exists() {
                            on_start_scan(path);
                        }
                    }
                }

                if ui.button("Refresh").clicked() {
                    state.available_drives = get_all_drives_space_info();
                    let path = PathBuf::from(&state.selected_path);
                    if path.exists() && !state.is_scanning {
                        on_start_scan(path);
                    }
                }

                ui.separator();

                // Drive Dropdown
                let current_drive_label = state
                    .available_drives
                    .iter()
                    .find(|d| state.selected_path.starts_with(&d.letter))
                    .map(|d| format!("Drive ({})", d.letter.trim_end_matches('\\')))
                    .unwrap_or_else(|| "Drives".to_string());

                ComboBox::from_id_salt("toolbar_drive_combo")
                    .selected_text(current_drive_label)
                    .show_ui(ui, |ui| {
                        for d in &state.available_drives {
                            let label = format!("Local Disk ({}) - {}", d.letter.trim_end_matches('\\'), state.selected_unit.format(d.total_bytes));
                            if ui.selectable_label(state.selected_path.starts_with(&d.letter), label).clicked() {
                                state.selected_path = d.letter.clone();
                            }
                        }
                    });

                // Target Path textfield
                ui.add(
                    egui::TextEdit::singleline(&mut state.selected_path)
                        .desired_width(280.0),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        state.selected_path = folder.to_string_lossy().to_string();
                    }
                }

                ui.separator();

                // Unit selection buttons (B, KB, MB, GB, AUTO)
                ui.label(RichText::new("Units:").color(ThemeColors::TEXT_MUTED));

                if ui.selectable_label(state.selected_unit == SizeUnit::Bytes, "B").clicked() {
                    state.selected_unit = SizeUnit::Bytes;
                }
                if ui.selectable_label(state.selected_unit == SizeUnit::Kilobytes, "KB").clicked() {
                    state.selected_unit = SizeUnit::Kilobytes;
                }
                if ui.selectable_label(state.selected_unit == SizeUnit::Megabytes, "MB").clicked() {
                    state.selected_unit = SizeUnit::Megabytes;
                }
                if ui.selectable_label(state.selected_unit == SizeUnit::Gigabytes, "GB").clicked() {
                    state.selected_unit = SizeUnit::Gigabytes;
                }
                if ui.selectable_label(state.selected_unit == SizeUnit::Auto, "AUTO").clicked() {
                    state.selected_unit = SizeUnit::Auto;
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
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0, ThemeColors::ACCENT))
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Scanning:").color(ThemeColors::ACCENT).strong());
                ui.label(RichText::new(current_scan_path).color(ThemeColors::TEXT_SECONDARY).monospace().size(11.0));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(RichText::new("Stop").color(ThemeColors::STATUS_ERROR)).clicked() {
                        on_cancel_scan();
                    }
                    ui.label(RichText::new(format!("{} bytes ({} files)", current_bytes_count, current_files_count)).color(ThemeColors::STATUS_INFO).strong());
                });
            });
        });
}

fn update_tree_node_recursive(node: &mut FolderTreeNode, res: &NodeScanResult) -> bool {
    if node.path == res.path {
        node.children = res.children.clone();
        node.direct_files = res.direct_files.clone();
        node.total_size = res.total_size;
        node.immediate_files_size = res.immediate_files_size;
        node.file_count = res.file_count;
        node.dir_count = res.dir_count;
        node.inaccessible_count = res.inaccessible_count;
        node.is_inaccessible = res.inaccessible_count > 0 && res.total_size == 0;
        node.is_deeper_scanned = true;
        node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
        return true;
    }
    for child in &mut node.children {
        if update_tree_node_recursive(child, res) {
            node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
            return true;
        }
    }
    false
}

// ------------------------------------------------------------------------------------------------
// 2. Top-Left: Directory Tree View (Folder | Size)
// ------------------------------------------------------------------------------------------------

fn render_top_left_folder_tree(ui: &mut Ui, state: &mut ExplorerViewState) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            // Header Bar
            ui.horizontal(|ui| {
                ui.label(RichText::new("Folder").strong().color(ThemeColors::TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Size").strong().color(ThemeColors::TEXT_PRIMARY));
                });
            });
            ui.separator();

            if let Some(summary) = &mut state.scan_summary {
                if let Some(root_node) = &mut summary.root_tree {
                    if state.expanded_paths.is_empty() {
                        state.expanded_paths.insert(root_node.path.clone());
                        if state.selected_node.is_none() {
                            state.selected_node = Some(root_node.clone());
                        }
                    }

                    let unit = state.selected_unit;
                    let mut node_to_select = None;

                    let scroll_max_h = (ui.available_height() - 4.0).max(60.0);
                    ScrollArea::vertical()
                        .id_salt("folder_tree_scroll_panel")
                        .auto_shrink([false, false])
                        .max_height(scroll_max_h)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            render_tree_recursive_row(
                                ui,
                                root_node,
                                unit,
                                0,
                                &mut state.expanded_paths,
                                &mut state.pending_expansions,
                                &state.node_expand_tx,
                                &mut node_to_select,
                            );
                        });

                    if let Some(mut node) = node_to_select {
                        if node.direct_files.is_empty() && node.immediate_files_size > 0 {
                            node.direct_files = crate::core::analyzer::load_directory_files(&node.path);
                        }
                        state.selected_node = Some(node);
                    }
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(RichText::new("Tree empty. Click 'Scan' above to explore.").color(ThemeColors::TEXT_MUTED));
                });
            }
        });
}

fn render_tree_recursive_row(
    ui: &mut Ui,
    node: &mut FolderTreeNode,
    unit: SizeUnit,
    depth: usize,
    expanded_paths: &mut HashSet<PathBuf>,
    pending_expansions: &mut HashSet<PathBuf>,
    node_expand_tx: &Sender<NodeScanResult>,
    node_to_select: &mut Option<FolderTreeNode>,
) {
    let is_expanded = expanded_paths.contains(&node.path);
    let is_pending = pending_expansions.contains(&node.path);
    let can_expand = node.dir_count > 0 || !node.children.is_empty();

    ui.horizontal(|ui| {
        if depth > 0 {
            ui.add_space((depth as f32) * 12.0);
        }

        // Expander
        if can_expand {
            let icon = if is_expanded { "▼" } else { "▶" };
            if ui.small_button(icon).clicked() {
                if is_expanded {
                    expanded_paths.remove(&node.path);
                } else {
                    expanded_paths.insert(node.path.clone());
                    // Asynchronously trigger deeper scan on background worker so GUI never freezes!
                    if !node.is_deeper_scanned && !is_pending {
                        pending_expansions.insert(node.path.clone());
                        let tx = node_expand_tx.clone();
                        let path = node.path.clone();
                        std::thread::spawn(move || {
                            let cancel = Arc::new(AtomicBool::new(false));
                            let res = crate::core::analyzer::scan_node_deep_headless(&path, cancel);
                            let _ = tx.send(res);
                        });
                    }
                }
            }
        } else {
            ui.add_space(16.0);
        }

        // Folder name
        let label = ui.selectable_label(
            false,
            RichText::new(&node.name).color(ThemeColors::TEXT_PRIMARY),
        );
        if label.clicked() {
            *node_to_select = Some(node.clone());
            if !node.is_deeper_scanned && !is_pending {
                pending_expansions.insert(node.path.clone());
                let tx = node_expand_tx.clone();
                let path = node.path.clone();
                std::thread::spawn(move || {
                    let cancel = Arc::new(AtomicBool::new(false));
                    let res = crate::core::analyzer::scan_node_deep_headless(&path, cancel);
                    let _ = tx.send(res);
                });
            }
        }

        // Real-time asynchronous scanning indicator badge
        if is_pending {
            ui.label(RichText::new("⟳ scanning...").color(ThemeColors::ACCENT).italics());
        }

        // Inaccessible warning indicator if errors were encountered in subtree
        if node.inaccessible_count > 0 || node.is_inaccessible {
            ui.label(RichText::new("[skipped]").color(ThemeColors::STATUS_WARNING).size(10.0))
                .on_hover_text(format!("{} skipped or inaccessible items in this subtree", node.inaccessible_count));
        }

        // Right-aligned Size
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(node.formatted_size_with_unit(unit)).color(ThemeColors::TEXT_PRIMARY));
        });
    });

    if is_expanded {
        if is_pending {
            ui.horizontal(|ui| {
                ui.add_space(((depth + 1) as f32) * 12.0 + 16.0);
                ui.label(RichText::new("Analyzing subfolders in background...").color(ThemeColors::TEXT_MUTED).italics());
            });
        } else {
            // STRICT REQUIREMENT: on left tree view folders should be sorted by size!
            node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));

            for child in &mut node.children {
                render_tree_recursive_row(
                    ui,
                    child,
                    unit,
                    depth + 1,
                    expanded_paths,
                    pending_expansions,
                    node_expand_tx,
                    node_to_select,
                );
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// 3. Top-Right: Contents Detailed List Table
// ------------------------------------------------------------------------------------------------

fn render_top_right_contents_table(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    on_inspect_file: &mut impl FnMut(&FileInfo),
    on_delete_single_file: &mut impl FnMut(&FileInfo),
    on_clean_folder_contents: &mut impl FnMut(&PathBuf),
) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            let unit = state.selected_unit;

            if let Some(node) = &mut state.selected_node {
                if node.direct_files.is_empty() && node.immediate_files_size > 0 {
                    node.direct_files = crate::core::analyzer::load_directory_files(&node.path);
                }

                // Header with breadcrumb and actions
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&node.name).strong().color(ThemeColors::TEXT_PRIMARY));
                    ui.label(RichText::new(format!("({})", unit.format(node.total_size))).color(ThemeColors::ACCENT));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(RichText::new("Clean contents").color(ThemeColors::STATUS_ERROR)).clicked() {
                            on_clean_folder_contents(&node.path);
                        }
                        if ui.small_button("Open in Explorer").clicked() {
                            let _ = Command::new("explorer.exe").arg(node.path.to_string_lossy().to_string()).spawn();
                        }
                    });
                });

                ui.separator();

                if state.pending_expansions.contains(&node.path) {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label(RichText::new("Analyzing directory contents in background...").color(ThemeColors::TEXT_MUTED));
                    });
                    return;
                }

                // Detailed Table
                let total_parent_size = node.total_size.max(1);
                let contents_max_h = (ui.available_height() - 4.0).max(60.0);

                ScrollArea::vertical()
                    .id_salt("pro_contents_scroll_panel")
                    .auto_shrink([false, false])
                    .max_height(contents_max_h)
                    .show(ui, |ui| {
                    egui::Grid::new("pro_contents_grid")
                        .striped(true)
                        .spacing([10.0, 4.0])
                        .min_col_width(60.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Name").color(ThemeColors::TEXT_MUTED));
                            ui.label(RichText::new("Size").color(ThemeColors::TEXT_MUTED));
                            ui.label(RichText::new("Share").color(ThemeColors::TEXT_MUTED));
                            ui.label(RichText::new("Files").color(ThemeColors::TEXT_MUTED));
                            ui.label(RichText::new("Folders").color(ThemeColors::TEXT_MUTED));
                            ui.label(RichText::new("Attrs").color(ThemeColors::TEXT_MUTED));
                            ui.label(RichText::new("Actions").color(ThemeColors::TEXT_MUTED));
                            ui.end_row();

                            // Subdirectories rows
                            for child in &node.children {
                                let pct = (child.total_size as f32 / total_parent_size as f32) * 100.0;

                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("dir").color(ThemeColors::STATUS_INFO).size(10.0));
                                    ui.label(RichText::new(&child.name).color(ThemeColors::TEXT_PRIMARY));
                                });

                                ui.label(RichText::new(unit.format(child.total_size)).color(ThemeColors::TEXT_PRIMARY));

                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{:.1}%", pct)).color(ThemeColors::TEXT_SECONDARY));
                                    ui.add(ProgressBar::new(pct / 100.0).desired_width(45.0));
                                });

                                ui.label(RichText::new(format!("{}", child.file_count)).color(ThemeColors::TEXT_MUTED));
                                ui.label(RichText::new(format!("{}", child.dir_count)).color(ThemeColors::TEXT_MUTED));
                                ui.label(RichText::new("D").color(ThemeColors::TEXT_MUTED));

                                ui.horizontal(|ui| {
                                    if ui.small_button("Clean").clicked() {
                                        on_clean_folder_contents(&child.path);
                                    }
                                });
                                ui.end_row();
                            }

                            // Direct files rows
                            for file in &node.direct_files {
                                let pct = (file.size as f32 / total_parent_size as f32) * 100.0;

                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("file").color(ThemeColors::TEXT_MUTED).size(10.0));
                                    ui.label(RichText::new(&file.name).color(ThemeColors::TEXT_PRIMARY));
                                });

                                ui.label(RichText::new(unit.format(file.size)).color(ThemeColors::TEXT_SECONDARY));

                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{:.1}%", pct)).color(ThemeColors::TEXT_MUTED));
                                    ui.add(ProgressBar::new(pct / 100.0).desired_width(45.0));
                                });

                                ui.label(RichText::new("-").color(ThemeColors::TEXT_MUTED));
                                ui.label(RichText::new("-").color(ThemeColors::TEXT_MUTED));
                                ui.label(RichText::new(if file.is_readonly { "RA" } else { "A" }).color(ThemeColors::TEXT_MUTED));

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
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(RichText::new("Select a folder in the tree to view its contents.").color(ThemeColors::TEXT_MUTED));
                });
            }
        });
}

// ------------------------------------------------------------------------------------------------
// 4. Bottom-Left: Drives Overview Table (Drive | Size | Used | Free)
// ------------------------------------------------------------------------------------------------

fn render_bottom_left_drives_table(
    ui: &mut Ui,
    state: &mut ExplorerViewState,
    on_start_scan: &mut impl FnMut(PathBuf),
) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Logical Drives Overview").strong().color(ThemeColors::TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Refresh Drives").clicked() {
                        state.available_drives = get_all_drives_space_info();
                    }
                });
            });
            ui.separator();

            let unit = state.selected_unit;
            let mut drive_to_scan = None;

            let drives_max_h = (ui.available_height() - 4.0).max(60.0);
            ScrollArea::vertical()
                .id_salt("bottom_drives_scroll_panel")
                .auto_shrink([false, false])
                .max_height(drives_max_h)
                .show(ui, |ui| {
                egui::Grid::new("bottom_drives_grid")
                    .striped(true)
                    .spacing([10.0, 6.0])
                    .min_col_width(65.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Drive").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Total Size").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Used Space").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Free Space").strong().color(ThemeColors::TEXT_MUTED));
                        ui.label(RichText::new("Usage %").strong().color(ThemeColors::TEXT_MUTED));
                        ui.end_row();

                        for drive in &state.available_drives {
                            let is_active = state.selected_path.starts_with(&drive.letter);

                            let label = ui.selectable_label(
                                is_active,
                                RichText::new(&drive.letter).strong().color(if is_active { ThemeColors::ACCENT } else { ThemeColors::TEXT_PRIMARY }),
                            );
                            if label.clicked() {
                                state.selected_path = drive.letter.clone();
                                drive_to_scan = Some(PathBuf::from(&drive.letter));
                            }

                            ui.label(RichText::new(drive.formatted_size(unit)).color(ThemeColors::TEXT_PRIMARY));
                            ui.label(RichText::new(drive.formatted_used(unit)).color(ThemeColors::STATUS_WARNING));
                            ui.label(RichText::new(drive.formatted_free(unit)).color(ThemeColors::STATUS_SUCCESS));

                            let pct = drive.used_percent();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{:.1}%", pct)).color(ThemeColors::TEXT_SECONDARY));
                                ui.add(ProgressBar::new(pct / 100.0).desired_width(60.0));
                            });

                            ui.end_row();
                        }
                    });
            });

            if let Some(target) = drive_to_scan {
                if !state.is_scanning {
                    on_start_scan(target);
                }
            }
        });
}

// ------------------------------------------------------------------------------------------------
// 5. Bottom-Right: Interactive Graphical Pie Chart
// ------------------------------------------------------------------------------------------------

fn render_bottom_right_chart(ui: &mut Ui, state: &mut ExplorerViewState) {
    Frame::none()
        .fill(ThemeColors::BG_CARD)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
        .inner_margin(Margin::same(8.0))
        .show(ui, |ui| {
            if let Some(node) = &state.selected_node {
                if state.pending_expansions.contains(&node.path) {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label(RichText::new("Calculating storage breakdown in background...").color(ThemeColors::TEXT_MUTED));
                    });
                } else {
                    render_pro_chart(ui, node, state.selected_unit, &mut state.chart_mode);
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(RichText::new("Select a folder above to view interactive storage visualization.").color(ThemeColors::TEXT_MUTED));
                });
            }
        });
}
