// Plugin registration and discovery system

use dashmap::DashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::Result;

use super::traits::{Language, LanguageConfig, LanguageError};

/// Language plugin registry
pub struct LanguageRegistry {
    plugins: DashMap<String, Arc<dyn Language>>,
    plugin_configs: DashMap<String, LanguageConfig>,
    detection_cache: DashMap<PathBuf, Option<String>>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self {
            plugins: DashMap::new(),
            plugin_configs: DashMap::new(),
            detection_cache: DashMap::new(),
        }
    }

    // Plugin management
    pub fn register_plugin(&self, language: Arc<dyn Language>) -> Result<()> {
        let name = language.language_name().to_string();

        // Validate plugin before registering
        language.validate_config(&language.default_config())?;

        self.plugins.insert(name.clone(), language.clone());
        self.plugin_configs.insert(name, language.default_config());

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
        // Check cache first
        if let Some(cached) = self.detection_cache.get(file_path) {
            return Ok(cached.clone());
        }

        let detected = self.detect_language_uncached(file_path)?;

        // Cache the result
        self.detection_cache
            .insert(file_path.to_path_buf(), detected.clone());

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

        let rust_plugin = Arc::new(super::rust::RustLanguagePlugin::new());
        self.register_plugin(rust_plugin)?;

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
