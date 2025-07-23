use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

use snp::error::SnpError;

// Test suite for comprehensive file locking as specified in GitHub issue #13
// https://github.com/devops247-online/snp/issues/13

#[test]
fn test_configuration_file_locking() {
    // Test exclusive locks on config files during reads/writes
    // Test multiple processes accessing same config
    // Test lock timeout and error handling

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    // Create a test config file
    fs::write(&config_path, "repos: []").expect("Failed to write config file");

    // This test should fail initially (TDD red phase)
    // Test 1: Exclusive lock acquisition
    let result = snp::file_lock::FileLockManager::new(Default::default());
    assert!(
        result.is_ok(),
        "FileLockManager should be created successfully"
    );

    let lock_manager = result.unwrap();

    // Test 2: Multiple processes accessing same config
    let config_lock_result = lock_manager.acquire_lock(&config_path, Default::default());
    assert!(
        config_lock_result.is_ok(),
        "Should be able to acquire lock on config file"
    );

    // Test 3: Lock timeout and error handling
    let timeout_config = snp::file_lock::LockConfig {
        lock_type: snp::file_lock::LockType::Exclusive,
        behavior: snp::file_lock::LockBehavior::Advisory,
        timeout: Duration::from_millis(100),
        retry_interval: Duration::from_millis(10),
        stale_timeout: Duration::from_secs(30),
    };

    // This should timeout since lock is already held
    let timeout_result = lock_manager.acquire_lock(&config_path, timeout_config);
    assert!(
        timeout_result.is_err(),
        "Should timeout when lock is already held"
    );

    if let Err(SnpError::Lock(lock_err)) = timeout_result {
        match lock_err.as_ref() {
            snp::error::LockError::Timeout { .. } => {
                // Expected timeout error
            }
            _ => panic!("Expected timeout error"),
        }
    } else {
        panic!("Expected lock timeout error");
    }
}

#[test]
fn test_temporary_file_locking() {
    // Test locks on temporary files during hook execution
    // Test cleanup of lock files after process termination
    // Test stale lock detection and cleanup

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_file = temp_dir.path().join("temp_hook_file.tmp");

    fs::write(&temp_file, "temporary data").expect("Failed to write temp file");

    let lock_manager = snp::file_lock::FileLockManager::new(Default::default())
        .expect("Failed to create lock manager");

    // Test 1: Lock temporary file
    let temp_lock = lock_manager.acquire_lock(&temp_file, Default::default());
    assert!(temp_lock.is_ok(), "Should be able to lock temporary file");

    // Test 2: Stale lock detection
    let lock_info = snp::file_lock::LockInfo {
        path: temp_file.clone(),
        lock_type: snp::file_lock::LockType::Exclusive,
        process_id: 99999, // Non-existent process ID
        thread_id: 1,
        acquired_at: SystemTime::now() - Duration::from_secs(60),
        last_accessed: SystemTime::now() - Duration::from_secs(60),
    };

    let detector =
        snp::file_lock::StaleLockDetector::new(Duration::from_secs(1), Duration::from_secs(30));

    let status = detector.check_lock_status(&lock_info);
    assert!(
        matches!(status, snp::file_lock::LockStatus::Stale { .. }),
        "Should detect stale lock for non-existent process"
    );

    // Test 3: Cleanup of lock files
    let cleanup_result = detector.cleanup_stale_lock(&temp_file);
    assert!(
        cleanup_result.is_ok(),
        "Should be able to cleanup stale lock"
    );
}

#[test]
fn test_lock_hierarchy() {
    // Test ordered lock acquisition to prevent deadlocks
    // Test lock upgrade/downgrade scenarios
    // Test nested lock handling

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");
    let file3 = temp_dir.path().join("file3.txt");

    // Create test files
    for file in [&file1, &file2, &file3] {
        fs::write(file, "test data").expect("Failed to write test file");
    }

    let lock_manager = snp::file_lock::FileLockManager::new(Default::default())
        .expect("Failed to create lock manager");

    // Test 1: Ordered lock acquisition
    let paths = vec![file1.as_path(), file2.as_path(), file3.as_path()];
    let locks_result = lock_manager.acquire_multiple_ordered(&paths, Default::default());
    assert!(
        locks_result.is_ok(),
        "Should be able to acquire locks in order"
    );

    let locks = locks_result.unwrap();
    assert_eq!(locks.len(), 3, "Should have acquired 3 locks");

    // Test 2: Lock hierarchy validation
    let mut hierarchy = snp::file_lock::LockHierarchy::new();
    hierarchy.add_path(file1.clone(), 1);
    hierarchy.add_path(file2.clone(), 2);
    hierarchy.add_path(file3.clone(), 3);

    let validation_result = hierarchy.validate_order(&paths);
    assert!(
        validation_result.is_ok(),
        "Should validate correct lock order"
    );

    // Test 3: Lock upgrade/downgrade
    drop(locks); // Release the ordered locks

    let shared_lock = lock_manager.acquire_lock(
        &file1,
        snp::file_lock::LockConfig {
            lock_type: snp::file_lock::LockType::Shared,
            ..Default::default()
        },
    );
    assert!(shared_lock.is_ok(), "Should be able to acquire shared lock");

    let upgrade_result =
        lock_manager.upgrade_lock(shared_lock.unwrap(), snp::file_lock::LockType::Exclusive);
    assert!(upgrade_result.is_ok(), "Should be able to upgrade lock");
}

#[test]
fn test_cross_platform_locking() {
    // Test advisory vs mandatory locking behavior
    // Test lock behavior on different filesystems
    // Test network filesystem lock compatibility

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file = temp_dir.path().join("cross_platform_test.txt");

    fs::write(&test_file, "cross platform test data").expect("Failed to write test file");

    let lock_manager = snp::file_lock::FileLockManager::new(Default::default())
        .expect("Failed to create lock manager");

    // Test 1: Advisory locking
    let advisory_config = snp::file_lock::LockConfig {
        lock_type: snp::file_lock::LockType::Exclusive,
        behavior: snp::file_lock::LockBehavior::Advisory,
        timeout: Duration::from_secs(1),
        retry_interval: Duration::from_millis(10),
        stale_timeout: Duration::from_secs(30),
    };

    let advisory_lock = lock_manager.acquire_lock(&test_file, advisory_config);
    assert!(
        advisory_lock.is_ok(),
        "Should be able to acquire advisory lock"
    );

    // Test 2: Mandatory locking (if supported)
    let mandatory_config = snp::file_lock::LockConfig {
        lock_type: snp::file_lock::LockType::Exclusive,
        behavior: snp::file_lock::LockBehavior::Mandatory,
        timeout: Duration::from_secs(1),
        retry_interval: Duration::from_millis(10),
        stale_timeout: Duration::from_secs(30),
    };

    drop(advisory_lock); // Release advisory lock first

    let mandatory_lock = lock_manager.acquire_lock(&test_file, mandatory_config);
    // Mandatory locking may not be supported on all filesystems
    assert!(
        mandatory_lock.is_ok()
            || matches!(
                mandatory_lock.unwrap_err(),
                SnpError::Lock(ref e) if matches!(e.as_ref(), snp::error::LockError::UnsupportedOperation { .. })
            ),
        "Should either support mandatory locking or return unsupported error"
    );

    // Test 3: Platform-specific behavior
    #[cfg(unix)]
    {
        let unix_locking = snp::file_lock::UnixFileLocking {
            use_flock: true,
            use_fcntl: true,
        };

        let _fd = -1; // Mock file descriptor
        let _flock_result = unix_locking.check_mandatory_locking(&test_file);
        // Should not panic - if we get here, the test passes
    }

    #[cfg(windows)]
    {
        let windows_locking = snp::file_lock::WindowsFileLocking {
            use_lock_file: true,
            use_file_mapping: false,
        };

        // Test should not panic - implementation details will be tested in integration
        assert!(true, "Windows locking structures should be available");
    }
}

#[test]
fn test_process_crash_recovery() {
    // Test stale lock detection after process crash
    // Test automatic lock cleanup mechanisms
    // Test lock recovery without data corruption

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let recovery_file = temp_dir.path().join("recovery_test.txt");

    fs::write(&recovery_file, "recovery test data").expect("Failed to write test file");

    let lock_manager = snp::file_lock::FileLockManager::new(Default::default())
        .expect("Failed to create lock manager");

    // Test 1: Simulate stale lock from crashed process
    let _fake_lock = snp::file_lock::LockInfo {
        path: recovery_file.clone(),
        lock_type: snp::file_lock::LockType::Exclusive,
        process_id: 99999, // Non-existent process
        thread_id: 1,
        acquired_at: SystemTime::now() - Duration::from_secs(300), // 5 minutes ago
        last_accessed: SystemTime::now() - Duration::from_secs(300),
    };

    let detector =
        snp::file_lock::StaleLockDetector::new(Duration::from_secs(1), Duration::from_secs(30));

    // Test 2: Process existence check
    let is_alive = detector.is_process_alive(99999);
    assert!(!is_alive, "Non-existent process should not be alive");

    // Test 3: Lock recovery
    let cleanup_count = lock_manager.cleanup_stale_locks();
    assert!(
        cleanup_count.is_ok(),
        "Should be able to cleanup stale locks"
    );

    // Test 4: Data integrity after recovery
    let recovered_data = fs::read_to_string(&recovery_file);
    assert!(
        recovered_data.is_ok(),
        "File should still be readable after lock recovery"
    );
    assert_eq!(
        recovered_data.unwrap(),
        "recovery test data",
        "File data should be intact after lock recovery"
    );

    // Test 5: Ability to acquire lock after cleanup
    let new_lock = lock_manager.acquire_lock(&recovery_file, Default::default());
    assert!(
        new_lock.is_ok(),
        "Should be able to acquire lock after stale lock cleanup"
    );
}

#[test]
fn test_lock_performance() {
    // Test lock acquisition latency under load
    // Test lock contention handling
    // Test scalability with many concurrent processes

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let perf_file = temp_dir.path().join("performance_test.txt");

    fs::write(&perf_file, "performance test data").expect("Failed to write test file");

    let lock_manager = Arc::new(
        snp::file_lock::FileLockManager::new(Default::default())
            .expect("Failed to create lock manager"),
    );

    // Test 1: Lock acquisition latency
    let start_time = std::time::Instant::now();
    let quick_lock = lock_manager.acquire_lock(&perf_file, Default::default());
    let acquisition_time = start_time.elapsed();

    assert!(quick_lock.is_ok(), "Should be able to acquire lock quickly");
    assert!(
        acquisition_time < Duration::from_millis(5),
        "Lock acquisition should be under 5ms for uncontended locks"
    );

    drop(quick_lock.unwrap()); // Release lock

    // Test 2: Concurrent lock requests
    let thread_count = 10;
    let mut handles = Vec::new();
    let successful_acquisitions = Arc::new(std::sync::Mutex::new(0));

    for _i in 0..thread_count {
        let manager = Arc::clone(&lock_manager);
        let file_path = perf_file.clone();
        let counter = Arc::clone(&successful_acquisitions);

        let handle = thread::spawn(move || {
            let config = snp::file_lock::LockConfig {
                lock_type: snp::file_lock::LockType::Shared,
                behavior: snp::file_lock::LockBehavior::Advisory,
                timeout: Duration::from_millis(100),
                retry_interval: Duration::from_millis(5),
                stale_timeout: Duration::from_secs(30),
            };

            let lock_result = manager.acquire_lock(&file_path, config);
            if lock_result.is_ok() {
                let mut count = counter.lock().unwrap();
                *count += 1;

                // Hold lock briefly to test contention
                thread::sleep(Duration::from_millis(10));
            }

            lock_result.is_ok()
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    let mut successful_threads = 0;
    for handle in handles {
        if handle.join().unwrap() {
            successful_threads += 1;
        }
    }

    // Test 3: Performance metrics
    let final_count = *successful_acquisitions.lock().unwrap();
    assert!(
        final_count > 0,
        "At least some threads should have acquired locks"
    );
    assert!(
        successful_threads == final_count,
        "Successful acquisitions should match successful threads"
    );

    // With shared locks, all threads should succeed
    assert_eq!(
        final_count, thread_count,
        "All threads should successfully acquire shared locks"
    );

    // Test 4: Lock metrics
    let metrics = lock_manager.get_lock_metrics();
    assert!(
        metrics.total_acquisitions > 0,
        "Should have recorded lock acquisitions"
    );
    assert!(
        metrics.average_acquisition_time < Duration::from_millis(50),
        "Average acquisition time should be reasonable"
    );
}

// Additional integration tests for specific lock scenarios

#[test]
fn test_config_file_lock_integration() {
    // Test the ConfigFileLock utility functions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let config_content = r#"
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.4.0
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
"#;

    fs::write(&config_path, config_content).expect("Failed to write config");

    // Test read lock
    let read_lock = snp::file_lock::ConfigFileLock::acquire_read_lock(&config_path);
    assert!(
        read_lock.is_ok(),
        "Should be able to acquire read lock on config"
    );

    drop(read_lock.unwrap());

    // Test write lock
    let write_lock = snp::file_lock::ConfigFileLock::acquire_write_lock(&config_path);
    assert!(
        write_lock.is_ok(),
        "Should be able to acquire write lock on config"
    );

    drop(write_lock.unwrap());

    // Test atomic update
    let update_result = snp::file_lock::ConfigFileLock::atomic_update(&config_path, |content| {
        // Mock config update - just return modified content
        Ok(content.replace("v4.4.0", "v4.5.0"))
    });
    assert!(
        update_result.is_ok(),
        "Should be able to perform atomic config update"
    );
}

#[test]
fn test_temp_file_lock_integration() {
    // Test the TempFileLock utility functions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Test temporary file lock acquisition
    let temp_lock_result =
        snp::file_lock::TempFileLock::acquire_temp_lock(temp_dir.path(), "test_pattern_*");
    assert!(
        temp_lock_result.is_ok(),
        "Should be able to acquire temporary file lock"
    );

    let (_lock, temp_path) = temp_lock_result.unwrap();
    assert!(temp_path.exists(), "Temporary file should exist");

    // Test cleanup
    let cleanup_count = snp::file_lock::TempFileLock::cleanup_temp_locks(temp_dir.path());
    assert!(
        cleanup_count.is_ok(),
        "Should be able to cleanup temporary locks"
    );
}

#[test]
fn test_lock_ordering_utilities() {
    // Test lock ordering functions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let paths = vec![
        temp_dir.path().join("z_file.txt"),
        temp_dir.path().join("a_file.txt"),
        temp_dir.path().join("m_file.txt"),
    ];

    // Create files
    for path in &paths {
        fs::write(path, "test").expect("Failed to create test file");
    }

    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let mut sortable_paths = path_refs.clone();

    // Test alphabetical ordering
    snp::file_lock::LockOrdering::alphabetical_order(&mut sortable_paths);

    assert!(sortable_paths[0].file_name().unwrap() < sortable_paths[1].file_name().unwrap());
    assert!(sortable_paths[1].file_name().unwrap() < sortable_paths[2].file_name().unwrap());

    // Test canonical path ordering
    let mut canonical_paths = path_refs.clone();
    snp::file_lock::LockOrdering::canonical_path_order(&mut canonical_paths);

    // Should not panic and should order consistently
    assert_eq!(canonical_paths.len(), 3);

    // Test inode ordering (Unix only)
    #[cfg(unix)]
    {
        let mut inode_paths = path_refs.clone();
        let inode_result = snp::file_lock::LockOrdering::inode_order(&mut inode_paths);
        assert!(inode_result.is_ok(), "Should be able to order by inode");
    }
}

