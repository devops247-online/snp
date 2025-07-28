// File System Utilities for SNP
// Comprehensive file system operations with cross-platform path handling,
// file type detection, and efficient file processing utilities.

use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{self, StreamExt};
use tokio::sync::Semaphore;

use crate::error::{ConfigError, Result, SnpError};

/// File system utilities for cross-platform operations
pub struct FileSystem;

impl FileSystem {
    /// Normalize path for cross-platform compatibility
    /// Converts backslashes to forward slashes on Windows for regex consistency
    pub fn normalize_path(path: &Path) -> PathBuf {
        // On Windows, convert backslashes to forward slashes for consistency
        // This matches the behavior in pre-commit Python implementation
        #[cfg(windows)]
        {
            let path_str = path.to_string_lossy();
            let normalized = path_str.replace('\\', "/");
            PathBuf::from(normalized)
        }

        #[cfg(not(windows))]
        {
            path.to_path_buf()
        }
    }

    /// Check if a file is binary based on content analysis
    pub fn is_binary_file(path: &Path) -> Result<bool> {
        if !path.exists() {
            return Err(SnpError::Config(Box::new(ConfigError::NotFound {
                path: path.to_path_buf(),
                suggestion: Some("File does not exist".to_string()),
            })));
        }

        let mut file = fs::File::open(path).map_err(|e| {
            SnpError::Config(Box::new(ConfigError::IOError {
                message: format!("Failed to open file: {e}"),
                path: Some(path.to_path_buf()),
            }))
        })?;

        // Read first 8192 bytes to check for binary content
        let mut buffer = vec![0u8; 8192];
        let bytes_read = file.read(&mut buffer).map_err(|e| {
            SnpError::Config(Box::new(ConfigError::IOError {
                message: format!("Failed to read file: {e}"),
                path: Some(path.to_path_buf()),
            }))
        })?;

        // Check for null bytes (common indicator of binary files)
        let is_binary = buffer[..bytes_read].contains(&0);

        Ok(is_binary)
    }

    /// Detect file types based on extension and content
    pub fn detect_file_type(path: &Path) -> Result<Vec<String>> {
        let mut types = Vec::new();

        // Add "file" type to all files (matches identify library behavior)
        types.push("file".to_string());

        // Extension-based detection
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                match ext_str.to_lowercase().as_str() {
                    "py" | "pyi" => {
                        types.push("python".to_string());
                        if ext_str == "pyi" {
                            types.push("pyi".to_string());
                        }
                    }
                    "rs" => types.push("rust".to_string()),
                    "js" => types.push("javascript".to_string()),
                    "ts" => types.push("typescript".to_string()),
                    "tsx" => {
                        types.push("typescript".to_string());
                        types.push("tsx".to_string());
                    }
                    "jsx" => {
                        types.push("javascript".to_string());
                        types.push("jsx".to_string());
                    }
                    "json" => types.push("json".to_string()),
                    "yaml" | "yml" => types.push("yaml".to_string()),
                    "toml" => types.push("toml".to_string()),
                    "md" => types.push("markdown".to_string()),
                    "txt" => types.push("text".to_string()),
                    "sh" => types.push("shell".to_string()),
                    "bash" => types.push("bash".to_string()),
                    "zsh" => types.push("zsh".to_string()),
                    "fish" => types.push("fish".to_string()),
                    "dockerfile" => types.push("dockerfile".to_string()),
                    _ => {}
                }
            }
        }

        // Check if it's a binary file
        if Self::is_binary_file(path)? {
            types.push("binary".to_string());
        } else {
            types.push("text".to_string());
        }

        // Special case for Dockerfile without extension
        if let Some(filename) = path.file_name() {
            if let Some(name_str) = filename.to_str() {
                if name_str.to_lowercase() == "dockerfile" {
                    types.push("dockerfile".to_string());
                }
            }
        }

        Ok(types)
    }

    /// Atomically write content to a file
    /// Creates a temporary file and renames it to prevent corruption
    pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
        let temp_path = path.with_extension(format!(
            "{}.tmp.{}",
            path.extension().and_then(|s| s.to_str()).unwrap_or(""),
            std::process::id()
        ));

        // Write to temporary file first
        {
            let mut temp_file = fs::File::create(&temp_path).map_err(|e| {
                SnpError::Config(Box::new(ConfigError::IOError {
                    message: format!("Failed to create temporary file: {e}"),
                    path: Some(temp_path.clone()),
                }))
            })?;

            temp_file.write_all(content).map_err(|e| {
                SnpError::Config(Box::new(ConfigError::IOError {
                    message: format!("Failed to write to temporary file: {e}"),
                    path: Some(temp_path.clone()),
                }))
            })?;

            temp_file.flush().map_err(|e| {
                SnpError::Config(Box::new(ConfigError::IOError {
                    message: format!("Failed to flush temporary file: {e}"),
                    path: Some(temp_path.clone()),
                }))
            })?;
        }

        // Atomically rename temporary file to target path
        fs::rename(&temp_path, path).map_err(|e| {
            // Clean up temporary file on failure
            let _ = fs::remove_file(&temp_path);
            SnpError::Config(Box::new(ConfigError::IOError {
                message: format!("Failed to rename temporary file: {e}"),
                path: Some(path.to_path_buf()),
            }))
        })?;

        Ok(())
    }

    /// Create a backup of a file
    pub fn backup_file(path: &Path) -> Result<PathBuf> {
        if !path.exists() {
            return Err(SnpError::Config(Box::new(ConfigError::NotFound {
                path: path.to_path_buf(),
                suggestion: Some("Cannot backup non-existent file".to_string()),
            })));
        }

        let backup_path = path.with_extension(format!(
            "{}.backup.{}",
            path.extension().and_then(|s| s.to_str()).unwrap_or(""),
            std::process::id()
        ));

        fs::copy(path, &backup_path).map_err(|e| {
            SnpError::Config(Box::new(ConfigError::IOError {
                message: format!("Failed to create backup: {e}"),
                path: Some(path.to_path_buf()),
            }))
        })?;

        Ok(backup_path)
    }

    /// Async version of detect_file_type
    pub async fn detect_file_type_async(path: &Path) -> Result<Vec<String>> {
        let mut types = Vec::new();

        // Add "file" type to all files (matches identify library behavior)
        types.push("file".to_string());

        // Extension-based detection (synchronous, no I/O needed)
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                match ext_str.to_lowercase().as_str() {
                    "py" | "pyi" => {
                        types.push("python".to_string());
                        if ext_str == "pyi" {
                            types.push("pyi".to_string());
                        }
                    }
                    "rs" => types.push("rust".to_string()),
                    "js" => types.push("javascript".to_string()),
                    "ts" => types.push("typescript".to_string()),
                    "tsx" => {
                        types.push("typescript".to_string());
                        types.push("tsx".to_string());
                    }
                    "jsx" => {
                        types.push("javascript".to_string());
                        types.push("jsx".to_string());
                    }
                    "json" => types.push("json".to_string()),
                    "yaml" | "yml" => types.push("yaml".to_string()),
                    "toml" => types.push("toml".to_string()),
                    "md" => types.push("markdown".to_string()),
                    "txt" => types.push("text".to_string()),
                    "sh" => types.push("shell".to_string()),
                    "bash" => types.push("bash".to_string()),
                    "zsh" => types.push("zsh".to_string()),
                    "fish" => types.push("fish".to_string()),
                    "dockerfile" => types.push("dockerfile".to_string()),
                    _ => {}
                }
            }
        }

        // Check if it's a binary file (async I/O)
        if Self::is_binary_file_async(path).await? {
            types.push("binary".to_string());
        } else {
            types.push("text".to_string());
        }

        // Special case for Dockerfile without extension
        if let Some(filename) = path.file_name() {
            if let Some(name_str) = filename.to_str() {
                if name_str.to_lowercase() == "dockerfile" {
                    types.push("dockerfile".to_string());
                }
            }
        }

        Ok(types)
    }

    /// Async version of is_binary_file
    pub async fn is_binary_file_async(path: &Path) -> Result<bool> {
        if !tokio::fs::try_exists(path).await.map_err(|e| {
            SnpError::Config(Box::new(ConfigError::IOError {
                message: format!("Failed to check if file exists: {e}"),
                path: Some(path.to_path_buf()),
            }))
        })? {
            return Err(SnpError::Config(Box::new(ConfigError::NotFound {
                path: path.to_path_buf(),
                suggestion: Some("File does not exist".to_string()),
            })));
        }

        let mut file = tokio::fs::File::open(path).await.map_err(|e| {
            SnpError::Config(Box::new(ConfigError::IOError {
                message: format!("Failed to open file: {e}"),
                path: Some(path.to_path_buf()),
            }))
        })?;

        // Read first 8192 bytes to check for binary content
        let mut buffer = vec![0u8; 8192];
        let bytes_read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
            .await
            .map_err(|e| {
                SnpError::Config(Box::new(ConfigError::IOError {
                    message: format!("Failed to read file: {e}"),
                    path: Some(path.to_path_buf()),
                }))
            })?;

        // Check for null bytes (common indicator of binary files)
        let is_binary = buffer[..bytes_read].contains(&0);

        Ok(is_binary)
    }
}

/// File filter for pattern matching and type-based filtering
pub struct FileFilter {
    include_patterns: Vec<Regex>,
    exclude_patterns: Vec<Regex>,
    file_types: HashSet<String>,
    exclude_types: HashSet<String>,
}

impl Clone for FileFilter {
    fn clone(&self) -> Self {
        Self {
            include_patterns: self.include_patterns.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            file_types: self.file_types.clone(),
            exclude_types: self.exclude_types.clone(),
        }
    }
}

impl FileFilter {
    /// Create a new file filter
    pub fn new() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            file_types: HashSet::new(),
            exclude_types: HashSet::new(),
        }
    }

    /// Add include patterns (files that match these patterns will be included)
    pub fn with_include_patterns(mut self, patterns: Vec<String>) -> Result<Self> {
        for pattern in patterns {
            let regex = Regex::new(&pattern).map_err(|e| {
                SnpError::Config(Box::new(ConfigError::InvalidRegex {
                    pattern: pattern.clone(),
                    field: "include".to_string(),
                    error: e.to_string(),
                    file_path: None,
                    line: None,
                }))
            })?;
            self.include_patterns.push(regex);
        }
        Ok(self)
    }

    /// Add exclude patterns (files that match these patterns will be excluded)
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Result<Self> {
        for pattern in patterns {
            let regex = Regex::new(&pattern).map_err(|e| {
                SnpError::Config(Box::new(ConfigError::InvalidRegex {
                    pattern: pattern.clone(),
                    field: "exclude".to_string(),
                    error: e.to_string(),
                    file_path: None,
                    line: None,
                }))
            })?;
            self.exclude_patterns.push(regex);
        }
        Ok(self)
    }

    /// Add file types to include
    pub fn with_file_types(mut self, types: Vec<String>) -> Self {
        self.file_types.extend(types);
        self
    }

    /// Add file types to exclude
    pub fn with_exclude_types(mut self, types: Vec<String>) -> Self {
        self.exclude_types.extend(types);
        self
    }

    /// Check if a file matches this filter
    pub fn matches(&self, path: &Path) -> Result<bool> {
        let normalized_path = FileSystem::normalize_path(path);
        let path_str = normalized_path.to_string_lossy();

        // Check exclude patterns first (early exit if excluded)
        for exclude_pattern in &self.exclude_patterns {
            if exclude_pattern.is_match(&path_str) {
                return Ok(false);
            }
        }

        // Check include patterns (if any are specified, at least one must match)
        if !self.include_patterns.is_empty() {
            let mut matches_include = false;
            for include_pattern in &self.include_patterns {
                if include_pattern.is_match(&path_str) {
                    matches_include = true;
                    break;
                }
            }
            if !matches_include {
                return Ok(false);
            }
        }

        // Check file type filtering
        if !self.file_types.is_empty() || !self.exclude_types.is_empty() {
            let detected_types = FileSystem::detect_file_type(path)?;
            let type_set: HashSet<String> = detected_types.into_iter().collect();

            // Check exclude types first
            if !self.exclude_types.is_empty() && !self.exclude_types.is_disjoint(&type_set) {
                return Ok(false);
            }

            // Check include types (if specified, file must have at least one matching type)
            if !self.file_types.is_empty() && self.file_types.is_disjoint(&type_set) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Filter a list of files based on this filter
    pub fn filter_files(&self, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let mut filtered = Vec::new();

        for file in files {
            if self.matches(file)? {
                filtered.push(file.clone());
            }
        }

        Ok(filtered)
    }

    /// Filter a list of files using arena allocation for better performance
    /// Returns an arena-allocated slice of filtered files
    pub fn filter_files_arena<'arena>(
        &self,
        arena: &'arena bumpalo::Bump,
        files: &[PathBuf],
    ) -> Result<&'arena [PathBuf]> {
        use bumpalo::collections::Vec as BumpVec;

        let mut arena_filtered = BumpVec::new_in(arena);

        for file in files {
            if self.matches(file)? {
                // Clone the PathBuf and allocate it in the arena
                let arena_path = arena.alloc(file.clone());
                arena_filtered.push(arena_path.clone());
            }
        }

        Ok(arena_filtered.into_bump_slice())
    }

    /// Async version of matches for individual file checking
    pub async fn matches_async(&self, path: &Path) -> Result<bool> {
        let normalized_path = FileSystem::normalize_path(path);
        let path_str = normalized_path.to_string_lossy();

        // Check exclude patterns first (early exit if excluded)
        for exclude_pattern in &self.exclude_patterns {
            if exclude_pattern.is_match(&path_str) {
                return Ok(false);
            }
        }

        // Check include patterns (if any are specified, at least one must match)
        if !self.include_patterns.is_empty() {
            let mut matches_include = false;
            for include_pattern in &self.include_patterns {
                if include_pattern.is_match(&path_str) {
                    matches_include = true;
                    break;
                }
            }
            if !matches_include {
                return Ok(false);
            }
        }

        // Check file type filtering
        if !self.file_types.is_empty() || !self.exclude_types.is_empty() {
            let detected_types = FileSystem::detect_file_type_async(path).await?;
            let type_set: HashSet<String> = detected_types.into_iter().collect();

            // Check exclude types first
            if !self.exclude_types.is_empty() && !self.exclude_types.is_disjoint(&type_set) {
                return Ok(false);
            }

            // Check include types (if specified, file must have at least one matching type)
            if !self.file_types.is_empty() && self.file_types.is_disjoint(&type_set) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Async version of filter_files with configurable concurrency and batching
    pub async fn filter_files_async(
        &self,
        files: &[PathBuf],
        config: Option<AsyncConfig>,
    ) -> Result<Vec<PathBuf>> {
        let config = config.unwrap_or_default();
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_files));

        let filter = self.clone();

        let results = stream::iter(files)
            .map(|file| {
                let sem = Arc::clone(&semaphore);
                let filter = filter.clone();
                let file = file.clone();

                async move {
                    let _permit = sem.acquire().await.map_err(|e| {
                        SnpError::Config(Box::new(ConfigError::IOError {
                            message: format!("Failed to acquire semaphore permit: {e}"),
                            path: Some(file.clone()),
                        }))
                    })?;

                    // Add timeout wrapper for I/O operations
                    let timeout_duration = config.io_timeout;
                    let result =
                        tokio::time::timeout(timeout_duration, filter.matches_async(&file)).await;

                    match result {
                        Ok(Ok(matches)) => {
                            if matches {
                                Ok(Some(file))
                            } else {
                                Ok(None)
                            }
                        }
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(SnpError::Config(Box::new(ConfigError::IOError {
                            message: format!("File operation timed out after {timeout_duration:?}"),
                            path: Some(file),
                        }))),
                    }
                }
            })
            .buffer_unordered(config.max_concurrent_files)
            .collect::<Vec<_>>()
            .await;

        // Collect successful results and handle errors
        let mut filtered_files = Vec::new();
        for result in results {
            match result {
                Ok(Some(file)) => filtered_files.push(file),
                Ok(None) => {} // File didn't match filter
                Err(e) => return Err(e),
            }
        }

        Ok(filtered_files)
    }
}

impl Default for FileFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for async file operations
#[derive(Debug, Clone)]
pub struct AsyncConfig {
    /// Maximum number of concurrent file operations (default: 64)
    pub max_concurrent_files: usize,
    /// Batch size for processing files (default: 100)
    pub batch_size: usize,
    /// I/O timeout for individual file operations (default: 30s)
    pub io_timeout: Duration,
    /// Number of retry attempts for transient I/O failures (default: 3)
    pub retry_attempts: usize,
}

impl Default for AsyncConfig {
    fn default() -> Self {
        Self {
            max_concurrent_files: 64,
            batch_size: 100,
            io_timeout: Duration::from_secs(30),
            retry_attempts: 3,
        }
    }
}

impl AsyncConfig {
    /// Create a new AsyncConfig with custom concurrency limit
    pub fn with_concurrency(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent_files = max_concurrent;
        self
    }

    /// Create a new AsyncConfig with custom batch size
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Create a new AsyncConfig with custom timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.io_timeout = timeout;
        self
    }

    /// Create a new AsyncConfig with custom retry attempts
    pub fn with_retry_attempts(mut self, attempts: usize) -> Self {
        self.retry_attempts = attempts;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        path
    }

    fn create_binary_file(dir: &Path, name: &str) -> PathBuf {
        let binary_content = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD];
        create_test_file(dir, name, &binary_content)
    }

    fn create_text_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        create_test_file(dir, name, content.as_bytes())
    }

    #[test]
    fn test_path_normalization_windows() {
        // Test Windows path conversion to forward slashes
        let path = Path::new("src\\main.rs");
        let normalized = FileSystem::normalize_path(path);

        #[cfg(windows)]
        {
            assert_eq!(normalized.to_string_lossy(), "src/main.rs");
        }

        #[cfg(not(windows))]
        {
            // On non-Windows systems, paths should remain unchanged
            assert_eq!(normalized, path);
        }
    }

    #[test]
    fn test_path_normalization_unix() {
        // Test Unix paths remain unchanged
        let path = Path::new("src/main.rs");
        let normalized = FileSystem::normalize_path(path);
        assert_eq!(normalized, path);
    }

    #[test]
    fn test_path_normalization_relative() {
        // Test relative path resolution
        let path = Path::new("./src/../lib/main.rs");
        let normalized = FileSystem::normalize_path(path);

        // Should preserve the relative structure (normalization doesn't resolve ..)
        let expected_str = "./src/../lib/main.rs";
        assert_eq!(normalized.to_string_lossy(), expected_str);
    }

    #[test]
    fn test_symlink_handling() {
        // Test symlink path normalization
        let temp_dir = TempDir::new().unwrap();
        let _target_file = create_text_file(temp_dir.path(), "target.txt", "content");

        // Note: Creating symlinks on Windows requires special permissions
        // This test will pass on Unix systems and skip on Windows in CI
        #[cfg(unix)]
        {
            let link_path = temp_dir.path().join("link.txt");
            std::os::unix::fs::symlink("target.txt", &link_path).unwrap();

            let normalized = FileSystem::normalize_path(&link_path);
            assert_eq!(normalized, link_path);
        }
    }

    #[test]
    fn test_file_type_detection_python() {
        let temp_dir = TempDir::new().unwrap();
        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");

        let types = FileSystem::detect_file_type(&py_file).unwrap();
        assert!(types.contains(&"file".to_string()));
        assert!(types.contains(&"python".to_string()));
        assert!(types.contains(&"text".to_string()));
        assert!(!types.contains(&"binary".to_string()));
    }

    #[test]
    fn test_file_type_detection_rust() {
        let temp_dir = TempDir::new().unwrap();
        let rs_file = create_text_file(temp_dir.path(), "main.rs", "fn main() {}");

        let types = FileSystem::detect_file_type(&rs_file).unwrap();
        assert!(types.contains(&"file".to_string()));
        assert!(types.contains(&"rust".to_string()));
        assert!(types.contains(&"text".to_string()));
    }

    #[test]
    fn test_file_type_detection_python_stub() {
        let temp_dir = TempDir::new().unwrap();
        let pyi_file = create_text_file(temp_dir.path(), "types.pyi", "def func() -> int: ...");

        let types = FileSystem::detect_file_type(&pyi_file).unwrap();
        assert!(types.contains(&"python".to_string()));
        assert!(types.contains(&"pyi".to_string()));
    }

    #[test]
    fn test_file_type_detection_javascript() {
        let temp_dir = TempDir::new().unwrap();
        let js_file = create_text_file(temp_dir.path(), "app.js", "console.log('hello');");

        let types = FileSystem::detect_file_type(&js_file).unwrap();
        assert!(types.contains(&"javascript".to_string()));
    }

    #[test]
    fn test_file_type_detection_typescript() {
        let temp_dir = TempDir::new().unwrap();
        let ts_file = create_text_file(temp_dir.path(), "app.ts", "const x: number = 1;");

        let types = FileSystem::detect_file_type(&ts_file).unwrap();
        assert!(types.contains(&"typescript".to_string()));
    }

    #[test]
    fn test_file_type_detection_jsx() {
        let temp_dir = TempDir::new().unwrap();
        let jsx_file = create_text_file(temp_dir.path(), "component.jsx", "<div>Hello</div>");

        let types = FileSystem::detect_file_type(&jsx_file).unwrap();
        assert!(types.contains(&"javascript".to_string()));
        assert!(types.contains(&"jsx".to_string()));
    }

    #[test]
    fn test_file_type_detection_tsx() {
        let temp_dir = TempDir::new().unwrap();
        let tsx_file = create_text_file(temp_dir.path(), "component.tsx", "<div>Hello</div>");

        let types = FileSystem::detect_file_type(&tsx_file).unwrap();
        assert!(types.contains(&"typescript".to_string()));
        assert!(types.contains(&"tsx".to_string()));
    }

    #[test]
    fn test_file_type_detection_dockerfile() {
        let temp_dir = TempDir::new().unwrap();
        let dockerfile = create_text_file(temp_dir.path(), "Dockerfile", "FROM ubuntu:20.04");

        let types = FileSystem::detect_file_type(&dockerfile).unwrap();
        assert!(types.contains(&"dockerfile".to_string()));
    }

    #[test]
    fn test_file_type_detection_content_based() {
        let temp_dir = TempDir::new().unwrap();
        let binary_file = create_binary_file(temp_dir.path(), "data.bin");

        let types = FileSystem::detect_file_type(&binary_file).unwrap();
        assert!(types.contains(&"binary".to_string()));
        assert!(!types.contains(&"text".to_string()));
    }

    #[test]
    fn test_file_type_detection_binary_vs_text() {
        let temp_dir = TempDir::new().unwrap();

        // Text file
        let text_file = create_text_file(temp_dir.path(), "readme.txt", "Hello world");
        let text_types = FileSystem::detect_file_type(&text_file).unwrap();
        assert!(text_types.contains(&"text".to_string()));
        assert!(!text_types.contains(&"binary".to_string()));

        // Binary file
        let binary_file = create_binary_file(temp_dir.path(), "data.bin");
        let binary_types = FileSystem::detect_file_type(&binary_file).unwrap();
        assert!(binary_types.contains(&"binary".to_string()));
        assert!(!binary_types.contains(&"text".to_string()));
    }

    #[test]
    fn test_is_binary_file() {
        let temp_dir = TempDir::new().unwrap();

        // Text file should not be binary
        let text_file = create_text_file(temp_dir.path(), "text.txt", "Hello world");
        assert!(!FileSystem::is_binary_file(&text_file).unwrap());

        // Binary file should be binary
        let binary_file = create_binary_file(temp_dir.path(), "binary.bin");
        assert!(FileSystem::is_binary_file(&binary_file).unwrap());
    }

    #[test]
    fn test_is_binary_file_nonexistent() {
        let nonexistent = Path::new("/nonexistent/file.txt");
        let result = FileSystem::is_binary_file(nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_filtering_regex_include() {
        let temp_dir = TempDir::new().unwrap();
        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let js_file = create_text_file(temp_dir.path(), "app.js", "console.log('hello')");
        let txt_file = create_text_file(temp_dir.path(), "readme.txt", "Hello");

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.py$".to_string()])
            .unwrap();

        assert!(filter.matches(&py_file).unwrap());
        assert!(!filter.matches(&js_file).unwrap());
        assert!(!filter.matches(&txt_file).unwrap());
    }

    #[test]
    fn test_file_filtering_regex_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let script_py = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let test_py = create_text_file(temp_dir.path(), "test_script.py", "print('test')");

        let filter = FileFilter::new()
            .with_exclude_patterns(vec![r"test_.*\.py$".to_string()])
            .unwrap();

        assert!(filter.matches(&script_py).unwrap());
        assert!(!filter.matches(&test_py).unwrap());
    }

    #[test]
    fn test_file_filtering_type_include() {
        let temp_dir = TempDir::new().unwrap();
        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let js_file = create_text_file(temp_dir.path(), "app.js", "console.log('hello')");

        let filter = FileFilter::new().with_file_types(vec!["python".to_string()]);

        assert!(filter.matches(&py_file).unwrap());
        assert!(!filter.matches(&js_file).unwrap());
    }

    #[test]
    fn test_file_filtering_type_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let binary_file = create_binary_file(temp_dir.path(), "data.bin");

        let filter = FileFilter::new().with_exclude_types(vec!["binary".to_string()]);

        assert!(filter.matches(&py_file).unwrap());
        assert!(!filter.matches(&binary_file).unwrap());
    }

    #[test]
    fn test_file_filtering_performance_large_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut files = Vec::new();

        // Create 1000 test files
        for i in 0..1000 {
            let filename = format!("file_{i}.py");
            files.push(create_text_file(
                temp_dir.path(),
                &filename,
                "print('hello')",
            ));
        }

        let filter = FileFilter::new().with_file_types(vec!["python".to_string()]);

        let start = std::time::Instant::now();
        let filtered = filter.filter_files(&files).unwrap();
        let duration = start.elapsed();

        assert_eq!(filtered.len(), 1000);
        // Should complete within reasonable time (< 100ms for 1000 files)
        assert!(duration.as_millis() < 100);
    }

    #[test]
    fn test_atomic_write_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = b"Hello, world!";

        FileSystem::atomic_write(&file_path, content).unwrap();

        let written_content = fs::read(&file_path).unwrap();
        assert_eq!(written_content, content);
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create initial file
        fs::write(&file_path, b"original").unwrap();

        // Atomically overwrite
        let new_content = b"updated content";
        FileSystem::atomic_write(&file_path, new_content).unwrap();

        let written_content = fs::read(&file_path).unwrap();
        assert_eq!(written_content, new_content);
    }

    #[test]
    fn test_atomic_write_no_temp_file_left() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = b"Hello, world!";

        FileSystem::atomic_write(&file_path, content).unwrap();

        // Check that no temporary files are left behind
        let entries: Vec<_> = fs::read_dir(temp_dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1); // Only the target file should exist
        assert_eq!(entries[0].as_ref().unwrap().file_name(), "test.txt");
    }

    #[test]
    fn test_atomic_write_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("concurrent.txt");

        // Simulate concurrent writes (this is a basic test)
        let content1 = b"Thread 1 content";
        let content2 = b"Thread 2 content";

        FileSystem::atomic_write(&file_path, content1).unwrap();
        FileSystem::atomic_write(&file_path, content2).unwrap();

        let final_content = fs::read(&file_path).unwrap();
        assert_eq!(final_content, content2); // Last write wins
    }

    #[test]
    fn test_backup_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let original_path = temp_dir.path().join("original.txt");
        let content = b"original content";

        fs::write(&original_path, content).unwrap();

        let backup_path = FileSystem::backup_file(&original_path).unwrap();

        // Backup should exist and have same content
        assert!(backup_path.exists());
        let backup_content = fs::read(&backup_path).unwrap();
        assert_eq!(backup_content, content);

        // Original should still exist
        assert!(original_path.exists());
        let original_content = fs::read(&original_path).unwrap();
        assert_eq!(original_content, content);
    }

    #[test]
    fn test_backup_file_nonexistent() {
        let nonexistent = Path::new("/nonexistent/file.txt");
        let result = FileSystem::backup_file(nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_and_restore_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("data.txt");
        let original_content = b"original data";
        let modified_content = b"modified data";

        // Create original file
        fs::write(&file_path, original_content).unwrap();

        // Create backup
        let backup_path = FileSystem::backup_file(&file_path).unwrap();

        // Modify original
        FileSystem::atomic_write(&file_path, modified_content).unwrap();

        // Verify modification
        let current_content = fs::read(&file_path).unwrap();
        assert_eq!(current_content, modified_content);

        // Restore from backup
        let backup_content = fs::read(&backup_path).unwrap();
        FileSystem::atomic_write(&file_path, &backup_content).unwrap();

        // Verify restoration
        let restored_content = fs::read(&file_path).unwrap();
        assert_eq!(restored_content, original_content);
    }

    #[test]
    fn test_filter_files_comprehensive() {
        let temp_dir = TempDir::new().unwrap();

        let files = vec![
            create_text_file(temp_dir.path(), "main.py", "print('hello')"),
            create_text_file(temp_dir.path(), "test_main.py", "print('test')"),
            create_text_file(temp_dir.path(), "app.js", "console.log('hello')"),
            create_text_file(temp_dir.path(), "lib.rs", "fn main() {}"),
            create_binary_file(temp_dir.path(), "data.bin"),
        ];

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.(py|rs)$".to_string()])
            .unwrap()
            .with_exclude_patterns(vec![r"test_.*".to_string()])
            .unwrap()
            .with_exclude_types(vec!["binary".to_string()]);

        let filtered = filter.filter_files(&files).unwrap();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|p| p.file_name().unwrap() == "main.py"));
        assert!(filtered.iter().any(|p| p.file_name().unwrap() == "lib.rs"));
    }

    #[test]
    fn test_filter_invalid_regex() {
        let result = FileFilter::new().with_include_patterns(vec!["[invalid".to_string()]);
        assert!(result.is_err());

        let result = FileFilter::new().with_exclude_patterns(vec!["*invalid*".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_filter_empty_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let file = create_text_file(temp_dir.path(), "any.txt", "content");

        let filter = FileFilter::new();
        assert!(filter.matches(&file).unwrap());
    }

    #[test]
    fn test_file_filter_multiple_include_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let rs_file = create_text_file(temp_dir.path(), "main.rs", "fn main() {}");
        let js_file = create_text_file(temp_dir.path(), "app.js", "console.log('hello')");

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.py$".to_string(), r"\.rs$".to_string()])
            .unwrap();

        assert!(filter.matches(&py_file).unwrap());
        assert!(filter.matches(&rs_file).unwrap());
        assert!(!filter.matches(&js_file).unwrap());
    }

    #[test]
    fn test_file_filter_multiple_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let main_py = create_text_file(temp_dir.path(), "main.py", "print('hello')");
        let test_py = create_text_file(temp_dir.path(), "test_main.py", "print('test')");
        let cache_py = create_text_file(temp_dir.path(), "__pycache__/module.py", "");

        let filter = FileFilter::new()
            .with_exclude_patterns(vec![r"test_.*".to_string(), r"__pycache__".to_string()])
            .unwrap();

        assert!(filter.matches(&main_py).unwrap());
        assert!(!filter.matches(&test_py).unwrap());
        assert!(!filter.matches(&cache_py).unwrap());
    }

    #[test]
    fn test_file_filter_type_intersection() {
        let temp_dir = TempDir::new().unwrap();
        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let binary_file = create_binary_file(temp_dir.path(), "script.py.bin");

        let filter =
            FileFilter::new().with_file_types(vec!["python".to_string(), "text".to_string()]);

        assert!(filter.matches(&py_file).unwrap());
        assert!(!filter.matches(&binary_file).unwrap());
    }

    #[test]
    fn test_complex_file_patterns() {
        let temp_dir = TempDir::new().unwrap();

        let files = vec![
            create_text_file(temp_dir.path(), "src/main.py", "print('hello')"),
            create_text_file(temp_dir.path(), "tests/test_main.py", "print('test')"),
            create_text_file(temp_dir.path(), "docs/readme.md", "# Title"),
            create_text_file(temp_dir.path(), "target/debug/app", "binary"),
            create_text_file(temp_dir.path(), "lib/utils.py", "def func(): pass"),
        ];

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.(py|md)$".to_string()])
            .unwrap()
            .with_exclude_patterns(vec![r"(test_|target/|__pycache__)".to_string()])
            .unwrap()
            .with_file_types(vec!["text".to_string()]);

        let filtered = filter.filter_files(&files).unwrap();

        assert_eq!(filtered.len(), 3);
        assert!(filtered
            .iter()
            .any(|p| p.to_string_lossy().contains("src/main.py")));
        assert!(filtered
            .iter()
            .any(|p| p.to_string_lossy().contains("lib/utils.py")));
        assert!(filtered
            .iter()
            .any(|p| p.to_string_lossy().contains("docs/readme.md")));
    }

    // Async tests
    #[tokio::test]
    async fn test_async_config_default() {
        let config = AsyncConfig::default();
        assert_eq!(config.max_concurrent_files, 64);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.io_timeout, Duration::from_secs(30));
        assert_eq!(config.retry_attempts, 3);
    }

    #[tokio::test]
    async fn test_async_config_builder() {
        let config = AsyncConfig::default()
            .with_concurrency(32)
            .with_batch_size(50)
            .with_timeout(Duration::from_secs(10))
            .with_retry_attempts(5);

        assert_eq!(config.max_concurrent_files, 32);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.io_timeout, Duration::from_secs(10));
        assert_eq!(config.retry_attempts, 5);
    }

    #[tokio::test]
    async fn test_async_binary_detection() {
        let temp_dir = TempDir::new().unwrap();

        let text_file = create_text_file(temp_dir.path(), "text.txt", "Hello world");
        assert!(!FileSystem::is_binary_file_async(&text_file).await.unwrap());

        let binary_file = create_binary_file(temp_dir.path(), "binary.bin");
        assert!(FileSystem::is_binary_file_async(&binary_file)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_async_file_type_detection() {
        let temp_dir = TempDir::new().unwrap();

        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let types = FileSystem::detect_file_type_async(&py_file).await.unwrap();

        assert!(types.contains(&"file".to_string()));
        assert!(types.contains(&"python".to_string()));
        assert!(types.contains(&"text".to_string()));
        assert!(!types.contains(&"binary".to_string()));
    }

    #[tokio::test]
    async fn test_async_file_filter_basic() {
        let temp_dir = TempDir::new().unwrap();

        let files = vec![
            create_text_file(temp_dir.path(), "main.py", "print('hello')"),
            create_text_file(temp_dir.path(), "app.js", "console.log('hello')"),
            create_text_file(temp_dir.path(), "readme.txt", "Hello"),
        ];

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.py$".to_string()])
            .unwrap();

        let filtered = filter.filter_files_async(&files, None).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].file_name().unwrap() == "main.py");
    }

    #[tokio::test]
    async fn test_async_file_filter_with_config() {
        let temp_dir = TempDir::new().unwrap();

        let files = vec![
            create_text_file(temp_dir.path(), "file1.py", "print('1')"),
            create_text_file(temp_dir.path(), "file2.py", "print('2')"),
            create_text_file(temp_dir.path(), "file3.py", "print('3')"),
            create_text_file(temp_dir.path(), "file4.py", "print('4')"),
        ];

        let filter = FileFilter::new().with_file_types(vec!["python".to_string()]);

        let config = AsyncConfig::default().with_concurrency(2);
        let filtered = filter
            .filter_files_async(&files, Some(config))
            .await
            .unwrap();

        assert_eq!(filtered.len(), 4);
    }

    #[tokio::test]
    async fn test_async_file_filter_large_set() {
        let temp_dir = TempDir::new().unwrap();

        let mut files = Vec::new();
        for i in 0..100 {
            let filename = format!("file_{i}.py");
            files.push(create_text_file(
                temp_dir.path(),
                &filename,
                "print('hello')",
            ));
        }

        let filter = FileFilter::new().with_file_types(vec!["python".to_string()]);

        let start = std::time::Instant::now();
        let filtered = filter.filter_files_async(&files, None).await.unwrap();
        let duration = start.elapsed();

        assert_eq!(filtered.len(), 100);
        assert!(duration.as_millis() < 1000);
    }

    #[tokio::test]
    async fn test_async_file_filter_concurrent_performance() {
        let temp_dir = TempDir::new().unwrap();

        let mut files = Vec::new();
        for i in 0..1000 {
            let filename = format!("file_{i}.py");
            files.push(create_text_file(
                temp_dir.path(),
                &filename,
                "print('hello')",
            ));
        }

        let filter = FileFilter::new().with_file_types(vec!["python".to_string()]);

        let config = AsyncConfig::default().with_concurrency(128);

        let start = std::time::Instant::now();
        let filtered = filter
            .filter_files_async(&files, Some(config))
            .await
            .unwrap();
        let async_duration = start.elapsed();

        let start = std::time::Instant::now();
        let sync_filtered = filter.filter_files(&files).unwrap();
        let sync_duration = start.elapsed();

        assert_eq!(filtered.len(), 1000);
        assert_eq!(sync_filtered.len(), 1000);

        println!("Async duration: {async_duration:?}, Sync duration: {sync_duration:?}");
    }

    #[tokio::test]
    async fn test_async_matches_individual() {
        let temp_dir = TempDir::new().unwrap();

        let py_file = create_text_file(temp_dir.path(), "script.py", "print('hello')");
        let js_file = create_text_file(temp_dir.path(), "app.js", "console.log('hello')");

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.py$".to_string()])
            .unwrap();

        assert!(filter.matches_async(&py_file).await.unwrap());
        assert!(!filter.matches_async(&js_file).await.unwrap());
    }

    #[tokio::test]
    async fn test_async_error_handling_nonexistent_file() {
        let nonexistent = PathBuf::from("/nonexistent/file.txt");
        let result = FileSystem::is_binary_file_async(&nonexistent).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_mixed_file_types() {
        let temp_dir = TempDir::new().unwrap();

        let files = vec![
            create_text_file(temp_dir.path(), "script.py", "print('hello')"),
            create_text_file(temp_dir.path(), "app.js", "console.log('hello')"),
            create_text_file(temp_dir.path(), "main.rs", "fn main() {}"),
            create_binary_file(temp_dir.path(), "data.bin"),
            create_text_file(temp_dir.path(), "readme.md", "# Title"),
        ];

        let filter = FileFilter::new()
            .with_include_patterns(vec![r"\.(py|rs|md)$".to_string()])
            .unwrap()
            .with_exclude_types(vec!["binary".to_string()]);

        let filtered = filter.filter_files_async(&files, None).await.unwrap();

        assert_eq!(filtered.len(), 3);
        assert!(filtered
            .iter()
            .any(|p| p.file_name().unwrap() == "script.py"));
        assert!(filtered.iter().any(|p| p.file_name().unwrap() == "main.rs"));
        assert!(filtered
            .iter()
            .any(|p| p.file_name().unwrap() == "readme.md"));
        assert!(!filtered.iter().any(|p| p.file_name().unwrap() == "app.js"));
        assert!(!filtered
            .iter()
            .any(|p| p.file_name().unwrap() == "data.bin"));
    }
}
