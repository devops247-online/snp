// File classification system for SNP
// Provides comprehensive file type detection, language identification, and pattern matching

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::error::{Result, SnpError};

/// Classification errors
#[derive(Debug, Error)]
pub enum ClassificationError {
    #[error("Invalid glob pattern: {pattern}")]
    InvalidPattern {
        pattern: String,
        #[source]
        error: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("File access error: {path}")]
    FileAccessError {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[error("Content analysis failed: {path}")]
    ContentAnalysisError {
        path: PathBuf,
        analyzer: String,
        error: String,
    },

    #[error("Encoding detection failed: {path}")]
    EncodingError {
        path: PathBuf,
        detected_bytes: Vec<u8>,
    },
}

/// File classification result
#[derive(Debug, Clone, PartialEq)]
pub struct FileClassification {
    pub file_type: FileType,
    pub language: Option<String>,
    pub encoding: Option<String>,
    pub confidence: f32,
    pub detection_methods: Vec<DetectionMethod>,
}

/// File type categories
#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    SourceCode { language: String },
    Configuration { format: String },
    Documentation { format: String },
    Binary { mime_type: Option<String> },
    Text { encoding: String },
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetectionMethod {
    Extension,
    Shebang,
    Content,
    MagicNumber,
    Heuristic,
}

/// Language detection configuration
#[derive(Debug, Clone)]
pub struct LanguageDetection {
    pub name: String,
    pub extensions: Vec<String>,
    pub shebangs: Vec<String>,
    pub content_patterns: Vec<String>,
    pub priority: u32,
}

/// Main file classifier
pub struct FileClassifier {
    extension_map: HashMap<String, FileType>,
    language_definitions: Vec<LanguageDetection>,
    classification_cache: HashMap<PathBuf, FileClassification>,
}

impl FileClassifier {
    pub fn new() -> Self {
        let mut classifier = Self {
            extension_map: HashMap::new(),
            language_definitions: Vec::new(),
            classification_cache: HashMap::new(),
        };
        classifier.load_builtin_definitions();
        classifier
    }

    fn load_builtin_definitions(&mut self) {
        self.language_definitions = LanguageDefinitions::all_languages();

        // Build extension map from language definitions
        for lang in &self.language_definitions {
            for ext in &lang.extensions {
                self.extension_map.insert(
                    ext.clone(),
                    FileType::SourceCode {
                        language: lang.name.clone(),
                    },
                );
            }
        }
    }

    /// Core classification methods
    pub fn classify_file(&mut self, path: &Path) -> Result<FileClassification> {
        // Check cache first
        if let Some(cached) = self.classification_cache.get(path) {
            return Ok(cached.clone());
        }

        let classification = self.classify_file_internal(path)?;
        self.classification_cache
            .insert(path.to_path_buf(), classification.clone());
        Ok(classification)
    }

    fn classify_file_internal(&self, path: &Path) -> Result<FileClassification> {
        // First, check if file exists (for error handling)
        if !path.exists() {
            return Err(SnpError::Classification(
                ClassificationError::FileAccessError {
                    path: path.to_path_buf(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
                },
            ));
        }

        let mut detection_methods = Vec::new();
        let mut confidence = 0.0;

        // Try shebang detection first (highest priority)
        if let Ok(Some(file_type)) = self.classify_by_shebang(path) {
            detection_methods.push(DetectionMethod::Shebang);
            confidence += 0.9;

            let language = match &file_type {
                FileType::SourceCode { language } => Some(language.clone()),
                _ => None,
            };

            return Ok(FileClassification {
                file_type,
                language,
                encoding: Some("utf-8".to_string()),
                confidence,
                detection_methods,
            });
        }

        // Try extension-based detection
        if let Some(file_type) = self.classify_by_extension(path) {
            detection_methods.push(DetectionMethod::Extension);
            confidence += 0.8;

            let language = match &file_type {
                FileType::SourceCode { language } => Some(language.clone()),
                _ => None,
            };

            return Ok(FileClassification {
                file_type,
                language,
                encoding: Some("utf-8".to_string()),
                confidence,
                detection_methods,
            });
        }

        // Default to unknown
        Ok(FileClassification {
            file_type: FileType::Unknown,
            language: None,
            encoding: None,
            confidence: 0.0,
            detection_methods: vec![DetectionMethod::Heuristic],
        })
    }

    fn classify_by_extension(&self, path: &Path) -> Option<FileType> {
        let extension = path.extension()?.to_str()?.to_lowercase();
        self.extension_map.get(&extension).cloned()
    }

    fn classify_by_shebang(&self, path: &Path) -> Result<Option<FileType>> {
        let file = File::open(path).map_err(|e| {
            SnpError::Classification(ClassificationError::FileAccessError {
                path: path.to_path_buf(),
                error: e,
            })
        })?;

        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).map_err(|e| {
            SnpError::Classification(ClassificationError::FileAccessError {
                path: path.to_path_buf(),
                error: e,
            })
        })?;

        if first_line.starts_with("#!") {
            let shebang = first_line.trim();
            // Sort by priority (higher priority first) to match most specific patterns first
            let mut sorted_langs = self.language_definitions.clone();
            sorted_langs.sort_by(|a, b| b.priority.cmp(&a.priority));

            for lang in &sorted_langs {
                for shebang_pattern in &lang.shebangs {
                    // Use word boundary matching to avoid false matches
                    // Check for /pattern, /pattern followed by number, or ending with pattern
                    let path_match = shebang.contains(&format!("/{shebang_pattern}"))
                        && (shebang.ends_with(shebang_pattern)
                            || shebang
                                .chars()
                                .nth(
                                    shebang.find(&format!("/{shebang_pattern}")).unwrap()
                                        + shebang_pattern.len()
                                        + 1,
                                )
                                .is_none_or(|c| c.is_ascii_digit()));

                    let space_match = shebang.contains(&format!(" {shebang_pattern}"))
                        && shebang.ends_with(shebang_pattern);

                    if path_match || space_match {
                        return Ok(Some(FileType::SourceCode {
                            language: lang.name.clone(),
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Pattern matching
    pub fn matches_pattern(&self, path: &Path, pattern: &str) -> Result<bool> {
        // Handle brace expansion patterns like *.{js,ts}
        if pattern.contains('{') && pattern.contains('}') {
            return self.matches_brace_pattern(path, pattern);
        }

        let glob = glob::Pattern::new(pattern).map_err(|e| {
            SnpError::Classification(ClassificationError::InvalidPattern {
                pattern: pattern.to_string(),
                error: Box::new(e),
            })
        })?;

        Ok(glob.matches_path(path))
    }

    fn matches_brace_pattern(&self, path: &Path, pattern: &str) -> Result<bool> {
        // Simple brace expansion for patterns like *.{js,ts}
        if let Some(start) = pattern.find('{') {
            if let Some(end) = pattern.find('}') {
                let prefix = &pattern[..start];
                let suffix = &pattern[end + 1..];
                let options = &pattern[start + 1..end];

                for option in options.split(',') {
                    let expanded_pattern = format!("{prefix}{option}{suffix}");
                    let glob = glob::Pattern::new(&expanded_pattern).map_err(|e| {
                        SnpError::Classification(ClassificationError::InvalidPattern {
                            pattern: expanded_pattern,
                            error: Box::new(e),
                        })
                    })?;

                    if glob.matches_path(path) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// Language detection
    pub fn detect_language(&self, path: &Path) -> Option<String> {
        if let Ok(classification) = self.classify_file_internal(path) {
            classification.language
        } else {
            None
        }
    }

    pub fn get_supported_languages(&self) -> Vec<String> {
        self.language_definitions
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<String>>()
    }

    /// Performance utilities
    pub fn clear_cache(&mut self) {
        self.classification_cache.clear();
    }
}

impl Default for FileClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in language definitions
pub struct LanguageDefinitions;

impl LanguageDefinitions {
    pub fn python() -> LanguageDetection {
        LanguageDetection {
            name: "python".to_string(),
            extensions: vec!["py", "pyw", "pyi"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            shebangs: vec!["python", "python3", "python2"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            content_patterns: vec![
                r"^#!/usr/bin/env python".to_string(),
                r"^#!/usr/bin/python".to_string(),
                r"^# -\*- coding:".to_string(),
            ],
            priority: 100,
        }
    }

    pub fn javascript() -> LanguageDetection {
        LanguageDetection {
            name: "javascript".to_string(),
            extensions: vec!["js", "jsx", "mjs", "cjs"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            shebangs: vec!["node"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            content_patterns: vec![
                r"^#!/usr/bin/env node".to_string(),
                r"^#!/usr/bin/node".to_string(),
            ],
            priority: 90,
        }
    }

    pub fn typescript() -> LanguageDetection {
        LanguageDetection {
            name: "typescript".to_string(),
            extensions: vec!["ts", "tsx"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            shebangs: vec!["ts-node"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            content_patterns: vec![r"^#!/usr/bin/env ts-node".to_string()],
            priority: 95,
        }
    }

    pub fn rust() -> LanguageDetection {
        LanguageDetection {
            name: "rust".to_string(),
            extensions: vec!["rs".to_string()],
            shebangs: vec![],
            content_patterns: vec![],
            priority: 85,
        }
    }

    pub fn go() -> LanguageDetection {
        LanguageDetection {
            name: "go".to_string(),
            extensions: vec!["go".to_string()],
            shebangs: vec![],
            content_patterns: vec![],
            priority: 80,
        }
    }

    pub fn shell() -> LanguageDetection {
        LanguageDetection {
            name: "shell".to_string(),
            extensions: vec!["sh", "bash", "zsh", "fish"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            shebangs: vec!["sh", "bash", "zsh", "fish"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>(),
            content_patterns: vec![
                r"^#!/bin/sh".to_string(),
                r"^#!/bin/bash".to_string(),
                r"^#!/usr/bin/env bash".to_string(),
            ],
            priority: 75,
        }
    }

    pub fn all_languages() -> Vec<LanguageDetection> {
        vec![
            Self::python(),
            Self::javascript(),
            Self::typescript(),
            Self::rust(),
            Self::go(),
            Self::shell(),
        ]
    }
}

// Add to the error module's SnpError enum
impl From<ClassificationError> for SnpError {
    fn from(error: ClassificationError) -> Self {
        SnpError::Classification(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_extension_detection() {
        let mut classifier = FileClassifier::new();

        // Test basic extension-based classification
        let temp_dir = TempDir::new().unwrap();

        // Python file
        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "print('hello')").unwrap();
        let result = classifier.classify_file(&py_file).unwrap();
        assert_eq!(
            result.file_type,
            FileType::SourceCode {
                language: "python".to_string()
            }
        );
        assert_eq!(result.language, Some("python".to_string()));
        assert!(result
            .detection_methods
            .contains(&DetectionMethod::Extension));

        // JavaScript file
        let js_file = temp_dir.path().join("script.js");
        fs::write(&js_file, "console.log('hello')").unwrap();
        let result = classifier.classify_file(&js_file).unwrap();
        assert_eq!(
            result.file_type,
            FileType::SourceCode {
                language: "javascript".to_string()
            }
        );
        assert_eq!(result.language, Some("javascript".to_string()));

        // Rust file
        let rs_file = temp_dir.path().join("main.rs");
        fs::write(&rs_file, "fn main() {}").unwrap();
        let result = classifier.classify_file(&rs_file).unwrap();
        assert_eq!(
            result.file_type,
            FileType::SourceCode {
                language: "rust".to_string()
            }
        );

        // Test multiple extension handling
        let test_js_file = temp_dir.path().join("component.test.js");
        fs::write(&test_js_file, "test('example', () => {})").unwrap();
        let result = classifier.classify_file(&test_js_file).unwrap();
        // Should still detect as JavaScript based on final extension
        assert_eq!(result.language, Some("javascript".to_string()));

        // Test case sensitivity and normalization
        let py_upper_file = temp_dir.path().join("Script.PY");
        fs::write(&py_upper_file, "print('hello')").unwrap();
        let result = classifier.classify_file(&py_upper_file).unwrap();
        assert_eq!(result.language, Some("python".to_string()));

        // Test files without extensions
        let no_ext_file = temp_dir.path().join("Makefile");
        fs::write(&no_ext_file, "all:\n\techo hello").unwrap();
        let result = classifier.classify_file(&no_ext_file).unwrap();
        assert_eq!(result.file_type, FileType::Unknown);
    }

    #[test]
    fn test_content_based_detection() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test shebang line detection for Python
        let python_shebang = temp_dir.path().join("script");
        fs::write(&python_shebang, "#!/usr/bin/env python3\nprint('hello')").unwrap();
        let result = classifier.classify_file(&python_shebang).unwrap();
        assert_eq!(
            result.file_type,
            FileType::SourceCode {
                language: "python".to_string()
            }
        );
        assert!(result.detection_methods.contains(&DetectionMethod::Shebang));

        // Test shebang line detection for bash
        let bash_shebang = temp_dir.path().join("script.sh");
        fs::write(&bash_shebang, "#!/bin/bash\necho hello").unwrap();
        let result = classifier.classify_file(&bash_shebang).unwrap();
        // Extension takes precedence, but both methods should work
        assert_eq!(result.language, Some("shell".to_string()));

        // Test Node.js shebang
        let node_shebang = temp_dir.path().join("node_script");
        fs::write(&node_shebang, "#!/usr/bin/env node\nconsole.log('hello')").unwrap();
        classifier.clear_cache(); // Clear cache to avoid conflicts
        let result = classifier.classify_file(&node_shebang).unwrap();

        assert_eq!(
            result.file_type,
            FileType::SourceCode {
                language: "javascript".to_string()
            }
        );

        // Test files without shebang (should fall back to unknown)
        let no_shebang = temp_dir.path().join("data");
        fs::write(&no_shebang, "some random data").unwrap();
        let result = classifier.classify_file(&no_shebang).unwrap();
        assert_eq!(result.file_type, FileType::Unknown);
    }

    #[test]
    fn test_glob_pattern_matching() {
        let classifier = FileClassifier::new();

        // Test basic glob patterns
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
        assert!(!classifier
            .matches_pattern(Path::new("tests/main.rs"), "src/**/*.rs")
            .unwrap());

        // Test complex patterns with brackets
        assert!(classifier
            .matches_pattern(Path::new("test.js"), "*.{js,ts}")
            .unwrap());
        assert!(classifier
            .matches_pattern(Path::new("test.ts"), "*.{js,ts}")
            .unwrap());
        assert!(!classifier
            .matches_pattern(Path::new("test.py"), "*.{js,ts}")
            .unwrap());

        // Test invalid patterns should return error
        let result = classifier.matches_pattern(Path::new("test.py"), "[invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_language_specific_classification() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test Python file detection
        let python_files = ["script.py", "module.pyw", "typing.pyi"];
        for filename in &python_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "# Python code").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some("python".to_string()));
        }

        // Test JavaScript/TypeScript detection
        let js_files = ["app.js", "component.jsx", "module.mjs", "config.cjs"];
        for filename in &js_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "// JavaScript code").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some("javascript".to_string()));
        }

        let ts_files = ["app.ts", "component.tsx"];
        for filename in &ts_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "// TypeScript code").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some("typescript".to_string()));
        }

        // Test shell scripts
        let shell_files = ["script.sh", "config.bash", "setup.zsh"];
        for filename in &shell_files {
            let file_path = temp_dir.path().join(filename);
            fs::write(&file_path, "# Shell script").unwrap();
            let result = classifier.classify_file(&file_path).unwrap();
            assert_eq!(result.language, Some("shell".to_string()));
        }

        // Test Rust files
        let rust_file = temp_dir.path().join("main.rs");
        fs::write(&rust_file, "fn main() {}").unwrap();
        let result = classifier.classify_file(&rust_file).unwrap();
        assert_eq!(result.language, Some("rust".to_string()));

        // Test Go files
        let go_file = temp_dir.path().join("main.go");
        fs::write(&go_file, "package main").unwrap();
        let result = classifier.classify_file(&go_file).unwrap();
        assert_eq!(result.language, Some("go".to_string()));
    }

    #[test]
    fn test_performance_optimization() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test caching of classification results
        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "print('hello')").unwrap();

        // First classification should cache the result
        let result1 = classifier.classify_file(&py_file).unwrap();
        assert_eq!(result1.language, Some("python".to_string()));

        // Second classification should use cache (same result)
        let result2 = classifier.classify_file(&py_file).unwrap();
        assert_eq!(result1, result2);

        // Test cache clearing
        classifier.clear_cache();
        let result3 = classifier.classify_file(&py_file).unwrap();
        assert_eq!(result1, result3); // Should still be the same, just recalculated
    }

    #[test]
    fn test_language_definitions_completeness() {
        let languages = LanguageDefinitions::all_languages();

        // Test that we have the expected core languages
        let language_names: Vec<&str> = languages.iter().map(|l| l.name.as_str()).collect();
        assert!(language_names.contains(&"python"));
        assert!(language_names.contains(&"javascript"));
        assert!(language_names.contains(&"typescript"));
        assert!(language_names.contains(&"rust"));
        assert!(language_names.contains(&"go"));
        assert!(language_names.contains(&"shell"));

        // Test that each language has reasonable configuration
        for lang in &languages {
            assert!(!lang.name.is_empty());
            assert!(!lang.extensions.is_empty() || !lang.shebangs.is_empty());
            assert!(lang.priority > 0);
        }

        // Test specific language configurations
        let python = LanguageDefinitions::python();
        assert_eq!(python.name, "python");
        assert!(python.extensions.contains(&"py".to_string()));
        assert!(python.shebangs.contains(&"python".to_string()));

        let javascript = LanguageDefinitions::javascript();
        assert_eq!(javascript.name, "javascript");
        assert!(javascript.extensions.contains(&"js".to_string()));
        assert!(javascript.shebangs.contains(&"node".to_string()));
    }

    #[test]
    fn test_supported_languages_api() {
        let classifier = FileClassifier::new();
        let languages = classifier.get_supported_languages();

        assert!(languages.contains(&"python".to_string()));
        assert!(languages.contains(&"javascript".to_string()));
        assert!(languages.contains(&"rust".to_string()));
        assert!(languages.len() >= 6); // At least the core languages
    }

    #[test]
    fn test_detect_language_api() {
        let classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Test language detection
        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "print('hello')").unwrap();
        assert_eq!(
            classifier.detect_language(&py_file),
            Some("python".to_string())
        );

        let js_file = temp_dir.path().join("app.js");
        fs::write(&js_file, "console.log('hello')").unwrap();
        assert_eq!(
            classifier.detect_language(&js_file),
            Some("javascript".to_string())
        );

        let unknown_file = temp_dir.path().join("data.unknown");
        fs::write(&unknown_file, "random data").unwrap();
        assert_eq!(classifier.detect_language(&unknown_file), None);
    }

    #[test]
    fn test_file_classification_confidence() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Extension-based detection should have high confidence
        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "print('hello')").unwrap();
        let result = classifier.classify_file(&py_file).unwrap();
        assert!(result.confidence >= 0.8);

        // Shebang-based detection should have very high confidence
        let shebang_file = temp_dir.path().join("script");
        fs::write(&shebang_file, "#!/usr/bin/python3\nprint('hello')").unwrap();
        let result = classifier.classify_file(&shebang_file).unwrap();
        assert!(result.confidence >= 0.9);

        // Unknown files should have low confidence
        let unknown_file = temp_dir.path().join("data.unknown");
        fs::write(&unknown_file, "random data").unwrap();
        let result = classifier.classify_file(&unknown_file).unwrap();
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_detection_methods_tracking() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Extension-based detection
        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "print('hello')").unwrap();
        let result = classifier.classify_file(&py_file).unwrap();
        assert!(result
            .detection_methods
            .contains(&DetectionMethod::Extension));

        // Shebang-based detection
        let shebang_file = temp_dir.path().join("script");
        fs::write(&shebang_file, "#!/usr/bin/python3\nprint('hello')").unwrap();
        let result = classifier.classify_file(&shebang_file).unwrap();
        assert!(result.detection_methods.contains(&DetectionMethod::Shebang));

        // Unknown file should use heuristic
        let unknown_file = temp_dir.path().join("data.unknown");
        fs::write(&unknown_file, "random data").unwrap();
        let result = classifier.classify_file(&unknown_file).unwrap();
        assert!(result
            .detection_methods
            .contains(&DetectionMethod::Heuristic));
    }

    #[test]
    fn test_error_handling() {
        let classifier = FileClassifier::new();

        // Test invalid pattern error
        let result = classifier.matches_pattern(Path::new("test.py"), "[invalid");
        assert!(result.is_err());
        match result.unwrap_err() {
            SnpError::Classification(ClassificationError::InvalidPattern { pattern, .. }) => {
                assert_eq!(pattern, "[invalid");
            }
            _ => panic!("Expected InvalidPattern error"),
        }

        // Test file access error (non-existent file)
        let mut classifier = FileClassifier::new();
        let result = classifier.classify_file(Path::new("/non/existent/file.py"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SnpError::Classification(ClassificationError::FileAccessError { path, .. }) => {
                assert_eq!(path, Path::new("/non/existent/file.py"));
            }
            _ => panic!("Expected FileAccessError"),
        }
    }

    #[test]
    fn test_edge_cases() {
        let mut classifier = FileClassifier::new();
        let temp_dir = TempDir::new().unwrap();

        // Empty file
        let empty_file = temp_dir.path().join("empty.py");
        fs::write(&empty_file, "").unwrap();
        let result = classifier.classify_file(&empty_file).unwrap();
        assert_eq!(result.language, Some("python".to_string()));

        // File with only shebang
        let shebang_only = temp_dir.path().join("script");
        fs::write(&shebang_only, "#!/usr/bin/python3").unwrap();
        let result = classifier.classify_file(&shebang_only).unwrap();
        assert_eq!(result.language, Some("python".to_string()));

        // File with unusual extension case
        let mixed_case = temp_dir.path().join("Script.Py");
        fs::write(&mixed_case, "print('hello')").unwrap();
        let result = classifier.classify_file(&mixed_case).unwrap();
        assert_eq!(result.language, Some("python".to_string()));

        // Multiple extensions
        let multi_ext = temp_dir.path().join("component.test.js");
        fs::write(&multi_ext, "test('hello', () => {})").unwrap();
        let result = classifier.classify_file(&multi_ext).unwrap();
        assert_eq!(result.language, Some("javascript".to_string()));
    }
}
