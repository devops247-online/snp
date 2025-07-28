// Plugin registration and discovery system

use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::error::Result;

use super::traits::{Language, LanguageConfig, LanguageError};

/// Registry configuration for atomic updates
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub max_cache_size: usize,
    pub enable_detection_cache: bool,
    pub cache_ttl_seconds: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            max_cache_size: 1000,
            enable_detection_cache: true,
            cache_ttl_seconds: 3600, // 1 hour
        }
    }
}

/// Registry statistics exposed atomically
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_registrations: u64,
    pub active_plugins: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub detection_requests: u64,
    pub cache_hit_rate: f64,
}

/// Language plugin registry with lock-free operations and atomic statistics
pub struct LanguageRegistry {
    // Lock-free concurrent data structures
    plugins: DashMap<String, Arc<dyn Language>>,
    plugin_configs: DashMap<String, LanguageConfig>,
    detection_cache: DashMap<PathBuf, Option<String>>,

    // Atomic counters for statistics tracking
    total_registrations: AtomicU64,
    active_plugins: AtomicUsize,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    detection_requests: AtomicU64,

    // Atomic configuration updates
    config: ArcSwap<RegistryConfig>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self {
            plugins: DashMap::new(),
            plugin_configs: DashMap::new(),
            detection_cache: DashMap::new(),
            total_registrations: AtomicU64::new(0),
            active_plugins: AtomicUsize::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            detection_requests: AtomicU64::new(0),
            config: ArcSwap::from_pointee(RegistryConfig::default()),
        }
    }

    /// Create a new registry with custom configuration
    pub fn with_config(config: RegistryConfig) -> Self {
        Self {
            plugins: DashMap::new(),
            plugin_configs: DashMap::new(),
            detection_cache: DashMap::new(),
            total_registrations: AtomicU64::new(0),
            active_plugins: AtomicUsize::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            detection_requests: AtomicU64::new(0),
            config: ArcSwap::from_pointee(config),
        }
    }

    /// Update registry configuration atomically
    pub fn update_config(&self, new_config: RegistryConfig) {
        self.config.store(Arc::new(new_config));
    }

    /// Get current configuration
    pub fn get_config(&self) -> Arc<RegistryConfig> {
        self.config.load().clone()
    }

    // Plugin management
    pub fn register_plugin(&self, language: Arc<dyn Language>) -> Result<()> {
        let name = language.language_name().to_string();

        // Validate plugin before registering
        language.validate_config(&language.default_config())?;

        // Lock-free insertion with atomic statistics tracking
        let was_new = self
            .plugins
            .insert(name.clone(), language.clone())
            .is_none();
        self.plugin_configs.insert(name, language.default_config());

        if was_new {
            self.total_registrations.fetch_add(1, Ordering::Relaxed);
            self.active_plugins.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    }

    pub fn unregister_plugin(&self, language_name: &str) -> Result<()> {
        if self.plugins.remove(language_name).is_none() {
            return Err(LanguageError::PluginNotFound {
                language: language_name.to_string(),
                available_languages: self.list_plugins(),
            }
            .into());
        }

        self.plugin_configs.remove(language_name);

        // Update atomic counters
        self.active_plugins.fetch_sub(1, Ordering::Relaxed);

        // Clear detection cache entries for this plugin
        self.detection_cache
            .retain(|_, cached_lang| cached_lang.as_ref() != Some(&language_name.to_string()));

        Ok(())
    }

    pub fn get_plugin(&self, language_name: &str) -> Option<Arc<dyn Language>> {
        self.plugins
            .get(language_name)
            .map(|entry| entry.value().clone())
    }

    pub fn list_plugins(&self) -> Vec<String> {
        self.plugins
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    // Language detection
    pub fn detect_language(&self, file_path: &Path) -> Result<Option<String>> {
        // Increment detection request counter
        self.detection_requests.fetch_add(1, Ordering::Relaxed);

        // Check if caching is enabled
        let config = self.config.load();
        if !config.enable_detection_cache {
            return self.detect_language_uncached(file_path);
        }

        // Check cache first
        if let Some(cached) = self.detection_cache.get(file_path) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached.clone());
        }

        // Cache miss - perform detection
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        let detected = self.detect_language_uncached(file_path)?;

        // Cache the result if cache size is within limits
        if self.detection_cache.len() < config.max_cache_size.max(1000) {
            self.detection_cache
                .insert(file_path.to_path_buf(), detected.clone());
        }

        Ok(detected)
    }

    pub fn detect_language_bulk(
        &self,
        file_paths: &[PathBuf],
    ) -> Result<HashMap<PathBuf, Option<String>>> {
        let mut results = HashMap::new();

        for path in file_paths {
            results.insert(path.clone(), self.detect_language(path)?);
        }

        Ok(results)
    }

    // Plugin discovery
    pub fn discover_plugins(&self, plugin_dirs: &[PathBuf]) -> Result<usize> {
        let mut discovered_count = 0;

        for plugin_dir in plugin_dirs {
            if plugin_dir.exists() && plugin_dir.is_dir() {
                discovered_count += self.discover_plugins_in_dir(plugin_dir)?;
            }
        }

        Ok(discovered_count)
    }

    pub fn load_builtin_plugins(&self) -> Result<()> {
        // Register built-in language plugins
        let system_plugin = Arc::new(super::system::SystemLanguagePlugin::new());
        self.register_plugin(system_plugin)?;

        let python_plugin = Arc::new(super::python::PythonLanguagePlugin::new());
        self.register_plugin(python_plugin)?;

        let nodejs_plugin = Arc::new(super::nodejs::NodejsLanguagePlugin::new());
        self.register_plugin(nodejs_plugin)?;

        let rust_plugin = Arc::new(super::rust::RustLanguagePlugin::new());
        self.register_plugin(rust_plugin)?;

        let docker_plugin = Arc::new(super::docker::DockerLanguagePlugin::new());
        self.register_plugin(docker_plugin)?;

        let golang_plugin = Arc::new(super::golang::GoLanguagePlugin::new());
        self.register_plugin(golang_plugin)?;

        let ruby_plugin = Arc::new(super::ruby::RubyLanguagePlugin::new());
        self.register_plugin(ruby_plugin)?;

        let script_plugin = Arc::new(super::script::ScriptLanguagePlugin::new());
        self.register_plugin(script_plugin)?;

        Ok(())
    }

    // Configuration
    pub fn configure_plugin(&self, language_name: &str, config: LanguageConfig) -> Result<()> {
        if !self.plugins.contains_key(language_name) {
            return Err(LanguageError::PluginNotFound {
                language: language_name.to_string(),
                available_languages: self.list_plugins(),
            }
            .into());
        }

        // Validate configuration with the plugin
        if let Some(plugin) = self.get_plugin(language_name) {
            plugin.validate_config(&config)?;
        }

        self.plugin_configs
            .insert(language_name.to_string(), config);
        Ok(())
    }

    pub fn get_plugin_config(&self, language_name: &str) -> Option<LanguageConfig> {
        self.plugin_configs
            .get(language_name)
            .map(|entry| entry.value().clone())
    }

    // Helper methods
    fn detect_language_uncached(&self, file_path: &Path) -> Result<Option<String>> {
        // Try extension-based detection first
        if let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) {
            for plugin_entry in self.plugins.iter() {
                let plugin = plugin_entry.value();
                if plugin.supported_extensions().contains(&extension) {
                    return Ok(Some(plugin_entry.key().clone()));
                }
            }
        }

        // Try pattern-based detection
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        for plugin_entry in self.plugins.iter() {
            let plugin = plugin_entry.value();
            for pattern in plugin.detection_patterns() {
                if file_name.contains(pattern) {
                    return Ok(Some(plugin_entry.key().clone()));
                }
            }
        }

        Ok(None)
    }

    fn discover_plugins_in_dir(&self, _plugin_dir: &Path) -> Result<usize> {
        // For now, just return 0 as we don't have dynamic loading implemented
        // This would be where we'd scan for .so/.dll files and load them
        Ok(0)
    }

    // Statistics and monitoring methods

    /// Get current registry statistics atomically
    pub fn get_stats(&self) -> RegistryStats {
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        let total_cache_requests = cache_hits + cache_misses;
        let cache_hit_rate = if total_cache_requests > 0 {
            cache_hits as f64 / total_cache_requests as f64
        } else {
            0.0
        };
        RegistryStats {
            total_registrations: self.total_registrations.load(Ordering::Relaxed),
            active_plugins: self.active_plugins.load(Ordering::Relaxed),
            cache_hits,
            cache_misses,
            detection_requests: self.detection_requests.load(Ordering::Relaxed),
            cache_hit_rate,
        }
    }
    /// Clear detection cache
    pub fn clear_cache(&self) {
        self.detection_cache.clear();
    }
    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.detection_cache.len()
    }
    /// Reset statistics counters
    pub fn reset_stats(&self) {
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.detection_requests.store(0, Ordering::Relaxed);
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin metadata for discovery
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub supported_extensions: Vec<String>,
    pub dependencies: Vec<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
}
