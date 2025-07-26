// Hook execution integration layer between language plugins and hook execution
#![allow(clippy::await_holding_lock)]

use lru::LruCache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::core::Hook;
use crate::error::Result;
use crate::execution::HookExecutionResult;

use super::environment::EnvironmentManager;
use super::environment::{EnvironmentConfig, EnvironmentInfo, LanguageEnvironment};
use super::registry::LanguageRegistry;

/// Integration layer between language plugins and hook execution
pub struct LanguageHookExecutor {
    registry: Arc<LanguageRegistry>,
    environment_manager: Arc<Mutex<EnvironmentManager>>,
    execution_cache: ExecutionCache,
}

impl LanguageHookExecutor {
    pub fn new(
        registry: Arc<LanguageRegistry>,
        environment_manager: Arc<Mutex<EnvironmentManager>>,
    ) -> Self {
        Self {
            registry,
            environment_manager,
            execution_cache: ExecutionCache::new(100), // Cache up to 100 executions
        }
    }

    // Hook execution
    pub async fn execute_hook(
        &mut self,
        hook: &Hook,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        // Get the language plugin
        let plugin = self.registry.get_plugin(&hook.language).ok_or_else(|| {
            super::traits::LanguageError::PluginNotFound {
                language: hook.language.clone(),
                available_languages: self.registry.list_plugins(),
            }
        })?;

        // Prepare environment
        let env = self.prepare_hook_environment(hook).await?;

        // Execute the hook
        let result = plugin.execute_hook(hook, &env, files).await?;

        // Cache the result
        self.execution_cache.cache_result(hook, files, &result);

        Ok(result)
    }

    pub async fn prepare_hook_environment(&mut self, hook: &Hook) -> Result<LanguageEnvironment> {
        let config = EnvironmentConfig {
            language_version: hook.minimum_pre_commit_version.clone(),
            additional_dependencies: hook.additional_dependencies.clone(),
            environment_variables: HashMap::new(),
            cache_strategy: super::environment::CacheStrategy::Both,
            isolation_level: super::environment::IsolationLevel::Partial,
            working_directory: None,
            version: hook.minimum_pre_commit_version.clone(),
            repository_path: None,
            hook_timeout: None,
        };

        let env = {
            let mut env_manager = self.environment_manager.lock().unwrap();
            env_manager
                .get_or_create_environment(&hook.language, &config)
                .await?
        };

        // Get the language plugin to install dependencies if needed
        let plugin = self.registry.get_plugin(&hook.language).ok_or_else(|| {
            super::traits::LanguageError::PluginNotFound {
                language: hook.language.clone(),
                available_languages: self.registry.list_plugins(),
            }
        })?;

        // Install additional dependencies if any
        if !hook.additional_dependencies.is_empty() {
            let dependencies = plugin
                .resolve_dependencies(&hook.additional_dependencies)
                .await?;
            plugin.install_dependencies(&env, &dependencies).await?;
        }

        Ok((*env).clone())
    }

    // Batch operations
    pub async fn execute_hooks_batch(
        &mut self,
        hooks: &[(Hook, Vec<PathBuf>)],
    ) -> Result<Vec<HookExecutionResult>> {
        let mut results = Vec::new();

        for (hook, files) in hooks {
            let result = self.execute_hook(hook, files).await?;
            results.push(result);
        }

        Ok(results)
    }

    // Environment management
    pub async fn ensure_hook_environment(&mut self, hook: &Hook) -> Result<()> {
        let _env = self.prepare_hook_environment(hook).await?;
        Ok(())
    }

    pub fn get_hook_environment_info(&self, hook: &Hook) -> Option<EnvironmentInfo> {
        let plugin = self.registry.get_plugin(&hook.language)?;

        // Try to get environment info from environment manager
        let env_manager = self.environment_manager.lock().ok()?;
        let environments = env_manager.list_environments(Some(&hook.language));

        // Return info for the first matching environment
        environments
            .first()
            .map(|env| plugin.get_environment_info(env))
    }
}

/// Execution cache for performance optimization
pub struct ExecutionCache {
    cache: Arc<Mutex<LruCache<String, HookExecutionResult>>>,
}

impl ExecutionCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(capacity.try_into().unwrap()))),
        }
    }

    pub fn cache_result(&self, hook: &Hook, files: &[PathBuf], result: &HookExecutionResult) {
        let cache_key = self.generate_cache_key(hook, files);
        let mut cache = self.cache.lock().unwrap();
        cache.put(cache_key, result.clone());
    }

    pub fn get_cached_result(&self, hook: &Hook, files: &[PathBuf]) -> Option<HookExecutionResult> {
        let cache_key = self.generate_cache_key(hook, files);
        let mut cache = self.cache.lock().unwrap();
        cache.get(&cache_key).cloned()
    }

    fn generate_cache_key(&self, hook: &Hook, files: &[PathBuf]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        hook.id.hash(&mut hasher);
        hook.entry.hash(&mut hasher);
        hook.args.hash(&mut hasher);

        // Hash file paths and their modification times
        for file in files {
            file.hash(&mut hasher);
            if let Ok(metadata) = std::fs::metadata(file) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        duration.as_secs().hash(&mut hasher);
                    }
                }
            }
        }

        format!("{:x}", hasher.finish())
    }

    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    pub fn len(&self) -> usize {
        let cache = self.cache.lock().unwrap();
        cache.len()
    }

    pub fn is_empty(&self) -> bool {
        let cache = self.cache.lock().unwrap();
        cache.is_empty()
    }
}
