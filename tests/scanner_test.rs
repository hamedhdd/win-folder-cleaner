use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use win_folder_cleaner::core::analyzer::scan_directory_tree;

#[test]
fn test_directory_tree_scanner_accuracy() {
    let temp_root = std::env::temp_dir().join("test_win_folder_cleaner_scan");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).expect("create test root");

    let sub_a = temp_root.join("sub_a");
    let sub_b = temp_root.join("sub_b");
    fs::create_dir_all(&sub_a).expect("create sub_a");
    fs::create_dir_all(&sub_b).expect("create sub_b");

    // Write file 1: 100 bytes (.txt)
    let file1_path = sub_a.join("file1.txt");
    let mut f1 = File::create(&file1_path).expect("create f1");
    f1.write_all(&vec![b'A'; 100]).expect("write f1");

    // Write file 2: 500 bytes (.log)
    let file2_path = sub_a.join("file2.log");
    let mut f2 = File::create(&file2_path).expect("create f2");
    f2.write_all(&vec![b'B'; 500]).expect("write f2");

    // Write file 3: 1000 bytes (.bin)
    let file3_path = sub_b.join("file3.bin");
    let mut f3 = File::create(&file3_path).expect("create f3");
    f3.write_all(&vec![b'C'; 1000]).expect("write f3");

    let cancel_token = Arc::new(AtomicBool::new(false));
    let summary = scan_directory_tree(&temp_root, cancel_token, None)
        .expect("scanner should succeed");

    assert_eq!(summary.total_files, 3);
    assert_eq!(summary.total_dirs, 2);
    assert_eq!(summary.total_bytes, 1600);
    assert_eq!(summary.accessible_bytes, 1600);
    assert_eq!(summary.inaccessible_files, 0);

    // Largest file should be file3.bin (1000 bytes)
    assert!(!summary.largest_files.is_empty());
    assert_eq!(summary.largest_files[0].name, "file3.bin");
    assert_eq!(summary.largest_files[0].size, 1000);

    // Extensions count
    let exts: Vec<String> = summary.extensions.iter().map(|e| e.ext.clone()).collect();
    assert!(exts.contains(&"txt".to_string()));
    assert!(exts.contains(&"log".to_string()));
    assert!(exts.contains(&"bin".to_string()));

    // Verify hierarchical FolderTreeNode structure
    assert!(summary.root_tree.is_some());
    let root_node = summary.root_tree.unwrap();
    assert_eq!(root_node.total_size, 1600);
    assert_eq!(root_node.children.len(), 2);
    // Children sorted descending by total_size: sub_b (1000 bytes) then sub_a (600 bytes)
    assert_eq!(root_node.children[0].name, "sub_b");
    assert_eq!(root_node.children[0].total_size, 1000);
    assert_eq!(root_node.children[1].name, "sub_a");
    assert_eq!(root_node.children[1].total_size, 600);

    // Clean up
    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_scanner_cancellation() {
    let temp_root = std::env::temp_dir().join("test_win_folder_cleaner_cancel");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).expect("create test root");

    let cancel_token = Arc::new(AtomicBool::new(true)); // Pre-cancelled
    let res = scan_directory_tree(&temp_root, cancel_token, None);

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("cancelled"));

    let _ = fs::remove_dir_all(&temp_root);
}
