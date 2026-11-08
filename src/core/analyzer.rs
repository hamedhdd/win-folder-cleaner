use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::Path;
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

/// Scans deeper levels ON-DEMAND for a specific folder node that the user expanded by clicking in the tree view.
///
/// 1. Scans its direct subdirectories concurrently across CPU cores using Rayon.
/// 2. For each subdirectory, computes its direct size, file count, and discovers its subfolder count.
/// 3. STRICT REQUIREMENT: Sorts children descending by total size so heaviest folders are at the top.
/// 4. Leaves deeper sub-subfolders unscanned until the user explicitly expands them.
pub fn scan_deeper_level_for_node(node: &mut FolderTreeNode) {
    if node.is_deeper_scanned {
        // Already scanned deeper level; ensure sorted by size
        node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
        return;
    }

    let entries = match fs::read_dir(&node.path) {
        Ok(iter) => iter,
        Err(_) => {
            node.is_inaccessible = true;
            node.is_deeper_scanned = true;
            return;
        }
    };

    let mut direct_files = Vec::new();
    let mut direct_size = 0u64;
    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if let Ok(meta) = fs::symlink_metadata(&entry_path) {
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                subdirs.push(entry_path);
            } else if ft.is_file() {
                let size = meta.len();
                direct_size += size;
                if direct_files.len() < 100 {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let ext = entry_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("(no extension)")
                        .to_lowercase();
                    direct_files.push(FileInfo {
                        path: entry_path,
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

    node.immediate_files_size = direct_size;
    direct_files.sort_by(|a, b| b.size.cmp(&a.size));
    node.direct_files = direct_files;

    // Scan each direct child subdirectory to get its direct size and count
    let mut children: Vec<FolderTreeNode> = subdirs
        .par_iter()
        .map(|subdir_path| {
            let name = subdir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| subdir_path.to_string_lossy().to_string());

            let mut child_direct_size = 0u64;
            let mut child_file_count = 0usize;
            let mut child_dir_count = 0usize;
            let mut child_direct_files = Vec::new();
            let mut is_inaccessible = false;

            if let Ok(entries) = fs::read_dir(subdir_path) {
                for entry in entries.flatten() {
                    let ep = entry.path();
                    if let Ok(m) = fs::symlink_metadata(&ep) {
                        let ft = m.file_type();
                        if ft.is_symlink() {
                            continue;
                        }
                        if ft.is_dir() {
                            child_dir_count += 1;
                        } else if ft.is_file() {
                            child_file_count += 1;
                            let s = m.len();
                            child_direct_size += s;
                            if child_direct_files.len() < 100 {
                                let fn_str = entry.file_name().to_string_lossy().to_string();
                                let ext = ep
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("(no extension)")
                                    .to_lowercase();
                                child_direct_files.push(FileInfo {
                                    path: ep,
                                    name: fn_str,
                                    extension: ext,
                                    size: s,
                                    modified: m.modified().ok(),
                                    is_dir: false,
                                    is_symlink: false,
                                    is_readonly: m.permissions().readonly(),
                                    is_locked: false,
                                    is_inaccessible: false,
                                });
                            }
                        }
                    }
                }
            } else {
                is_inaccessible = true;
            }

            child_direct_files.sort_by(|a, b| b.size.cmp(&a.size));

            FolderTreeNode {
                path: subdir_path.clone(),
                name,
                total_size: child_direct_size,
                immediate_files_size: child_direct_size,
                file_count: child_file_count,
                dir_count: child_dir_count,
                children: Vec::new(),
                direct_files: child_direct_files,
                is_inaccessible,
                is_deeper_scanned: false,
            }
        })
        .collect();

    // STRICT REQUIREMENT 1: "on left tree view folders should be sorted by size"
    children.sort_by(|a, b| b.total_size.cmp(&a.total_size));

    let children_total_size: u64 = children.iter().map(|c| c.total_size).sum();
    let children_file_count: usize = children.iter().map(|c| c.file_count).sum();

    node.total_size = direct_size + children_total_size;
    node.file_count = node.direct_files.len() + children_file_count;
    node.dir_count = children.len();
    node.children = children;
    node.is_deeper_scanned = true;
}

/// Initial root scanner:
/// Scans the root folder and its Level 1 direct subdirectories.
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

    // Scan root and Level 1 children
    scan_deeper_level_for_node(&mut root_node);

    if cancel_token.load(Ordering::Relaxed) {
        return Err("Scan cancelled by user.".to_string());
    }

    let mut min_heap: BinaryHeap<LargestFileItem> = BinaryHeap::new();
    const MAX_LARGEST_FILES: usize = 100;
    let mut ext_map: HashMap<String, (usize, u64)> = HashMap::new();

    // Aggregate files from root
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

    // Aggregate files from Level 1 children
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
