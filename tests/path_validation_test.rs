use std::path::Path;
use win_folder_cleaner::core::security::is_critical_system_path;

#[test]
fn test_critical_system_paths_are_protected() {
    assert!(is_critical_system_path(Path::new("C:\\")));
    assert!(is_critical_system_path(Path::new("c:\\windows")));
    assert!(is_critical_system_path(Path::new("C:\\Windows\\System32")));
    assert!(is_critical_system_path(Path::new("C:\\Windows\\SysWOW64")));
    assert!(is_critical_system_path(Path::new("C:\\Program Files")));
    assert!(is_critical_system_path(Path::new("C:\\Program Files (x86)")));
    assert!(is_critical_system_path(Path::new("C:\\ProgramData")));
    assert!(is_critical_system_path(Path::new("C:\\Users")));
    assert!(is_critical_system_path(Path::new("C:\\Users\\Default")));
}

#[test]
fn test_safe_temp_locations_are_not_blocked() {
    // Temp locations themselves should not be blocked from content cleaning
    assert!(!is_critical_system_path(Path::new("C:\\Windows\\Temp\\sample.tmp")));
    assert!(!is_critical_system_path(Path::new("C:\\Users\\Alice\\AppData\\Local\\Temp")));
    assert!(!is_critical_system_path(Path::new("D:\\TestFolder\\Temp")));
}

#[test]
fn test_path_traversal_attempts_are_guarded() {
    // Attempting traversal to reach Windows directory
    let traversal = Path::new("C:\\Windows\\Temp\\..\\..\\Windows\\System32");
    if traversal.exists() {
        assert!(is_critical_system_path(traversal));
    }
}
