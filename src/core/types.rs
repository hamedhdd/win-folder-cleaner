use std::path::PathBuf;
use std::time::SystemTime;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Detailed file metadata for sorting, display, and deletion operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_readonly: bool,
    pub is_locked: bool,
    pub is_inaccessible: bool,
}

impl FileInfo {
    pub fn formatted_size(&self) -> String {
        format_bytes(self.size)
    }

    pub fn formatted_modified(&self) -> String {
        match self.modified {
            Some(time) => {
                let dt: DateTime<Local> = time.into();
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            }
            None => "Unknown".to_string(),
        }
    }
}

/// Hierarchical tree node representing a folder and its subdirectories with cumulative metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderTreeNode {
    pub path: PathBuf,
    pub name: String,
    pub total_size: u64,
    pub immediate_files_size: u64,
    pub file_count: usize,
    pub dir_count: usize,
    pub children: Vec<FolderTreeNode>,
    pub direct_files: Vec<FileInfo>,
    pub is_inaccessible: bool,
}

impl FolderTreeNode {
    pub fn formatted_size(&self) -> String {
        format_bytes(self.total_size)
    }

    pub fn formatted_immediate_size(&self) -> String {
        format_bytes(self.immediate_files_size)
    }
}

/// Directory item summary with immediate and recursive subtree sizes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntrySummary {
    pub path: PathBuf,
    pub name: String,
    pub total_size: u64,
    pub immediate_size: u64,
    pub file_count: usize,
    pub dir_count: usize,
    pub has_subdirs: bool,
}

impl DirEntrySummary {
    pub fn formatted_size(&self) -> String {
        format_bytes(self.total_size)
    }
}

/// Aggregated statistics grouped by file extension
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionStat {
    pub ext: String,
    pub count: usize,
    pub total_bytes: u64,
    pub percentage: f32,
}

impl ExtensionStat {
    pub fn formatted_size(&self) -> String {
        format_bytes(self.total_bytes)
    }
}

/// Full summary produced after completing a directory scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirScanSummary {
    pub root_path: PathBuf,
    pub total_bytes: u64,
    pub accessible_bytes: u64,
    pub inaccessible_bytes: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    pub inaccessible_files: usize,
    pub largest_files: Vec<FileInfo>,
    pub largest_subdirs: Vec<DirEntrySummary>,
    pub extensions: Vec<ExtensionStat>,
    pub root_tree: Option<FolderTreeNode>,
    pub scan_timestamp: DateTime<Local>,
    pub duration_ms: u128,
}

#[allow(dead_code)]
impl DirScanSummary {
    pub fn formatted_total_size(&self) -> String {
        format_bytes(self.total_bytes)
    }

    pub fn formatted_accessible_size(&self) -> String {
        format_bytes(self.accessible_bytes)
    }

    pub fn formatted_inaccessible_size(&self) -> String {
        format_bytes(self.inaccessible_bytes)
    }
}

/// Real-time progress updates delivered from the scanning thread
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScanProgress {
    pub current_path: String,
    pub files_scanned: usize,
    pub dirs_scanned: usize,
    pub bytes_scanned: u64,
    pub inaccessible_count: usize,
    pub is_completed: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

/// Predefined or custom temporary directory locations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempLocation {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub description: String,
    pub is_system: bool,
    pub exists: bool,
    pub selected: bool,
    pub summary: Option<DirScanSummary>,
    pub locked_files: Vec<FileInfo>,
}

/// Age filter options for Temp cleaner
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeFilter {
    All,
    OlderThan7Days,
    OlderThan30Days,
    OlderThan60Days,
    OlderThan90Days,
}

impl AgeFilter {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgeFilter::All => "All files (Any age)",
            AgeFilter::OlderThan7Days => "Older than 7 days",
            AgeFilter::OlderThan30Days => "Older than 30 days",
            AgeFilter::OlderThan60Days => "Older than 60 days",
            AgeFilter::OlderThan90Days => "Older than 90 days",
        }
    }

    pub fn days_threshold(&self) -> Option<u64> {
        match self {
            AgeFilter::All => None,
            AgeFilter::OlderThan7Days => Some(7),
            AgeFilter::OlderThan30Days => Some(30),
            AgeFilter::OlderThan60Days => Some(60),
            AgeFilter::OlderThan90Days => Some(90),
        }
    }
}

/// Sort criteria for folder explorer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Size,
    DateModified,
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Operation status for audit log entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogStatus {
    Success,
    Warning,
    Error,
    Skipped,
    DryRun,
}

/// An entry in the persistent operations log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub target_path: String,
    pub action: String,
    pub status: LogStatus,
    pub details: String,
}

/// Process locking a file returned by Restart Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockingProcessInfo {
    pub pid: u32,
    pub app_name: String,
    pub service_name: String,
}

/// Helper function to format byte sizes into readable units (Bytes, KB, MB, GB, TB)
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}
