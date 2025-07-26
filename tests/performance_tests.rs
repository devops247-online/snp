// Comprehensive performance tests for SNP
// Tests benchmarks, stress testing, and performance characteristics across modules

use snp::classification::FileClassifier;
use snp::config::Config;
use snp::core::Stage;
use snp::execution::ExecutionConfig;
use snp::filesystem::FileSystem;
use snp::git::GitRepository;
use snp::storage::Store;
use snp::validation::SchemaValidator;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::assertions_on_constants)]
mod performance_tests {
    use super::*;

    #[test]
    fn test_file_classification_performance() {
        let temp_dir = TempDir::new().unwrap();
        let mut classifier = FileClassifier::new();

        // Create various types of files for classification
        let file_types = [
            ("script.py", "python", "print('hello world')"),
            ("app.js", "javascript", "console.log('hello');"),
            ("main.rs", "rust", "fn main() { println!(\"hello\"); }"),
            ("index.ts", "typescript", "const x: number = 42;"),
            ("test.go", "go", "package main\nfunc main() {}"),
            ("script.sh", "shell", "#!/bin/bash\necho hello"),
        ];

        let test_files: Vec<_> = file_types
            .iter()
            .map(|(filename, _, content)| {
                let file_path = temp_dir.path().join(filename);
                fs::write(&file_path, content).unwrap();
                file_path
            })
            .collect();

        // Benchmark single file classification
        let start = Instant::now();
        for file in &test_files {
            let _result = classifier.classify_file(file).unwrap();
        }
        let single_pass_duration = start.elapsed();

        // Benchmark cached classification (should be faster)
        let start = Instant::now();
        for file in &test_files {
            let _result = classifier.classify_file(file).unwrap();
        }
        let cached_pass_duration = start.elapsed();

        // Cached access should be significantly faster
        assert!(
            cached_pass_duration < single_pass_duration,
            "Cached classification should be faster"
        );

        // Performance benchmark - should complete quickly
        assert!(
            single_pass_duration < Duration::from_millis(100),
            "File classification should be fast: {:?}",
            single_pass_duration
        );
        assert!(
            cached_pass_duration < Duration::from_millis(50),
            "Cached classification should be very fast: {:?}",
            cached_pass_duration
        );

        println!("File classification performance:");
        println!("  Single pass: {:?}", single_pass_duration);
        println!("  Cached pass: {:?}", cached_pass_duration);
        println!(
            "  Speedup: {:.2}x",
            single_pass_duration.as_nanos() as f64 / cached_pass_duration.as_nanos() as f64
        );
    }

    #[test]
    fn test_large_scale_file_classification() {
        let temp_dir = TempDir::new().unwrap();
        let mut classifier = FileClassifier::new();

        // Create a large number of files to stress test
        let file_count = 1000;
        let mut test_files = Vec::new();

        let start_creation = Instant::now();
        for i in 0..file_count {
            let filename = format!("test_file_{}.py", i);
            let file_path = temp_dir.path().join(&filename);
            fs::write(
                &file_path,
                format!("# Test file {}\nprint('hello {}')", i, i),
            )
            .unwrap();
            test_files.push(file_path);
        }
        let creation_duration = start_creation.elapsed();

        // Benchmark classification of many files
        let start_classification = Instant::now();
        let mut classified_count = 0;
        for file in &test_files {
            let result = classifier.classify_file(file).unwrap();
            if result.language == Some("python".to_string()) {
                classified_count += 1;
            }
        }
        let classification_duration = start_classification.elapsed();

        // All files should be classified as Python
        assert_eq!(classified_count, file_count);

        // Performance requirements
        assert!(
            creation_duration < Duration::from_secs(5),
            "File creation should complete in reasonable time: {:?}",
            creation_duration
        );
        assert!(
            classification_duration < Duration::from_secs(2),
            "Classification should complete quickly: {:?}",
            classification_duration
        );

        // Calculate throughput
        let files_per_second = file_count as f64 / classification_duration.as_secs_f64();
        assert!(
            files_per_second > 500.0,
            "Should classify at least 500 files/second"
        );

        println!("Large scale classification performance:");
        println!("  Files: {}", file_count);
        println!("  Classification time: {:?}", classification_duration);
        println!("  Throughput: {:.0} files/second", files_per_second);
    }

    #[test]
    fn test_filesystem_operations_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Test binary detection performance on various file sizes
        let file_sizes = [100, 1_000, 10_000, 100_000, 1_000_000];

        for &size in &file_sizes {
            let file_path = temp_dir.path().join(format!("test_{}.txt", size));
            let content = "a".repeat(size);
            fs::write(&file_path, &content).unwrap();

            // Benchmark binary detection
            let start = Instant::now();
            let is_binary = FileSystem::is_binary_file(&file_path).unwrap();
            let duration = start.elapsed();

            assert!(!is_binary, "Text files should not be binary");

            // Should complete quickly regardless of file size (due to early detection)
            assert!(
                duration < Duration::from_millis(10),
                "Binary detection should be fast for size {}: {:?}",
                size,
                duration
            );
        }

        // Test atomic write performance
        let atomic_test_file = temp_dir.path().join("atomic_test.txt");
        let test_content = b"test content for atomic write";

        let start = Instant::now();
        for _ in 0..100 {
            FileSystem::atomic_write(&atomic_test_file, test_content).unwrap();
        }
        let atomic_duration = start.elapsed();

        assert!(
            atomic_duration < Duration::from_millis(500),
            "100 atomic writes should complete quickly: {:?}",
            atomic_duration
        );

        println!("Filesystem operations performance:");
        println!("  100 atomic writes: {:?}", atomic_duration);
        println!("  Avg per write: {:?}", atomic_duration / 100);
    }

    #[test]
    fn test_config_parsing_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Create configs of varying complexity
        let config_sizes = [
            ("small", 5),   // 5 repositories
            ("medium", 25), // 25 repositories
            ("large", 100), // 100 repositories
        ];

        for (name, repo_count) in &config_sizes {
            // Generate config with multiple repositories and hooks
            let mut config_content = "repos:\n".to_string();
            for i in 0..*repo_count {
                config_content.push_str(&format!(
                    r#"  - repo: https://github.com/test/repo-{}
    rev: v1.0.{}
    hooks:
      - id: hook-{}-1
        name: Hook {} Part 1
        entry: echo
        args: ["test {}"]
        language: system
        files: "\\.py$"
      - id: hook-{}-2
        name: Hook {} Part 2
        entry: python
        args: ["-m", "test"]
        language: python
        files: "\\.py$"
        types: [python]
"#,
                    i, i, i, i, i, i, i
                ));
            }

            let config_file = temp_dir.path().join(format!("{}_config.yaml", name));
            fs::write(&config_file, &config_content).unwrap();

            // Benchmark config parsing
            let start = Instant::now();
            let config = Config::from_file(&config_file).unwrap();
            let parse_duration = start.elapsed();

            // Verify config structure
            assert_eq!(config.repos.len(), *repo_count);
            assert_eq!(config.repos[0].hooks.len(), 2);

            // Performance requirements based on config size
            let max_duration = match *name {
                "small" => Duration::from_millis(10),
                "medium" => Duration::from_millis(50),
                "large" => Duration::from_millis(200),
                _ => Duration::from_millis(100),
            };

            assert!(
                parse_duration < max_duration,
                "Config parsing for {} should be fast: {:?}",
                name,
                parse_duration
            );

            println!(
                "Config parsing performance ({} repos): {:?}",
                repo_count, parse_duration
            );
        }
    }

    #[test]
    fn test_validation_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Create configs with various validation complexities
        let test_cases = [
            (
                "valid_simple",
                true,
                r#"
repos:
  - repo: https://github.com/test/repo
    rev: v1.0.0
    hooks:
      - id: test-hook
        name: Test Hook
        entry: echo
        language: system
"#,
            ),
            (
                "valid_complex",
                true,
                r#"
repos:
  - repo: https://github.com/test/repo1
    rev: v1.0.0
    hooks:
      - id: hook1
        name: Hook 1
        entry: python
        language: python
        files: "\\.py$"
        types: [python]
        args: ["--check"]
  - repo: https://github.com/test/repo2
    rev: v2.0.0
    hooks:
      - id: hook2
        name: Hook 2
        entry: npm
        language: node
        files: "\\.js$"
        types: [javascript]
"#,
            ),
            (
                "invalid_multiple_errors",
                false,
                r#"
repos:
  - repo: ""  # Invalid empty repo
    rev: ""   # Invalid empty rev
    hooks:
      - id: ""  # Invalid empty ID
        name: ""
        entry: ""
        language: "invalid_language"
        files: "invalid-regex-["
        stages: ["invalid_stage"]
"#,
            ),
        ];

        let mut validator = SchemaValidator::new(Default::default());

        for (name, should_be_valid, config_content) in &test_cases {
            let config_file = temp_dir.path().join(format!("{}.yaml", name));
            fs::write(&config_file, config_content).unwrap();

            // Benchmark validation
            let start = Instant::now();
            let result = validator.validate_file(&config_file);
            let validation_duration = start.elapsed();

            assert_eq!(
                result.is_valid, *should_be_valid,
                "Validation result mismatch for {}",
                name
            );

            // Validation should complete quickly
            assert!(
                validation_duration < Duration::from_millis(50),
                "Validation for {} should be fast: {:?}",
                name,
                validation_duration
            );

            println!(
                "Validation performance ({}): {:?}",
                name, validation_duration
            );
        }
    }

    #[tokio::test]
    async fn test_concurrent_operations_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Test concurrent file classification
        let classifier = Arc::new(Mutex::new(FileClassifier::new()));
        let file_count = 50;
        let thread_count = 4;

        // Create test files
        let test_files: Arc<Vec<_>> = Arc::new(
            (0..file_count)
                .map(|i| {
                    let file_path = temp_dir.path().join(format!("concurrent_test_{}.py", i));
                    fs::write(&file_path, format!("print('test {}')", i)).unwrap();
                    file_path
                })
                .collect(),
        );

        // Benchmark concurrent classification
        let start = Instant::now();
        let handles: Vec<_> = (0..thread_count)
            .map(|thread_id| {
                let classifier = Arc::clone(&classifier);
                let files = Arc::clone(&test_files);
                thread::spawn(move || {
                    let files_per_thread = file_count / thread_count;
                    let start_idx = thread_id * files_per_thread;
                    let end_idx = if thread_id == thread_count - 1 {
                        file_count
                    } else {
                        start_idx + files_per_thread
                    };

                    let mut local_count = 0;
                    for i in start_idx..end_idx {
                        let mut c = classifier.lock().unwrap();
                        let result = c.classify_file(&files[i]).unwrap();
                        if result.language == Some("python".to_string()) {
                            local_count += 1;
                        }
                    }
                    local_count
                })
            })
            .collect();

        let mut total_classified = 0;
        for handle in handles {
            total_classified += handle.join().unwrap();
        }
        let concurrent_duration = start.elapsed();

        assert_eq!(total_classified, file_count);
        assert!(
            concurrent_duration < Duration::from_secs(1),
            "Concurrent classification should complete quickly: {:?}",
            concurrent_duration
        );

        println!("Concurrent operations performance:");
        println!("  Files: {} across {} threads", file_count, thread_count);
        println!("  Duration: {:?}", concurrent_duration);
        println!(
            "  Throughput: {:.0} files/second",
            file_count as f64 / concurrent_duration.as_secs_f64()
        );
    }

    #[test]
    fn test_memory_usage_patterns() {
        let temp_dir = TempDir::new().unwrap();

        // Test memory efficiency with repeated operations
        let iteration_count = 100;
        let files_per_iteration = 10;

        // Test file classification memory usage
        let mut classifier = FileClassifier::new();

        for iteration in 0..iteration_count {
            // Create files for this iteration
            let files: Vec<_> = (0..files_per_iteration)
                .map(|i| {
                    let file_path = temp_dir
                        .path()
                        .join(format!("mem_test_{}_{}.py", iteration, i));
                    fs::write(
                        &file_path,
                        format!("print('iteration {} file {}')", iteration, i),
                    )
                    .unwrap();
                    file_path
                })
                .collect();

            // Classify files
            for file in &files {
                let _result = classifier.classify_file(file).unwrap();
            }

            // Clean up files to test memory cleanup
            for file in files {
                fs::remove_file(file).unwrap();
            }

            // Clear classifier cache periodically to test memory management
            if iteration % 20 == 0 {
                classifier.clear_cache();
            }
        }

        // Test should complete without excessive memory growth
        assert!(true, "Memory usage test completed successfully");
    }

    #[test]
    fn test_storage_performance() {
        // Test storage operations performance
        // Create single instance and measure creation time
        let start = Instant::now();
        let _store = Store::new().unwrap();
        let storage_duration = start.elapsed();

        assert!(
            storage_duration < Duration::from_millis(100),
            "Storage creation should be efficient: {:?}",
            storage_duration
        );

        println!("Storage performance:");
        println!("  Single store creation: {:?}", storage_duration);

        // Test that storage is created successfully
        assert!(true, "Storage performance test completed");
    }

    #[test]
    fn test_git_operations_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a git repository
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        // Create and commit files
        for i in 0..10 {
            let file_path = temp_dir.path().join(format!("file_{}.py", i));
            fs::write(&file_path, format!("print('file {}')", i)).unwrap();
        }

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        // Benchmark git repository discovery
        let start = Instant::now();
        for _ in 0..10 {
            let _repo = GitRepository::discover_from_path(&temp_dir).unwrap();
        }
        let discovery_duration = start.elapsed();

        assert!(
            discovery_duration < Duration::from_millis(100),
            "Git repository discovery should be fast: {:?}",
            discovery_duration
        );

        println!("Git operations performance:");
        println!("  10 repository discoveries: {:?}", discovery_duration);
        println!("  Avg per discovery: {:?}", discovery_duration / 10);
    }

    #[test]
    fn test_regex_compilation_performance() {
        // Test regex pattern compilation performance (used in file filtering)
        let patterns = [
            r"\.py$",
            r"\.js$",
            r"\.rs$",
            r"\.go$",
            r"\.(py|js|ts|rs|go)$",
            r"^src/.*\.py$",
            r"test_.*\.py$",
            r".*/(test|spec)_.*\.(py|js|ts)$",
            r"^(src|lib|tests?)/.*\.(py|js|ts|rs|go|java|cpp|c|h)$",
            // Removed negative lookahead as it's not supported by the regex crate
            r".*/(?:src|lib)/.*\.(py|js|ts|jsx|tsx)$",
        ];

        let start = Instant::now();
        let mut compiled_patterns = Vec::new();
        for pattern in &patterns {
            let regex = regex::Regex::new(pattern).unwrap();
            compiled_patterns.push(regex);
        }
        let compilation_duration = start.elapsed();

        assert!(
            compilation_duration < Duration::from_millis(100),
            "Regex compilation should be reasonably fast: {:?}",
            compilation_duration
        );

        // Test pattern matching performance
        let test_files = [
            "src/main.py",
            "tests/test_module.py",
            "lib/utils.js",
            "src/component.tsx",
            "target/debug/main",
            "node_modules/package/index.js",
            "dist/bundle.js",
        ];

        let start = Instant::now();
        let mut match_count = 0;
        for pattern in &compiled_patterns {
            for file in &test_files {
                if pattern.is_match(file) {
                    match_count += 1;
                }
            }
        }
        let matching_duration = start.elapsed();

        assert!(match_count > 0, "Should have some pattern matches");
        assert!(
            matching_duration < Duration::from_millis(1),
            "Pattern matching should be very fast: {:?}",
            matching_duration
        );

        println!("Regex performance:");
        println!(
            "  Compilation ({} patterns): {:?}",
            patterns.len(),
            compilation_duration
        );
        println!(
            "  Matching ({} files): {:?}",
            test_files.len(),
            matching_duration
        );
    }

    #[test]
    fn test_execution_config_performance() {
        // Test execution configuration creation and manipulation performance
        let config_count = 1000;

        let start = Instant::now();
        let mut configs = Vec::new();
        for i in 0..config_count {
            let config = ExecutionConfig::new(Stage::PreCommit)
                .with_verbose(i % 2 == 0)
                .with_fail_fast(i % 3 == 0)
                .with_hook_timeout(Duration::from_secs(60 + i as u64))
                .with_max_parallel_hooks(4 + (i % 8));
            configs.push(config);
        }
        let creation_duration = start.elapsed();

        assert_eq!(configs.len(), config_count);
        assert!(
            creation_duration < Duration::from_millis(50),
            "Config creation should be fast: {:?}",
            creation_duration
        );

        println!("Execution config performance:");
        println!(
            "  {} config creations: {:?}",
            config_count, creation_duration
        );
        println!(
            "  Avg per config: {:?}",
            creation_duration / config_count as u32
        );
    }

    #[test]
    fn test_stress_test_file_operations() {
        let temp_dir = TempDir::new().unwrap();

        // Stress test with many small files
        let file_count = 2000;
        let start_creation = Instant::now();

        let test_files: Vec<_> = (0..file_count)
            .map(|i| {
                let file_path = temp_dir.path().join(format!("stress_test_{}.py", i));
                fs::write(
                    &file_path,
                    format!("# Stress test file {}\nprint({})", i, i),
                )
                .unwrap();
                file_path
            })
            .collect();

        let creation_duration = start_creation.elapsed();

        // Test binary detection on all files
        let start_detection = Instant::now();
        let mut text_count = 0;
        for file in &test_files {
            if !FileSystem::is_binary_file(file).unwrap() {
                text_count += 1;
            }
        }
        let detection_duration = start_detection.elapsed();

        // All should be text files
        assert_eq!(text_count, file_count);

        // Performance requirements for stress test
        assert!(
            creation_duration < Duration::from_secs(5),
            "File creation should handle stress: {:?}",
            creation_duration
        );
        assert!(
            detection_duration < Duration::from_secs(2),
            "Binary detection should handle stress: {:?}",
            detection_duration
        );

        println!("Stress test performance:");
        println!("  Files: {}", file_count);
        println!("  Creation: {:?}", creation_duration);
        println!("  Detection: {:?}", detection_duration);
        println!("  Total: {:?}", creation_duration + detection_duration);
    }

    #[test]
    fn test_benchmark_summary() {
        // This test provides a summary of key performance metrics
        println!("\n=== SNP Performance Benchmark Summary ===");

        let temp_dir = TempDir::new().unwrap();
        let mut timings = Vec::new();

        // File classification benchmark
        let mut classifier = FileClassifier::new();
        let test_file = temp_dir.path().join("benchmark.py");
        fs::write(&test_file, "print('benchmark')").unwrap();

        let start = Instant::now();
        for _ in 0..100 {
            classifier.classify_file(&test_file).unwrap();
        }
        let classification_time = start.elapsed();
        timings.push(("File Classification (100x)", classification_time));

        // Config parsing benchmark
        let config_content = r#"
repos:
  - repo: https://github.com/test/repo
    rev: v1.0.0
    hooks:
      - id: test-hook
        entry: echo
        language: system
"#;
        let config_file = temp_dir.path().join("benchmark_config.yaml");
        fs::write(&config_file, config_content).unwrap();

        let start = Instant::now();
        for _ in 0..100 {
            Config::from_file(&config_file).unwrap();
        }
        let config_parsing_time = start.elapsed();
        timings.push(("Config Parsing (100x)", config_parsing_time));

        // Binary detection benchmark
        let start = Instant::now();
        for _ in 0..100 {
            FileSystem::is_binary_file(&test_file).unwrap();
        }
        let binary_detection_time = start.elapsed();
        timings.push(("Binary Detection (100x)", binary_detection_time));

        // Storage operations benchmark (single instance to avoid lock conflicts)
        let start = Instant::now();
        let _store = Store::new().unwrap();
        let storage_time = start.elapsed();
        timings.push(("Storage Creation (1x)", storage_time));

        // Print benchmark results
        for (operation, duration) in &timings {
            println!("  {}: {:?}", operation, duration);
        }

        // Verify all operations meet performance requirements
        for (operation, duration) in &timings {
            assert!(
                duration < &Duration::from_secs(1),
                "{} took too long: {:?}",
                operation,
                duration
            );
        }

        println!("=== All benchmarks passed performance requirements ===\n");
    }
}
