use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use chrono::Local;
use crossbeam_channel::Sender;

use crate::core::reboot_del::schedule_deletion_on_reboot;
use crate::core::restart_mgr::get_locking_processes;
use crate::core::security::{
    grant_administrators_acl, is_critical_system_path, remove_readonly, take_ownership,
};
use crate::core::types::{AgeFilter, FileInfo, LogEntry, LogStatus};

/// Configuration options for the advanced deletion engine
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CleanExecutionConfig {
    pub is_dry_run: bool,
    pub force_ownership_and_acl: bool,
    pub age_filter: AgeFilter,
    pub schedule_reboot_if_locked: bool,
    pub delete_only_contents: bool,
}

impl Default for CleanExecutionConfig {
    fn default() -> Self {
        Self {
            is_dry_run: false,
            force_ownership_and_acl: false,
            age_filter: AgeFilter::All,
            schedule_reboot_if_locked: false,
            delete_only_contents: true,
        }
    }
}

/// Statistics and outcomes from a clean / deletion operation
#[derive(Debug, Clone, Default)]
pub struct DeletionReport {
    pub total_examined: usize,
    pub deleted_count: usize,
    pub deleted_bytes: u64,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub locked_files: Vec<FileInfo>,
    pub scheduled_reboot_count: usize,
    pub log_entries: Vec<LogEntry>,
}

/// Cleans the contents of a directory (such as a Temp folder) while strictly preserving the root folder itself.
pub fn clean_directory_contents(
    dir_path: &Path,
    config: &CleanExecutionConfig,
    cancel_token: Arc<AtomicBool>,
    progress_tx: Option<Sender<String>>,
) -> DeletionReport {
    let mut report = DeletionReport::default();

    if is_critical_system_path(dir_path) {
        let entry = LogEntry {
            timestamp: Local::now(),
            target_path: dir_path.to_string_lossy().to_string(),
            action: "Safety Check".to_string(),
            status: LogStatus::Error,
            details: "Target path is a protected critical Windows system location. Operation aborted.".to_string(),
        };
        report.log_entries.push(entry);
        report.failed_count += 1;
        return report;
    }

    if !dir_path.exists() {
        let entry = LogEntry {
            timestamp: Local::now(),
            target_path: dir_path.to_string_lossy().to_string(),
            action: "Validate Path".to_string(),
            status: LogStatus::Warning,
            details: "Target path does not exist.".to_string(),
        };
        report.log_entries.push(entry);
        return report;
    }

    // If target is an individual file, execute deletion directly
    if dir_path.is_file() {
        delete_path_recursive(dir_path, config, &mut report, &cancel_token, &progress_tx);
        return report;
    }

    let entries = match fs::read_dir(dir_path) {
        Ok(iter) => iter,
        Err(e) => {
            let entry = LogEntry {
                timestamp: Local::now(),
                target_path: dir_path.to_string_lossy().to_string(),
                action: "Read Directory".to_string(),
                status: LogStatus::Error,
                details: format!("Failed to read folder entries: {}", e),
            };
            report.log_entries.push(entry);
            report.failed_count += 1;
            return report;
        }
    };

    for item in entries.flatten() {
        if cancel_token.load(Ordering::Relaxed) {
            let entry = LogEntry {
                timestamp: Local::now(),
                target_path: dir_path.to_string_lossy().to_string(),
                action: "Cleanup Cancelled".to_string(),
                status: LogStatus::Warning,
                details: "Operation cancelled by user.".to_string(),
            };
            report.log_entries.push(entry);
            break;
        }

        let item_path = item.path();
        delete_path_recursive(&item_path, config, &mut report, &cancel_token, &progress_tx);
    }

    report
}

#[cfg(windows)]
pub fn is_reparse_point_or_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    if let Ok(meta) = fs::symlink_metadata(path) {
        // FILE_ATTRIBUTE_REPARSE_POINT = 0x00000400
        (meta.file_attributes() & 0x00000400) != 0
    } else {
        false
    }
}

#[cfg(not(windows))]
pub fn is_reparse_point_or_junction(_path: &Path) -> bool {
    false
}

/// Recursively deletes an individual path (file or folder).
pub fn delete_path_recursive(
    path: &Path,
    config: &CleanExecutionConfig,
    report: &mut DeletionReport,
    cancel_token: &Arc<AtomicBool>,
    progress_tx: &Option<Sender<String>>,
) {
    if cancel_token.load(Ordering::Relaxed) {
        return;
    }

    if is_critical_system_path(path) {
        report.skipped_count += 1;
        report.log_entries.push(LogEntry {
            timestamp: Local::now(),
            target_path: path.to_string_lossy().to_string(),
            action: "Delete Guard".to_string(),
            status: LogStatus::Warning,
            details: "Skipped protected critical Windows path.".to_string(),
        });
        return;
    }

    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            report.failed_count += 1;
            report.log_entries.push(LogEntry {
                timestamp: Local::now(),
                target_path: path.to_string_lossy().to_string(),
                action: "Inspect Metadata".to_string(),
                status: LogStatus::Error,
                details: format!("Inaccessible metadata: {}", e),
            });
            return;
        }
    };

    let file_type = metadata.file_type();

    // Check age filter if applicable
    if let Some(days) = config.age_filter.days_threshold() {
        if let Ok(mod_time) = metadata.modified() {
            if let Ok(elapsed) = SystemTime::now().duration_since(mod_time) {
                let days_elapsed = elapsed.as_secs() / 86400;
                if days_elapsed < days {
                    report.skipped_count += 1;
                    return;
                }
            }
        }
    }

    report.total_examined += 1;

    if let Some(tx) = progress_tx {
        let _ = tx.send(path.to_string_lossy().to_string());
    }

    if config.is_dry_run {
        report.deleted_count += 1;
        report.deleted_bytes += metadata.len();
        report.log_entries.push(LogEntry {
            timestamp: Local::now(),
            target_path: path.to_string_lossy().to_string(),
            action: "Dry Run Simulation".to_string(),
            status: LogStatus::DryRun,
            details: format!("Would delete {} ({} bytes)", if file_type.is_dir() { "directory" } else { "file" }, metadata.len()),
        });
        return;
    }

    // Do not follow symlinks, directory junctions, mount points, or reparse points
    let is_junction = is_reparse_point_or_junction(path);
    if file_type.is_symlink() || is_junction {
        let res = if file_type.is_dir() || is_junction {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        };
        match res {
            Ok(_) => {
                report.deleted_count += 1;
                report.log_entries.push(LogEntry {
                    timestamp: Local::now(),
                    target_path: path.to_string_lossy().to_string(),
                    action: "Delete Junction / Symlink".to_string(),
                    status: LogStatus::Success,
                    details: "Removed directory junction/symlink reparse point without traversing into target.".to_string(),
                });
            }
            Err(e) => {
                report.failed_count += 1;
                report.log_entries.push(LogEntry {
                    timestamp: Local::now(),
                    target_path: path.to_string_lossy().to_string(),
                    action: "Delete Junction / Symlink".to_string(),
                    status: LogStatus::Error,
                    details: format!("Failed to remove reparse point: {}", e),
                });
            }
        }
        return;
    }

    if file_type.is_dir() {
        // Recurse into children first
        if let Ok(children) = fs::read_dir(path) {
            for child in children.flatten() {
                delete_path_recursive(&child.path(), config, report, cancel_token, progress_tx);
            }
        }
        // Then delete the empty directory itself
        match fs::remove_dir(path) {
            Ok(_) => {
                report.deleted_count += 1;
                report.log_entries.push(LogEntry {
                    timestamp: Local::now(),
                    target_path: path.to_string_lossy().to_string(),
                    action: "Delete Folder".to_string(),
                    status: LogStatus::Success,
                    details: "Folder successfully removed.".to_string(),
                });
            }
            Err(e) => {
                handle_deletion_failure(path, true, e, config, report);
            }
        }
    } else {
        // Individual file deletion
        let file_size = metadata.len();
        let _ = remove_readonly(path);

        match fs::remove_file(path) {
            Ok(_) => {
                report.deleted_count += 1;
                report.deleted_bytes += file_size;
                report.log_entries.push(LogEntry {
                    timestamp: Local::now(),
                    target_path: path.to_string_lossy().to_string(),
                    action: "Delete File".to_string(),
                    status: LogStatus::Success,
                    details: format!("File removed ({} bytes).", file_size),
                });
            }
            Err(e) => {
                handle_deletion_failure(path, false, e, config, report);
            }
        }
    }
}

/// Handles failure when normal deletion encounters permission denial or sharing violations.
fn handle_deletion_failure(
    path: &Path,
    is_dir: bool,
    initial_err: std::io::Error,
    config: &CleanExecutionConfig,
    report: &mut DeletionReport,
) {
    let mut retry_success = false;

    // Check if locked by running processes via Restart Manager
    let locking_apps = get_locking_processes(path);
    let is_locked = !locking_apps.is_empty();

    if is_locked {
        let proc_names: Vec<String> = locking_apps
            .iter()
            .map(|p| format!("{} (PID: {})", p.app_name, p.pid))
            .collect();

        let details = format!("Locked by active processes: {}", proc_names.join(", "));

        report.locked_files.push(FileInfo {
            path: path.to_path_buf(),
            name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            extension: path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default(),
            size: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            modified: fs::metadata(path).and_then(|m| m.modified()).ok(),
            is_dir,
            is_symlink: false,
            is_readonly: false,
            is_locked: true,
            is_inaccessible: false,
        });

        if config.schedule_reboot_if_locked {
            if let Ok(_) = schedule_deletion_on_reboot(path) {
                report.scheduled_reboot_count += 1;
                report.log_entries.push(LogEntry {
                    timestamp: Local::now(),
                    target_path: path.to_string_lossy().to_string(),
                    action: "Schedule Reboot Deletion".to_string(),
                    status: LogStatus::Warning,
                    details: format!("File is locked. Scheduled for deletion on next Windows reboot. {}", details),
                });
                return;
            }
        }

        report.failed_count += 1;
        report.log_entries.push(LogEntry {
            timestamp: Local::now(),
            target_path: path.to_string_lossy().to_string(),
            action: "File Locked".to_string(),
            status: LogStatus::Warning,
            details,
        });
        return;
    }

    // If force mode is active, attempt ownership acquisition and ACL modification
    if config.force_ownership_and_acl {
        let _ = remove_readonly(path);

        if let Ok(_) = take_ownership(path) {
            report.log_entries.push(LogEntry {
                timestamp: Local::now(),
                target_path: path.to_string_lossy().to_string(),
                action: "Take Ownership".to_string(),
                status: LogStatus::Success,
                details: "Ownership successfully transferred to Administrators group.".to_string(),
            });

            if let Ok(_) = grant_administrators_acl(path) {
                report.log_entries.push(LogEntry {
                    timestamp: Local::now(),
                    target_path: path.to_string_lossy().to_string(),
                    action: "Grant Permissions".to_string(),
                    status: LogStatus::Success,
                    details: "Granted full control (*S-1-5-32-544) preserving existing ACLs.".to_string(),
                });

                // Retry deletion
                let retry_res = if is_dir {
                    fs::remove_dir(path)
                } else {
                    fs::remove_file(path)
                };

                if retry_res.is_ok() {
                    retry_success = true;
                    report.deleted_count += 1;
                    report.log_entries.push(LogEntry {
                        timestamp: Local::now(),
                        target_path: path.to_string_lossy().to_string(),
                        action: "Force Delete".to_string(),
                        status: LogStatus::Success,
                        details: "Deleted successfully after ownership and permission elevation.".to_string(),
                    });
                }
            }
        }
    }

    if !retry_success {
        report.failed_count += 1;
        report.log_entries.push(LogEntry {
            timestamp: Local::now(),
            target_path: path.to_string_lossy().to_string(),
            action: "Delete Failed".to_string(),
            status: LogStatus::Error,
            details: format!("Deletion failed: {}", initial_err),
        });
    }
}
