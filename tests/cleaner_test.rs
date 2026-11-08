use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use win_folder_cleaner::core::cleaner::{clean_directory_contents, CleanExecutionConfig};
use win_folder_cleaner::core::types::AgeFilter;

#[test]
fn test_dry_run_does_not_delete_files() {
    let test_dir = std::env::temp_dir().join("test_win_folder_cleaner_dry_run");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test_dir");

    let file_path = test_dir.join("sample.txt");
    let mut f = File::create(&file_path).expect("create file");
    f.write_all(b"Hello dry run").expect("write bytes");

    let config = CleanExecutionConfig {
        is_dry_run: true,
        force_ownership_and_acl: false,
        age_filter: AgeFilter::All,
        schedule_reboot_if_locked: false,
        delete_only_contents: true,
    };

    let cancel_token = Arc::new(AtomicBool::new(false));
    let report = clean_directory_contents(&test_dir, &config, cancel_token, None);

    assert_eq!(report.deleted_count, 1);
    assert_eq!(report.deleted_bytes, 13);
    assert!(file_path.exists(), "File should still exist in dry-run mode");

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_safe_cleanup_preserves_root_directory() {
    let test_dir = std::env::temp_dir().join("test_win_folder_cleaner_safe_root");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test_dir");

    let sub_folder = test_dir.join("sub_folder");
    fs::create_dir_all(&sub_folder).expect("create sub_folder");

    let file1 = test_dir.join("test1.tmp");
    let file2 = sub_folder.join("test2.tmp");
    File::create(&file1).expect("create file1");
    File::create(&file2).expect("create file2");

    let config = CleanExecutionConfig {
        is_dry_run: false,
        force_ownership_and_acl: false,
        age_filter: AgeFilter::All,
        schedule_reboot_if_locked: false,
        delete_only_contents: true,
    };

    let cancel_token = Arc::new(AtomicBool::new(false));
    let report = clean_directory_contents(&test_dir, &config, cancel_token, None);

    // Root directory must still exist!
    assert!(test_dir.exists(), "Root directory itself must NEVER be deleted");

    // Children must be removed
    assert!(!file1.exists(), "File1 should be deleted");
    assert!(!file2.exists(), "File2 should be deleted");
    assert!(!sub_folder.exists(), "Subfolder should be deleted");
    assert_eq!(report.failed_count, 0);

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_readonly_file_is_removed() {
    let test_dir = std::env::temp_dir().join("test_win_folder_cleaner_readonly");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test_dir");

    let ro_file = test_dir.join("readonly_test.txt");
    {
        let mut f = File::create(&ro_file).expect("create file");
        f.write_all(b"locked by readonly attribute").expect("write file");
        let mut perms = f.metadata().unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro_file, perms).expect("set readonly");
    }

    assert!(ro_file.metadata().unwrap().permissions().readonly());

    let config = CleanExecutionConfig {
        is_dry_run: false,
        force_ownership_and_acl: false,
        age_filter: AgeFilter::All,
        schedule_reboot_if_locked: false,
        delete_only_contents: true,
    };

    let cancel_token = Arc::new(AtomicBool::new(false));
    let report = clean_directory_contents(&test_dir, &config, cancel_token, None);

    assert!(!ro_file.exists(), "Readonly file should be stripped of attribute and deleted");
    assert_eq!(report.failed_count, 0);

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_move_to_recycle_bin() {
    let test_dir = std::env::temp_dir().join("test_win_folder_cleaner_recycle_bin");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test_dir");

    let sample_file = test_dir.join("recycle_me.txt");
    {
        let mut f = File::create(&sample_file).expect("create file");
        f.write_all(b"send to recycle bin").expect("write bytes");
    }
    assert!(sample_file.exists());

    let res = win_folder_cleaner::core::shell::move_to_recycle_bin(&sample_file);
    assert!(res.is_ok(), "move_to_recycle_bin should succeed");
    assert!(!sample_file.exists(), "File should no longer be at original location");

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_move_folder_to_recycle_bin() {
    let test_dir = std::env::temp_dir().join("test_win_folder_cleaner_recycle_folder");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("create test_dir");

    let sub_folder = test_dir.join("sub_folder");
    fs::create_dir_all(&sub_folder).expect("create sub_folder");
    let sample_file = sub_folder.join("nested.txt");
    {
        let mut f = File::create(&sample_file).expect("create file");
        f.write_all(b"nested recycle").expect("write bytes");
    }
    assert!(sub_folder.exists());

    let res = win_folder_cleaner::core::shell::move_to_recycle_bin(&sub_folder);
    assert!(res.is_ok(), "moving folder to recycle bin should succeed");
    assert!(!sub_folder.exists(), "Folder should no longer be at original location");

    let _ = fs::remove_dir_all(&test_dir);
}
