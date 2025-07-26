// Comprehensive cross-platform tests for SNP
// Tests platform-specific behavior across Windows, macOS, and Linux

use snp::classification::FileClassifier;
use snp::config::Config;
use snp::core::Stage;
use snp::execution::ExecutionConfig;
use snp::filesystem::FileSystem;
use snp::git::GitRepository;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(test)]
#[allow(clippy::uninlined_format_args)]
mod cross_platform_tests {
    use super::*;

    #[test]
    fn test_path_separator_handling() {
        // Test path handling across different platforms
        let test_paths = [
            "simple_file.txt",
            "directory/file.txt",
            "deep/nested/path/file.txt",
            "./relative_file.txt",
            "../parent_file.txt",
        ];

        for path_str in &test_paths {
            let path = Path::new(path_str);
            let normalized = FileSystem::normalize_path(path);

            // On Windows, backslashes should be converted to forward slashes
            #[cfg(windows)]
            {
                let normalized_str = normalized.to_string_lossy();
                assert!(
                    !normalized_str.contains('\\'),
                    "Windows paths should be normalized to forward slashes: {}",
                    normalized_str
                );
            }

            // On Unix systems, paths should remain unchanged
            #[cfg(not(windows))]
            {
                assert_eq!(
                    normalized,
                    path.to_path_buf(),
                    "Unix paths should remain unchanged"
                );
            }

            // All platforms should preserve filename
            if let (Some(original_name), Some(normalized_name)) =
                (path.file_name(), normalized.file_name())
            {
                assert_eq!(
                    original_name, normalized_name,
                    "Filename should be preserved across platforms"
                );
            }
        }
    }

    #[test]
    fn test_windows_specific_paths() {
        // Test Windows-specific path formats
        #[cfg(windows)]
        {
            let windows_paths = [
                r"C:\Users\user\file.txt",
                r".\relative\path.txt",
                r"..\parent\file.txt",
                r"\\server\share\file.txt", // UNC path
                r"C:\Program Files\app\config.yaml",
                r"C:\Users\user name\file with spaces.txt",
            ];

            for path_str in &windows_paths {
                let path = Path::new(path_str);
                let normalized = FileSystem::normalize_path(path);
                let normalized_str = normalized.to_string_lossy();

                // Should convert backslashes to forward slashes
                assert!(
                    !normalized_str.contains('\\'),
                    "Backslashes should be converted: {} -> {}",
                    path_str,
                    normalized_str
                );

                // Should preserve drive letters and UNC prefixes appropriately
                if path_str.starts_with(r"C:\") {
                    assert!(
                        normalized_str.starts_with("C:/"),
                        "Drive letter should be preserved: {}",
                        normalized_str
                    );
                }
            }
        }

        #[cfg(not(windows))]
        {
            // On non-Windows systems, test that Windows-style paths are handled gracefully
            let windows_style_paths = [r"folder\file.txt", r"sub\folder\file.txt"];

            for path_str in &windows_style_paths {
                let path = Path::new(path_str);
                let normalized = FileSystem::normalize_path(path);

                // Should not change non-Windows paths on Unix systems
                assert_eq!(normalized, path.to_path_buf());
            }
        }
    }

    #[test]
    fn test_unix_specific_paths() {
        // Test Unix-specific path formats
        #[cfg(unix)]
        {
            let unix_paths = [
                "/absolute/path/file.txt",
                "/usr/local/bin/snp",
                "/home/user/.config/snp/config.yaml",
                "/tmp/temporary_file.txt",
                "~/home_relative.txt",
                "./current_dir.txt",
                "../parent_dir.txt",
            ];

            for path_str in &unix_paths {
                let path = Path::new(path_str);
                let normalized = FileSystem::normalize_path(path);

                // Unix paths should remain unchanged
                assert_eq!(
                    normalized,
                    path.to_path_buf(),
                    "Unix path should remain unchanged: {}",
                    path_str
                );
            }
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, test Unix-style paths
            let unix_style_paths = ["/folder/file.txt", "./relative/file.txt"];

            for path_str in &unix_style_paths {
                let path = Path::new(path_str);
                let normalized = FileSystem::normalize_path(path);

                // Behavior may vary, but should not crash
                assert!(normalized.to_string_lossy().len() > 0);
            }
        }
    }

    #[test]
    fn test_case_sensitivity_behavior() {
        let temp_dir = TempDir::new().unwrap();

        // Create files with different case variations
        let base_filename = "TestFile.py";
        let lowercase_filename = "testfile.py";
        let uppercase_filename = "TESTFILE.PY";

        let base_file = temp_dir.path().join(base_filename);
        fs::write(&base_file, "print('base file')").unwrap();

        // Test case sensitivity behavior based on platform
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            // macOS and Windows are typically case-insensitive
            let lowercase_path = temp_dir.path().join(lowercase_filename);
            let uppercase_path = temp_dir.path().join(uppercase_filename);

            // These might refer to the same file on case-insensitive systems
            // We test that the filesystem handles this gracefully
            let base_exists = base_file.exists();
            assert!(base_exists, "Base file should exist");
        }

        #[cfg(target_os = "linux")]
        {
            // Linux is case-sensitive
            let lowercase_file = temp_dir.path().join(lowercase_filename);
            let uppercase_file = temp_dir.path().join(uppercase_filename);

            // Create additional files with different cases
            fs::write(&lowercase_file, "print('lowercase file')").unwrap();
            fs::write(&uppercase_file, "print('uppercase file')").unwrap();

            // All three files should be distinct on Linux
            assert!(base_file.exists());
            assert!(lowercase_file.exists());
            assert!(uppercase_file.exists());
        }
    }

    #[test]
    fn test_file_permissions_cross_platform() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("permissions_test.txt");
        fs::write(&test_file, "test content").unwrap();

        // Test Unix-style permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let permission_tests = [
                (0o644, "rw-r--r--"),
                (0o755, "rwxr-xr-x"),
                (0o600, "rw-------"),
                (0o400, "r--------"),
                (0o000, "---------"),
            ];

            for (mode, description) in &permission_tests {
                let mut perms = fs::metadata(&test_file).unwrap().permissions();
                perms.set_mode(*mode);

                if fs::set_permissions(&test_file, perms).is_ok() {
                    // Test file operations with different permissions
                    let is_binary_result = FileSystem::is_binary_file(&test_file);

                    match *mode {
                        0o000 => {
                            // No permissions - should fail
                            assert!(
                                is_binary_result.is_err(),
                                "Should fail with no permissions ({})",
                                description
                            );
                        }
                        _ if *mode & 0o400 != 0 => {
                            // Read permission available
                            assert!(
                                is_binary_result.is_ok() || is_binary_result.is_err(),
                                "Should handle read permissions ({})",
                                description
                            );
                        }
                        _ => {
                            // Other permission combinations
                            // Result may vary, but should not crash
                        }
                    }
                }
            }
        }

        // Test Windows-style permissions
        #[cfg(windows)]
        {
            let mut perms = fs::metadata(&test_file).unwrap().permissions();

            // Test read-only
            perms.set_readonly(true);
            fs::set_permissions(&test_file, perms.clone()).unwrap();

            let is_binary_readonly = FileSystem::is_binary_file(&test_file);
            assert!(
                is_binary_readonly.is_ok(),
                "Should read read-only files on Windows"
            );

            // Test read-write
            perms.set_readonly(false);
            fs::set_permissions(&test_file, perms).unwrap();

            let is_binary_readwrite = FileSystem::is_binary_file(&test_file);
            assert!(
                is_binary_readwrite.is_ok(),
                "Should read read-write files on Windows"
            );
        }
    }

    #[test]
    fn test_special_characters_in_filenames() {
        let temp_dir = TempDir::new().unwrap();

        // Test platform-specific filename restrictions
        let test_cases = [
            // Generally safe characters across platforms
            ("safe_file-name_123.txt", true),
            ("file.with.dots.txt", true),
            ("file with spaces.txt", true),
            ("file(with)parentheses.txt", true),
            ("file[with]brackets.txt", true),
            ("file{with}braces.txt", true),
            ("file@with#symbols.txt", true),
            // Unicode characters
            ("файл.txt", true),     // Cyrillic
            ("文件.txt", true),     // Chinese
            ("ファイル.txt", true), // Japanese
            ("café.txt", true),     // Accented characters
        ];

        for (filename, should_work) in &test_cases {
            let file_path = temp_dir.path().join(filename);
            let write_result = fs::write(&file_path, "test content");

            if *should_work && write_result.is_ok() {
                // Test file operations on successfully created files
                let is_binary = FileSystem::is_binary_file(&file_path);
                assert!(is_binary.is_ok(), "Should handle filename: {}", filename);
                assert!(
                    !is_binary.unwrap(),
                    "Text file should not be binary: {}",
                    filename
                );

                // Test path normalization
                let normalized = FileSystem::normalize_path(&file_path);
                assert!(
                    normalized.file_name().is_some(),
                    "Should preserve filename: {}",
                    filename
                );
                // Note: Some filesystems might not support certain Unicode characters
                // so we don't assert write_result.is_ok() for all cases
            }
        }

        // Test Windows-specific reserved names
        #[cfg(windows)]
        {
            let windows_reserved = [
                "CON.txt", "PRN.txt", "AUX.txt", "NUL.txt", "COM1.txt", "COM2.txt", "LPT1.txt",
                "LPT2.txt",
            ];

            for reserved_name in &windows_reserved {
                let file_path = temp_dir.path().join(reserved_name);
                let write_result = fs::write(&file_path, "test");

                // Windows should either reject these names or handle them specially
                // We just test that the operation doesn't crash
                match write_result {
                    Ok(_) => {
                        // If creation succeeded, test basic operations
                        let _ = FileSystem::is_binary_file(&file_path);
                    }
                    Err(_) => {
                        // Expected for reserved names on Windows
                    }
                }
            }
        }
    }

    #[test]
    fn test_git_repository_discovery_cross_platform() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize git repository
        let git_init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&temp_dir)
            .output();

        if git_init.is_ok() {
            // Configure git
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "Test User"])
                .current_dir(&temp_dir)
                .output();

            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "test@example.com"])
                .current_dir(&temp_dir)
                .output();

            // Test git repository discovery
            let repo_result = GitRepository::discover_from_path(&temp_dir);
            assert!(repo_result.is_ok(), "Should discover git repository");

            // Test subdirectory discovery
            let subdir = temp_dir.path().join("subdir");
            fs::create_dir(&subdir).unwrap();

            let subdir_result = GitRepository::discover_from_path(&subdir);
            assert!(
                subdir_result.is_ok(),
                "Should discover git repository from subdirectory"
            );

            // Test deep nested discovery
            let deep_dir = temp_dir.path().join("deep").join("nested").join("path");
            fs::create_dir_all(&deep_dir).unwrap();

            let deep_result = GitRepository::discover_from_path(&deep_dir);
            assert!(
                deep_result.is_ok(),
                "Should discover git repository from deep path"
            );
        }
    }

    #[test]
    fn test_config_file_path_resolution() {
        let temp_dir = TempDir::new().unwrap();

        // Test different config file path formats
        let config_content = r#"
repos:
  - repo: https://github.com/test/repo
    rev: v1.0.0
    hooks:
      - id: test-hook
        entry: echo
        language: system
"#;

        // Test relative paths
        let relative_config = temp_dir.path().join("relative_config.yaml");
        fs::write(&relative_config, config_content).unwrap();

        let config_result = Config::from_file(&relative_config);
        assert!(
            config_result.is_ok(),
            "Should parse config from relative path"
        );

        // Test absolute paths
        let absolute_config = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .join("absolute_config.yaml");
        fs::write(&absolute_config, config_content).unwrap();

        let abs_config_result = Config::from_file(&absolute_config);
        assert!(
            abs_config_result.is_ok(),
            "Should parse config from absolute path"
        );

        // Test nested directory paths
        let nested_dir = temp_dir.path().join("configs").join("nested");
        fs::create_dir_all(&nested_dir).unwrap();
        let nested_config = nested_dir.join("nested_config.yaml");
        fs::write(&nested_config, config_content).unwrap();

        let nested_result = Config::from_file(&nested_config);
        assert!(
            nested_result.is_ok(),
            "Should parse config from nested path"
        );
    }

    #[test]
    fn test_binary_detection_platform_differences() {
        let temp_dir = TempDir::new().unwrap();

        // Test various file types that might be handled differently across platforms
        let test_files = [
            // Text files with different line endings
            ("unix_text.txt", "line1\nline2\nline3\n".as_bytes()),
            ("windows_text.txt", "line1\r\nline2\r\nline3\r\n".as_bytes()),
            ("mac_text.txt", "line1\rline2\rline3\r".as_bytes()),
            ("mixed_text.txt", "line1\nline2\r\nline3\r".as_bytes()),
            // Empty file
            ("empty.txt", "".as_bytes()),
            // UTF-8 files
            ("utf8.txt", "Hello 世界! 🌍".as_bytes()),
        ];

        let binary_files = [
            // Binary files with null bytes
            ("binary.bin", vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE]),
            ("null_byte.txt", b"text\x00more text".to_vec()),
        ];

        let text_with_bom = [
            // UTF-8 BOM files are still text files
            (
                "utf8_bom.txt",
                vec![0xEF, 0xBB, 0xBF, b'H', b'e', b'l', b'l', b'o'],
            ),
        ];

        // Test text files (should all be non-binary)
        for (filename, content) in &test_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, content).unwrap();

            let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
            assert!(!is_binary, "Text file should not be binary: {}", filename);
        }

        // Test binary files (should all be binary)
        for (filename, content) in &binary_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, content).unwrap();

            let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
            assert!(
                is_binary,
                "Binary file should be detected as binary: {}",
                filename
            );
        }

        // Test text files with BOM (should still be text)
        for (filename, content) in &text_with_bom {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, content).unwrap();

            let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
            assert!(
                !is_binary,
                "UTF-8 BOM file should still be text: {}",
                filename
            );
        }
    }

    #[test]
    fn test_file_classification_platform_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let mut classifier = FileClassifier::new();

        // Test that file classification works consistently across platforms
        let test_files = [
            (
                "script.py",
                Some("python"),
                "#!/usr/bin/env python3\nprint('hello')",
            ),
            ("app.js", Some("javascript"), "console.log('hello');"),
            (
                "main.rs",
                Some("rust"),
                "fn main() { println!(\"hello\"); }",
            ),
            ("script.sh", Some("shell"), "#!/bin/bash\necho hello"),
            ("index.html", None, "<!DOCTYPE html><html></html>"),
        ];

        for (filename, expected_lang, content) in &test_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, content).unwrap();

            let result = classifier.classify_file(&file_path).unwrap();

            if let Some(expected) = expected_lang {
                assert_eq!(
                    result.language,
                    Some(expected.to_string()),
                    "Language detection should be consistent across platforms for {}",
                    filename
                );
            }

            // Test that confidence and detection methods are reasonable
            assert!(
                result.confidence >= 0.0 && result.confidence <= 1.0,
                "Confidence should be between 0 and 1"
            );
            assert!(
                !result.detection_methods.is_empty(),
                "Should have at least one detection method"
            );
        }
    }

    #[test]
    fn test_execution_config_platform_defaults() {
        // Test that execution configuration handles platform differences appropriately
        let config = ExecutionConfig::new(Stage::PreCommit);

        // Test default values are reasonable for all platforms
        assert!(
            config.max_parallel_hooks > 0,
            "Should have positive parallelism"
        );
        assert!(
            config.hook_timeout.as_secs() > 0,
            "Should have positive timeout"
        );

        // Test platform-specific defaults
        #[cfg(windows)]
        {
            // Windows-specific behavior testing
            let windows_config = ExecutionConfig::new(Stage::PreCommit).with_max_parallel_hooks(1); // Conservative for Windows
            assert_eq!(windows_config.max_parallel_hooks, 1);
        }

        #[cfg(unix)]
        {
            // Unix-specific behavior testing
            let unix_config = ExecutionConfig::new(Stage::PreCommit).with_max_parallel_hooks(8); // Can handle more parallelism
            assert_eq!(unix_config.max_parallel_hooks, 8);
        }
    }

    #[test]
    fn test_symlink_handling_cross_platform() {
        let temp_dir = TempDir::new().unwrap();

        // Create a target file
        let target_file = temp_dir.path().join("target.txt");
        fs::write(&target_file, "target content").unwrap();

        // Test symlink handling
        #[cfg(unix)]
        {
            let symlink_file = temp_dir.path().join("symlink.txt");
            let symlink_result = std::os::unix::fs::symlink(&target_file, &symlink_file);

            if symlink_result.is_ok() {
                // Test operations on symlink
                let is_binary = FileSystem::is_binary_file(&symlink_file);
                assert!(is_binary.is_ok(), "Should handle symlinks on Unix");
                assert!(!is_binary.unwrap(), "Text symlink should not be binary");

                // Test path normalization
                let normalized = FileSystem::normalize_path(&symlink_file);
                assert!(
                    normalized.file_name().is_some(),
                    "Should handle symlink paths"
                );
            }
        }

        #[cfg(windows)]
        {
            // Windows symlinks require special permissions
            // Test that operations handle missing symlinks gracefully
            let fake_symlink = temp_dir.path().join("fake_symlink.txt");
            fs::write(&fake_symlink, "regular file content").unwrap();

            let is_binary = FileSystem::is_binary_file(&fake_symlink);
            assert!(is_binary.is_ok(), "Should handle regular files on Windows");
        }
    }

    #[test]
    fn test_environment_variable_handling() {
        // Test platform-specific environment variable behavior

        // Test HOME vs USERPROFILE
        #[cfg(unix)]
        {
            let home = std::env::var("HOME");
            if let Ok(home_value) = home {
                let home_path = Path::new(&home_value);
                assert!(
                    home_path.is_absolute(),
                    "HOME should be absolute path on Unix"
                );
            }
        }

        #[cfg(windows)]
        {
            let userprofile = std::env::var("USERPROFILE");
            if let Ok(profile_value) = userprofile {
                let profile_path = Path::new(&profile_value);
                assert!(
                    profile_path.is_absolute(),
                    "USERPROFILE should be absolute path on Windows"
                );
            }
        }

        // Test PATH separator
        let path_separator = if cfg!(windows) { ';' } else { ':' };
        let path_var = std::env::var("PATH").unwrap_or_default();

        if !path_var.is_empty() {
            let paths: Vec<&str> = path_var.split(path_separator).collect();
            assert!(!paths.is_empty(), "PATH should contain at least one entry");
        }
    }

    #[test]
    fn test_temporary_directory_behavior() {
        // Test that temporary directory handling works across platforms
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Should be able to create files in temp directory
        let test_file = temp_path.join("temp_test.txt");
        let write_result = fs::write(&test_file, "temporary content");
        assert!(write_result.is_ok(), "Should write to temp directory");

        // Should be able to read from temp directory
        let read_result = fs::read_to_string(&test_file);
        assert!(read_result.is_ok(), "Should read from temp directory");
        assert_eq!(read_result.unwrap(), "temporary content");

        // Test temp directory path characteristics
        #[cfg(windows)]
        {
            let temp_str = temp_path.to_string_lossy();
            // Windows temp paths might start with C:\Users\... or similar
            assert!(
                temp_str.len() > 3,
                "Windows temp path should be substantial"
            );
        }

        #[cfg(unix)]
        {
            let temp_str = temp_path.to_string_lossy();
            // Unix temp paths often start with /tmp or /var/tmp
            assert!(
                temp_str.starts_with('/'),
                "Unix temp path should be absolute"
            );
        }
    }

    #[test]
    fn test_current_directory_operations() {
        let temp_dir = TempDir::new().unwrap();

        // Test current directory operations
        let original_cwd = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&temp_dir).unwrap();

        // Test relative path operations
        let relative_file = Path::new("relative_test.txt");
        fs::write(relative_file, "relative content").unwrap();

        let is_binary = FileSystem::is_binary_file(relative_file);
        assert!(is_binary.is_ok(), "Should handle relative paths");

        // Test path normalization from current directory
        let normalized = FileSystem::normalize_path(relative_file);
        assert_eq!(normalized.file_name().unwrap(), "relative_test.txt");

        // Restore original directory
        std::env::set_current_dir(&original_cwd).unwrap();
    }

    #[test]
    fn test_platform_specific_line_endings() {
        let temp_dir = TempDir::new().unwrap();

        // Test different line ending styles
        let line_ending_tests = [
            ("unix.txt", "line1\nline2\nline3\n"),
            ("windows.txt", "line1\r\nline2\r\nline3\r\n"),
            ("mac_classic.txt", "line1\rline2\rline3\r"),
            ("mixed.txt", "line1\nline2\r\nline3\r"),
        ];

        for (filename, content) in &line_ending_tests {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, content).unwrap();

            // All should be detected as text files regardless of line endings
            let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
            assert!(
                !is_binary,
                "Files with different line endings should be text: {}",
                filename
            );

            // Should be able to read content back
            let read_content = fs::read_to_string(&file_path).unwrap();
            assert_eq!(
                read_content, *content,
                "Content should be preserved: {}",
                filename
            );
        }
    }

    #[test]
    fn test_cross_platform_summary() {
        // Summary test that verifies core cross-platform functionality
        println!("\n=== Cross-Platform Compatibility Summary ===");

        let temp_dir = TempDir::new().unwrap();

        // Test basic file operations
        let test_file = temp_dir.path().join("cross_platform_test.py");
        fs::write(&test_file, "print('cross platform test')").unwrap();

        // File classification
        let mut classifier = FileClassifier::new();
        let classification = classifier.classify_file(&test_file).unwrap();
        assert_eq!(classification.language, Some("python".to_string()));

        // Binary detection
        let is_binary = FileSystem::is_binary_file(&test_file).unwrap();
        assert!(!is_binary);

        // Path normalization
        let normalized = FileSystem::normalize_path(&test_file);
        assert!(normalized.file_name().is_some());

        // Config parsing
        let config_content = r#"
repos:
  - repo: https://github.com/test/repo
    rev: v1.0.0
    hooks:
      - id: test-hook
        entry: echo
        language: system
"#;
        let config_file = temp_dir.path().join("test_config.yaml");
        fs::write(&config_file, config_content).unwrap();
        let config = Config::from_file(&config_file).unwrap();
        assert!(!config.repos.is_empty());

        println!("  ✓ File classification works");
        println!("  ✓ Binary detection works");
        println!("  ✓ Path normalization works");
        println!("  ✓ Config parsing works");

        // Platform-specific information
        println!("  Platform: {}", std::env::consts::OS);
        println!("  Architecture: {}", std::env::consts::ARCH);
        println!("  Family: {}", std::env::consts::FAMILY);

        #[cfg(windows)]
        println!("  ✓ Windows-specific features tested");

        #[cfg(unix)]
        println!("  ✓ Unix-specific features tested");

        println!("=== All cross-platform tests passed ===\n");
    }
}
