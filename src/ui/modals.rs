use eframe::egui::{self, RichText, Window};
use std::path::PathBuf;
use crate::ui::theme::ThemeColors;
use crate::core::types::{FileInfo, format_bytes};

/// State for the deletion confirmation modal
#[derive(Debug, Clone)]
pub struct DeleteModalState {
    pub is_open: bool,
    pub target_name: String,
    pub target_paths: Vec<PathBuf>,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_bytes: u64,
    pub use_recycle_bin: bool,
    pub is_dry_run: bool,
    pub force_ownership_and_acl: bool,
    pub schedule_reboot_if_locked: bool,
}

impl Default for DeleteModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            target_name: String::new(),
            target_paths: Vec::new(),
            total_files: 0,
            total_dirs: 0,
            total_bytes: 0,
            use_recycle_bin: true,
            is_dry_run: false,
            force_ownership_and_acl: false,
            schedule_reboot_if_locked: false,
        }
    }
}

/// State for inspecting a locked file
#[derive(Debug, Clone, Default)]
pub struct LockedFileModalState {
    pub is_open: bool,
    pub file_info: Option<FileInfo>,
    pub locking_processes: Vec<crate::core::types::LockingProcessInfo>,
    pub reboot_scheduled: bool,
}

/// Renders the deletion confirmation modal window
pub fn show_delete_confirmation_modal(
    ctx: &egui::Context,
    state: &mut DeleteModalState,
    on_confirm: impl FnOnce(bool, bool, bool, bool),
) {
    if !state.is_open {
        return;
    }

    let mut open = state.is_open;
    let mut execute = false;
    let mut close_modal = false;
    let dry_run = state.is_dry_run;
    let force = state.force_ownership_and_acl;
    let reboot = state.schedule_reboot_if_locked;
    let use_recycle_bin = state.use_recycle_bin;

    Window::new("Confirm deletion")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(500.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

            ui.heading(RichText::new("Confirm deletion").color(ThemeColors::TEXT_PRIMARY));
            ui.label(
                RichText::new(format!(
                    "Target: {}",
                    state.target_name
                ))
                .color(ThemeColors::TEXT_SECONDARY),
            );

            ui.separator();

            egui::Grid::new("delete_modal_summary_grid")
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Total items:").color(ThemeColors::TEXT_MUTED));
                    ui.label(RichText::new(format!("{} files, {} folders", state.total_files, state.total_dirs)).color(ThemeColors::TEXT_PRIMARY));
                    ui.end_row();

                    ui.label(RichText::new("Total size:").color(ThemeColors::TEXT_MUTED));
                    ui.label(RichText::new(format_bytes(state.total_bytes)).color(ThemeColors::TEXT_PRIMARY).strong());
                    ui.end_row();
                });

            ui.separator();

            ui.label(RichText::new("Execution options").color(ThemeColors::TEXT_PRIMARY));

            // Recycle Bin Safe Delete
            let recycle_bin_color = if state.use_recycle_bin { ThemeColors::STATUS_SUCCESS } else { ThemeColors::TEXT_SECONDARY };
            ui.checkbox(
                &mut state.use_recycle_bin,
                RichText::new("Move to Recycle Bin (safe, reversible soft delete)")
                    .color(recycle_bin_color),
            );

            let dry_run_label = RichText::new("Simulate without deleting (dry run)")
                .color(if state.is_dry_run { ThemeColors::STATUS_INFO } else { ThemeColors::TEXT_SECONDARY });
            ui.checkbox(&mut state.is_dry_run, dry_run_label);

            if !state.use_recycle_bin {
                ui.checkbox(
                    &mut state.force_ownership_and_acl,
                    RichText::new("Force delete (strip read-only, take ownership, grant ACL)")
                        .color(ThemeColors::TEXT_SECONDARY),
                );

                ui.checkbox(
                    &mut state.schedule_reboot_if_locked,
                    RichText::new("Schedule locked files for deletion on reboot (MoveFileEx)")
                        .color(ThemeColors::TEXT_SECONDARY),
                );
            }

            ui.add_space(6.0);

            if !state.is_dry_run {
                if state.use_recycle_bin {
                    ui.label(
                        RichText::new("Files will be moved to the Windows Recycle Bin and can be restored if needed.")
                            .color(ThemeColors::STATUS_SUCCESS)
                            .size(11.0),
                    );
                } else {
                    ui.label(
                        RichText::new("Notice: Permanent deletion bypasses the Recycle Bin and cannot be easily undone.")
                            .color(ThemeColors::STATUS_WARNING)
                            .size(11.0),
                    );
                }
            }

            ui.add_space(8.0);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close_modal = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm_text = if state.is_dry_run {
                        "Run simulation"
                    } else if state.use_recycle_bin {
                        "Move to Recycle Bin"
                    } else {
                        "Permanently delete"
                    };

                    let btn_color = if state.is_dry_run {
                        ThemeColors::TEXT_PRIMARY
                    } else if state.use_recycle_bin {
                        ThemeColors::STATUS_SUCCESS
                    } else {
                        ThemeColors::STATUS_ERROR
                    };

                    if ui.button(RichText::new(confirm_text).color(btn_color)).clicked() {
                        execute = true;
                        close_modal = true;
                    }
                });
            });
        });

    if close_modal {
        state.is_open = false;
    } else {
        state.is_open = open;
    }

    if execute {
        on_confirm(dry_run, force, reboot, use_recycle_bin);
    }
}

/// Renders the locked file inspector modal
pub fn show_locked_file_modal(
    ctx: &egui::Context,
    state: &mut LockedFileModalState,
    on_schedule_reboot: impl FnOnce(&PathBuf),
) {
    if !state.is_open {
        return;
    }

    let mut open = state.is_open;
    let mut close_modal = false;
    let mut schedule_target: Option<PathBuf> = None;

    Window::new("Locked file details")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

            if let Some(file) = &state.file_info {
                ui.heading(RichText::new(&file.name).color(ThemeColors::TEXT_PRIMARY));
                ui.label(RichText::new(file.path.to_string_lossy()).color(ThemeColors::TEXT_MUTED));

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Size:").color(ThemeColors::TEXT_MUTED));
                    ui.label(RichText::new(file.formatted_size()).color(ThemeColors::TEXT_PRIMARY));

                    ui.add_space(16.0);
                    ui.label(RichText::new("Modified:").color(ThemeColors::TEXT_MUTED));
                    ui.label(RichText::new(file.formatted_modified()).color(ThemeColors::TEXT_PRIMARY));
                });

                ui.separator();
                ui.label(RichText::new("Active locking processes (Restart Manager):").color(ThemeColors::TEXT_PRIMARY));

                if state.locking_processes.is_empty() {
                    ui.label(RichText::new("No active process handles identified. The file may be locked by a driver or non-Restart-Manager service.").color(ThemeColors::TEXT_SECONDARY));
                } else {
                    for proc in &state.locking_processes {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&proc.app_name).color(ThemeColors::STATUS_WARNING));
                                ui.label(RichText::new(format!("(PID: {})", proc.pid)).color(ThemeColors::TEXT_MUTED));
                                if !proc.service_name.is_empty() {
                                    ui.label(RichText::new(format!("Service: {}", proc.service_name)).color(ThemeColors::TEXT_SECONDARY));
                                }
                            });
                        });
                    }
                }

                ui.add_space(8.0);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        close_modal = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !state.reboot_scheduled {
                            if ui.button(RichText::new("Schedule delete on reboot").color(ThemeColors::TEXT_PRIMARY)).clicked() {
                                schedule_target = Some(file.path.clone());
                                state.reboot_scheduled = true;
                            }
                        } else {
                            ui.label(RichText::new("Scheduled for deletion on next restart").color(ThemeColors::STATUS_SUCCESS));
                        }
                    });
                });
            }
        });

    if close_modal {
        state.is_open = false;
    } else {
        state.is_open = open;
    }

    if let Some(target) = schedule_target {
        on_schedule_reboot(&target);
    }
}
