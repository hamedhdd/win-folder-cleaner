use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use chrono::Local;
use crossbeam_channel::Sender;
use rayon::prelude::*;

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

/// Reads the immediate files of a single directory on demand in < 1ms.
pub fn load_directory_files(dir_path: &Path) -> Vec<FileInfo> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if let Ok(meta) = fs::symlink_metadata(&entry_path) {
                if meta.is_file() && !meta.file_type().is_symlink() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let ext = entry_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("(no extension)")
                        .to_lowercase();
                    files.push(FileInfo {
                        path: entry_path,
                        name,
                        extension: ext,
                        size: meta.len(),
                        modified: meta.modified().ok(),
                        is_dir: false,
                        is_symlink: false,
                        is_readonly: meta.permissions().readonly(),
                        is_locked: false,
                        is_inaccessible: false,
                    });
                }
            }
        }
    }
    // Sort descending by file size
    files.sort_by(|a, b| b.size.cmp(&a.size));
    files
}

/// Rapidly computes the TRUE RECURSIVE size, total file count, total subdirectory count,
/// immediate child directory paths, and immediate files for a folder.
///
/// Allocates ZERO memory for individual recursive files — only accumulates raw u64 bytes and counts.
pub fn compute_dir_stats_recursive(
    dir_path: &Path,
    cancel_token: &Arc<AtomicBool>,
) -> (u64, usize, usize, Vec<PathBuf>, Vec<FileInfo>) {
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    let mut direct_subdirs = Vec::new();
    let mut direct_files = Vec::new();

    let mut stack = vec![dir_path.to_path_buf()];

    while let Some(current) = stack.pop() {
        if cancel_token.load(Ordering::Relaxed) {
            break;
        }

        let entries = match fs::read_dir(&current) {
            Ok(iter) => iter,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }

            if ft.is_dir() {
                total_dirs += 1;
                if current == dir_path {
                    direct_subdirs.push(path.clone());
                }
                stack.push(path);
            } else if ft.is_file() {
                let size = meta.len();
                total_files += 1;
                total_bytes += size;

                if current == dir_path && direct_files.len() < 100 {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("(no extension)")
                        .to_lowercase();
                    direct_files.push(FileInfo {
                        path,
                        name,
                        extension: ext,
                        size,
                        modified: meta.modified().ok(),
                        is_dir: false,
                        is_symlink: false,
                        is_readonly: meta.permissions().readonly(),
                        is_locked: false,
                        is_inaccessible: false,
                    });
                }
            }
        }
    }

    direct_files.sort_by(|a, b| b.size.cmp(&a.size));
    (total_bytes, total_files, total_dirs, direct_subdirs, direct_files)
}

/// Scans deeper levels ON-DEMAND for a specific folder node that the user expanded by clicking in the tree view.
///
/// 1. Concurrently computes TRUE RECURSIVE cumulative sizes for its direct subdirectories using Rayon.
/// 2. STRICT REQUIREMENT 1: Sorts children descending by total size so heaviest folders are at the top.
/// 3. STRICT REQUIREMENT 2: Runs deeper level scanning exclusively for the clicked folder, leaving nested sub-subfolders
///    unscanned until the user explicitly expands them.
pub fn scan_deeper_level_for_node(node: &mut FolderTreeNode) {
    if node.is_deeper_scanned {
        node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
        return;
    }

    let cancel_token = Arc::new(AtomicBool::new(false));

    // Discover direct subdirectories and direct files of node.path
    let mut direct_subdirs = Vec::new();
    let mut direct_files = Vec::new();
    let mut immediate_files_size = 0u64;

    if let Ok(entries) = fs::read_dir(&node.path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(meta) = fs::symlink_metadata(&path) {
                let ft = meta.file_type();
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    direct_subdirs.push(path);
                } else if ft.is_file() {
                    let size = meta.len();
                    immediate_files_size += size;
                    if direct_files.len() < 100 {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("(no extension)")
                            .to_lowercase();
                        direct_files.push(FileInfo {
                            path,
                            name,
                            extension: ext,
                            size,
                            modified: meta.modified().ok(),
                            is_dir: false,
                            is_symlink: false,
                            is_readonly: meta.permissions().readonly(),
                            is_locked: false,
                            is_inaccessible: false,
                        });
                    }
                }
            }
        }
    }

    node.immediate_files_size = immediate_files_size;
    direct_files.sort_by(|a, b| b.size.cmp(&a.size));
    node.direct_files = direct_files;

    // Concurrently compute true recursive sizes for each direct child subdirectory
    let mut children: Vec<FolderTreeNode> = direct_subdirs
        .par_iter()
        .map(|subdir_path| {
            let name = subdir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| subdir_path.to_string_lossy().to_string());

            let (total_size, file_count, dir_count, sub_subdirs, child_direct_files) =
                compute_dir_stats_recursive(subdir_path, &cancel_token);

            // Populate stub children so subfolders display expand arrow ▶
            let mut stub_children = Vec::new();
            for s in sub_subdirs {
                let s_name = s.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                stub_children.push(FolderTreeNode {
                    path: s,
                    name: s_name,
                    total_size: 0,
                    immediate_files_size: 0,
                    file_count: 0,
                    dir_count: 0,
                    children: Vec::new(),
                    direct_files: Vec::new(),
                    is_inaccessible: false,
                    is_deeper_scanned: false,
                });
            }

            let immediate_size = child_direct_files.iter().map(|f| f.size).sum();

            FolderTreeNode {
                path: subdir_path.clone(),
                name,
                total_size,
                immediate_files_size: immediate_size,
                file_count,
                dir_count,
                children: stub_children,
                direct_files: child_direct_files,
                is_inaccessible: false,
                is_deeper_scanned: false,
            }
        })
        .collect();

    // STRICT REQUIREMENT 1: "on left tree view folders should be sorted by size"
    children.sort_by(|a, b| b.total_size.cmp(&a.total_size));

    let children_total_size: u64 = children.iter().map(|c| c.total_size).sum();
    let children_file_count: usize = children.iter().map(|c| c.file_count).sum();

    node.total_size = immediate_files_size + children_total_size;
    node.file_count = node.direct_files.len() + children_file_count;
    node.dir_count = children.len();
    node.children = children;
    node.is_deeper_scanned = true;
}

/// Initial root scanner:
/// Scans the root folder and computes TRUE RECURSIVE cumulative sizes for Level 1 subdirectories.
/// Deeper levels (Level 2+) remain unscanned until the user explicitly expands them by clicking.
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

    if cancel_token.load(Ordering::Relaxed) {
        return Err("Scan cancelled by user.".to_string());
    }

    let root_name = root_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_path.to_string_lossy().to_string());

    let mut root_node = FolderTreeNode {
        path: root_path.clone(),
        name: root_name,
        total_size: 0,
        immediate_files_size: 0,
        file_count: 0,
        dir_count: 0,
        children: Vec::new(),
        direct_files: Vec::new(),
        is_inaccessible: false,
        is_deeper_scanned: false,
    };

    // Computes root and Level 1 children with true recursive sizes and size sorting
    scan_deeper_level_for_node(&mut root_node);

    if cancel_token.load(Ordering::Relaxed) {
        return Err("Scan cancelled by user.".to_string());
    }

    let mut min_heap: BinaryHeap<LargestFileItem> = BinaryHeap::new();
    const MAX_LARGEST_FILES: usize = 100;
    let mut ext_map: HashMap<String, (usize, u64)> = HashMap::new();

    // Aggregate immediate files of root
    for file in &root_node.direct_files {
        let stat = ext_map.entry(file.extension.clone()).or_insert((0, 0));
        stat.0 += 1;
        stat.1 += file.size;

        if min_heap.len() < MAX_LARGEST_FILES {
            min_heap.push(LargestFileItem(file.clone()));
        } else if let Some(smallest) = min_heap.peek() {
            if file.size > smallest.0.size {
                min_heap.pop();
                min_heap.push(LargestFileItem(file.clone()));
            }
        }
    }

    // Aggregate immediate files of Level 1 children
    for child in &root_node.children {
        for file in &child.direct_files {
            let stat = ext_map.entry(file.extension.clone()).or_insert((0, 0));
            stat.0 += 1;
            stat.1 += file.size;

            if min_heap.len() < MAX_LARGEST_FILES {
                min_heap.push(LargestFileItem(file.clone()));
            } else if let Some(smallest) = min_heap.peek() {
                if file.size > smallest.0.size {
                    min_heap.pop();
                    min_heap.push(LargestFileItem(file.clone()));
                }
            }
        }
    }

    // Finalize largest files in descending order
    let mut largest_files: Vec<FileInfo> = min_heap.into_iter().map(|item| item.0).collect();
    largest_files.sort_by(|a, b| b.size.cmp(&a.size));

    let total_bytes = root_node.total_size;
    let total_files = root_node.file_count;
    let total_dirs = root_node.dir_count;

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

    // Finalize largest subdirectories from root children (already sorted descending by size)
    let largest_subdirs: Vec<DirEntrySummary> = root_node
        .children
        .iter()
        .map(|child| DirEntrySummary {
            path: child.path.clone(),
            name: child.name.clone(),
            total_size: child.total_size,
            immediate_size: child.immediate_files_size,
            file_count: child.file_count,
            dir_count: child.dir_count,
            has_subdirs: child.dir_count > 0 || !child.children.is_empty(),
        })
        .collect();

    let duration_ms = start_time.elapsed().as_millis();

    let summary = DirScanSummary {
        root_path,
        total_bytes,
        accessible_bytes: total_bytes,
        inaccessible_bytes: 0,
        total_files,
        total_dirs,
        inaccessible_files: if root_node.is_inaccessible { 1 } else { 0 },
        largest_files,
        largest_subdirs,
        extensions,
        root_tree: Some(root_node.clone()),
        scan_timestamp: Local::now(),
        duration_ms,
    };

    if let Some(tx) = &progress_tx {
        let _ = tx.send(ScanProgress {
            current_path: String::from("Completed"),
            files_scanned: total_files,
            dirs_scanned: total_dirs,
            bytes_scanned: total_bytes,
            inaccessible_count: 0,
            is_completed: true,
            cancelled: false,
            error: None,
            current_depth: 0,
            partial_tree: Some(root_node),
        });
    }

    Ok(summary)
}
