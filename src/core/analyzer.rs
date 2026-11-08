use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
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

#[derive(Clone)]
struct DirNodeData {
    path: PathBuf,
    name: String,
    direct_size: u64,
    direct_file_count: usize,
    direct_files: Vec<FileInfo>,
    child_subdirs: Vec<PathBuf>,
    is_inaccessible: bool,
}

/// Output data from scanning a single directory.
/// Keeps memory minimal by bounding candidate largest files to 100 per folder
/// and only caching direct files for top-level folders (depth <= 1).
struct SingleDirScanOutput {
    path: PathBuf,
    name: String,
    direct_size: u64,
    file_count: usize,
    direct_files: Vec<FileInfo>,
    subdirs: Vec<PathBuf>,
    inaccessible_count: usize,
    is_inaccessible: bool,
    ext_stats: HashMap<String, (usize, u64)>,
    largest_files: Vec<FileInfo>,
}

/// Scans a single directory without descending into child subdirectories.
/// Child directories are collected and returned so they can be processed level-by-level.
fn scan_single_directory(
    dir_path: &Path,
    depth: usize,
    cancel_token: &Arc<AtomicBool>,
) -> SingleDirScanOutput {
    let dir_name = dir_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir_path.to_string_lossy().to_string());

    let mut output = SingleDirScanOutput {
        path: dir_path.to_path_buf(),
        name: dir_name,
        direct_size: 0,
        file_count: 0,
        direct_files: Vec::new(),
        subdirs: Vec::new(),
        inaccessible_count: 0,
        is_inaccessible: false,
        ext_stats: HashMap::new(),
        largest_files: Vec::new(),
    };

    let entries = match fs::read_dir(dir_path) {
        Ok(iter) => iter,
        Err(_) => {
            output.inaccessible_count = 1;
            output.is_inaccessible = true;
            return output;
        }
    };

    // Bounded local min-heap for top candidate files in this folder (max 100)
    let mut local_largest: BinaryHeap<LargestFileItem> = BinaryHeap::new();
    const MAX_LOCAL_LARGEST: usize = 100;

    for entry in entries {
        if cancel_token.load(Ordering::Relaxed) {
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                output.inaccessible_count += 1;
                continue;
            }
        };

        let entry_path = entry.path();

        let metadata = match fs::symlink_metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => {
                output.inaccessible_count += 1;
                continue;
            }
        };

        let file_type = metadata.file_type();

        // DO NOT follow symlinks or junctions/reparse points to avoid infinite loops and boundary crossing
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            output.subdirs.push(entry_path);
        } else if file_type.is_file() {
            output.file_count += 1;
            let size = metadata.len();
            output.direct_size += size;

            let name = entry.file_name().to_string_lossy().to_string();
            let ext = entry_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("(no extension)")
                .to_lowercase();

            let modified = metadata.modified().ok();
            let readonly = metadata.permissions().readonly();

            let file_info = FileInfo {
                path: entry_path,
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

            // Memory optimization: Store immediate files only for top-level folders (depth <= 1).
            // Deeper folders load files on demand in < 1ms when selected in the UI.
            if depth <= 1 && output.direct_files.len() < 100 {
                output.direct_files.push(file_info.clone());
            }

            // Memory optimization: Bound candidate largest files to top 100 per directory
            if local_largest.len() < MAX_LOCAL_LARGEST {
                local_largest.push(LargestFileItem(file_info));
            } else if let Some(smallest) = local_largest.peek() {
                if size > smallest.0.size {
                    local_largest.pop();
                    local_largest.push(LargestFileItem(file_info));
                }
            }

            let stat = output.ext_stats.entry(ext).or_insert((0, 0));
            stat.0 += 1;
            stat.1 += size;
        }
    }

    output.largest_files = local_largest.into_iter().map(|item| item.0).collect();
    output
}

/// Reads the immediate files of a single directory on demand (in < 1ms)
/// so that deep directories don't have to cache millions of files in memory.
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
    // Sort descending by size
    files.sort_by(|a, b| b.size.cmp(&a.size));
    files
}

/// High-performance Breadth-First (Level-Order) directory scanner with strict memory bounds.
///
/// Scans upper-level folders first before descending into deeper levels.
/// At each level, multiple sibling folders are scanned concurrently across CPU cores using Rayon.
///
/// Memory optimizations:
/// 1. Candidate largest files bounded to top 100 per folder.
/// 2. Direct files only cached for depth <= 1 (deeper loaded on demand).
/// 3. Zero cloning of dir_map on deep levels (eliminating massive allocation thrashing).
/// 4. Drains dir_map into root_tree on completion.
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
    let mut dir_map: HashMap<PathBuf, DirNodeData> = HashMap::new();

    // Level-by-level breadth-first traversal: upper levels scanned first, then deeper levels
    let mut current_level: Vec<PathBuf> = vec![root_path.clone()];
    let mut current_depth: usize = 0;
    let mut last_progress_time = Instant::now();

    while !current_level.is_empty() {
        if cancel_token.load(Ordering::Relaxed) {
            if let Some(tx) = &progress_tx {
                let _ = tx.send(ScanProgress {
                    current_path: "Scan cancelled".to_string(),
                    files_scanned: total_files,
                    dirs_scanned: total_dirs,
                    bytes_scanned: total_bytes,
                    inaccessible_count: inaccessible_files,
                    is_completed: false,
                    cancelled: true,
                    error: None,
                    current_depth,
                    partial_tree: None,
                });
            }
            return Err("Scan cancelled by user.".to_string());
        }

        // Process all directories at current_depth.
        // When there are multiple folders at this depth, scan them concurrently with Rayon.
        let results: Vec<SingleDirScanOutput> = if current_level.len() > 1 {
            current_level
                .par_iter()
                .map(|path| scan_single_directory(path, current_depth, &cancel_token))
                .collect()
        } else {
            current_level
                .iter()
                .map(|path| scan_single_directory(path, current_depth, &cancel_token))
                .collect()
        };

        if cancel_token.load(Ordering::Relaxed) {
            return Err("Scan cancelled by user.".to_string());
        }

        let mut next_level: Vec<PathBuf> = Vec::new();

        // Aggregate results from this level
        for res in results {
            total_files += res.file_count;
            total_dirs += res.subdirs.len();
            total_bytes += res.direct_size;
            accessible_bytes += res.direct_size;
            inaccessible_files += res.inaccessible_count;

            for (ext, (cnt, bts)) in res.ext_stats {
                let stat = ext_map.entry(ext).or_insert((0, 0));
                stat.0 += cnt;
                stat.1 += bts;
            }

            for file in res.largest_files {
                if min_heap.len() < MAX_LARGEST_FILES {
                    min_heap.push(LargestFileItem(file));
                } else if let Some(smallest) = min_heap.peek() {
                    if file.size > smallest.0.size {
                        min_heap.pop();
                        min_heap.push(LargestFileItem(file));
                    }
                }
            }

            next_level.extend(res.subdirs.clone());

            dir_map.insert(
                res.path.clone(),
                DirNodeData {
                    path: res.path,
                    name: res.name,
                    direct_size: res.direct_size,
                    direct_file_count: res.file_count,
                    direct_files: res.direct_files,
                    child_subdirs: res.subdirs,
                    is_inaccessible: res.is_inaccessible,
                },
            );
        }

        // Memory optimization:
        // Only emit partial_tree for depth 0 and depth 1 (where dir_map has only 1..30 items).
        // On deeper levels, do NOT clone dir_map!
        let partial_tree = if current_depth <= 1 && progress_tx.is_some() {
            let mut snapshot_map = dir_map.clone();
            build_folder_tree(&root_path, &mut snapshot_map)
        } else {
            None
        };

        // Throttle progress updates to ~10 times per second or on initial upper levels
        if last_progress_time.elapsed() >= Duration::from_millis(100) || partial_tree.is_some() {
            last_progress_time = Instant::now();
            if let Some(tx) = &progress_tx {
                let status_msg = if current_depth == 0 {
                    format!("Scanning root level: {}", root_path.to_string_lossy())
                } else {
                    format!("Level {} ({} queued folders)", current_depth, next_level.len())
                };

                let _ = tx.send(ScanProgress {
                    current_path: status_msg,
                    files_scanned: total_files,
                    dirs_scanned: total_dirs,
                    bytes_scanned: total_bytes,
                    inaccessible_count: inaccessible_files,
                    is_completed: false,
                    cancelled: false,
                    error: None,
                    current_depth,
                    partial_tree,
                });
            }
        }

        current_level = next_level;
        current_depth += 1;
    }

    // Finalize largest files list in descending order
    let mut largest_files: Vec<FileInfo> = min_heap.into_iter().map(|item| item.0).collect();
    largest_files.sort_by(|a, b| b.size.cmp(&a.size));

    // Finalize extensions distribution
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

    // Assemble final full hierarchical folder tree, draining dir_map to free memory
    let root_tree = build_folder_tree(&root_path, &mut dir_map);

    // Finalize largest direct subdirectories from the root tree children
    let mut largest_subdirs: Vec<DirEntrySummary> = Vec::new();
    if let Some(tree) = &root_tree {
        for child in &tree.children {
            largest_subdirs.push(DirEntrySummary {
                path: child.path.clone(),
                name: child.name.clone(),
                total_size: child.total_size,
                immediate_size: child.immediate_files_size,
                file_count: child.file_count,
                dir_count: child.dir_count,
                has_subdirs: !child.children.is_empty(),
            });
        }
    }
    largest_subdirs.sort_by(|a, b| b.total_size.cmp(&a.total_size));

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
        root_tree: root_tree.clone(),
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
            current_depth,
            partial_tree: root_tree,
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
    let mut file_count = data.direct_file_count;
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
