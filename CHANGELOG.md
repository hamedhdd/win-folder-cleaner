# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
