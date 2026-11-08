use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Color32, Frame, Margin, RichText, Stroke, TopBottomPanel};

use crate::core::analyzer::scan_directory_tree;
use crate::core::cleaner::{clean_directory_contents, CleanExecutionConfig, DeletionReport};
use crate::core::reboot_del::schedule_deletion_on_reboot;
use crate::core::restart_mgr::get_locking_processes;
use crate::core::security::{grant_administrators_acl, is_elevated, relaunch_as_admin, take_ownership};
use crate::core::temp_dirs::discover_temp_locations;
use crate::core::types::{
    AgeFilter, DirScanSummary, LogEntry, LogStatus, ScanProgress, TempLocation,
};
use crate::ui::cleaner_view::{render_cleaner_view, CleanerViewState};
use crate::ui::dashboard_view::{render_dashboard, DashboardMetrics};
use crate::ui::explorer_view::{render_explorer_view, ExplorerViewState};
use crate::ui::logs_view::{render_logs_view, LogsViewState};
use crate::ui::modals::{
    show_delete_confirmation_modal, show_locked_file_modal, DeleteModalState,
    LockedFileModalState,
};
use crate::ui::theme::{apply_clean_theme, ThemeColors};

pub struct WinFolderCleanerApp {
    pub active_tab: usize, // 0 = Dashboard, 1 = Explorer, 2 = Temp Cleaner, 3 = Logs
    pub is_admin: bool,

    // Sub-view states
    pub explorer_state: ExplorerViewState,
    pub cleaner_state: CleanerViewState,
    pub logs_state: LogsViewState,
    pub logs: Vec<LogEntry>,

    // Modals
    pub delete_modal: DeleteModalState,
    pub locked_file_modal: LockedFileModalState,

    // Background workers & channels
    pub scan_cancel_token: Arc<AtomicBool>,
    pub scan_progress_rx: Option<Receiver<ScanProgress>>,
    pub scan_result_rx: Option<Receiver<Result<DirScanSummary, String>>>,

    pub clean_cancel_token: Arc<AtomicBool>,
    pub clean_progress_rx: Option<Receiver<String>>,
    pub clean_result_rx: Option<Receiver<DeletionReport>>,

    // Live tracking
    pub current_scan_path: String,
    pub current_files_count: usize,
    pub current_bytes_count: u64,
}

impl WinFolderCleanerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_clean_theme(&cc.egui_ctx);
        let is_admin = is_elevated();
        let locations = discover_temp_locations();

        let mut cleaner_state = CleanerViewState::default();
        cleaner_state.locations = locations;

        let mut app = Self {
            active_tab: 0,
            is_admin,
            explorer_state: ExplorerViewState::default(),
            cleaner_state,
            logs_state: LogsViewState::default(),
            logs: Vec::new(),
            delete_modal: DeleteModalState::default(),
            locked_file_modal: LockedFileModalState::default(),
            scan_cancel_token: Arc::new(AtomicBool::new(false)),
            scan_progress_rx: None,
            scan_result_rx: None,
            clean_cancel_token: Arc::new(AtomicBool::new(false)),
            clean_progress_rx: None,
            clean_result_rx: None,
            current_scan_path: String::new(),
            current_files_count: 0,
            current_bytes_count: 0,
        };

        app.logs.push(LogEntry {
            timestamp: chrono::Local::now(),
            target_path: "Application Startup".to_string(),
            action: "System Initialization".to_string(),
            status: LogStatus::Success,
            details: format!(
                "Application started. Privilege level: {}",
                if is_admin { "Administrator (Elevated)" } else { "Standard User" }
            ),
        });

        app
    }

    fn calculate_dashboard_metrics(&self) -> DashboardMetrics {
        let mut total_scanned_bytes = 0u64;
        let mut total_files = 0usize;
        let mut total_dirs = 0usize;
        let mut total_inaccessible = 0usize;
        let mut total_locked = 0usize;

        // From Explorer scan
        if let Some(summary) = &self.explorer_state.scan_summary {
            total_scanned_bytes += summary.total_bytes;
            total_files += summary.total_files;
            total_dirs += summary.total_dirs;
            total_inaccessible += summary.inaccessible_files;
        }

        // From Cleaner locations
        for loc in &self.cleaner_state.locations {
            if let Some(summary) = &loc.summary {
                total_scanned_bytes += summary.total_bytes;
                total_files += summary.total_files;
                total_dirs += summary.total_dirs;
                total_inaccessible += summary.inaccessible_files;
            }
            total_locked += loc.locked_files.len();
        }

        DashboardMetrics {
            total_scanned_bytes,
            total_files,
            total_dirs,
            total_inaccessible,
            total_locked,
            is_admin: self.is_admin,
        }
    }

    fn start_explorer_scan(&mut self, path: PathBuf) {
        self.explorer_state.is_scanning = true;
        self.current_scan_path = path.to_string_lossy().to_string();
        self.current_files_count = 0;
        self.current_bytes_count = 0;

        self.scan_cancel_token.store(false, Ordering::Relaxed);
        let cancel_token = self.scan_cancel_token.clone();

        let (progress_tx, progress_rx) = unbounded();
        let (result_tx, result_rx) = unbounded();

        self.scan_progress_rx = Some(progress_rx);
        self.scan_result_rx = Some(result_rx);

        thread::spawn(move || {
            let res = scan_directory_tree(&path, cancel_token, Some(progress_tx));
            let _ = result_tx.send(res);
        });
    }

    fn cancel_scan(&mut self) {
        self.scan_cancel_token.store(true, Ordering::Relaxed);
        self.explorer_state.is_scanning = false;
    }

    fn start_locations_scan(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        for path in paths {
            let cancel_token = Arc::new(AtomicBool::new(false));
            if let Ok(summary) = scan_directory_tree(&path, cancel_token, None) {
                // Update corresponding location in cleaner_state
                for loc in &mut self.cleaner_state.locations {
                    if loc.path == path {
                        loc.summary = Some(summary.clone());
                        break;
                    }
                }
            }
        }
    }

    fn execute_clean_pipeline(
        &mut self,
        paths: Vec<PathBuf>,
        age_filter: AgeFilter,
        is_dry_run: bool,
        force: bool,
        schedule_reboot: bool,
    ) {
        self.cleaner_state.is_operating = true;
        self.cleaner_state.current_operation = if is_dry_run {
            "Simulating deletion (Dry Run)...".to_string()
        } else {
            "Executing safe cleanup...".to_string()
        };

        self.clean_cancel_token.store(false, Ordering::Relaxed);
        let cancel_token = self.clean_cancel_token.clone();

        let (progress_tx, progress_rx) = unbounded();
        let (result_tx, result_rx) = unbounded();

        self.clean_progress_rx = Some(progress_rx);
        self.clean_result_rx = Some(result_rx);

        let config = CleanExecutionConfig {
            is_dry_run,
            force_ownership_and_acl: force,
            age_filter,
            schedule_reboot_if_locked: schedule_reboot,
            delete_only_contents: true,
        };

        thread::spawn(move || {
            let mut combined_report = DeletionReport::default();
            for path in paths {
                if cancel_token.load(Ordering::Relaxed) {
                    break;
                }
                let report = clean_directory_contents(
                    &path,
                    &config,
                    cancel_token.clone(),
                    Some(progress_tx.clone()),
                );
                combined_report.total_examined += report.total_examined;
                combined_report.deleted_count += report.deleted_count;
                combined_report.deleted_bytes += report.deleted_bytes;
                combined_report.failed_count += report.failed_count;
                combined_report.skipped_count += report.skipped_count;
                combined_report.scheduled_reboot_count += report.scheduled_reboot_count;
                combined_report.locked_files.extend(report.locked_files);
                combined_report.log_entries.extend(report.log_entries);
            }
            let _ = result_tx.send(combined_report);
        });
    }

    fn poll_background_workers(&mut self) {
        // Poll scanner progress
        if let Some(rx) = &self.scan_progress_rx {
            while let Ok(progress) = rx.try_recv() {
                self.current_scan_path = progress.current_path;
                self.current_files_count = progress.files_scanned;
                self.current_bytes_count = progress.bytes_scanned;
            }
        }

        // Poll scanner result
        if let Some(rx) = &self.scan_result_rx {
            if let Ok(result) = rx.try_recv() {
                self.explorer_state.is_scanning = false;
                match result {
                    Ok(summary) => {
                        self.logs.push(LogEntry {
                            timestamp: chrono::Local::now(),
                            target_path: summary.root_path.to_string_lossy().to_string(),
                            action: "Directory Scan".to_string(),
                            status: LogStatus::Success,
                            details: format!(
                                "Scan complete: {} files, {} bytes, {} inaccessible items in {} ms.",
                                summary.total_files, summary.total_bytes, summary.inaccessible_files, summary.duration_ms
                            ),
                        });
                        self.explorer_state.scan_summary = Some(summary);
                    }
                    Err(err) => {
                        self.logs.push(LogEntry {
                            timestamp: chrono::Local::now(),
                            target_path: self.explorer_state.selected_path.clone(),
                            action: "Directory Scan".to_string(),
                            status: LogStatus::Error,
                            details: format!("Scan error: {}", err),
                        });
                    }
                }
                self.scan_result_rx = None;
                self.scan_progress_rx = None;
            }
        }

        // Poll cleaner progress
        if let Some(rx) = &self.clean_progress_rx {
            while let Ok(item) = rx.try_recv() {
                self.cleaner_state.operation_progress_item = item;
            }
        }

        // Poll cleaner result
        if let Some(rx) = &self.clean_result_rx {
            if let Ok(report) = rx.try_recv() {
                self.cleaner_state.is_operating = false;
                self.cleaner_state.current_operation.clear();
                self.cleaner_state.operation_progress_item.clear();

                for entry in report.log_entries {
                    self.logs.push(entry);
                }

                // Refresh scan summary for affected locations
                let selected_paths: Vec<PathBuf> = self
                    .cleaner_state
                    .locations
                    .iter()
                    .filter(|l| l.selected && l.exists)
                    .map(|l| l.path.clone())
                    .collect();
                self.start_locations_scan(selected_paths);

                self.clean_result_rx = None;
                self.clean_progress_rx = None;
            }
        }
    }
}

impl eframe::App for WinFolderCleanerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Continuously poll worker channels without blocking UI thread
        self.poll_background_workers();

        // If scanning or cleaning, request repaint to show live numbers
        if self.explorer_state.is_scanning || self.cleaner_state.is_operating {
            ctx.request_repaint();
        }

        // Top Navigation Bar
        TopBottomPanel::top("app_top_bar")
            .frame(
                Frame::none()
                    .fill(ThemeColors::BG_PANEL)
                    .stroke(Stroke::new(1.0, ThemeColors::BORDER_QUIET))
                    .inner_margin(Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Folder Cleaner Pro")
                            .color(ThemeColors::TEXT_PRIMARY)
                            .strong()
                            .size(16.0),
                    );

                    ui.add_space(24.0);

                    if ui.selectable_label(self.active_tab == 0, "Dashboard").clicked() {
                        self.active_tab = 0;
                    }
                    if ui.selectable_label(self.active_tab == 1, "Folder Explorer").clicked() {
                        self.active_tab = 1;
                    }
                    if ui.selectable_label(self.active_tab == 2, "Temp Cleaner").clicked() {
                        self.active_tab = 2;
                    }
                    if ui.selectable_label(self.active_tab == 3, "Operation Log").clicked() {
                        self.active_tab = 3;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.is_admin {
                            ui.label(
                                RichText::new("● Elevated (Admin)")
                                    .color(ThemeColors::STATUS_SUCCESS)
                                    .size(12.0),
                            );
                        } else {
                            ui.horizontal(|ui| {
                                if ui
                                    .small_button(
                                        RichText::new("Relaunch Elevated").color(Color32::WHITE),
                                    )
                                    .clicked()
                                {
                                    let _ = relaunch_as_admin();
                                }
                                ui.label(
                                    RichText::new("● Standard User")
                                        .color(ThemeColors::STATUS_WARNING)
                                        .size(12.0),
                                );
                            });
                        }
                    });
                });
            });

        // Main Content Area
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(ThemeColors::BG_APP)
                    .inner_margin(Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                match self.active_tab {
                    0 => {
                        let metrics = self.calculate_dashboard_metrics();
                        let mut nav_target = None;
                        let mut quick_scan = false;

                        render_dashboard(
                            ui,
                            &metrics,
                            &mut |target| nav_target = Some(target),
                            &mut || quick_scan = true,
                        );

                        if let Some(target) = nav_target {
                            self.active_tab = target;
                        }

                        if quick_scan {
                            let sys_temp = PathBuf::from("C:\\Windows\\Temp");
                            self.start_locations_scan(vec![sys_temp]);
                        }
                    }
                    1 => {
                        let mut start_scan = None;
                        let mut cancel_scan = false;
                        let mut inspect_target = None;
                        let mut delete_target = None;

                        let cur_scan_path = self.current_scan_path.clone();
                        let cur_files = self.current_files_count;
                        let cur_bytes = self.current_bytes_count;

                        let mut clean_folder_target = None;

                        render_explorer_view(
                            ui,
                            &mut self.explorer_state,
                            &cur_scan_path,
                            cur_files,
                            cur_bytes,
                            &mut |path| start_scan = Some(path),
                            &mut || cancel_scan = true,
                            &mut |file| inspect_target = Some(file.clone()),
                            &mut |file| delete_target = Some(file.clone()),
                            &mut |folder| clean_folder_target = Some(folder.clone()),
                        );

                        if let Some(folder) = clean_folder_target {
                            self.delete_modal = DeleteModalState {
                                is_open: true,
                                target_name: folder.file_name().unwrap_or_default().to_string_lossy().to_string(),
                                target_paths: vec![folder.clone()],
                                total_files: 0,
                                total_dirs: 0,
                                total_bytes: 0,
                                is_dry_run: false,
                                force_ownership_and_acl: false,
                                schedule_reboot_if_locked: false,
                            };
                        }

                        if let Some(path) = start_scan {
                            self.start_explorer_scan(path);
                        }
                        if cancel_scan {
                            self.cancel_scan();
                        }
                        if let Some(file) = inspect_target {
                            let processes = get_locking_processes(&file.path);
                            self.locked_file_modal = LockedFileModalState {
                                is_open: true,
                                file_info: Some(file),
                                locking_processes: processes,
                                reboot_scheduled: false,
                            };
                        }
                        if let Some(file) = delete_target {
                            self.delete_modal = DeleteModalState {
                                is_open: true,
                                target_name: file.name.clone(),
                                target_paths: vec![file.path.clone()],
                                total_files: 1,
                                total_dirs: 0,
                                total_bytes: file.size,
                                is_dry_run: false,
                                force_ownership_and_acl: false,
                                schedule_reboot_if_locked: false,
                            };
                        }
                    }
                    2 => {
                        let mut scan_paths = None;
                        let mut take_own_paths = None;
                        let mut fix_perm_paths = None;
                        let mut clean_req = None;
                        let mut add_dir = None;

                        render_cleaner_view(
                            ui,
                            &mut self.cleaner_state,
                            &mut |paths| scan_paths = Some(paths),
                            &mut |paths| take_own_paths = Some(paths),
                            &mut |paths| fix_perm_paths = Some(paths),
                            &mut |paths, filter, dry_run| clean_req = Some((paths, filter, dry_run)),
                            &mut |path| add_dir = Some(path),
                        );

                        if let Some(paths) = scan_paths {
                            self.start_locations_scan(paths);
                        }

                        if let Some(paths) = take_own_paths {
                            for p in paths {
                                match take_ownership(&p) {
                                    Ok(_) => {
                                        self.logs.push(LogEntry {
                                            timestamp: chrono::Local::now(),
                                            target_path: p.to_string_lossy().to_string(),
                                            action: "Take Ownership".to_string(),
                                            status: LogStatus::Success,
                                            details: "Ownership successfully transferred to Administrators.".to_string(),
                                        });
                                    }
                                    Err(e) => {
                                        self.logs.push(LogEntry {
                                            timestamp: chrono::Local::now(),
                                            target_path: p.to_string_lossy().to_string(),
                                            action: "Take Ownership".to_string(),
                                            status: LogStatus::Error,
                                            details: e,
                                        });
                                    }
                                }
                            }
                        }

                        if let Some(paths) = fix_perm_paths {
                            for p in paths {
                                match grant_administrators_acl(&p) {
                                    Ok(_) => {
                                        self.logs.push(LogEntry {
                                            timestamp: chrono::Local::now(),
                                            target_path: p.to_string_lossy().to_string(),
                                            action: "Fix Permissions".to_string(),
                                            status: LogStatus::Success,
                                            details: "Granted Administrators (*S-1-5-32-544) Full Control.".to_string(),
                                        });
                                    }
                                    Err(e) => {
                                        self.logs.push(LogEntry {
                                            timestamp: chrono::Local::now(),
                                            target_path: p.to_string_lossy().to_string(),
                                            action: "Fix Permissions".to_string(),
                                            status: LogStatus::Error,
                                            details: e,
                                        });
                                    }
                                }
                            }
                        }

                        if let Some((paths, filter, dry_run)) = clean_req {
                            self.cleaner_state.selected_age_filter = filter;
                            let mut total_files = 0;
                            let mut total_dirs = 0;
                            let mut total_bytes = 0;

                            for p in &paths {
                                for loc in &self.cleaner_state.locations {
                                    if loc.path == *p {
                                        if let Some(s) = &loc.summary {
                                            total_files += s.total_files;
                                            total_dirs += s.total_dirs;
                                            total_bytes += s.total_bytes;
                                        }
                                    }
                                }
                            }

                            self.delete_modal = DeleteModalState {
                                is_open: true,
                                target_name: format!("{} selected locations", paths.len()),
                                target_paths: paths,
                                total_files,
                                total_dirs,
                                total_bytes,
                                is_dry_run: dry_run,
                                force_ownership_and_acl: false,
                                schedule_reboot_if_locked: false,
                            };
                        }

                        if let Some(dir) = add_dir {
                            let id = format!("custom_{}", dir.file_name().unwrap_or_default().to_string_lossy());
                            let name = format!("Custom: {}", dir.file_name().unwrap_or_default().to_string_lossy());
                            let exists = dir.is_dir();
                            self.cleaner_state.locations.push(TempLocation {
                                id,
                                name,
                                path: dir.clone(),
                                description: "User-added custom folder for analysis and cleanup".to_string(),
                                is_system: false,
                                exists,
                                selected: true,
                                summary: None,
                                locked_files: Vec::new(),
                            });
                        }
                    }
                    3 => {
                        render_logs_view(ui, &mut self.logs_state, &mut self.logs);
                    }
                    _ => {}
                }
            });

        // Modals rendering
        let mut trigger_delete = None;
        show_delete_confirmation_modal(
            ctx,
            &mut self.delete_modal,
            |dry_run, force, reboot| {
                trigger_delete = Some((dry_run, force, reboot));
            },
        );

        if let Some((dry_run, force, reboot)) = trigger_delete {
            let paths = self.delete_modal.target_paths.clone();
            let age_filter = self.cleaner_state.selected_age_filter;
            self.execute_clean_pipeline(paths, age_filter, dry_run, force, reboot);
        }

        let mut trigger_schedule_reboot = None;
        show_locked_file_modal(
            ctx,
            &mut self.locked_file_modal,
            |target_path| {
                trigger_schedule_reboot = Some(target_path.clone());
            },
        );

        if let Some(target) = trigger_schedule_reboot {
            match schedule_deletion_on_reboot(&target) {
                Ok(_) => {
                    self.logs.push(LogEntry {
                        timestamp: chrono::Local::now(),
                        target_path: target.to_string_lossy().to_string(),
                        action: "Schedule Reboot Deletion".to_string(),
                        status: LogStatus::Success,
                        details: "File scheduled for deletion upon next Windows restart.".to_string(),
                    });
                }
                Err(e) => {
                    self.logs.push(LogEntry {
                        timestamp: chrono::Local::now(),
                        target_path: target.to_string_lossy().to_string(),
                        action: "Schedule Reboot Deletion".to_string(),
                        status: LogStatus::Error,
                        details: e,
                    });
                }
            }
        }
    }
}
