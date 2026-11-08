use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use win_folder_cleaner::core::analyzer::{scan_deeper_level_for_node, scan_directory_tree};

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

#[test]
fn test_upper_levels_scanned_and_on_demand_deeper_expansion() {
    let temp_root = std::env::temp_dir().join("test_win_folder_cleaner_levels");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).expect("create test root");

    // Level 0: root contains level1_a and level1_b
    let level1_a = temp_root.join("level1_a");
    let level1_b = temp_root.join("level1_b");
    fs::create_dir_all(&level1_a).expect("create level1_a");
    fs::create_dir_all(&level1_b).expect("create level1_b");

    // Level 1: level1_a contains level2_nested
    let level2_nested = level1_a.join("level2_nested");
    fs::create_dir_all(&level2_nested).expect("create level2_nested");

    // Level 2: level2_nested contains deep file (250 bytes)
    let deep_file = level2_nested.join("deep.txt");
    let mut f_deep = File::create(&deep_file).expect("create deep file");
    f_deep.write_all(&vec![b'D'; 250]).expect("write deep file");
    drop(f_deep);

    // Level 1: level1_b contains shallow file (100 bytes)
    let shallow_file = level1_b.join("shallow.txt");
    let mut f_shallow = File::create(&shallow_file).expect("create shallow file");
    f_shallow.write_all(&vec![b'S'; 100]).expect("write shallow file");
    drop(f_shallow);

    let cancel_token = Arc::new(AtomicBool::new(false));
    let summary = scan_directory_tree(&temp_root, cancel_token, None)
        .expect("scan should succeed");

    let mut root_node = summary.root_tree.expect("root tree should exist");
    // Verify Level 1 children are sorted descending by TRUE recursive size
    // level1_a has 250 bytes recursive size; level1_b has 100 bytes recursive size
    assert_eq!(root_node.children.len(), 2);
    assert_eq!(root_node.children[0].name, "level1_a");
    assert_eq!(root_node.children[0].total_size, 250);
    assert_eq!(root_node.children[1].name, "level1_b");
    assert_eq!(root_node.children[1].total_size, 100);

    // Verify deeper level of level1_a is NOT scanned yet (on-demand only)
    assert!(!root_node.children[0].is_deeper_scanned);

    // Now simulate user expanding level1_a: deeper level scan is triggered ONLY upon expansion!
    scan_deeper_level_for_node(&mut root_node.children[0]);

    assert!(root_node.children[0].is_deeper_scanned);
    assert_eq!(root_node.children[0].children.len(), 1);
    assert_eq!(root_node.children[0].children[0].name, "level2_nested");
    assert_eq!(root_node.children[0].children[0].total_size, 250);
    assert_eq!(root_node.children[0].total_size, 250);

    // Test headless asynchronous node expansion
    let cancel = Arc::new(AtomicBool::new(false));
    let res = win_folder_cleaner::core::analyzer::scan_node_deep_headless(&level1_a, cancel);
    assert_eq!(res.children.len(), 1);
    assert_eq!(res.children[0].name, "level2_nested");
    assert_eq!(res.total_size, 250);
    assert_eq!(res.file_count, 1);

    // Clean up
    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_scanner_real_time_progress_pipeline() {
    let temp_root = std::env::temp_dir().join("test_win_folder_cleaner_progress");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).expect("create test root");

    let sub1 = temp_root.join("sub1");
    let sub2 = temp_root.join("sub2");
    fs::create_dir_all(&sub1).expect("create sub1");
    fs::create_dir_all(&sub2).expect("create sub2");

    for i in 0..10 {
        let p = sub1.join(format!("file_{}.dat", i));
        let mut f = File::create(&p).unwrap();
        f.write_all(&vec![b'X'; 500]).unwrap();
    }
    for i in 0..10 {
        let p = sub2.join(format!("file_{}.dat", i));
        let mut f = File::create(&p).unwrap();
        f.write_all(&vec![b'Y'; 1000]).unwrap();
    }

    let cancel_token = Arc::new(AtomicBool::new(false));
    let (tx, rx) = crossbeam_channel::unbounded();

    let summary = scan_directory_tree(&temp_root, cancel_token, Some(tx))
        .expect("scan should succeed");

    assert_eq!(summary.total_files, 20);
    assert_eq!(summary.total_bytes, 15000);

    // Collect all progress updates delivered during scan
    let mut received_progress = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        received_progress.push(msg);
    }

    // Must have received at least the initial immediate update and the final completion update
    assert!(received_progress.len() >= 2);

    // Initial update must arrive immediately with partial_tree containing root node
    let first = &received_progress[0];
    assert!(first.partial_tree.is_some());
    let init_tree = first.partial_tree.as_ref().unwrap();
    assert_eq!(init_tree.children.len(), 2);
    assert!(!first.is_completed);

    // Last update must mark completion and have all files and bytes
    let last = received_progress.last().unwrap();
    assert!(last.is_completed);
    assert_eq!(last.files_scanned, 20);
    assert_eq!(last.bytes_scanned, 15000);

    let _ = fs::remove_dir_all(&temp_root);
}
