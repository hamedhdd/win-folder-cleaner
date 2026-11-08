use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use crate::core::types::TempLocation;

/// Discovers standard Windows Temp locations including current user, Windows system temp,
/// and all user profile directories on the system drive.
pub fn discover_temp_locations() -> Vec<TempLocation> {
    let mut locations = Vec::new();
    let mut seen_paths = HashSet::new();

    // 1. Windows System Temp (C:\Windows\Temp)
    let win_dir = env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let sys_temp = PathBuf::from(&win_dir).join("Temp");
    add_location_if_new(
        &mut locations,
        &mut seen_paths,
        "win_temp",
        "Windows System Temp",
        sys_temp,
        "System-wide temporary directory used by Windows services and installers",
        true,
    );

    // 2. %TEMP% environment variable
    if let Ok(temp_env) = env::var("TEMP") {
        let temp_path = PathBuf::from(temp_env);
        add_location_if_new(
            &mut locations,
            &mut seen_paths,
            "env_temp",
            "Current User %TEMP%",
            temp_path,
            "Environment variable %TEMP% for current logged-in user",
            false,
        );
    }

    // 3. %LOCALAPPDATA%\Temp
    if let Ok(localappdata) = env::var("LOCALAPPDATA") {
        let local_temp = PathBuf::from(localappdata).join("Temp");
        add_location_if_new(
            &mut locations,
            &mut seen_paths,
            "localappdata_temp",
            "Local AppData Temp",
            local_temp,
            "Local application temporary storage for current user",
            false,
        );
    }

    // 4. All User Profiles' Temp folders under C:\Users\*\AppData\Local\Temp
    let users_dir = Path::new("C:\\Users");
    if users_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(users_dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let profile_name = entry.file_name().to_string_lossy().to_string();
                        // Ignore system / default folders
                        let lower = profile_name.to_lowercase();
                        if lower == "public" || lower == "all users" || lower == "default" || lower == "default user" {
                            continue;
                        }

                        let user_temp = entry.path().join("AppData").join("Local").join("Temp");
                        let location_id = format!("user_{}_temp", profile_name.to_lowercase().replace(' ', "_"));
                        let location_name = format!("User Profile ({}) Temp", profile_name);
                        let desc = format!("Temporary directory for user account '{}'", profile_name);

                        add_location_if_new(
                            &mut locations,
                            &mut seen_paths,
                            &location_id,
                            &location_name,
                            user_temp,
                            &desc,
                            false,
                        );
                    }
                }
            }
        }
    }

    locations
}

fn add_location_if_new(
    locations: &mut Vec<TempLocation>,
    seen_paths: &mut HashSet<String>,
    id: &str,
    name: &str,
    path: PathBuf,
    description: &str,
    is_system: bool,
) {
    let normalized = path
        .canonicalize()
        .unwrap_or_else(|_| path.clone())
        .to_string_lossy()
        .to_ascii_uppercase();

    if seen_paths.insert(normalized) {
        let exists = path.is_dir();
        locations.push(TempLocation {
            id: id.to_string(),
            name: name.to_string(),
            path,
            description: description.to_string(),
            is_system,
            exists,
            selected: exists, // Pre-select existing directories
            summary: None,
            locked_files: Vec::new(),
        });
    }
}
