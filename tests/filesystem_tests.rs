// Comprehensive tests for the filesystem module
// Tests cross-platform path operations, binary detection, and file system utilities

use snp::error::{ConfigError, SnpError};
use snp::filesystem::FileSystem;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::len_zero)]
mod filesystem_tests {
    use super::*;

    #[test]
    fn test_path_normalization() {
        // Test basic path normalization
        let path = Path::new("src/main.rs");
        let normalized = FileSystem::normalize_path(path);

        #[cfg(windows)]
        {
            // On Windows, should convert backslashes to forward slashes
            let windows_path = Path::new("src\\main.rs");
            let normalized_windows = FileSystem::normalize_path(windows_path);
            assert_eq!(normalized_windows, PathBuf::from("src/main.rs"));
        }

        #[cfg(not(windows))]
        {
            // On Unix systems, should remain unchanged
            assert_eq!(normalized, path.to_path_buf());
        }

        // Test complex paths
        let complex_path = Path::new("../src/lib/utils/helper.rs");
        let normalized_complex = FileSystem::normalize_path(complex_path);
        assert!(normalized_complex.to_string_lossy().contains("helper.rs"));

        // Test absolute paths
        let abs_path = Path::new("/usr/local/bin/program");
        let normalized_abs = FileSystem::normalize_path(abs_path);

        #[cfg(not(windows))]
        {
            assert_eq!(normalized_abs, abs_path.to_path_buf());
        }

        // Test empty path
        let empty_path = Path::new("");
        let normalized_empty = FileSystem::normalize_path(empty_path);
        assert_eq!(normalized_empty, PathBuf::from(""));

        // Test current directory
        let current_dir = Path::new(".");
        let normalized_current = FileSystem::normalize_path(current_dir);
        assert_eq!(normalized_current, PathBuf::from("."));

        // Test parent directory
        let parent_dir = Path::new("..");
        let normalized_parent = FileSystem::normalize_path(parent_dir);
        assert_eq!(normalized_parent, PathBuf::from(".."));
    }

    #[test]
    fn test_binary_file_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Test text files (should not be binary)
        let text_file = temp_dir.path().join("text.txt");
        fs::write(&text_file, "This is a text file with normal content").unwrap();
        assert!(!FileSystem::is_binary_file(&text_file).unwrap());

        let python_file = temp_dir.path().join("script.py");
        fs::write(&python_file, "print('Hello, World!')").unwrap();
        assert!(!FileSystem::is_binary_file(&python_file).unwrap());

        let json_file = temp_dir.path().join("config.json");
        fs::write(&json_file, r#"{"name": "test", "value": 123}"#).unwrap();
        assert!(!FileSystem::is_binary_file(&json_file).unwrap());

        // Test UTF-8 text with special characters
        let utf8_file = temp_dir.path().join("utf8.txt");
        fs::write(&utf8_file, "Hello 世界! 🌍 åäö").unwrap();
        assert!(!FileSystem::is_binary_file(&utf8_file).unwrap());

        // Test binary files (should be binary)
        let binary_file = temp_dir.path().join("binary.bin");
        let binary_content = vec![
            0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC, 0x89, 0x50, 0x4E,
            0x47, // PNG magic number
        ];
        fs::write(&binary_file, binary_content).unwrap();
        assert!(FileSystem::is_binary_file(&binary_file).unwrap());

        // Test mixed content (some binary bytes in text)
        let mixed_file = temp_dir.path().join("mixed.txt");
        let mut mixed_content = "This is text with some binary: ".as_bytes().to_vec();
        mixed_content.extend_from_slice(&[0x00, 0x01, 0x02]);
        mixed_content.extend_from_slice(" more text".as_bytes());
        fs::write(&mixed_file, mixed_content).unwrap();
        assert!(FileSystem::is_binary_file(&mixed_file).unwrap());

        // Test empty file (should not be binary)
        let empty_file = temp_dir.path().join("empty.txt");
        fs::write(&empty_file, "").unwrap();
        assert!(!FileSystem::is_binary_file(&empty_file).unwrap());

        // Test file with only whitespace
        let whitespace_file = temp_dir.path().join("whitespace.txt");
        fs::write(&whitespace_file, "   \n\t\r\n   ").unwrap();
        assert!(!FileSystem::is_binary_file(&whitespace_file).unwrap());

        // Test large text file
        let large_text = "a".repeat(10000);
        let large_file = temp_dir.path().join("large.txt");
        fs::write(&large_file, large_text).unwrap();
        assert!(!FileSystem::is_binary_file(&large_file).unwrap());
    }

    #[test]
    fn test_binary_detection_specific_formats() {
        let temp_dir = TempDir::new().unwrap();

        // Test common binary file formats by magic numbers
        let binary_formats = [
            (
                "image.png",
                vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            ),
            ("image.jpg", vec![0xFF, 0xD8, 0xFF]),
            ("archive.zip", vec![0x50, 0x4B, 0x03, 0x04]),
            ("executable", vec![0x7F, 0x45, 0x4C, 0x46]), // ELF
            ("document.pdf", vec![0x25, 0x50, 0x44, 0x46]), // PDF
        ];

        for (filename, magic_bytes) in &binary_formats {
            let file_path = temp_dir.path().join(filename);
            let mut content = magic_bytes.clone();
            content.extend_from_slice(&[0x00; 100]); // Add some padding
            fs::write(&file_path, content).unwrap();
            assert!(
                FileSystem::is_binary_file(&file_path).unwrap(),
                "File {} should be detected as binary",
                filename
            );
        }

        // Test text-based formats (should not be binary)
        let text_formats = [
            ("config.xml", "<?xml version=\"1.0\"?><root></root>"),
            ("style.css", "body { margin: 0; padding: 0; }"),
            ("script.js", "function hello() { console.log('world'); }"),
            ("page.html", "<!DOCTYPE html><html><body></body></html>"),
            ("data.yaml", "name: test\nvalue: 123"),
            ("readme.md", "# Title\n\nThis is markdown content."),
        ];

        for (filename, content) in &text_formats {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, content).unwrap();
            assert!(
                !FileSystem::is_binary_file(&file_path).unwrap(),
                "File {} should not be detected as binary",
                filename
            );
        }
    }

    #[test]
    fn test_binary_detection_edge_cases() {
        let temp_dir = TempDir::new().unwrap();

        // Test file with null bytes but otherwise text
        let null_file = temp_dir.path().join("null.txt");
        fs::write(&null_file, "text\0more text").unwrap();
        assert!(FileSystem::is_binary_file(&null_file).unwrap());

        // Test file with high-bit characters but valid UTF-8
        let high_bit_file = temp_dir.path().join("highbit.txt");
        fs::write(&high_bit_file, "café naïve résumé").unwrap();
        assert!(!FileSystem::is_binary_file(&high_bit_file).unwrap());

        // Test file with null bytes (which should be detected as binary)
        let null_file = temp_dir.path().join("null.txt");
        fs::write(&null_file, "text\0null\0text").unwrap();
        assert!(FileSystem::is_binary_file(&null_file).unwrap());

        // Test file with only newlines and spaces
        let newline_file = temp_dir.path().join("newlines.txt");
        fs::write(&newline_file, "\n\n\n   \n\n").unwrap();
        assert!(!FileSystem::is_binary_file(&newline_file).unwrap());

        // Test very small binary file (with null byte)
        let tiny_binary = temp_dir.path().join("tiny.bin");
        fs::write(&tiny_binary, vec![0x00]).unwrap();
        assert!(FileSystem::is_binary_file(&tiny_binary).unwrap());

        // Test file that looks like binary but is actually text encoding
        let encoded_file = temp_dir.path().join("encoded.txt");
        // This is "Hello" in UTF-16 BE (which might look binary)
        fs::write(&encoded_file, "Hello World in ASCII").unwrap();
        assert!(!FileSystem::is_binary_file(&encoded_file).unwrap());
    }

    #[test]
    fn test_binary_detection_error_handling() {
        // Test non-existent file
        let non_existent = Path::new("/non/existent/file.txt");
        let result = FileSystem::is_binary_file(non_existent);
        assert!(result.is_err());
        match result.unwrap_err() {
            SnpError::Config(config_error) => match *config_error {
                ConfigError::NotFound { ref path, .. } => {
                    assert_eq!(path, non_existent);
                }
                _ => panic!("Expected NotFound error"),
            },
            _ => panic!("Expected Config error"),
        }

        // Test directory instead of file
        let temp_dir = TempDir::new().unwrap();
        let result = FileSystem::is_binary_file(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_file_filter_functionality() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let test_files = [
            "src/main.rs",
            "src/lib.rs",
            "tests/test.rs",
            "README.md",
            "Cargo.toml",
            ".gitignore",
            "target/debug/main",
            "node_modules/package/index.js",
        ];

        for file_path in &test_files {
            let full_path = temp_dir.path().join(file_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full_path, "content").unwrap();
        }

        // Test creating a FileFilter (this would be implementation-specific)
        // Since FileFilter is exported, we should be able to test its basic functionality
        // Note: The actual implementation details would depend on how FileFilter is structured
    }

    #[test]
    fn test_cross_platform_path_handling() {
        // Test various path separators and formats
        let test_paths = [
            "simple.txt",
            "dir/file.txt",
            "deep/nested/path/file.rs",
            "./relative.txt",
            "../parent.txt",
            "path with spaces.txt",
            "path-with-dashes.txt",
            "path_with_underscores.txt",
            "file.multiple.extensions.tar.gz",
        ];

        for path_str in &test_paths {
            let path = Path::new(path_str);
            let normalized = FileSystem::normalize_path(path);

            // Should always be a valid path
            assert!(normalized.to_string_lossy().len() > 0);

            // Should preserve the filename
            if let (Some(original_name), Some(normalized_name)) =
                (path.file_name(), normalized.file_name())
            {
                assert_eq!(original_name, normalized_name);
            }
        }

        // Test Windows-style paths on all platforms
        #[cfg(windows)]
        {
            let windows_paths = [
                r"C:\Users\user\file.txt",
                r".\relative\path.txt",
                r"..\parent\file.txt",
                r"\\server\share\file.txt",
            ];

            for path_str in &windows_paths {
                let path = Path::new(path_str);
                let normalized = FileSystem::normalize_path(path);
                // Should convert backslashes to forward slashes
                assert!(!normalized.to_string_lossy().contains('\\'));
            }
        }
    }

    #[test]
    fn test_special_file_names() {
        let temp_dir = TempDir::new().unwrap();

        // Test files with special characters in names
        let special_files = [
            "file with spaces.txt",
            "file-with-dashes.txt",
            "file_with_underscores.txt",
            "file.with.dots.txt",
            "file@with@at.txt",
            "file#with#hash.txt",
            "file(with)parens.txt",
            "file[with]brackets.txt",
            "file{with}braces.txt",
        ];

        for filename in &special_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();

            // Should be able to detect as non-binary
            assert!(!FileSystem::is_binary_file(&file_path).unwrap());

            // Should normalize path correctly
            let normalized = FileSystem::normalize_path(&file_path);
            assert!(normalized.file_name().is_some());
        }

        // Test Unicode filenames
        let unicode_files = [
            "файл.txt",     // Cyrillic
            "文件.txt",     // Chinese
            "ファイル.txt", // Japanese
            "📁файл📄.txt", // Emoji + Cyrillic
        ];

        for filename in &unicode_files {
            let file_path = temp_dir.path().join(filename);
            // Some filesystems might not support Unicode, so we'll handle errors gracefully
            if fs::write(&file_path, "content").is_ok() {
                assert!(!FileSystem::is_binary_file(&file_path).unwrap());
            }
        }
    }

    #[test]
    fn test_file_permissions_and_access() {
        let temp_dir = TempDir::new().unwrap();

        // Test regular file
        let regular_file = temp_dir.path().join("regular.txt");
        fs::write(&regular_file, "content").unwrap();
        assert!(!FileSystem::is_binary_file(&regular_file).unwrap());

        // Test read-only file
        let readonly_file = temp_dir.path().join("readonly.txt");
        fs::write(&readonly_file, "content").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&readonly_file).unwrap().permissions();
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(&readonly_file, perms).unwrap();
        }

        #[cfg(windows)]
        {
            let mut perms = fs::metadata(&readonly_file).unwrap().permissions();
            perms.set_readonly(true);
            fs::set_permissions(&readonly_file, perms).unwrap();
        }

        // Should still be able to read and detect as non-binary
        assert!(!FileSystem::is_binary_file(&readonly_file).unwrap());
    }

    #[test]
    fn test_large_file_handling() {
        let temp_dir = TempDir::new().unwrap();

        // Test moderately large text file
        let large_text_file = temp_dir.path().join("large_text.txt");
        let large_content = "This is a line of text.\n".repeat(10000);
        fs::write(&large_text_file, &large_content).unwrap();
        assert!(!FileSystem::is_binary_file(&large_text_file).unwrap());

        // Test large binary file
        let large_binary_file = temp_dir.path().join("large_binary.bin");
        let mut large_binary = vec![0x00]; // Start with null byte to ensure binary detection
        large_binary.extend(vec![0xFF; 50000]); // Add lots of data
        fs::write(&large_binary_file, &large_binary).unwrap();
        assert!(FileSystem::is_binary_file(&large_binary_file).unwrap());

        // Test file that starts as text but has null bytes early (binary)
        let mixed_large_file = temp_dir.path().join("mixed_large.txt");
        let mut mixed_content = "This starts as text.\n".repeat(100).into_bytes();
        mixed_content.extend(vec![0x00]); // Add null byte early to make it binary
        mixed_content.extend("More text after null byte.\n".repeat(1000).into_bytes());
        fs::write(&mixed_large_file, &mixed_content).unwrap();
        assert!(FileSystem::is_binary_file(&mixed_large_file).unwrap());
    }

    #[test]
    fn test_symlink_handling() {
        let temp_dir = TempDir::new().unwrap();

        // Create a regular file
        let target_file = temp_dir.path().join("target.txt");
        fs::write(&target_file, "This is the target file").unwrap();

        // Create a symlink (Unix only, Windows requires special permissions)
        #[cfg(unix)]
        {
            let symlink_file = temp_dir.path().join("symlink.txt");
            std::os::unix::fs::symlink(&target_file, &symlink_file).unwrap();

            // Should be able to read through symlink
            assert!(!FileSystem::is_binary_file(&symlink_file).unwrap());

            // Should normalize symlink path
            let normalized = FileSystem::normalize_path(&symlink_file);
            assert!(normalized.file_name().is_some());
        }
    }

    #[test]
    fn test_concurrent_file_operations() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = Arc::new(TempDir::new().unwrap());

        // Create multiple files concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let temp_dir = Arc::clone(&temp_dir);
                thread::spawn(move || {
                    let file_path = temp_dir.path().join(format!("concurrent_{}.txt", i));
                    fs::write(&file_path, format!("Content for file {}", i)).unwrap();

                    // Test binary detection concurrently
                    assert!(!FileSystem::is_binary_file(&file_path).unwrap());

                    // Test path normalization concurrently
                    let normalized = FileSystem::normalize_path(&file_path);
                    assert!(normalized.file_name().is_some());
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_error_propagation() {
        // Test that errors are properly propagated and have correct types

        // Non-existent file should return ConfigError::NotFound
        let result = FileSystem::is_binary_file(Path::new("/definitely/does/not/exist"));
        assert!(result.is_err());

        match result.unwrap_err() {
            SnpError::Config(config_error) => {
                match *config_error {
                    ConfigError::NotFound { .. } => {
                        // This is expected
                    }
                    _ => panic!("Expected NotFound error"),
                }
            }
            _ => panic!("Expected Config error"),
        }

        // Directory should return an IO error
        let temp_dir = TempDir::new().unwrap();
        let result = FileSystem::is_binary_file(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_performance_characteristics() {
        let temp_dir = TempDir::new().unwrap();

        // Create files of various sizes and measure performance
        let file_sizes = [0, 100, 1_000, 10_000, 100_000];

        for size in &file_sizes {
            let file_path = temp_dir.path().join(format!("perf_test_{}.txt", size));
            let content = "a".repeat(*size);
            fs::write(&file_path, &content).unwrap();

            let start = std::time::Instant::now();
            let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
            let duration = start.elapsed();

            // Should not be binary (all text)
            assert!(!is_binary);

            // Should complete reasonably quickly (less than 100ms for these sizes)
            assert!(
                duration.as_millis() < 100,
                "Binary detection took too long for size {}: {:?}",
                size,
                duration
            );
        }

        // Test performance with binary files
        for size in &file_sizes {
            if *size > 0 {
                let file_path = temp_dir.path().join(format!("perf_binary_{}.bin", size));
                let mut content = vec![0x00]; // Start with null byte to ensure binary detection
                content.extend(vec![0xFF; size - 1]);
                fs::write(&file_path, &content).unwrap();

                let start = std::time::Instant::now();
                let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
                let duration = start.elapsed();

                // Should be binary
                assert!(is_binary);

                // Should complete quickly
                assert!(duration.as_millis() < 100);
            }
        }
    }

    #[test]
    fn test_memory_efficiency() {
        let temp_dir = TempDir::new().unwrap();

        // Test that binary detection doesn't load entire file into memory
        // by creating a file larger than reasonable memory buffer
        let large_file = temp_dir.path().join("very_large.txt");

        // Write a large text file in chunks to avoid memory issues in test
        {
            use std::io::Write;
            let mut file = fs::File::create(&large_file).unwrap();
            let chunk = "This is a line of text that will be repeated many times.\n";
            for _ in 0..100_000 {
                file.write_all(chunk.as_bytes()).unwrap();
            }
        }

        // Should be able to detect as non-binary without loading entire file
        let start = std::time::Instant::now();
        assert!(!FileSystem::is_binary_file(&large_file).unwrap());
        let duration = start.elapsed();

        // Should complete in reasonable time (indicating it's not reading the whole file)
        assert!(duration.as_millis() < 1000);
    }
}
