# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
