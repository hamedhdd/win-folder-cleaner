use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::Local;
use crossbeam_channel::Sender;

use crate::core::types::{
    DirEntrySummary, DirScanSummary, ExtensionStat, FileInfo, FolderTreeNode, ScanProgress,
};

#[derive(Eq, PartialEq)]
struct LargestFileItem(FileInfo);

impl Ord for LargestFileItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse order so BinaryHeap behaves as a min-heap for top N largest
        other.0.size.cmp(&self.0.size)
    }
}

impl PartialOrd for LargestFileItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct DirNodeData {
    path: PathBuf,
    name: String,
    direct_size: u64,
    direct_files: Vec<FileInfo>,
    child_subdirs: Vec<PathBuf>,
    is_inaccessible: bool,
}

/// Recursively scans the specified folder, gathering size, file counts, largest files,
/// subdirectories, extension distributions, full hierarchical folder tree, and inaccessible statistics.
///
/// Respects cancellation token and delivers non-blocking progress updates via channel.
pub fn scan_directory_tree(
    root: &Path,
    cancel_token: Arc<AtomicBool>,
    progress_tx: Option<Sender<ScanProgress>>,
) -> Result<DirScanSummary, String> {
    let start_time = Instant::now();
    let root_path = root.to_path_buf();

    if !root_path.exists() {
        return Err(format!("Path does not exist: {:?}", root_path));
    }

    let mut total_bytes: u64 = 0;
    let mut accessible_bytes: u64 = 0;
    let inaccessible_bytes: u64 = 0;
    let mut total_files: usize = 0;
    let mut total_dirs: usize = 0;
    let mut inaccessible_files: usize = 0;

    let mut min_heap: BinaryHeap<LargestFileItem> = BinaryHeap::new();
    const MAX_LARGEST_FILES: usize = 100;

    let mut ext_map: HashMap<String, (usize, u64)> = HashMap::new();
    let mut direct_subdirs: HashMap<PathBuf, (u64, usize, usize)> = HashMap::new();
    let mut dir_map: HashMap<PathBuf, DirNodeData> = HashMap::new();

    // Enumerate first-level subdirectories to calculate their direct and cumulative sizes
    if let Ok(entries) = fs::read_dir(&root_path) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() && !ft.is_symlink() {
                    direct_subdirs.insert(entry.path(), (0, 0, 0));
                }
            }
        }
    }

    let mut stack: Vec<PathBuf> = vec![root_path.clone()];
    let mut last_progress_time = Instant::now();

    while let Some(current_dir) = stack.pop() {
        if cancel_token.load(Ordering::Relaxed) {
            if let Some(tx) = &progress_tx {
                let _ = tx.send(ScanProgress {
                    current_path: current_dir.to_string_lossy().to_string(),
                    files_scanned: total_files,
                    dirs_scanned: total_dirs,
                    bytes_scanned: total_bytes,
                    inaccessible_count: inaccessible_files,
                    is_completed: false,
                    cancelled: true,
                    error: None,
                });
            }
            return Err("Scan cancelled by user.".to_string());
        }

        // Throttle progress updates to ~10 times per second
        if last_progress_time.elapsed() >= Duration::from_millis(100) {
            last_progress_time = Instant::now();
            if let Some(tx) = &progress_tx {
                let _ = tx.send(ScanProgress {
                    current_path: current_dir.to_string_lossy().to_string(),
                    files_scanned: total_files,
                    dirs_scanned: total_dirs,
                    bytes_scanned: total_bytes,
                    inaccessible_count: inaccessible_files,
                    is_completed: false,
                    cancelled: false,
                    error: None,
                });
            }
        }

        let dir_name = current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| current_dir.to_string_lossy().to_string());

        let mut node_data = DirNodeData {
            path: current_dir.clone(),
            name: dir_name,
            direct_size: 0,
            direct_files: Vec::new(),
            child_subdirs: Vec::new(),
            is_inaccessible: false,
        };

        let entries = match fs::read_dir(&current_dir) {
            Ok(iter) => iter,
            Err(_) => {
                inaccessible_files += 1;
                node_data.is_inaccessible = true;
                dir_map.insert(current_dir.clone(), node_data);
                continue;
            }
        };

        for entry in entries {
            if cancel_token.load(Ordering::Relaxed) {
                return Err("Scan cancelled by user.".to_string());
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    inaccessible_files += 1;
                    continue;
                }
            };

            let entry_path = entry.path();
            let metadata = match fs::symlink_metadata(&entry_path) {
                Ok(m) => m,
                Err(_) => {
                    inaccessible_files += 1;
                    continue;
                }
            };

            let file_type = metadata.file_type();

            // DO NOT follow symlinks or junctions/reparse points to avoid infinite loops and boundary traversal
            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                total_dirs += 1;
                stack.push(entry_path.clone());
                node_data.child_subdirs.push(entry_path.clone());

                // Attribute to direct subdirectory if within one
                for (subdir_path, stats) in direct_subdirs.iter_mut() {
                    if entry_path.starts_with(subdir_path) {
                        stats.2 += 1; // dir count
                        break;
                    }
                }
            } else if file_type.is_file() {
                total_files += 1;
                let size = metadata.len();
                total_bytes += size;
                accessible_bytes += size;
                node_data.direct_size += size;

                let name = entry.file_name().to_string_lossy().to_string();
                let ext = entry_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("(no extension)")
                    .to_lowercase();

                let modified = metadata.modified().ok();
                let readonly = metadata.permissions().readonly();

                let file_info = FileInfo {
                    path: entry_path.clone(),
                    name,
                    extension: ext.clone(),
                    size,
                    modified,
                    is_dir: false,
                    is_symlink: false,
                    is_readonly: readonly,
                    is_locked: false,
                    is_inaccessible: false,
                };

                // Track immediate files (up to 100 per folder for inspector)
                if node_data.direct_files.len() < 100 {
                    node_data.direct_files.push(file_info.clone());
                }

                // Track largest files across whole scan (top N)
                if min_heap.len() < MAX_LARGEST_FILES {
                    min_heap.push(LargestFileItem(file_info));
                } else if let Some(smallest) = min_heap.peek() {
                    if size > smallest.0.size {
                        min_heap.pop();
                        min_heap.push(LargestFileItem(file_info));
                    }
                }

                // Extension aggregation
                let stat = ext_map.entry(ext).or_insert((0, 0));
                stat.0 += 1;
                stat.1 += size;

                // Attribute to direct subdirectory if within one
                for (subdir_path, stats) in direct_subdirs.iter_mut() {
                    if entry_path.starts_with(subdir_path) {
                        stats.0 += size;
                        stats.1 += 1;
                        break;
                    }
                }
            }
        }

        dir_map.insert(current_dir.clone(), node_data);
    }

    // Finalize largest files list in descending order
    let mut largest_files: Vec<FileInfo> = min_heap.into_iter().map(|item| item.0).collect();
    largest_files.sort_by(|a, b| b.size.cmp(&a.size));

    // Finalize largest subdirectories
    let mut largest_subdirs: Vec<DirEntrySummary> = direct_subdirs
        .into_iter()
        .map(|(path, (size, file_count, dir_count))| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            DirEntrySummary {
                path,
                name,
                total_size: size,
                immediate_size: size,
                file_count,
                dir_count,
                has_subdirs: dir_count > 0,
            }
        })
        .collect();
    largest_subdirs.sort_by(|a, b| b.total_size.cmp(&a.total_size));

    // Finalize extensions
    let mut extensions: Vec<ExtensionStat> = ext_map
        .into_iter()
        .map(|(ext, (count, bytes))| {
            let percentage = if total_bytes > 0 {
                (bytes as f32 / total_bytes as f32) * 100.0
            } else {
                0.0
            };
            ExtensionStat {
                ext,
                count,
                total_bytes: bytes,
                percentage,
            }
        })
        .collect();
    extensions.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    // Assemble full hierarchical folder tree
    let root_tree = build_folder_tree(&root_path, &mut dir_map);

    let duration_ms = start_time.elapsed().as_millis();

    let summary = DirScanSummary {
        root_path,
        total_bytes,
        accessible_bytes,
        inaccessible_bytes,
        total_files,
        total_dirs,
        inaccessible_files,
        largest_files,
        largest_subdirs,
        extensions,
        root_tree,
        scan_timestamp: Local::now(),
        duration_ms,
    };

    if let Some(tx) = &progress_tx {
        let _ = tx.send(ScanProgress {
            current_path: String::from("Completed"),
            files_scanned: total_files,
            dirs_scanned: total_dirs,
            bytes_scanned: total_bytes,
            inaccessible_count: inaccessible_files,
            is_completed: true,
            cancelled: false,
            error: None,
        });
    }

    Ok(summary)
}

fn build_folder_tree(
    current_path: &Path,
    dir_map: &mut HashMap<PathBuf, DirNodeData>,
) -> Option<FolderTreeNode> {
    let data = dir_map.remove(current_path)?;
    let mut total_size = data.direct_size;
    let mut file_count = data.direct_files.len();
    let mut dir_count = data.child_subdirs.len();
    let mut children = Vec::new();

    for child_path in &data.child_subdirs {
        if let Some(child_node) = build_folder_tree(child_path, dir_map) {
            total_size += child_node.total_size;
            file_count += child_node.file_count;
            dir_count += child_node.dir_count;
            children.push(child_node);
        }
    }

    // Sort children descending by total_size so heaviest subfolders are prioritized at top
    children.sort_by(|a, b| b.total_size.cmp(&a.total_size));

    Some(FolderTreeNode {
        path: data.path,
        name: data.name,
        total_size,
        immediate_files_size: data.direct_size,
        file_count,
        dir_count,
        children,
        direct_files: data.direct_files,
        is_inaccessible: data.is_inaccessible,
    })
}
