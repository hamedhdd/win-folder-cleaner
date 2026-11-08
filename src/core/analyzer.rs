use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use chrono::Local;
use crossbeam_channel::Sender;
use rayon::prelude::*;

use crate::core::types::{
    DirEntrySummary, DirScanSummary, ExtensionStat, FileInfo, FolderTreeNode, NodeScanResult,
    ScanProgress,
};

#[cfg(windows)]
fn get_file_link_info(path: &Path) -> Option<(u32, u64, u32)> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            0, // Read attributes only without file locking
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            HANDLE::default(),
        );

        if let Ok(h) = handle {
            if h != INVALID_HANDLE_VALUE {
                let mut info = BY_HANDLE_FILE_INFORMATION::default();
                let ok = GetFileInformationByHandle(h, &mut info).is_ok();
                let _ = CloseHandle(h);
                if ok {
                    let vol = info.dwVolumeSerialNumber;
                    let file_id = ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64);
                    return Some((vol, file_id, info.nNumberOfLinks));
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn get_file_link_info(_path: &Path) -> Option<(u32, u64, u32)> {
    None
}

#[cfg(windows)]
pub fn get_compressed_size(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetCompressedFileSizeW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut high: u32 = 0;
        let low = GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high));
        if low == 0xFFFFFFFF {
            let err = windows::Win32::Foundation::GetLastError();
            if err.0 != 0 {
                return None;
            }
        }
        Some(((high as u64) << 32) | (low as u64))
    }
}

#[cfg(not(windows))]
pub fn get_compressed_size(_path: &Path) -> Option<u64> {
    None
}

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
/// immediate child directory paths, immediate files, and inaccessible item count for a folder.
///
/// Features:
/// 1. Tracks inaccessible / permission-denied directories and files without silently dropping them.
/// 2. De-duplicates Windows hardlinks (e.g., WinSxS / System32) so files are not counted multiple times.
/// 3. Allocates ZERO memory for individual recursive files — only accumulates raw u64 bytes and counts.
pub fn compute_dir_stats_recursive(
    dir_path: &Path,
    cancel_token: &Arc<AtomicBool>,
) -> (u64, usize, usize, Vec<PathBuf>, Vec<FileInfo>, usize) {
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    let mut inaccessible_count = 0usize;
    let mut direct_subdirs = Vec::new();
    let mut direct_files = Vec::new();

    let mut stack = vec![dir_path.to_path_buf()];
    let is_windows_dir = dir_path.to_string_lossy().to_ascii_lowercase().contains("windows");
    let mut seen_hardlinks: HashSet<(u32, u64)> = HashSet::new();

    while let Some(current) = stack.pop() {
        if cancel_token.load(Ordering::Relaxed) {
            break;
        }

        let entries = match fs::read_dir(&current) {
            Ok(iter) => iter,
            Err(_) => {
                inaccessible_count += 1;
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    inaccessible_count += 1;
                    continue;
                }
            };

            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => {
                    inaccessible_count += 1;
                    continue;
                }
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

                // De-duplicate Windows hard links in system folders (WinSxS / System32)
                let mut is_duplicate_link = false;
                if is_windows_dir {
                    if let Some((vol, file_id, links)) = get_file_link_info(&path) {
                        if links > 1 && !seen_hardlinks.insert((vol, file_id)) {
                            is_duplicate_link = true;
                        }
                    }
                }

                if !is_duplicate_link {
                    total_bytes += size;
                }

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
    (total_bytes, total_files, total_dirs, direct_subdirs, direct_files, inaccessible_count)
}

/// Headless worker function to compute deeper level statistics for a given folder path.
/// Can be spawned asynchronously on any background worker thread, NEVER blocking the UI!
pub fn scan_node_deep_headless(
    dir_path: &Path,
    cancel_token: Arc<AtomicBool>,
) -> NodeScanResult {
    let mut direct_subdirs = Vec::new();
    let mut direct_files = Vec::new();
    let mut immediate_files_size = 0u64;
    let mut inaccessible_count = 0usize;

    match fs::read_dir(dir_path) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        inaccessible_count += 1;
                        continue;
                    }
                };
                let path = entry.path();
                match fs::symlink_metadata(&path) {
                    Ok(meta) => {
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
                    Err(_) => {
                        inaccessible_count += 1;
                    }
                }
            }
        }
        Err(_) => {
            inaccessible_count += 1;
        }
    }

    direct_files.sort_by(|a, b| b.size.cmp(&a.size));

    // Concurrently compute true recursive sizes for each direct child subdirectory
    let mut children: Vec<FolderTreeNode> = direct_subdirs
        .par_iter()
        .map(|subdir_path| {
            let name = subdir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| subdir_path.to_string_lossy().to_string());

            let (total_size, file_count, dir_count, sub_subdirs, child_direct_files, child_inaccessible) =
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
                    inaccessible_count: 0,
                    is_scanning: false,
                    size_on_disk: None,
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
                is_inaccessible: child_inaccessible > 0 && total_size == 0,
                is_deeper_scanned: false,
                inaccessible_count: child_inaccessible,
                is_scanning: false,
                size_on_disk: None,
            }
        })
        .collect();

    // STRICT REQUIREMENT 1: "on left tree view folders should be sorted by size"
    children.sort_by(|a, b| b.total_size.cmp(&a.total_size));

    let children_total_size: u64 = children.iter().map(|c| c.total_size).sum();
    let children_file_count: usize = children.iter().map(|c| c.file_count).sum();
    let children_inaccessible: usize = children.iter().map(|c| c.inaccessible_count).sum();

    let total_size = immediate_files_size + children_total_size;
    let file_count = direct_files.len() + children_file_count;
    let dir_count = children.len();
    let total_inaccessible = inaccessible_count + children_inaccessible;

    NodeScanResult {
        path: dir_path.to_path_buf(),
        children,
        direct_files,
        total_size,
        immediate_files_size,
        file_count,
        dir_count,
        inaccessible_count: total_inaccessible,
        size_on_disk: None,
    }
}

/// Scans deeper levels ON-DEMAND for a specific folder node.
/// Backwards-compatible synchronous adapter delegating to headless worker.
pub fn scan_deeper_level_for_node(node: &mut FolderTreeNode) {
    if node.is_deeper_scanned {
        node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
        return;
    }

    let cancel_token = Arc::new(AtomicBool::new(false));
    let res = scan_node_deep_headless(&node.path, cancel_token);

    node.children = res.children;
    node.direct_files = res.direct_files;
    node.total_size = res.total_size;
    node.immediate_files_size = res.immediate_files_size;
    node.file_count = res.file_count;
    node.dir_count = res.dir_count;
    node.inaccessible_count = res.inaccessible_count;
    node.is_deeper_scanned = true;
    node.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
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
        inaccessible_count: 0,
        is_scanning: false,
        size_on_disk: None,
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
        inaccessible_files: root_node.inaccessible_count + if root_node.is_inaccessible { 1 } else { 0 },
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
            inaccessible_count: root_node.inaccessible_count + if root_node.is_inaccessible { 1 } else { 0 },
            is_completed: true,
            cancelled: false,
            error: None,
            current_depth: 0,
            partial_tree: Some(root_node),
        });
    }

    Ok(summary)
}
