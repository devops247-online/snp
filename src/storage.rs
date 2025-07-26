// Storage System for SNP - SQLite-based caching for repositories, environments, and configuration
// Provides compatibility with pre-commit's store functionality while optimizing for performance and reliability

use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

use crate::error::{Result, SnpError, StorageError};

/// Current database schema version for migrations
const SCHEMA_VERSION: u32 = 1;

/// Default cache directory name
const CACHE_DIR_NAME: &str = "snp";

/// SQLite-based storage system for caching repository clones, hook environments, and configuration tracking
#[derive(Clone)]
pub struct Store {
    cache_dir: PathBuf,
    readonly: bool,
    connection: Arc<Mutex<Connection>>,
}

/// File lock handle for exclusive access to the store
pub struct StoreLock {
    _file: fs::File,
}

/// Repository information stored in the cache
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryInfo {
    pub url: String,
    pub revision: String,
    pub path: PathBuf,
    pub last_used: SystemTime,
    pub dependencies: Vec<String>,
}

/// Environment information stored in the cache
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentInfo {
    pub language: String,
    pub dependencies: Vec<String>,
    pub path: PathBuf,
    pub last_used: SystemTime,
}

/// Configuration file usage tracking
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigInfo {
    pub path: PathBuf,
    pub last_used: SystemTime,
}

impl Store {
    /// Create a new Store instance with the default cache directory
    pub fn new() -> Result<Self> {
        let cache_dir = Self::get_default_cache_directory()?;
        Self::with_cache_directory(cache_dir)
    }

    /// Create a new Store instance with a custom cache directory
    pub fn with_cache_directory(cache_dir: PathBuf) -> Result<Self> {
        let db_path = cache_dir.join("db.db");
        let readonly = cache_dir.exists() && !Self::is_writable(&cache_dir)?;

        // Create cache directory if it doesn't exist
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).map_err(|e| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: cache_dir.clone(),
                    error: e.to_string(),
                }))
            })?;

            // Create README file to explain the directory purpose
            let readme_path = cache_dir.join("README");
            let readme_content = format!(
                "This directory is maintained by the SNP (Shell Not Pass) project.\n\
                 Learn more: https://github.com/devops247-online/snp\n\
                 \n\
                 Directory structure:\n\
                 - db.db: SQLite database for metadata\n\
                 - repos/: Git repository clones\n\
                 - envs/: Language environments\n\
                 \n\
                 Last updated: {:?}\n",
                SystemTime::now()
            );

            if let Err(e) = fs::write(&readme_path, readme_content) {
                tracing::warn!("Failed to create README in cache directory: {}", e);
            }
        }

        // Initialize database connection
        let connection = Self::initialize_database(&db_path, readonly)?;
        let connection = Arc::new(Mutex::new(connection));

        Ok(Store {
            cache_dir,
            readonly,
            connection,
        })
    }

    /// Get the default cache directory following XDG conventions
    pub fn get_default_cache_directory() -> Result<PathBuf> {
        // Use process-specific directory name to avoid conflicts during parallel testing
        let dir_name = if std::env::var("CARGO").is_ok() || cfg!(test) {
            // During tests or when running under cargo, use process ID to create unique cache directories
            format!("{}-{}", CACHE_DIR_NAME, std::process::id())
        } else {
            CACHE_DIR_NAME.to_string()
        };

        let cache_dir = if let Ok(snp_home) = std::env::var("SNP_HOME") {
            PathBuf::from(snp_home)
        } else if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg_cache).join(&dir_name)
        } else {
            let home = std::env::var("HOME").map_err(|_| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: PathBuf::from("$HOME"),
                    error: "HOME environment variable not set".to_string(),
                }))
            })?;
            PathBuf::from(home).join(".cache").join(&dir_name)
        };

        Ok(cache_dir)
    }

    /// Check if a directory is writable
    fn is_writable(path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(true); // Will be created
        }

        // Try to create a temporary file in the directory
        match tempfile::NamedTempFile::new_in(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
            Err(e) => Err(SnpError::Storage(Box::new(
                StorageError::CacheDirectoryFailed {
                    path: path.to_path_buf(),
                    error: e.to_string(),
                },
            ))),
        }
    }

    /// Initialize the SQLite database with proper schema
    fn initialize_database(db_path: &Path, readonly: bool) -> Result<Connection> {
        if readonly && !db_path.exists() {
            return Err(SnpError::Storage(Box::new(
                StorageError::ConnectionFailed {
                    message: "Database does not exist and cannot be created in readonly mode"
                        .to_string(),
                    database_path: Some(db_path.to_path_buf()),
                },
            )));
        }

        let connection = Connection::open(db_path).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::ConnectionFailed {
                message: e.to_string(),
                database_path: Some(db_path.to_path_buf()),
            }))
        })?;

        // Enable foreign keys and WAL mode for better performance and consistency
        connection.execute("PRAGMA foreign_keys = ON", [])?;
        if !readonly {
            // These PRAGMAs return results, so we need to use query methods
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "NORMAL")?;

            // Additional configurations for better concurrent access
            connection.pragma_update(None, "cache_size", "-64000")?; // 64MB cache
            connection.pragma_update(None, "temp_store", "MEMORY")?;
            connection.pragma_update(None, "mmap_size", "268435456")?; // 256MB mmap

            // Configure busy timeout for better handling of concurrent access
            connection.busy_timeout(std::time::Duration::from_millis(30000))?;
        }

        // Create schema if database is new
        if !readonly {
            Self::create_schema(&connection)?;
        }

        Ok(connection)
    }

    /// Create the database schema
    fn create_schema(connection: &Connection) -> Result<()> {
        let tx = connection.unchecked_transaction()?;

        // Create repositories table
        tx.execute(
            "CREATE TABLE IF NOT EXISTS repositories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                revision TEXT NOT NULL,
                path TEXT NOT NULL,
                dependencies TEXT NOT NULL DEFAULT '[]',
                last_used INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (cast(strftime('%s', 'now') as integer)),
                UNIQUE(url, revision, dependencies)
            )",
            [],
        )?;

        // Create environments table
        tx.execute(
            "CREATE TABLE IF NOT EXISTS environments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                language TEXT NOT NULL,
                dependencies TEXT NOT NULL,
                path TEXT NOT NULL,
                last_used INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (cast(strftime('%s', 'now') as integer)),
                UNIQUE(language, dependencies)
            )",
            [],
        )?;

        // Create configs table
        tx.execute(
            "CREATE TABLE IF NOT EXISTS configs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                last_used INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;

        // Create schema version table for migrations
        tx.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL,
                applied_at INTEGER NOT NULL DEFAULT (cast(strftime('%s', 'now') as integer))
            )",
            [],
        )?;

        // Insert current schema version
        tx.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            params![SCHEMA_VERSION],
        )?;

        // Create indexes for better query performance
        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_repositories_url_revision
             ON repositories(url, revision)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_environments_language
             ON environments(language)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_configs_path
             ON configs(path)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_repositories_last_used
             ON repositories(last_used)",
            [],
        )?;

        tx.execute(
            "CREATE INDEX IF NOT EXISTS idx_environments_last_used
             ON environments(last_used)",
            [],
        )?;

        tx.commit()?;

        Ok(())
    }

    /// Get the cache directory path
    pub fn cache_directory(&self) -> &Path {
        &self.cache_dir
    }

    /// Check if the store is in readonly mode
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }

    /// Get current database schema version
    pub fn schema_version(&self) -> Result<u32> {
        let connection = self.connection.lock().unwrap();
        let version: u32 = connection
            .query_row(
                "SELECT version FROM schema_version ORDER BY applied_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(version)
    }

    /// Convert SystemTime to Unix timestamp for database storage
    fn system_time_to_timestamp(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs()
    }

    /// Convert Unix timestamp to SystemTime
    fn timestamp_to_system_time(timestamp: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(timestamp)
    }

    /// Serialize dependencies vector to JSON string for database storage
    fn serialize_dependencies(deps: &[String]) -> String {
        serde_json::to_string(deps).unwrap_or_else(|_| "[]".to_string())
    }

    /// Deserialize dependencies from JSON string
    fn deserialize_dependencies(deps_json: &str) -> Vec<String> {
        serde_json::from_str(deps_json).unwrap_or_default()
    }

    /// Convert database row to RepositoryInfo
    fn row_to_repository_info(row: &Row) -> rusqlite::Result<RepositoryInfo> {
        let dependencies_json: String = row.get("dependencies")?;
        let dependencies = Self::deserialize_dependencies(&dependencies_json);
        let last_used_timestamp: u64 = row.get("last_used")?;
        let last_used = Self::timestamp_to_system_time(last_used_timestamp);

        Ok(RepositoryInfo {
            url: row.get("url")?,
            revision: row.get("revision")?,
            path: PathBuf::from(row.get::<_, String>("path")?),
            last_used,
            dependencies,
        })
    }

    /// Convert database row to EnvironmentInfo
    fn row_to_environment_info(row: &Row) -> rusqlite::Result<EnvironmentInfo> {
        let dependencies_json: String = row.get("dependencies")?;
        let dependencies = Self::deserialize_dependencies(&dependencies_json);
        let last_used_timestamp: u64 = row.get("last_used")?;
        let last_used = Self::timestamp_to_system_time(last_used_timestamp);

        Ok(EnvironmentInfo {
            language: row.get("language")?,
            dependencies,
            path: PathBuf::from(row.get::<_, String>("path")?),
            last_used,
        })
    }

    /// Convert database row to ConfigInfo
    fn row_to_config_info(row: &Row) -> rusqlite::Result<ConfigInfo> {
        let last_used_timestamp: u64 = row.get("last_used")?;
        let last_used = Self::timestamp_to_system_time(last_used_timestamp);

        Ok(ConfigInfo {
            path: PathBuf::from(row.get::<_, String>("path")?),
            last_used,
        })
    }
}

impl Store {
    // =============================================================================
    // Concurrent Access and Locking Methods
    // =============================================================================

    /// Get exclusive lock on the store for safe concurrent access
    pub fn exclusive_lock(&self) -> Result<StoreLock> {
        let lock_file_path = self.cache_dir.join(".lock");
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_file_path)
            .map_err(|e| {
                SnpError::Storage(Box::new(StorageError::FileLockFailed {
                    path: lock_file_path.clone(),
                    error: e.to_string(),
                    timeout_secs: None,
                }))
            })?;

        lock_file.lock_exclusive().map_err(|e| {
            SnpError::Storage(Box::new(StorageError::FileLockFailed {
                path: lock_file_path,
                error: e.to_string(),
                timeout_secs: None,
            }))
        })?;

        Ok(StoreLock { _file: lock_file })
    }

    /// Get exclusive lock with timeout
    pub fn exclusive_lock_with_timeout(&self, timeout: Duration) -> Result<StoreLock> {
        let start = SystemTime::now();
        loop {
            match self.exclusive_lock() {
                Ok(lock) => return Ok(lock),
                Err(_) if start.elapsed().unwrap_or(Duration::from_secs(0)) < timeout => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    // =============================================================================
    // Migration and Schema Methods
    // =============================================================================

    /// Migrate database schema from one version to another
    pub fn migrate_schema(&self, from_version: u32, to_version: u32) -> Result<()> {
        if self.readonly {
            return Err(SnpError::Storage(Box::new(StorageError::MigrationFailed {
                from_version,
                to_version,
                error: "Cannot migrate schema in readonly mode".to_string(),
            })));
        }

        if from_version == to_version {
            return Ok(()); // No migration needed
        }

        let connection = self.connection.lock().unwrap();
        let tx = connection.unchecked_transaction().map_err(|e| {
            SnpError::Storage(Box::new(StorageError::MigrationFailed {
                from_version,
                to_version,
                error: e.to_string(),
            }))
        })?;

        // For now, we only support migration to version 1
        if to_version == 1 {
            // Schema is already created in create_schema, so just update version
            tx.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?)",
                params![to_version],
            )
            .map_err(|e| {
                SnpError::Storage(Box::new(StorageError::MigrationFailed {
                    from_version,
                    to_version,
                    error: e.to_string(),
                }))
            })?;
        }

        tx.commit().map_err(|e| {
            SnpError::Storage(Box::new(StorageError::MigrationFailed {
                from_version,
                to_version,
                error: e.to_string(),
            }))
        })?;

        Ok(())
    }

    /// Migrate from pre-commit cache format
    pub fn migrate_from_precommit_cache(precommit_cache_dir: &Path) -> Result<Store> {
        let precommit_db = precommit_cache_dir.join("db.db");

        if !precommit_db.exists() {
            return Err(SnpError::Storage(Box::new(StorageError::MigrationFailed {
                from_version: 0,
                to_version: 1,
                error: "Pre-commit database not found".to_string(),
            })));
        }

        // Create new SNP store
        let snp_cache_dir = Store::get_default_cache_directory()?;
        let store = Store::with_cache_directory(snp_cache_dir)?;

        // Open pre-commit database
        let precommit_conn = Connection::open(&precommit_db).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::MigrationFailed {
                from_version: 0,
                to_version: 1,
                error: format!("Failed to open pre-commit database: {e}"),
            }))
        })?;

        // Migrate repositories table
        let mut stmt = precommit_conn
            .prepare("SELECT repo, ref, path FROM repos")
            .map_err(|e| {
                SnpError::Storage(Box::new(StorageError::MigrationFailed {
                    from_version: 0,
                    to_version: 1,
                    error: format!("Failed to query pre-commit repos: {e}"),
                }))
            })?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>("repo")?,
                    row.get::<_, String>("ref")?,
                    row.get::<_, String>("path")?,
                ))
            })
            .map_err(|e| {
                SnpError::Storage(Box::new(StorageError::MigrationFailed {
                    from_version: 0,
                    to_version: 1,
                    error: format!("Failed to read pre-commit repos: {e}"),
                }))
            })?;

        {
            let connection = store.connection.lock().unwrap();
            for row in rows {
                let (url, revision, path) = row.map_err(|e| {
                    SnpError::Storage(Box::new(StorageError::MigrationFailed {
                        from_version: 0,
                        to_version: 1,
                        error: format!("Failed to parse pre-commit repo row: {e}"),
                    }))
                })?;

                let now = Self::system_time_to_timestamp(SystemTime::now());
                let empty_deps = Self::serialize_dependencies(&[]);

                let _ = connection.execute(
                    "INSERT OR IGNORE INTO repositories (url, revision, path, dependencies, last_used) VALUES (?, ?, ?, ?, ?)",
                    params![url, revision, path, empty_deps, now],
                );
            }

            // Migrate configs table if it exists
            if let Ok(mut stmt) = precommit_conn.prepare("SELECT path FROM configs") {
                let rows = stmt.query_map([], |row| row.get::<_, String>("path"));

                if let Ok(rows) = rows {
                    for path in rows.flatten() {
                        let now = Self::system_time_to_timestamp(SystemTime::now());
                        let _ = connection.execute(
                            "INSERT OR IGNORE INTO configs (path, last_used) VALUES (?, ?)",
                            params![path, now],
                        );
                    }
                }
            }
        }

        Ok(store)
    }

    // =============================================================================
    // Repository Caching Methods
    // =============================================================================

    /// Clone a repository and store it in the cache
    pub async fn clone_repository(
        &self,
        url: &str,
        revision: &str,
        dependencies: &[String],
    ) -> Result<PathBuf> {
        if self.readonly {
            return Err(SnpError::Storage(Box::new(
                StorageError::ConcurrencyConflict {
                    operation: "clone_repository".to_string(),
                    error: "Store is in readonly mode".to_string(),
                    retry_suggested: false,
                },
            )));
        }

        // Check if repository already exists
        if let Ok(existing_path) = self.get_repository(url, revision, dependencies) {
            // Update last used timestamp
            self.update_repository_last_used(url, revision, dependencies)?;
            return Ok(existing_path);
        }

        // Create new repository directory
        let repos_dir = self.cache_dir.join("repos");
        fs::create_dir_all(&repos_dir).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: repos_dir.clone(),
                error: e.to_string(),
            }))
        })?;

        // Generate unique directory name based on URL, revision, and dependencies
        let repo_hash = self.generate_repo_hash(url, revision, dependencies);
        let repo_dir = repos_dir.join(format!("repo_{repo_hash}"));

        // Ensure parent directory exists
        if let Some(parent) = repo_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: parent.to_path_buf(),
                    error: format!("Failed to create parent directory: {e}"),
                }))
            })?;
        }

        // Remove existing directory if it exists to ensure clean clone
        if repo_dir.exists() {
            debug!("Removing existing directory: {}", repo_dir.display());
            fs::remove_dir_all(&repo_dir).map_err(|e| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: repo_dir.clone(),
                    error: format!("Failed to remove existing directory: {e}"),
                }))
            })?;
        }

        // Clone the repository using git2 (wrapped in spawn_blocking to avoid blocking async runtime)
        debug!("Cloning repository {} to {}", url, repo_dir.display());
        let url_clone = url.to_string();
        let revision_clone = revision.to_string();
        let repo_dir_clone = repo_dir.clone();

        let git_repo = tokio::task::spawn_blocking(move || {
            let mut builder = git2::build::RepoBuilder::new();

            // Add optimization settings for faster clones
            let mut callbacks = git2::RemoteCallbacks::new();
            callbacks.pack_progress(|stage, current, total| match stage {
                git2::PackBuilderStage::AddingObjects => {
                    debug!("Adding objects: {}/{}", current, total);
                }
                git2::PackBuilderStage::Deltafication => {
                    debug!("Deltafication: {}/{}", current, total);
                }
            });

            // Configure for optimal performance
            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);

            // For specific revisions (not HEAD), we can use shallow clone for better performance
            if revision_clone != "HEAD" {
                // Use depth=1 for shallow clone when cloning a specific revision
                // This significantly reduces clone time and bandwidth
                fetch_options.depth(1);
            }

            builder.fetch_options(fetch_options);

            // Set checkout options for better performance
            let mut checkout_opts = git2::build::CheckoutBuilder::new();
            checkout_opts.progress(|path, cur, total| {
                if let Some(path) = path {
                    debug!("Checkout progress: {}/{} - {:?}", cur, total, path);
                }
            });
            builder.with_checkout(checkout_opts);

            builder.clone(&url_clone, &repo_dir_clone)
        })
        .await
        .map_err(|e| {
            SnpError::Storage(Box::new(StorageError::ConcurrencyConflict {
                operation: "git_clone".to_string(),
                error: format!("Task execution error: {e}"),
                retry_suggested: true,
            }))
        })?
        .map_err(|e| {
            error!(
                "Failed to clone repository {} to {}: {}",
                url,
                repo_dir.display(),
                e.message()
            );
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: repo_dir.clone(),
                error: format!("Failed to clone repository '{}': {}", url, e.message()),
            }))
        })?;

        // Checkout specific revision if requested and not HEAD (also wrapped in spawn_blocking)
        if revision != "HEAD" {
            debug!("Checking out revision: {}", revision);
            let revision_clone = revision.to_string();
            let repo_dir_clone = repo_dir.clone();

            tokio::task::spawn_blocking(move || {
                // Try to resolve and checkout the revision
                match git_repo.revparse_single(&revision_clone) {
                    Ok(obj) => {
                        debug!("Found revision object: {}", obj.id());
                        git_repo.checkout_tree(&obj, None).map_err(|e| {
                            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                                path: repo_dir_clone.clone(),
                                error: format!(
                                    "Failed to checkout revision '{}': {}",
                                    revision_clone,
                                    e.message()
                                ),
                            }))
                        })?;

                        // Set HEAD to the checked out revision
                        git_repo.set_head_detached(obj.id()).map_err(|e| {
                            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                                path: repo_dir_clone,
                                error: format!(
                                    "Failed to set HEAD to revision '{}': {}",
                                    revision_clone,
                                    e.message()
                                ),
                            }))
                        })?;
                        Ok::<(), crate::error::SnpError>(())
                    }
                    Err(e) => {
                        debug!(
                            "Could not resolve revision '{}': {}",
                            revision_clone,
                            e.message()
                        );
                        // Continue with HEAD if revision can't be resolved
                        Ok::<(), crate::error::SnpError>(())
                    }
                }
            })
            .await
            .map_err(|e| {
                SnpError::Storage(Box::new(StorageError::ConcurrencyConflict {
                    operation: "git_checkout".to_string(),
                    error: format!("Task execution error: {e}"),
                    retry_suggested: true,
                }))
            })??;
        }

        // Create marker file for tracking
        let marker_file = repo_dir.join(".snp_repo_marker");
        let marker_content =
            format!("Repository: {url}\nRevision: {revision}\nDependencies: {dependencies:?}\n");
        fs::write(&marker_file, marker_content).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: marker_file,
                error: e.to_string(),
            }))
        })?;

        // Store in database (wrapped in spawn_blocking for better concurrency)
        let url_clone = url.to_string();
        let revision_clone = revision.to_string();
        let repo_dir_clone = repo_dir.clone();
        let deps_clone = dependencies.to_vec();
        let connection = Arc::clone(&self.connection);

        tokio::task::spawn_blocking(move || {
            let connection = connection.lock().unwrap();
            let now = Self::system_time_to_timestamp(SystemTime::now());
            let deps_json = Self::serialize_dependencies(&deps_clone);

            connection.execute(
                "INSERT INTO repositories (url, revision, path, dependencies, last_used) VALUES (?, ?, ?, ?, ?)",
                params![url_clone, revision_clone, repo_dir_clone.to_string_lossy().as_ref(), deps_json, now],
            )
        })
        .await
        .map_err(|e| {
            SnpError::Storage(Box::new(StorageError::ConcurrencyConflict {
                operation: "database_insert".to_string(),
                error: format!("Task execution error: {e}"),
                retry_suggested: true,
            }))
        })??;

        Ok(repo_dir)
    }

    /// Get a cached repository path
    pub fn get_repository(
        &self,
        url: &str,
        revision: &str,
        dependencies: &[String],
    ) -> Result<PathBuf> {
        let connection = self.connection.lock().unwrap();
        let deps_json = Self::serialize_dependencies(dependencies);

        let result = connection
            .query_row(
                "SELECT path FROM repositories WHERE url = ? AND revision = ? AND dependencies = ?",
                params![url, revision, deps_json],
                |row| {
                    let path_str: String = row.get(0)?;
                    Ok(PathBuf::from(path_str))
                },
            )
            .optional()?;

        match result {
            Some(path) => {
                if path.exists() {
                    // Update last used timestamp
                    drop(connection); // Release lock
                    self.update_repository_last_used(url, revision, dependencies)?;
                    Ok(path)
                } else {
                    Err(SnpError::Storage(Box::new(
                        StorageError::RepositoryNotFound {
                            url: url.to_string(),
                            revision: revision.to_string(),
                            suggestion: Some(
                                "Repository directory was deleted. Try running the command again."
                                    .to_string(),
                            ),
                        },
                    )))
                }
            }
            None => Err(SnpError::Storage(Box::new(
                StorageError::RepositoryNotFound {
                    url: url.to_string(),
                    revision: revision.to_string(),
                    suggestion: Some(
                        "Repository not found in cache. It will be cloned on next use.".to_string(),
                    ),
                },
            ))),
        }
    }

    /// Update the last used timestamp for a repository
    fn update_repository_last_used(
        &self,
        url: &str,
        revision: &str,
        dependencies: &[String],
    ) -> Result<()> {
        if self.readonly {
            return Ok(()); // Skip updates in readonly mode
        }

        let connection = self.connection.lock().unwrap();
        let deps_json = Self::serialize_dependencies(dependencies);
        let now = Self::system_time_to_timestamp(SystemTime::now());

        connection.execute(
            "UPDATE repositories SET last_used = ? WHERE url = ? AND revision = ? AND dependencies = ?",
            params![now, url, revision, deps_json],
        )?;

        Ok(())
    }

    /// Generate a hash for repository directory naming
    pub fn generate_repo_hash(&self, url: &str, revision: &str, dependencies: &[String]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        revision.hash(&mut hasher);
        dependencies.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get the repositories directory path
    pub fn repos_directory(&self) -> PathBuf {
        self.cache_dir.join("repos")
    }

    /// Add a repository to the database (for testing)
    pub fn add_repository(
        &self,
        url: &str,
        revision: &str,
        dependencies: &[String],
        path: &Path,
    ) -> Result<()> {
        let connection = self.connection.lock().unwrap();
        let now = Self::system_time_to_timestamp(SystemTime::now());
        let deps_json = Self::serialize_dependencies(dependencies);

        connection.execute(
            "INSERT OR REPLACE INTO repositories (url, revision, path, dependencies, created_at, last_used) VALUES (?, ?, ?, ?, ?, ?)",
            params![url, revision, path.to_string_lossy().as_ref(), deps_json, now, now],
        )?;

        Ok(())
    }

    /// List all cached repositories
    pub fn list_repositories(&self) -> Result<Vec<RepositoryInfo>> {
        let connection = self.connection.lock().unwrap();
        let mut stmt = connection.prepare(
            "SELECT url, revision, path, dependencies, last_used FROM repositories ORDER BY last_used DESC"
        )?;

        let rows = stmt.query_map([], Self::row_to_repository_info)?;

        let mut repositories = Vec::new();
        for row in rows {
            repositories.push(row?);
        }

        Ok(repositories)
    }

    // =============================================================================
    // Environment Tracking Methods
    // =============================================================================

    /// Install a language environment and cache it
    pub fn install_environment(&self, language: &str, dependencies: &[String]) -> Result<PathBuf> {
        if self.readonly {
            return Err(SnpError::Storage(Box::new(
                StorageError::ConcurrencyConflict {
                    operation: "install_environment".to_string(),
                    error: "Store is in readonly mode".to_string(),
                    retry_suggested: false,
                },
            )));
        }

        // Check if environment already exists
        if let Ok(existing_path) = self.get_environment(language, dependencies) {
            // Update last used timestamp
            self.update_environment_last_used(language, dependencies)?;
            return Ok(existing_path);
        }

        // Create new environment directory
        let envs_dir = self.cache_dir.join("envs");
        fs::create_dir_all(&envs_dir).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: envs_dir.clone(),
                error: e.to_string(),
            }))
        })?;

        // Generate unique directory name
        let env_hash = self.generate_env_hash(language, dependencies);
        let env_dir = envs_dir.join(format!("{language}_{env_hash}"));

        // Create the environment directory
        fs::create_dir_all(&env_dir).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: env_dir.clone(),
                error: e.to_string(),
            }))
        })?;

        // Create environment marker file
        let marker_file = env_dir.join(".snp_env_marker");
        let marker_content = format!("Language: {language}\nDependencies: {dependencies:?}\n");
        fs::write(&marker_file, marker_content).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: marker_file,
                error: e.to_string(),
            }))
        })?;

        // Store in database
        let connection = self.connection.lock().unwrap();
        let now = Self::system_time_to_timestamp(SystemTime::now());
        let deps_json = Self::serialize_dependencies(dependencies);

        connection.execute(
            "INSERT INTO environments (language, dependencies, path, last_used) VALUES (?, ?, ?, ?)",
            params![language, deps_json, env_dir.to_string_lossy().as_ref(), now],
        )?;

        Ok(env_dir)
    }

    /// Get a cached environment path (updates last_used timestamp)
    pub fn get_environment(&self, language: &str, dependencies: &[String]) -> Result<PathBuf> {
        let connection = self.connection.lock().unwrap();
        let deps_json = Self::serialize_dependencies(dependencies);

        let result = connection
            .query_row(
                "SELECT path FROM environments WHERE language = ? AND dependencies = ?",
                params![language, deps_json],
                |row| {
                    let path_str: String = row.get(0)?;
                    Ok(PathBuf::from(path_str))
                },
            )
            .optional()?;

        match result {
            Some(path) => {
                if path.exists() {
                    // Update last used timestamp
                    drop(connection); // Release lock
                    self.update_environment_last_used(language, dependencies)?;
                    Ok(path)
                } else {
                    Err(SnpError::Storage(Box::new(
                        StorageError::EnvironmentNotFound {
                            language: language.to_string(),
                            dependencies: dependencies.to_vec(),
                            suggestion: Some(
                                "Environment directory was deleted. Try running the command again."
                                    .to_string(),
                            ),
                        },
                    )))
                }
            }
            None => Err(SnpError::Storage(Box::new(
                StorageError::EnvironmentNotFound {
                    language: language.to_string(),
                    dependencies: dependencies.to_vec(),
                    suggestion: Some(
                        "Environment not found in cache. It will be created on next use."
                            .to_string(),
                    ),
                },
            ))),
        }
    }

    /// Get environment info with metadata
    pub fn get_environment_info(
        &self,
        language: &str,
        dependencies: &[String],
    ) -> Result<EnvironmentInfo> {
        let connection = self.connection.lock().unwrap();
        let deps_json = Self::serialize_dependencies(dependencies);

        let result = connection
            .query_row(
                "SELECT language, dependencies, path, last_used FROM environments WHERE language = ? AND dependencies = ?",
                params![language, deps_json],
                Self::row_to_environment_info,
            )
            .optional()?;

        match result {
            Some(env_info) => {
                if env_info.path.exists() {
                    Ok(env_info)
                } else {
                    Err(SnpError::Storage(Box::new(
                        StorageError::EnvironmentNotFound {
                            language: language.to_string(),
                            dependencies: dependencies.to_vec(),
                            suggestion: Some(
                                "Environment directory was deleted. Try running the command again."
                                    .to_string(),
                            ),
                        },
                    )))
                }
            }
            None => Err(SnpError::Storage(Box::new(
                StorageError::EnvironmentNotFound {
                    language: language.to_string(),
                    dependencies: dependencies.to_vec(),
                    suggestion: Some(
                        "Environment not found in cache. It will be created on next use."
                            .to_string(),
                    ),
                },
            ))),
        }
    }

    /// Update the last used timestamp for an environment
    fn update_environment_last_used(&self, language: &str, dependencies: &[String]) -> Result<()> {
        if self.readonly {
            return Ok(()); // Skip updates in readonly mode
        }

        let connection = self.connection.lock().unwrap();
        let deps_json = Self::serialize_dependencies(dependencies);
        let now = Self::system_time_to_timestamp(SystemTime::now());

        connection.execute(
            "UPDATE environments SET last_used = ? WHERE language = ? AND dependencies = ?",
            params![now, language, deps_json],
        )?;

        Ok(())
    }

    /// Generate a hash for environment directory naming
    fn generate_env_hash(&self, language: &str, dependencies: &[String]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        language.hash(&mut hasher);
        dependencies.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// List all cached environments
    pub fn list_environments(&self) -> Result<Vec<EnvironmentInfo>> {
        let connection = self.connection.lock().unwrap();
        let mut stmt = connection.prepare(
            "SELECT language, dependencies, path, last_used FROM environments ORDER BY last_used DESC"
        )?;

        let rows = stmt.query_map([], Self::row_to_environment_info)?;

        let mut environments = Vec::new();
        for row in rows {
            environments.push(row?);
        }

        Ok(environments)
    }

    /// List environments for a specific language
    pub fn list_environments_for_language(&self, language: &str) -> Result<Vec<EnvironmentInfo>> {
        let connection = self.connection.lock().unwrap();
        let mut stmt = connection.prepare(
            "SELECT language, dependencies, path, last_used FROM environments WHERE language = ? ORDER BY last_used DESC"
        )?;

        let rows = stmt.query_map([language], Self::row_to_environment_info)?;

        let mut environments = Vec::new();
        for row in rows {
            environments.push(row?);
        }

        Ok(environments)
    }

    // =============================================================================
    // Configuration Tracking Methods
    // =============================================================================

    /// Mark a configuration file as used
    pub fn mark_config_used(&self, config_path: &Path) -> Result<()> {
        if self.readonly {
            return Ok(()); // Skip updates in readonly mode
        }

        // Convert to absolute path for consistency
        let abs_path = if config_path.is_absolute() {
            config_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(SnpError::Io)?
                .join(config_path)
        };

        let connection = self.connection.lock().unwrap();
        let now = Self::system_time_to_timestamp(SystemTime::now());
        let path_str = abs_path.to_string_lossy();

        // Use INSERT OR REPLACE to handle both new and existing configs
        connection.execute(
            "INSERT OR REPLACE INTO configs (path, last_used) VALUES (?, ?)",
            params![path_str.as_ref(), now],
        )?;

        Ok(())
    }

    /// List all tracked configuration files
    pub fn list_configs(&self) -> Result<Vec<ConfigInfo>> {
        let connection = self.connection.lock().unwrap();
        let mut stmt =
            connection.prepare("SELECT path, last_used FROM configs ORDER BY last_used DESC")?;

        let rows = stmt.query_map([], Self::row_to_config_info)?;

        let mut configs = Vec::new();
        for row in rows {
            configs.push(row?);
        }

        Ok(configs)
    }

    // =============================================================================
    // Cleanup and Garbage Collection Methods
    // =============================================================================

    /// Perform garbage collection to clean up old repositories and environments
    pub fn garbage_collect(&self) -> Result<usize> {
        if self.readonly {
            return Ok(0);
        }

        let mut cleaned_count = 0;

        // Clean up repositories older than 30 days
        cleaned_count += self.cleanup_old_repositories(Duration::from_secs(30 * 24 * 60 * 60))?;

        // Clean up environments older than 30 days
        cleaned_count += self.cleanup_environments(Duration::from_secs(30 * 24 * 60 * 60))?;

        // Clean up missing config references
        cleaned_count += self.cleanup_missing_configs()?;

        Ok(cleaned_count)
    }

    /// Clean up old repositories
    pub fn cleanup_old_repositories(&self, max_age: Duration) -> Result<usize> {
        let connection = self.connection.lock().unwrap();
        let cutoff_time = Self::system_time_to_timestamp(
            SystemTime::now()
                .checked_sub(max_age)
                .unwrap_or(SystemTime::UNIX_EPOCH),
        );

        // Get repositories to delete
        let mut stmt = connection.prepare(
            "SELECT url, revision, path, dependencies FROM repositories WHERE last_used < ?",
        )?;

        let rows = stmt.query_map([cutoff_time], |row| {
            let path_str: String = row.get("path")?;
            Ok(PathBuf::from(path_str))
        })?;

        let mut paths_to_delete: Vec<PathBuf> = Vec::new();
        for row in rows {
            paths_to_delete.push(row?);
        }

        // Delete from filesystem
        let mut deleted_count = 0;
        for path in &paths_to_delete {
            if path.exists() {
                if let Err(e) = fs::remove_dir_all(path) {
                    tracing::warn!(
                        "Failed to remove repository directory {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    deleted_count += 1;
                }
            }
        }

        // Delete from database
        connection.execute(
            "DELETE FROM repositories WHERE last_used < ?",
            [cutoff_time],
        )?;

        Ok(deleted_count)
    }

    /// Clean up old environments
    pub fn cleanup_environments(&self, max_age: Duration) -> Result<usize> {
        if self.readonly {
            return Ok(0);
        }

        let connection = self.connection.lock().unwrap();
        let cutoff_time = Self::system_time_to_timestamp(
            SystemTime::now()
                .checked_sub(max_age)
                .unwrap_or(SystemTime::UNIX_EPOCH),
        );

        // Get environments to delete
        let mut stmt = connection.prepare("SELECT path FROM environments WHERE last_used < ?")?;

        let rows = stmt.query_map([cutoff_time], |row| {
            let path_str: String = row.get(0)?;
            Ok(PathBuf::from(path_str))
        })?;

        let mut paths_to_delete: Vec<PathBuf> = Vec::new();
        for row in rows {
            paths_to_delete.push(row?);
        }

        // Delete from filesystem
        let mut deleted_count = 0;
        for path in &paths_to_delete {
            if path.exists() {
                if let Err(e) = fs::remove_dir_all(path) {
                    tracing::warn!(
                        "Failed to remove environment directory {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    deleted_count += 1;
                }
            }
        }

        // Delete from database
        connection.execute(
            "DELETE FROM environments WHERE last_used < ?",
            [cutoff_time],
        )?;

        Ok(deleted_count)
    }

    /// Clean up references to missing configuration files
    pub fn cleanup_missing_configs(&self) -> Result<usize> {
        if self.readonly {
            return Ok(0);
        }

        let configs = self.list_configs()?;
        let mut missing_paths = Vec::new();

        for config in configs {
            if !config.path.exists() {
                missing_paths.push(config.path);
            }
        }

        let connection = self.connection.lock().unwrap();
        let mut deleted_count = 0;

        for path in missing_paths {
            let path_str = path.to_string_lossy();
            let affected =
                connection.execute("DELETE FROM configs WHERE path = ?", [path_str.as_ref()])?;
            deleted_count += affected;
        }

        Ok(deleted_count)
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new().expect("Failed to create default Store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_store() -> (Store, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let cache_dir = temp_dir.path().join("cache");
        let store = Store::with_cache_directory(cache_dir).expect("Failed to create store");
        (store, temp_dir)
    }

    #[test]
    fn test_store_creation() {
        let (store, _temp_dir) = create_temp_store();
        assert!(!store.is_readonly());
        assert!(store.cache_directory().exists());
    }

    #[test]
    fn test_schema_version() {
        let (store, _temp_dir) = create_temp_store();
        let version = store
            .schema_version()
            .expect("Failed to get schema version");
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn test_cache_directory_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let cache_dir = temp_dir.path().join("new_cache");

        assert!(!cache_dir.exists());
        let _store =
            Store::with_cache_directory(cache_dir.clone()).expect("Failed to create store");

        assert!(cache_dir.exists());
        assert!(cache_dir.join("README").exists());
        assert!(cache_dir.join("db.db").exists());
    }

    #[test]
    fn test_readonly_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let cache_dir = temp_dir.path().join("cache");

        // Create cache directory first
        fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");

        // Make it readonly (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&cache_dir).unwrap().permissions();
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(&cache_dir, perms).unwrap();

            let store = Store::with_cache_directory(cache_dir.clone());
            // Should succeed in readonly mode but database won't be created
            assert!(store.is_err() || store.unwrap().is_readonly());
        }
    }

    #[test]
    fn test_default_cache_directory() {
        let cache_dir =
            Store::get_default_cache_directory().expect("Failed to get default cache dir");

        // During tests, should end with "snp-{process_id}", otherwise "snp"
        let dir_name = cache_dir.file_name().unwrap().to_string_lossy();
        if cfg!(test) {
            assert!(dir_name.starts_with("snp-"));
        } else {
            assert_eq!(dir_name, "snp");
        }

        // Should be an absolute path
        assert!(cache_dir.is_absolute());
    }

    #[test]
    fn test_system_time_conversion() {
        let now = SystemTime::now();
        let timestamp = Store::system_time_to_timestamp(now);
        let converted_back = Store::timestamp_to_system_time(timestamp);

        // Should be within 1 second (precision loss due to u64 conversion)
        let diff = if now >= converted_back {
            now.duration_since(converted_back)
                .unwrap_or(Duration::from_secs(0))
        } else {
            converted_back
                .duration_since(now)
                .unwrap_or(Duration::from_secs(0))
        };
        assert!(diff < Duration::from_secs(1));
    }

    #[test]
    fn test_dependencies_serialization() {
        let deps = vec!["dep1".to_string(), "dep2".to_string()];
        let serialized = Store::serialize_dependencies(&deps);
        let deserialized = Store::deserialize_dependencies(&serialized);
        assert_eq!(deps, deserialized);
    }

    #[test]
    fn test_dependencies_serialization_empty() {
        let deps: Vec<String> = vec![];
        let serialized = Store::serialize_dependencies(&deps);
        let deserialized = Store::deserialize_dependencies(&serialized);
        assert_eq!(deps, deserialized);
    }

    #[test]
    fn test_dependencies_deserialization_invalid() {
        let invalid_json = "not valid json";
        let deserialized = Store::deserialize_dependencies(invalid_json);
        assert_eq!(deserialized, Vec::<String>::new());
    }
}
