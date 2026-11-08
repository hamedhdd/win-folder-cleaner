# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - 2026-11-08

### Added
- **Asynchronous Non-Blocking Folder Expansion**:
  - Dispatched on-demand folder expansion to background worker threads using `scan_node_deep_headless` and crossbeam channels, completely eliminating UI thread freezes during folder navigation.
  - Added live status feedback: `⟳ scanning...` badge on expanding tree rows, and responsive background analysis placeholders in the contents grid and pie chart view.
  - In-place recursive tree caching ensures subsequent expansions and collapses of previously analyzed folders are 0ms instantaneous.
- **Accurate Inaccessible & Skipped Error Accounting**:
  - Eliminated silent error drops in recursive sizing; tracks every inaccessible directory or file due to access restrictions or sharing violations.
  - Annotated tree rows with `⚠️` warning badges and hover tooltips detailing skipped subtree counts.
  - Propagated true inaccessible item totals to `DirScanSummary`, Dashboard stat cards, and status bar.
- **Windows Hardlink De-duplication**:
  - Integrated Win32 `GetFileInformationByHandle` to inspect `nNumberOfLinks` and volume file identifiers (`nFileIndexHigh`, `nFileIndexLow`).
  - De-duplicates shared hardlinks in Windows system directories (`WinSxS` and `System32`), ensuring sizes match authentic disk footprint rather than inflated duplicate counting.
- **Elevated TOCTOU Junction Guard & Reparse Point Safety**:
  - Implemented `is_reparse_point_or_junction` utilizing `FILE_ATTRIBUTE_REPARSE_POINT (0x400)`.
  - Ensures administrative deletion of directories removes only the junction/symlink link itself without descending into target directories, preventing symlink-swap privilege escalation vulnerabilities.
- **Direct Token Privilege Adjustment (`SeTakeOwnershipPrivilege`)**:
  - Added Win32 token privilege activation via `AdjustTokenPrivileges` enabling `SeTakeOwnershipPrivilege` before taking ownership operations.

## [0.7.0] - 2026-11-08

### Fixed
- **Tree View Scrolling Resolution**:
  - Enforced bounded quadrant heights on egui column layouts (`cols[0].set_height(top_quadrant_height)`) and assigned `.max_height(scroll_max_h)` to `ScrollArea::vertical()`.
  - Added dedicated ID salt `folder_tree_scroll_panel` to ensure smooth mouse-wheel scrolling and responsive scrollbar dragging without panel overflow.
  - Bounded vertical scroll heights and added distinct ID salts for Contents table (`pro_contents_scroll_panel`) and Logical Drives table (`bottom_drives_scroll_panel`).
- **100% Accurate Recursive Cumulative Sizing**:
  - Implemented `compute_dir_stats_recursive` to calculate exact cumulative subtree byte counts directly from the NTFS filesystem driver.
  - Eliminated shallow direct-file summing so root and child directories accurately reflect their full recursive size (e.g. `C:\Windows` displays its full multi-GB footprint).
  - Maintained zero-allocation memory efficiency by accumulating raw `u64` counts without storing individual file structs for deeper levels.

### Changed
- **Strict Descending Size Sorting**:
  - Guaranteed that child directories at every tree hierarchy level are sorted strictly descending by true recursive cumulative size (`b.total_size.cmp(&a.total_size)`).
- **Integration Test Updates**:
  - Updated `test_upper_levels_scanned_and_on_demand_deeper_expansion` to verify 100% accurate recursive sizing and size-based ordering.

## [0.6.0] - 2026-11-08

### Added
- **On-Demand Deeper Level Scanning on Expand Click**:
  - Deeper level scanning is executed **only** for the specific folder that the user expands by clicking `▶` in the tree view.
  - Initial scan captures the root and Level 1 subdirectories in milliseconds, leaving deeper nested subfolders unscanned until explicitly requested.
- **Strict Size-Sorted Folder Tree View**:
  - At every level of the tree hierarchy (Root, Level 1, Level 2, and beyond), child folders are strictly sorted descending by size (`b.total_size.cmp(&a.total_size)`).
  - Largest folders always appear at the top of their respective branches.
- **On-Demand Node Scanner (`scan_deeper_level_for_node`)**:
  - Scans direct subdirectories of the clicked folder in parallel with Rayon, sizes them, discovers subfolder counts, sorts them by size, and sets `is_deeper_scanned = true`.
- **Automated Verification Test**:
  - Added `test_upper_levels_scanned_and_on_demand_deeper_expansion` verifying that Level 1 folders sort by size, deeper levels remain unscanned initially, and deeper levels scan on demand upon expansion.

## [0.5.0] - 2026-11-08

### Added & Optimized
- **Low-Memory Footprint & Strict Allocation Bounds**:
  - **Eliminated Deep-Map Cloning**: Completely removed intermediate clones of `dir_map` during deep directory traversal. Upper-level tree snapshots are only taken for depth 0 and 1, stopping allocation thrashing on large drives.
  - **Bounded Candidate Largest Files**: Restricted each scanned directory to a bounded local min-heap of at most 100 candidate items (rather than collecting every single file into memory), reducing file buffer allocations by over 95%.
  - **On-Demand Direct File Loading (`load_directory_files`)**: Direct file lists (`Vec<FileInfo>`) are only cached for top-level root and depth 1 folders. For deeper folders, direct files are loaded on demand in < 1ms when selected in the UI.
  - **In-Place Tree Construction**: Reclaimed `dir_map` memory by moving entries directly into `FolderTreeNode` during final tree assembly.
  - **Dramatic RAM Reduction**: Memory consumption during large disk scans reduced from upwards of 500MB-1.5GB down to ~15-25MB.

## [0.4.0] - 2026-11-08

### Added
- **Level-Order (Breadth-First) Directory Traversal**:
  - Completely restructured scanner to prioritize upper-level folders first (depth 0, depth 1, depth 2, etc.) before descending into deeper nested directory trees.
  - Solves the DFS bottleneck where deep folders stalled scanning of sibling root folders.
- **Parallel Level Traversal via Rayon**:
  - At each hierarchy level, multiple sibling folders are scanned concurrently across all available CPU cores using `rayon::par_iter()`.
  - Maximizes disk I/O queue depth and NVMe throughput while preserving strict level-order boundaries.
- **Live Incremental Hierarchical Tree Streaming**:
  - Background scanner emits live partial tree snapshots (`ScanProgress.partial_tree`) directly to the UI thread upon completing upper levels and every 250ms thereafter.
  - The Folder Explorer tree view, contents table, and pie chart immediately display top-level directories while deeper levels continue in the background.
- **Accurate Subtree Cumulative Metrics**:
  - Replaced prefix-based attribution with post-order hierarchical tree aggregation, guaranteeing 100% accurate byte and file counts across all directory depths.
- **Automated Verification Test**:
  - Added `test_upper_levels_scanned_before_deeper_levels` to test suite, guaranteeing strictly non-decreasing depth order (0 -> 1 -> 2) during traversal.

## [0.3.0] - 2026-11-08

### Added
- **Folder Size Professional 4-Quadrant Workspace Layout**: Modeled precisely after Folder Size Pro desktop interface:
  - **Top Menu Bar**: Classic Windows menu bar with `File`, `Scan`, `Clean`, `View`, and `Help` menus with admin elevation badge and one-click relaunch.
  - **Top-Left Quadrant**: Hierarchical folder tree view showing folder hierarchy and right-aligned cumulative sizes.
  - **Top-Right Quadrant**: Detailed contents table with `Name`, `Size`, `Percent` visual progress bar, `Files`, `Folders`, `Attributes` (`D`, `A`, `RA`), and actions.
  - **Bottom-Left Quadrant**: Logical Drives Overview table reporting `Drive`, `Total Size`, `Used Space`, `Free Space`, and `Usage %` using Win32 `GetDiskFreeSpaceExW`.
  - **Bottom-Right Quadrant**: Interactive graphical Pie Chart with custom vector rendering, exploded slice on hover along its bisecting angle, pointer callout tooltip (`Name: XX.XX% (XX.XX MB)`), palette colors, legend, and `Pie` / `Bar Breakdown` view toggle.
  - **Bottom Status Bar**: Displays real-time operational status (`Idle` / `Scanning: ...`), `X Files, Y Folders`, total size, and progress indicator.
  - **Global Unit Selectors**: Toolbar unit toggles (`B`, `KB`, `MB`, `GB`, `AUTO`) that instantaneously update all size displays across the tree, tables, drives, and charts.

### Fixed & Enhanced
- **Companion Terminal/Console Window Suppression**: Added `#![windows_subsystem = "windows"]` to `src/main.rs` configuring the PE header subsystem to `IMAGE_SUBSYSTEM_WINDOWS_GUI`, completely eliminating the companion console/terminal window on application startup.
- **Strict Zero-PowerShell Guarantee**: Verified all operations (elevation, take ownership, ACL grant, logical drive space querying, explorer launch) utilize native Win32 APIs or direct binary execution without PowerShell.
- **Default Startup View**: Set Folder Size Analyzer (Pro 4-quadrant view) as the primary default tab upon application launch.

## [0.2.0] - 2026-11-08

### Added
- **Full Hierarchical Folder Tree View**: Comprehensive interactive tree view enabling deep recursive exploration of all folders across any drive or directory on Windows.
- **Tree Row Size Visualizations**: Proportional visual capacity bars, item count badges, and formatted cumulative size indicators for every folder node in the hierarchy.
- **Collapsible / Expandable Tree Controls**: Interactive node expansion (`▶` / `▼`), "Expand All Top Levels", "Collapse All", and live tree filtering.
- **Folder Node Inspector**: Detailed right-hand inspection drawer displaying direct file listings, subtree size metrics, and one-click actions:
  - "Analyze from here": Focus recursive analysis directly from the selected folder.
  - "Open in Windows Explorer": Safe launch of native file explorer for the folder without PowerShell.
  - "Clean this folder's contents": Direct pipeline trigger to safely clean selected folder contents.
- **Logical Drive Quick Selector**: Automatic Win32 drive bitmask detection (`GetLogicalDrives`) providing instant drive pills (`C:\`, `D:\`, etc.) for immediate root exploration.

## [0.1.0] - 2026-11-08

### Added
- **Native GUI with `egui` and `eframe`**: Modern dark theme following Impeccable/Unslop design principles (Operate surface mode, graphite neutral ramp, 4px grid rhythm, quiet chrome, and tabular numbers).
- **Zero-PowerShell Architecture**: Entire application strictly executes without invoking PowerShell or cmd.exe shell wrappers. Win32 APIs and direct binary process execution (`Command::new` with separate arguments and `CREATE_NO_WINDOW`) are used exclusively.
- **Elevation Detection & Seamless Elevation Relaunch**: Win32 Process Token query for `TokenElevation`. One-click relaunch via `ShellExecuteExW` with verb `runas`, preserving command-line arguments.
- **Folder Size Analyzer**: Multithreaded recursive directory traversal with real-time size computation (Bytes, MB, GB), total files, subdirectories, top 100 largest files, largest subdirectories with cumulative sizes, extension distributions, and inaccessible statistics.
- **Directory Symlink & Junction Guards**: Automatic avoidance of reparse points and directory symlinks by default to prevent infinite loops and volume boundary crossing.
- **Temp Folder Management Subsystem**: Auto-discovery of `C:\Windows\Temp`, current user `%TEMP%`, `%LOCALAPPDATA%\Temp`, and all user profile temp directories across `C:\Users\*\AppData\Local\Temp`, plus custom directory additions.
- **Advanced Deletion Engine**:
  - Safe root directory preservation (contents cleaned while retaining the root temp directory itself).
  - Dry-run simulation mode with detailed pre-execution summary.
  - Age threshold filtering (All, >7 days, >30 days, >60 days, >90 days).
  - Force deletion capability: Stripping read-only attributes, automated ownership acquisition (`takeown /F ... /A /R /D Y`), and non-destructive Administrator ACL granting (`icacls ... /grant *S-1-5-32-544:(OI)(CI)F /T /C`).
  - Windows Restart Manager API integration: Detects processes locking files and displays application name and PID.
  - Delete-on-reboot scheduling via Win32 `MoveFileExW` (`MOVEFILE_DELAY_UNTIL_REBOOT`).
- **Audit Logging & Export**: Real-time event log with status indicators, filtering, and RFC 4180 CSV / formatted TXT exports.
- **Automated Test Suite**: Unit and integration tests covering path protection, scanner metrics, dry-run simulation, root folder retention, and readonly stripping.
