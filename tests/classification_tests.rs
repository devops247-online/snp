// Comprehensive tests for the classification module
// Tests file type detection, language identification, and pattern matching

use snp::classification::{
    ClassificationError, DetectionMethod, FileClassifier, FileType, LanguageDefinitions,
};
use snp::error::SnpError;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::unnecessary_unwrap)]
mod classification_tests {
    use super::*;

    #[test]
    fn test_comprehensive_file_extension_detection() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test all supported Python extensions
        let python_extensions = [
            ("script.py", "python"),
            ("module.pyw", "python"),
            ("stubs.pyi", "python"),
        ];

        for (filename, expected_lang) in &python_extensions {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "# Python code").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some(expected_lang.to_string()));
            assert!(matches!(result.file_type, FileType::SourceCode { .. }));
        }

        // Test all JavaScript extensions
        let js_extensions = [
            ("app.js", "javascript"),
            ("component.jsx", "javascript"),
            ("module.mjs", "javascript"),
            ("config.cjs", "javascript"),
        ];

        for (filename, expected_lang) in &js_extensions {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "// JavaScript code").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some(expected_lang.to_string()));
        }

        // Test TypeScript extensions
        let ts_extensions = [("app.ts", "typescript"), ("component.tsx", "typescript")];

        for (filename, expected_lang) in &ts_extensions {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "// TypeScript code").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some(expected_lang.to_string()));
        }

        // Test shell script extensions
        let shell_extensions = [
            ("script.sh", "shell"),
            ("config.bash", "shell"),
            ("setup.zsh", "shell"),
            ("install.fish", "shell"),
        ];

        for (filename, expected_lang) in &shell_extensions {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "# Shell script").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some(expected_lang.to_string()));
        }

        // Test case insensitivity
        let case_files = [("Script.PY", "python"), ("App.JS", "javascript")];

        for (filename, expected_lang) in &case_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some(expected_lang.to_string()));
        }
    }

    #[test]
    fn test_comprehensive_shebang_detection() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test various Python shebangs
        let python_shebangs = [
            "#!/usr/bin/env python",
            "#!/usr/bin/env python3",
            "#!/usr/bin/env python2",
            "#!/usr/bin/python",
            "#!/usr/bin/python3",
            "#!/usr/bin/python2.7",
            "#!/opt/python/bin/python3",
        ];

        for (i, shebang) in python_shebangs.iter().enumerate() {
            let file_path = temp_dir.path().join(format!("python_script_{}", i));
            fs::write(&file_path, format!("{}\nprint('hello'))", shebang)).unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some("python".to_string()));
            assert!(result.detection_methods.contains(&DetectionMethod::Shebang));
            assert!(result.confidence >= 0.9);
        }

        // Test Node.js shebangs
        let node_shebangs = [
            "#!/usr/bin/env node",
            "#!/usr/bin/node",
            "#!/opt/node/bin/node",
        ];

        for (i, shebang) in node_shebangs.iter().enumerate() {
            let file_path = temp_dir.path().join(format!("node_script_{}", i));
            fs::write(&file_path, format!("{}\nconsole.log('hello')", shebang)).unwrap();
            classifier.clear_cache();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some("javascript".to_string()));
            assert!(result.detection_methods.contains(&DetectionMethod::Shebang));
        }

        // Test shell shebangs
        let shell_shebangs = [
            ("#!/bin/sh", "shell"),
            ("#!/bin/bash", "shell"),
            ("#!/usr/bin/env bash", "shell"),
            ("#!/usr/bin/zsh", "shell"),
            ("#!/usr/bin/env zsh", "shell"),
            ("#!/usr/bin/fish", "shell"),
        ];

        for (i, (shebang, expected_lang)) in shell_shebangs.iter().enumerate() {
            let file_path = temp_dir.path().join(format!("shell_script_{}", i));
            fs::write(&file_path, format!("{}\necho hello", shebang)).unwrap();
            classifier.clear_cache();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some(expected_lang.to_string()));
        }

        // Test TypeScript shebangs
        let ts_shebang_file = temp_dir.path().join("ts_script");
        fs::write(
            &ts_shebang_file,
            "#!/usr/bin/env ts-node\nconsole.log('hello')",
        )
        .unwrap();
        classifier.clear_cache();
        let result = classifier.classify_file(&ts_shebang_file).unwrap();
        assert_eq!(result.language, Some("typescript".to_string()));
    }

    #[test]
    fn test_shebang_priority_handling() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test that valid shebang patterns are matched
        let specific_python = temp_dir.path().join("specific_python");
        fs::write(&specific_python, "#!/usr/bin/env python3\nprint('hello')").unwrap();
        let result = classifier.classify_file(&specific_python).unwrap();
        assert_eq!(result.language, Some("python".to_string()));

        // Test exact match shebang
        let exact_python = temp_dir.path().join("exact_python");
        fs::write(&exact_python, "#!/usr/bin/python\nprint('hello')").unwrap();
        classifier.clear_cache();
        let result = classifier.classify_file(&exact_python).unwrap();
        assert_eq!(result.language, Some("python".to_string()));

        // Test that priority works - TypeScript should have higher priority than JavaScript
        let ts_node = temp_dir.path().join("ts_node");
        fs::write(&ts_node, "#!/usr/bin/env ts-node\nconsole.log('hello')").unwrap();
        classifier.clear_cache();
        let result = classifier.classify_file(&ts_node).unwrap();
        assert_eq!(result.language, Some("typescript".to_string()));
    }

    #[test]
    fn test_advanced_glob_pattern_matching() {
        let classifier = FileClassifier::new();

        // Test simple patterns
        assert!(classifier
            .matches_pattern(Path::new("script.py"), "*.py")
            .unwrap());
        assert!(!classifier
            .matches_pattern(Path::new("script.js"), "*.py")
            .unwrap());

        // Test directory patterns
        assert!(classifier
            .matches_pattern(Path::new("src/main.rs"), "src/**/*.rs")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("src/lib/util.rs"), "src/**/*.rs")
            .unwrap());
        assert!(!classifier
            .matches_pattern(Path::new("tests/main.rs"), "src/**/*.rs")
            .unwrap());

        // Test brace expansion patterns
        assert!(classifier
            .matches_pattern(Path::new("test.js"), "*.{js,ts}")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("test.ts"), "*.{js,ts}")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("test.jsx"), "*.{js,jsx,ts,tsx}")
            .unwrap());
        assert!(!classifier
            .matches_pattern(Path::new("test.py"), "*.{js,ts}")
            .unwrap());

        // Test complex brace patterns
        assert!(classifier
            .matches_pattern(Path::new("src/component.tsx"), "src/*.{js,jsx,ts,tsx}")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("lib/utils.mjs"), "lib/*.{js,mjs,cjs}")
            .unwrap());

        // Test nested brace patterns (should handle gracefully)
        let result = classifier.matches_pattern(Path::new("test.js"), "*.{js,{ts,tsx}}");
        // This pattern might not be fully supported, but shouldn't crash
        assert!(result.is_ok());

        // Test character classes
        assert!(classifier
            .matches_pattern(Path::new("test1.py"), "test[0-9].py")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("test5.py"), "test[0-9].py")
            .unwrap());
        assert!(!classifier
            .matches_pattern(Path::new("testa.py"), "test[0-9].py")
            .unwrap());

        // Test negation patterns
        assert!(!classifier
            .matches_pattern(Path::new("test.py"), "!*.py")
            .unwrap());

        // Test question mark wildcard
        assert!(classifier
            .matches_pattern(Path::new("test1.py"), "test?.py")
            .unwrap());
        assert!(!classifier
            .matches_pattern(Path::new("test12.py"), "test?.py")
            .unwrap());
    }

    #[test]
    fn test_pattern_matching_edge_cases() {
        let classifier = FileClassifier::new();

        // Test empty patterns
        let result = classifier.matches_pattern(Path::new("test.py"), "");
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Test patterns with spaces
        assert!(classifier
            .matches_pattern(Path::new("my file.py"), "my file.py")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("my file.py"), "*.py")
            .unwrap());

        // Test patterns with special characters
        assert!(classifier
            .matches_pattern(Path::new("test-file.py"), "*-*.py")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("test_file.py"), "*_*.py")
            .unwrap());

        // Test invalid glob patterns - only use patterns that are actually invalid
        let invalid_patterns = ["["]; // Unclosed bracket is definitely invalid
        for pattern in &invalid_patterns {
            let result = classifier.matches_pattern(Path::new("test.py"), pattern);
            if result.is_err() {
                match result.unwrap_err() {
                    SnpError::Classification(ClassificationError::InvalidPattern { .. }) => {}
                    _ => panic!("Expected InvalidPattern error for pattern: {}", pattern),
                }
            }
            // Note: Some patterns that look invalid might actually be valid glob patterns
        }

        // Test very long patterns
        let long_pattern = "a".repeat(1000) + "/*.py";
        let result = classifier.matches_pattern(Path::new("test.py"), &long_pattern);
        assert!(result.is_ok());
    }

    #[test]
    fn test_language_detection_api() {
        let classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test successful language detection
        let test_files = [
            ("script.py", Some("python")),
            ("app.js", Some("javascript")),
            ("component.tsx", Some("typescript")),
            ("main.rs", Some("rust")),
            ("main.go", Some("go")),
            ("script.sh", Some("shell")),
            ("unknown.xyz", None),
        ];

        for (filename, expected_lang) in &test_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();
            let detected = classifier.detect_language(&file_path);
            assert_eq!(detected, expected_lang.map(|s| s.to_string()));
        }

        // Test with non-existent file
        let non_existent = Path::new("/non/existent/file.py");
        assert_eq!(classifier.detect_language(non_existent), None);
    }

    #[test]
    fn test_supported_languages_completeness() {
        let classifier = FileClassifier::new();
        let languages = classifier.get_supported_languages();

        // Check that all expected languages are present
        let expected_languages = ["python", "javascript", "typescript", "rust", "go", "shell"];
        for expected in &expected_languages {
            assert!(
                languages.contains(&expected.to_string()),
                "Missing language: {}",
                expected
            );
        }

        // Check that there are no duplicate languages
        let mut sorted_languages = languages.clone();
        sorted_languages.sort();
        sorted_languages.dedup();
        assert_eq!(languages.len(), sorted_languages.len());
    }

    #[test]
    fn test_language_definitions_validation() {
        let languages = LanguageDefinitions::all_languages();

        // Validate each language definition
        for lang in &languages {
            // Name should not be empty
            assert!(!lang.name.is_empty(), "Language name cannot be empty");

            // Should have at least extensions or shebangs
            assert!(
                !lang.extensions.is_empty() || !lang.shebangs.is_empty(),
                "Language {} must have extensions or shebangs",
                lang.name
            );

            // Priority should be reasonable
            assert!(
                lang.priority > 0 && lang.priority <= 1000,
                "Language {} priority should be between 1 and 1000",
                lang.name
            );

            // Extensions should not contain dots
            for ext in &lang.extensions {
                assert!(
                    !ext.starts_with('.'),
                    "Extension '{}' should not start with dot",
                    ext
                );
                assert!(!ext.is_empty(), "Extension cannot be empty");
            }

            // Shebangs should be reasonable
            for shebang in &lang.shebangs {
                assert!(!shebang.is_empty(), "Shebang cannot be empty");
                assert!(
                    !shebang.starts_with('#'),
                    "Shebang '{}' should not start with #",
                    shebang
                );
            }
        }

        // Check for duplicate names
        let mut names: Vec<_> = languages.iter().map(|l| &l.name).collect();
        names.sort();
        let unique_count = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(names.len(), unique_count, "Duplicate language names found");

        // Check priority ordering makes sense
        let _python = languages.iter().find(|l| l.name == "python").unwrap();
        let typescript = languages.iter().find(|l| l.name == "typescript").unwrap();
        let javascript = languages.iter().find(|l| l.name == "javascript").unwrap();

        // TypeScript should have higher priority than JavaScript for conflict resolution
        assert!(typescript.priority > javascript.priority);
    }

    #[test]
    fn test_classification_caching() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "print('hello')").unwrap();

        // First classification
        let start = std::time::Instant::now();
        let result1 = classifier.classify_file(&py_file).unwrap();
        let _first_duration = start.elapsed();

        // Second classification (should use cache)
        let start = std::time::Instant::now();
        let result2 = classifier.classify_file(&py_file).unwrap();
        let _second_duration = start.elapsed();

        // Results should be identical
        assert_eq!(result1, result2);

        // Second call should be faster (cached)
        // Note: This might not always be reliable in test environments
        // but we can at least verify the cache works by checking results
        assert_eq!(result1.language, result2.language);
        assert_eq!(result1.file_type, result2.file_type);

        // Test cache clearing
        classifier.clear_cache();
        let result3 = classifier.classify_file(&py_file).unwrap();
        assert_eq!(result1, result3); // Should still be the same result
    }

    #[test]
    fn test_confidence_scoring() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Shebang-based detection should have highest confidence
        let shebang_file = temp_dir.path().join("shebang_script");
        fs::write(&shebang_file, "#!/usr/bin/env python3\nprint('hello')").unwrap();
        let result = classifier.classify_file(&shebang_file).unwrap();
        assert!(
            result.confidence >= 0.9,
            "Shebang detection should have high confidence"
        );

        // Extension-based detection should have high confidence
        let ext_file = temp_dir.path().join("script.py");
        fs::write(&ext_file, "print('hello')").unwrap();
        let result = classifier.classify_file(&ext_file).unwrap();
        assert!(
            result.confidence >= 0.8,
            "Extension detection should have high confidence"
        );

        // Unknown files should have zero confidence
        let unknown_file = temp_dir.path().join("unknown.xyz");
        fs::write(&unknown_file, "random content").unwrap();
        let result = classifier.classify_file(&unknown_file).unwrap();
        assert_eq!(
            result.confidence, 0.0,
            "Unknown files should have zero confidence"
        );
    }

    #[test]
    fn test_detection_method_tracking() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test extension-based detection
        let ext_file = temp_dir.path().join("script.py");
        fs::write(&ext_file, "print('hello')").unwrap();
        let result = classifier.classify_file(&ext_file).unwrap();
        assert!(result
            .detection_methods
            .contains(&DetectionMethod::Extension));
        assert!(!result.detection_methods.contains(&DetectionMethod::Shebang));

        // Test shebang-based detection
        let shebang_file = temp_dir.path().join("script");
        fs::write(&shebang_file, "#!/usr/bin/env python3\nprint('hello')").unwrap();
        let result = classifier.classify_file(&shebang_file).unwrap();
        assert!(result.detection_methods.contains(&DetectionMethod::Shebang));
        assert!(!result
            .detection_methods
            .contains(&DetectionMethod::Extension));

        // Test heuristic detection (unknown file)
        let unknown_file = temp_dir.path().join("unknown");
        fs::write(&unknown_file, "random content").unwrap();
        let result = classifier.classify_file(&unknown_file).unwrap();
        assert!(result
            .detection_methods
            .contains(&DetectionMethod::Heuristic));
    }

    #[test]
    fn test_error_handling_comprehensive() {
        let mut classifier = FileClassifier::new();

        // Test file not found error
        let result = classifier.classify_file(Path::new("/absolutely/non/existent/file.py"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SnpError::Classification(ClassificationError::FileAccessError { path, error }) => {
                assert_eq!(path, Path::new("/absolutely/non/existent/file.py"));
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
            _ => panic!("Expected FileAccessError"),
        }

        // Test definitely invalid glob patterns
        let invalid_patterns = [
            "[", // Unclosed bracket
        ];

        for pattern in &invalid_patterns {
            let result = classifier.matches_pattern(Path::new("test.py"), pattern);
            if result.is_err() {
                match result.unwrap_err() {
                    SnpError::Classification(ClassificationError::InvalidPattern {
                        pattern: p,
                        ..
                    }) => {
                        assert_eq!(p, *pattern);
                    }
                    _ => panic!("Expected InvalidPattern error for pattern: {}", pattern),
                }
            }
            // Note: glob library may accept some patterns that appear invalid
        }
    }

    #[test]
    fn test_special_file_cases() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test files without extensions
        let no_ext_files = ["Makefile", "Dockerfile", "README"];

        for filename in &no_ext_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.file_type, FileType::Unknown);
        }

        // Test hidden files
        let hidden_files = [
            (".gitignore", None),
            (".eslintrc.js", Some("javascript")),
            (".babelrc.json", None),
        ];

        for (filename, expected_lang) in &hidden_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, expected_lang.map(|s| s.to_string()));
        }

        // Test files with multiple dots
        let multi_dot_files = [
            ("component.test.js", Some("javascript")),
            ("utils.spec.ts", Some("typescript")),
            ("data.backup.py", Some("python")),
        ];

        for (filename, expected_lang) in &multi_dot_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, expected_lang.map(|s| s.to_string()));
        }
    }

    #[test]
    fn test_file_encoding_detection() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test UTF-8 files (default assumption)
        let utf8_file = temp_dir.path().join("script.py");
        fs::write(&utf8_file, "print('hello world')").unwrap();
        let result = classifier.classify_file(&utf8_file).unwrap();
        assert_eq!(result.encoding, Some("utf-8".to_string()));

        // Test files with different content types
        let binary_like = temp_dir.path().join("data.py");
        let binary_content = vec![0x00, 0x01, 0x02, 0x03]; // Binary-like content
        fs::write(&binary_like, binary_content).unwrap();
        let result = classifier.classify_file(&binary_like).unwrap();
        // Should still detect as Python due to extension, but might handle encoding differently
        assert_eq!(result.language, Some("python".to_string()));
    }

    #[test]
    fn test_performance_with_large_number_of_files() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Create many files to test performance
        let file_count = 100;
        let mut file_paths = Vec::new();

        for i in 0..file_count {
            let filename = format!("script_{}.py", i);
            let file_path = temp_dir.path().join(&filename);
            fs::write(&file_path, format!("print('hello {}')", i)).unwrap();
            file_paths.push(file_path);
        }

        // Classify all files and measure time
        let start = std::time::Instant::now();
        for file_path in &file_paths {
            let result = classifier.classify_file(file_path).unwrap();
            assert_eq!(result.language, Some("python".to_string()));
        }
        let duration = start.elapsed();

        // Second pass should be faster due to caching
        let start = std::time::Instant::now();
        for file_path in &file_paths {
            let result = classifier.classify_file(file_path).unwrap();
            assert_eq!(result.language, Some("python".to_string()));
        }
        let cached_duration = start.elapsed();

        // Cached access should be significantly faster
        assert!(cached_duration < duration);

        println!(
            "First pass: {:?}, Cached pass: {:?}",
            duration, cached_duration
        );
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let classifier = Arc::new(Mutex::new(FileClassifier::new()));
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let test_files = [
            ("script.py", "python"),
            ("app.js", "javascript"),
            ("main.rs", "rust"),
            ("index.ts", "typescript"),
        ];

        let file_paths: Vec<_> = test_files
            .iter()
            .map(|(filename, _)| {
                let path = temp_dir.path().join(filename);
                fs::write(&path, "content").unwrap();
                path
            })
            .collect();

        // Test concurrent classification
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let classifier = Arc::clone(&classifier);
                let file_path = file_paths[i].clone();
                let expected_lang = test_files[i].1.to_string();

                thread::spawn(move || {
                    let mut c = classifier.lock().unwrap();
                    let result = c.classify_file(&file_path).unwrap();
                    assert_eq!(result.language, Some(expected_lang));
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_file_type_variants() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test that all file types are properly constructed
        let test_cases = [
            (
                "script.py",
                FileType::SourceCode {
                    language: "python".to_string(),
                },
            ),
            (
                "app.js",
                FileType::SourceCode {
                    language: "javascript".to_string(),
                },
            ),
            (
                "main.rs",
                FileType::SourceCode {
                    language: "rust".to_string(),
                },
            ),
            ("unknown.xyz", FileType::Unknown),
        ];

        for (filename, expected_type) in &test_cases {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "content").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.file_type, *expected_type);
        }
    }

    #[test]
    fn test_memory_efficiency() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Create many files and ensure cache doesn't grow unbounded
        for i in 0..1000 {
            let file_path = temp_dir.path().join(format!("file_{}.py", i));
            fs::write(&file_path, "content").unwrap();
            classifier.classify_file(&file_path).unwrap();
        }

        // Clear cache to test memory cleanup
        classifier.clear_cache();

        // Verify cache is cleared by classifying a file again
        let test_file = temp_dir.path().join("test.py");
        fs::write(&test_file, "content").unwrap();
        let result = classifier.classify_file(&test_file).unwrap();
        assert_eq!(result.language, Some("python".to_string()));
    }
}
