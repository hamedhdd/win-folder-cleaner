# Windows Folder Analyzer & Advanced Cleaner Pro

A high-performance, native Windows desktop application written in **Rust** for recursive storage diagnostics, file size analysis, process lock detection, and administrative cleanup operations.

Built with **`egui` / `eframe`** and direct **Windows APIs** (`windows` crate), with **zero PowerShell dependencies**.

---

## Key Features

1. **Folder Size Professional 4-Quadrant Workspace Layout**:
   - **Top Menu Bar**: Classic Windows menu bar with `File`, `Scan`, `Clean`, `View`, and `Help` menus with admin elevation badge and one-click relaunch.
   - **Top-Left Quadrant**: Hierarchical folder tree view showing folder hierarchy and right-aligned cumulative sizes.
   - **Top-Right Quadrant**: Detailed contents table with `Name`, `Size`, `Percent` visual progress bar, `Files`, `Folders`, `Attributes` (`D`, `A`, `RA`), and actions.
   - **Bottom-Left Quadrant**: Logical Drives Overview table reporting `Drive`, `Total Size`, `Used Space`, `Free Space`, and `Usage %` using Win32 `GetDiskFreeSpaceExW`.
   - **Bottom-Right Quadrant**: Interactive graphical Pie Chart with custom vector rendering, exploded slice on hover along its bisecting angle, pointer callout tooltip (`Name: XX.XX% (XX.XX MB)`), palette colors, legend, and `Pie` / `Bar Breakdown` view toggle.
   - **Bottom Status Bar**: Displays real-time operational status (`Idle` / `Scanning: ...`), `X Files, Y Folders`, total size, and progress indicator.
   - **Global Unit Selectors**: Toolbar unit toggles (`B`, `KB`, `MB`, `GB`, `AUTO`) that instantaneously update all size displays across the tree, tables, drives, and charts.

2. **High-Performance Level-Order & On-Demand Folder Analyzer**:
   - **Zero UI Freezes (Asynchronous Expansion)**: Deeper level folder scans run exclusively on background worker threads (`scan_node_deep_headless`). The GUI maintains a solid 60 FPS without stuttering when expanding multi-gigabyte directories.
   - **Smooth Tree View Scrolling**: Constrained quadrant bounds and isolated scroll panels with dedicated ID salts allow seamless vertical mouse wheel scrolling and scrollbar dragging across thousands of folders.
   - **100% Accurate Recursive Cumulative Sizing**: Rapidly traverses subtrees directly from the NTFS driver with zero memory overhead, guaranteeing that root folders (like `C:\Windows`) and subfolders reflect their exact, true cumulative footprint.
   - **Windows Hardlink De-duplication**: Utilizes Win32 `BY_HANDLE_FILE_INFORMATION` to prevent duplicate byte counting for shared hard links in system directories (`WinSxS` and `System32`).
   - **Inaccessible & Skipped Items Accounting**: Counts and reports all directories or files skipped due to access restrictions, displaying quiet status badges (`[skipped]`) with item count tooltips.
   - **Strict Size Sorting**: All folders in the left tree view are strictly sorted descending by size at every depth level, placing the largest directories at the top.
   - **On-Demand Deeper Scanning on Click**: Initial scans calculate the root and Level 1 folders. Deeper levels are only scanned for the specific folder that the user expands by clicking `▶`, keeping scans fast and memory consumption minimal.
   - **Low-Memory Footprint & Strict Bounding**: Memory consumption stays within ~10-20MB even on huge drives. Eliminates map cloning, bounds candidate largest files to 100 items per folder, and loads deep directory files on demand.
   - **On-Demand Direct File Loading**: High-level folders cache direct files upfront; deeper nested folders read their direct files in < 1ms on demand when selected in the Explorer view.
   - **Multithreaded Rayon Parallelism**: Sibling folders at each depth level are processed concurrently across all CPU cores via `rayon::par_iter()`, maximizing I/O queue depth on modern NVMe and SSD drives.
   - **Live Tree Streaming to UI**: Emits real-time partial tree snapshots (`partial_tree`) as upper levels finish scanning, so the tree view, table, and pie chart populate immediately while deep background scanning continues.
   - **Hierarchical Folder Tree View**: Recursively analyzes and models every folder on any selected drive or directory into an interactive, expandable tree hierarchy (`▶` / `▼`).
   - **Proportional Size Bars**: Visual capacity bar for each directory node showing percentage allocation relative to parent or root.
   - **Detailed Folder Inspector**: Click any folder in the tree to inspect its immediate files, total subtree size, file counts, and trigger focused analysis or content cleanup.
   - **Logical Drives Quick Bar**: Automatically detects available Windows drives (`C:\`, `D:\`, etc.) via Win32 `GetLogicalDrives` and `GetDiskFreeSpaceExW` for instant root switching.
   - Recursive directory traversal calculating total size (Bytes, KB, MB, GB, TB).
   - Counts files, subdirectories, and inaccessible / access-denied entries.
   - Identifies Top 100 largest files and largest subdirectories with cumulative size.
   - Groups files by extension with percentage share and visual proportion bars.
   - Interactive sortable columns (Name, Size, Extension, Last Modified).
   - Does not traverse symlinks or directory junctions by default.
   - Non-blocking background worker with cancellation support and real-time throttled UI updates.

3. **Subsystem & Console Window Suppression**:
   - Configured with `#![windows_subsystem = "windows"]` in `src/main.rs`.
   - Configures the PE binary header to `IMAGE_SUBSYSTEM_WINDOWS_GUI`, completely eliminating the companion terminal/console window on startup.

4. **Privilege & Elevation Management**:
   - Win32 Process Token Elevation detection (`OpenProcessToken` + `GetTokenInformation`).
   - One-click "Relaunch as Administrator" button using `ShellExecuteExW` with `runas`, preserving CLI arguments.
   - Prominent privilege badge in UI top navigation bar.

3. **Predefined & Custom Temp Location Management**:
   - Automatically detects and populates:
     - `C:\Windows\Temp` (System Temp)
     - `%TEMP%` (Current user temp environment variable)
     - `%LOCALAPPDATA%\Temp`
     - `C:\Users\<username>\AppData\Local\Temp` for all user profiles on the system drive.
   - Support for custom user-added target directories via native folder picker (`rfd`).
   - Age filter presets: All files, >7 days, >30 days, >60 days, >90 days.

4. **Advanced Deletion Engine & Safety Controls**:
   - **Safe Root Preservation**: When cleaning Temp folders, contents are deleted while strictly preserving the root directory.
   - **Critical Path Protection**: Guards against deletion of `C:\`, `C:\Windows`, `C:\Windows\System32`, `C:\Program Files`, `C:\ProgramData`, and user profile roots.
   - **Dry Run Simulation**: Accurately simulates the cleanup operation, reporting exact item count and bytes that would be deleted without modifying the filesystem.
   - **Read-Only Attribute Clearing**: Automatically removes read-only attributes before deletion.
   - **Ownership Management**: Acquires administrative ownership (`takeown.exe /F <path> /A /R /D Y`) without touching unrelated parents.
   - **Permission Management**: Grants Administrators group SID (`*S-1-5-32-544:(OI)(CI)F`) Full Control without resetting existing ACLs.
   - **Process Lock Detection (Restart Manager API)**: Uses Win32 Restart Manager (`RmStartSession`, `RmRegisterResources`, `RmGetList`) to pinpoint active processes (e.g. `chrome.exe`, `explorer.exe`) locking files.
   - **Delete-on-Reboot (`MoveFileExW`)**: Schedules stubborn or locked files for deletion during the next Windows boot cycle using `MOVEFILE_DELAY_UNTIL_REBOOT`.

5. **Operational Audit Log & Export**:
   - Timestamped record of every action (Safety Check, Scan, Delete, Dry Run, Take Ownership, Permission Fix, Reboot Schedule).
   - Search filter and status filtering (Success, Warning, Error, Dry Run, Skipped).
   - One-click export to RFC 4180 CSV or formatted TXT.

6. **Zero PowerShell Footprint**:
   - Does not invoke PowerShell or cmd.exe shell wrappers.
   - Direct Win32 APIs and isolated `std::process::Command` calls with separate parameters and hidden console flags (`CREATE_NO_WINDOW = 0x08000000`).

---

## Architecture & SOLID Principles

- **SRP (Single Responsibility)**: Modules cleanly divided into domain models (`types.rs`), directory analyzer (`analyzer.rs`), deletion engine (`cleaner.rs`), security & permissions (`security.rs`), Restart Manager (`restart_mgr.rs`), reboot queue (`reboot_del.rs`), and UI views (`ui/*`).
- **OCP (Open/Closed)**: Extensible age filters, sort columns, and modular location providers without altering core traversal logic.
- **LSP & ISP**: Focused interface contracts and decoupled data flow via `crossbeam-channel` message queues.
- **DIP (Dependency Inversion)**: UI components depend on abstract channels and domain structures, entirely decoupled from low-level Win32 syscalls.

---

## Prerequisites

- **OS**: Windows 10 (x64) or Windows 11 (x64).
- **Rust Toolchain**: Rust 1.80+ (stable). Verify with:
  ```pwsh
  rustc --version
  cargo --version
  ```

---

## Build Instructions

### 1. Build and Run in Development Mode
```pwsh
cargo run
```

### 2. Run All Automated Tests
```pwsh
cargo test
```

### 3. Build Optimized Release Standalone Executable
```pwsh
cargo build --release
```
The resulting standalone executable is located at:
```text
target\release\win-folder-cleaner.exe
```

---

## Usage Guide

1. **Launch the Application**: Run `win-folder-cleaner.exe`. If launched as a standard user, click the "Relaunch Elevated" button in the top navigation bar to access administrative operations.
2. **Dashboard**: View high-level metrics across all analyzed locations, privilege level, and quick action shortcuts.
3. **Folder Explorer**:
   - Type or browse for any target folder on any connected drive.
   - Click "Analyze Folder" to launch the background scanner.
   - Switch between **Largest Files**, **Largest Subdirectories**, and **File Extensions Distribution**.
   - Click column headers ("Name", "Size", "Extension", "Last Modified") to sort in ascending or descending order.
   - Click "Inspect" next to any file to query locking process handles via the Windows Restart Manager.
4. **Temp Cleaner**:
   - Check or uncheck locations (e.g. `C:\Windows\Temp`, User Temp folders, or custom added folders).
   - Select an age threshold from the dropdown (e.g., "Older than 30 days").
   - Click "Dry Run Simulation" to preview exactly which files and bytes will be cleared.
   - Click "Clean Eligible Contents" to execute safe deletion.
   - Use "Take Ownership" or "Fix Permissions" if files have corrupted ACLs or ownership issues.
5. **Operation Log**:
   - Search operations by path or error description.
   - Filter by status (Errors only, Warnings only, Successes).
   - Click "Export to CSV" or "Export to TXT" to generate audit compliance reports.
