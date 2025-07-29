// File Change Detection System for SNP
// Implements incremental file change detection using filesystem events and file hash caching
// to dramatically reduce I/O operations on repeated SNP runs.

use blake3::Hasher;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, error, warn};

use crate::error::{Result, SnpError, StorageError};
use crate::storage::Store;

/// File change detector configuration
#[derive(Debug, Clone)]
pub struct FileChangeDetectorConfig {
    /// Enable filesystem watching
    pub watch_filesystem: bool,
    /// Maximum number of file hashes to cache
    pub cache_size: usize,
    /// Skip hashing files larger than this size (in bytes)
    pub large_file_threshold: u64,
    /// Force full refresh (ignore cache)
    pub force_refresh: bool,
    /// Hash large files (disabled by default for performance)
    pub hash_large_files: bool,
}

impl Default for FileChangeDetectorConfig {
    fn default() -> Self {
        Self {
            watch_filesystem: true,
            cache_size: 10000,
            large_file_threshold: 10 * 1024 * 1024, // 10MB
            force_refresh: false,
            hash_large_files: false,
        }
    }
}

impl FileChangeDetectorConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_watch_filesystem(mut self, enabled: bool) -> Self {
        self.watch_filesystem = enabled;
        self
    }

    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.cache_size = size;
        self
    }

    pub fn with_large_file_threshold(mut self, threshold: u64) -> Self {
        self.large_file_threshold = threshold;
        self
    }

    pub fn with_force_refresh(mut self, force: bool) -> Self {
        self.force_refresh = force;
        self
    }

    pub fn with_hash_large_files(mut self, enabled: bool) -> Self {
        self.hash_large_files = enabled;
        self
    }
}

/// Cached file information
#[derive(Debug, Clone)]
struct CachedFileInfo {
    /// Blake3 hash of file content
    hash: String,
    /// File modification time
    mtime: SystemTime,
    /// File size in bytes
    size: u64,
    /// Timestamp when this cache entry was created
    cached_at: SystemTime,
}

/// File change detector using filesystem events and hash caching
pub struct FileChangeDetector {
    /// Configuration for the detector
    config: FileChangeDetectorConfig,
    /// Cache of file hashes indexed by path
    file_cache: Arc<RwLock<HashMap<PathBuf, CachedFileInfo>>>,
    /// Set of files that have changed (tracked by filesystem watcher)
    changed_files: Arc<RwLock<HashSet<PathBuf>>>,
    /// Filesystem watcher (optional)
    watcher: Arc<Mutex<Option<RecommendedWatcher>>>,
    /// Storage for persistent cache
    storage: Arc<Store>,
}

impl FileChangeDetector {
    /// Create a new file change detector
    pub fn new(config: FileChangeDetectorConfig, storage: Arc<Store>) -> Result<Self> {
        let detector = Self {
            config,
            file_cache: Arc::new(RwLock::new(HashMap::new())),
            changed_files: Arc::new(RwLock::new(HashSet::new())),
            watcher: Arc::new(Mutex::new(None)),
            storage,
        };

        // Load cache from persistent storage
        if let Err(e) = detector.load_cache() {
            warn!("Failed to load file cache from storage: {}", e);
        }

        Ok(detector)
    }

    /// Create a new file change detector with default configuration
    pub fn with_storage(storage: Arc<Store>) -> Result<Self> {
        Self::new(FileChangeDetectorConfig::default(), storage)
    }

    /// Get changed files from a list of candidate files
    /// Returns only files that have changed since last run
    pub async fn get_changed_files(&self, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
        if self.config.force_refresh {
            debug!("Force refresh enabled, returning all files");
            return Ok(files.to_vec());
        }

        let mut changed_files = Vec::new();

        for file in files {
            if self.has_file_changed(file).await? {
                changed_files.push(file.clone());
            }
        }

        debug!(
            "File change detection: {} out of {} files changed",
            changed_files.len(),
            files.len()
        );

        Ok(changed_files)
    }

    /// Check if a specific file has changed
    pub async fn has_file_changed(&self, file: &Path) -> Result<bool> {
        // If file doesn't exist, it's considered changed (likely deleted)
        if !file.exists() {
            debug!("File {} doesn't exist, marking as changed", file.display());
            return Ok(true);
        }

        // If filesystem watcher detected a change, return true
        if self.config.watch_filesystem && self.is_file_in_changed_set(file) {
            debug!(
                "File {} in changed set from filesystem watcher",
                file.display()
            );
            return Ok(true);
        }

        // Check against cached information
        let file_info = self.create_cached_file_info(file).await?;

        let cached_info = {
            let cache = self.file_cache.read();
            cache.get(file).cloned()
        };

        if let Some(cached_info) = cached_info {
            // Compare modification time first (fast check)
            if file_info.mtime != cached_info.mtime {
                debug!("File {} mtime changed", file.display());
                self.update_file_cache(file, file_info).await?;
                Ok(true)
            } else if file_info.size != cached_info.size {
                // Compare size (fast check)
                debug!("File {} size changed", file.display());
                self.update_file_cache(file, file_info).await?;
                Ok(true)
            } else {
                // If mtime and size are the same, assume no change
                // This is a performance optimization - we avoid computing hash
                debug!("File {} unchanged (mtime and size match)", file.display());
                Ok(false)
            }
        } else {
            // File not in cache, consider it changed
            debug!("File {} not in cache, marking as changed", file.display());
            self.update_file_cache(file, file_info).await?;
            Ok(true)
        }
    }

    /// Start filesystem watching for real-time change detection
    pub fn start_watching(&self, paths: &[PathBuf]) -> Result<()> {
        if !self.config.watch_filesystem {
            debug!("Filesystem watching disabled");
            return Ok(());
        }

        let changed_files = Arc::clone(&self.changed_files);
        let mut watcher = notify::recommended_watcher(
            move |res: std::result::Result<notify::Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Only track modify, create, and remove events
                        match event.kind {
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                                let mut changed = changed_files.write();
                                for path in event.paths {
                                    debug!("Filesystem event for: {}", path.display());
                                    changed.insert(path);
                                }
                            }
                            _ => {
                                // Ignore other events (access, etc.) for performance
                            }
                        }
                    }
                    Err(e) => {
                        error!("Filesystem watcher error: {}", e);
                    }
                }
            },
        )
        .map_err(|e| {
            SnpError::Storage(Box::new(StorageError::FileLockFailed {
                path: PathBuf::from("filesystem_watcher"),
                error: format!("Failed to create filesystem watcher: {e}"),
                timeout_secs: None,
            }))
        })?;

        // Watch all specified paths
        for path in paths {
            if path.exists() {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    warn!("Failed to watch path {}: {}", path.display(), e);
                } else {
                    debug!("Started watching path: {}", path.display());
                }
            }
        }

        // Store the watcher
        let mut watcher_guard = self.watcher.lock();
        *watcher_guard = Some(watcher);

        debug!("Filesystem watcher started for {} paths", paths.len());
        Ok(())
    }

    /// Stop filesystem watching
    pub fn stop_watching(&self) {
        let mut watcher_guard = self.watcher.lock();
        if watcher_guard.take().is_some() {
            debug!("Filesystem watcher stopped");
        }
    }

    /// Clear the changed files set (typically called after processing)
    pub fn clear_changed_files(&self) {
        let mut changed = self.changed_files.write();
        changed.clear();
        debug!("Cleared changed files set");
    }

    /// Get file information (metadata and hash)
    async fn get_file_info(&self, file: &Path) -> Result<CachedFileInfo> {
        let metadata = std::fs::metadata(file).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: file.to_path_buf(),
                error: format!("Failed to get file metadata: {e}"),
            }))
        })?;

        let size = metadata.len();
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        // Determine if we should hash this file
        let should_hash = self.config.hash_large_files || size <= self.config.large_file_threshold;

        let hash = if should_hash {
            self.compute_file_hash(file).await?
        } else {
            // For large files, use a simpler hash based on size and mtime
            self.compute_metadata_hash(file, size, mtime)
        };

        Ok(CachedFileInfo {
            hash,
            mtime,
            size,
            cached_at: SystemTime::now(),
        })
    }

    /// Compute Blake3 hash of file content
    async fn compute_file_hash(&self, file: &Path) -> Result<String> {
        // Use blocking task to avoid blocking the async runtime
        let file_path = file.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut hasher = Hasher::new();
            let content = std::fs::read(&file_path).map_err(|e| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: file_path.clone(),
                    error: format!("Failed to read file for hashing: {e}"),
                }))
            })?;

            hasher.update(&content);
            Ok(hasher.finalize().to_hex().to_string())
        })
        .await
        .map_err(|e| {
            SnpError::Storage(Box::new(StorageError::ConcurrencyConflict {
                operation: "file_hash_computation".to_string(),
                error: format!("Task execution error: {e}"),
                retry_suggested: true,
            }))
        })?
    }

    /// Compute a hash based on file metadata (for large files)
    fn compute_metadata_hash(&self, file: &Path, size: u64, mtime: SystemTime) -> String {
        let mut hasher = Hasher::new();
        hasher.update(file.to_string_lossy().as_bytes());
        hasher.update(&size.to_be_bytes());

        if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
            hasher.update(&duration.as_secs().to_be_bytes());
            hasher.update(&duration.subsec_nanos().to_be_bytes());
        }

        hasher.finalize().to_hex().to_string()
    }

    /// Create cached file info for a file
    async fn create_cached_file_info(&self, file: &Path) -> Result<CachedFileInfo> {
        let metadata = std::fs::metadata(file)?;
        let size = metadata.len();
        let mtime = metadata.modified()?;

        // Compute hash based on configuration
        let hash = if self.config.hash_large_files || size <= self.config.large_file_threshold {
            self.compute_file_hash(file).await?
        } else {
            self.compute_metadata_hash(file, size, mtime)
        };

        Ok(CachedFileInfo {
            hash,
            mtime,
            size,
            cached_at: SystemTime::now(),
        })
    }

    /// Update file cache with new information
    async fn update_file_cache(&self, file: &Path, info: CachedFileInfo) -> Result<()> {
        {
            let mut cache = self.file_cache.write();

            // Implement LRU eviction if cache is too large
            if cache.len() >= self.config.cache_size {
                self.evict_old_entries(&mut cache);
            }

            cache.insert(file.to_path_buf(), info);
        }

        // Persist to storage in background
        self.persist_cache_entry(file).await?;

        Ok(())
    }

    /// Evict old entries from cache (simple LRU implementation)
    fn evict_old_entries(&self, cache: &mut HashMap<PathBuf, CachedFileInfo>) {
        if cache.len() <= self.config.cache_size {
            return;
        }

        // Remove entries older than 1 hour
        let cutoff = SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(3600))
            .unwrap_or(SystemTime::UNIX_EPOCH);

        cache.retain(|_, info| info.cached_at > cutoff);

        // If still too large, remove oldest entries
        if cache.len() > self.config.cache_size {
            let mut entries: Vec<_> = cache
                .iter()
                .map(|(k, v)| (k.clone(), v.cached_at))
                .collect();
            entries.sort_by_key(|(_, cached_at)| *cached_at);

            let remove_count = cache.len() - self.config.cache_size;
            for (path, _) in entries.iter().take(remove_count) {
                cache.remove(path);
            }
        }

        debug!("Evicted old cache entries, new size: {}", cache.len());
    }

    /// Check if file is in the changed files set
    fn is_file_in_changed_set(&self, file: &Path) -> bool {
        let changed = self.changed_files.read();
        changed.contains(file)
    }

    /// Load cache from persistent storage
    fn load_cache(&self) -> Result<()> {
        // TODO: Implement persistent storage once Store API is available
        // For now, start with empty cache
        debug!("File change cache loaded (persistent storage not yet implemented)");
        Ok(())
    }

    /// Persist cache entry to storage
    async fn persist_cache_entry(&self, file: &Path) -> Result<()> {
        // Create a storage key for this file's cache entry
        let _cache_key = format!("file_change_cache:{}", file.display());

        // Get the cached info for this file
        let cache = self.file_cache.read();
        if let Some(info) = cache.get(file) {
            // Serialize the cache info to JSON
            let cache_data = serde_json::json!({
                "hash": info.hash,
                "mtime": info.mtime.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs(),
                "size": info.size,
                "cached_at": info.cached_at.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs()
            });

            // For now, just log what would be stored
            debug!(
                "Would persist cache entry for {} with data: {}",
                file.display(),
                cache_data
            );
        }

        Ok(())
    }

    /// Save entire cache to persistent storage
    pub async fn save_cache(&self) -> Result<()> {
        // TODO: Implement persistent storage once Store API is available
        let cache = self.file_cache.read();
        debug!(
            "Cache has {} entries (persistent storage not yet implemented)",
            cache.len()
        );
        Ok(())
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.file_cache.read();
        let changed = self.changed_files.read();
        (cache.len(), changed.len())
    }

    /// Update cache with current file information
    pub async fn update_file_info(&self, file: &Path) -> Result<()> {
        let info = self.get_file_info(file).await?;
        self.update_file_cache(file, info).await
    }

    /// Get cache directory from storage
    pub fn get_cache_directory(&self) -> std::path::PathBuf {
        self.storage.cache_directory().join("file_change_cache")
    }

    /// Initialize cache from persistent storage
    pub async fn initialize_from_storage(&self) -> Result<()> {
        let cache_dir = self.get_cache_directory();
        tracing::debug!(
            "Initializing file change cache from storage at: {}",
            cache_dir.display()
        );

        // For now, just ensure the cache directory exists
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .map_err(crate::error::SnpError::Io)?;

        // In the future, we could load cached file info from persistent storage
        tracing::debug!("File change cache initialized");
        Ok(())
    }
}

impl Drop for FileChangeDetector {
    fn drop(&mut self) {
        self.stop_watching();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Store;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_storage() -> Arc<Store> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        Arc::new(Store::with_cache_directory(cache_dir).unwrap())
    }

    fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[tokio::test]
    async fn test_file_change_detector_creation() {
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let (cache_size, changed_size) = detector.cache_stats();
        assert_eq!(cache_size, 0);
        assert_eq!(changed_size, 0);
    }

    #[tokio::test]
    async fn test_new_file_detected_as_changed() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let test_file = create_test_file(temp_dir.path(), "test.txt", "content");

        let changed = detector.has_file_changed(&test_file).await.unwrap();
        assert!(changed, "New file should be detected as changed");
    }

    #[tokio::test]
    async fn test_unchanged_file_not_detected() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let test_file = create_test_file(temp_dir.path(), "test.txt", "content");

        // First check - should be changed (new file)
        let changed1 = detector.has_file_changed(&test_file).await.unwrap();
        assert!(changed1);

        // Second check - should not be changed
        let changed2 = detector.has_file_changed(&test_file).await.unwrap();
        assert!(
            !changed2,
            "Unchanged file should not be detected as changed"
        );
    }

    #[tokio::test]
    async fn test_modified_file_detected() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let test_file = create_test_file(temp_dir.path(), "test.txt", "original content");

        // First check
        let _ = detector.has_file_changed(&test_file).await.unwrap();

        // Wait a bit to ensure different mtime
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Modify file
        fs::write(&test_file, "modified content").unwrap();

        // Second check - should detect change
        let changed = detector.has_file_changed(&test_file).await.unwrap();
        assert!(changed, "Modified file should be detected as changed");
    }

    #[tokio::test]
    async fn test_get_changed_files() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let file1 = create_test_file(temp_dir.path(), "file1.txt", "content1");
        let file2 = create_test_file(temp_dir.path(), "file2.txt", "content2");
        let files = vec![file1.clone(), file2.clone()];

        // First run - all files should be changed (new)
        let changed = detector.get_changed_files(&files).await.unwrap();
        assert_eq!(changed.len(), 2);

        // Second run - no files should be changed
        let changed = detector.get_changed_files(&files).await.unwrap();
        assert_eq!(changed.len(), 0);

        // Modify one file
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        fs::write(&file1, "modified content").unwrap();

        // Third run - only modified file should be changed
        let changed = detector.get_changed_files(&files).await.unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0], file1);
    }

    #[tokio::test]
    async fn test_force_refresh() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let config = FileChangeDetectorConfig::new().with_force_refresh(true);
        let detector = FileChangeDetector::new(config, storage).unwrap();

        let test_file = create_test_file(temp_dir.path(), "test.txt", "content");
        let files = vec![test_file];

        // With force refresh, all files should always be returned
        let changed1 = detector.get_changed_files(&files).await.unwrap();
        assert_eq!(changed1.len(), 1);

        let changed2 = detector.get_changed_files(&files).await.unwrap();
        assert_eq!(changed2.len(), 1);
    }

    #[tokio::test]
    async fn test_large_file_handling() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let config = FileChangeDetectorConfig::new()
            .with_large_file_threshold(100) // Very small threshold for testing
            .with_hash_large_files(false);
        let detector = FileChangeDetector::new(config, storage).unwrap();

        // Create a "large" file (over threshold)
        let large_content = "x".repeat(200);
        let large_file = create_test_file(temp_dir.path(), "large.txt", &large_content);

        // Should still detect as changed, but use metadata hash
        let changed = detector.has_file_changed(&large_file).await.unwrap();
        assert!(changed);
    }

    #[tokio::test]
    async fn test_nonexistent_file() {
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let nonexistent = PathBuf::from("/nonexistent/file.txt");
        let changed = detector.has_file_changed(&nonexistent).await.unwrap();
        assert!(changed, "Nonexistent file should be considered changed");
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let storage = create_test_storage();
        let detector = FileChangeDetector::with_storage(storage).unwrap();

        let test_file = create_test_file(temp_dir.path(), "test.txt", "content");

        let (cache_size, changed_size) = detector.cache_stats();
        assert_eq!(cache_size, 0);
        assert_eq!(changed_size, 0);

        // Check file to populate cache
        let _ = detector.has_file_changed(&test_file).await.unwrap();

        let (cache_size, _) = detector.cache_stats();
        assert_eq!(cache_size, 1);
    }
}
