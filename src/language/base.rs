// Base implementation for common language plugin functionality

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Result;

use super::dependency::DependencyManager;
use super::environment::{EnvironmentConfig, LanguageEnvironment, ValidationReport};
use super::traits::{Command, LanguageConfig};

/// Base implementation for common language plugin functionality
pub struct BaseLanguagePlugin {
    pub name: String,
    pub extensions: Vec<String>,
    pub config: LanguageConfig,
    pub dependency_manager: Box<dyn DependencyManager>,
}

impl BaseLanguagePlugin {
    pub fn new(name: String, extensions: Vec<String>) -> Self {
        Self {
            name,
            extensions,
            config: LanguageConfig::default(),
            dependency_manager: Box::new(super::dependency::MockDependencyManager::new()),
        }
    }

    // Common implementations
    pub fn detect_by_extension(&self, file_path: &Path) -> bool {
        if let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) {
            self.extensions.iter().any(|ext| ext == extension)
        } else {
            false
        }
    }

    pub fn generate_environment_id(&self, config: &EnvironmentConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.name.hash(&mut hasher);
        config.language_version.hash(&mut hasher);
        config.additional_dependencies.hash(&mut hasher);

        format!("{}-{:x}", self.name, hasher.finish())
    }

    pub fn setup_base_environment(
        &self,
        config: &EnvironmentConfig,
    ) -> Result<LanguageEnvironment> {
        let env_id = self.generate_environment_id(config);
        let root_path = std::env::temp_dir().join("snp-environments").join(&env_id);

        std::fs::create_dir_all(&root_path)?;

        Ok(LanguageEnvironment {
            language: self.name.clone(),
            environment_id: env_id,
            root_path: root_path.clone(),
            executable_path: PathBuf::new(), // To be set by specific language implementation
            environment_variables: config.environment_variables.clone(),
            installed_dependencies: Vec::new(),
            metadata: super::environment::EnvironmentMetadata {
                created_at: std::time::SystemTime::now(),
                last_used: std::time::SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count: 0,
                language_version: config
                    .language_version
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    // Utility methods
    pub fn find_executable(&self, name: &str, env: &LanguageEnvironment) -> Result<PathBuf> {
        // First check in environment's bin directory
        let env_bin = env.root_path.join("bin").join(name);
        if env_bin.exists() {
            return Ok(env_bin);
        }

        // Then check system PATH
        if let Ok(path) = which::which(name) {
            return Ok(path);
        }

        Err(crate::error::SnpError::Process(Box::new(
            crate::error::ProcessError::CommandNotFound {
                command: name.to_string(),
                suggestion: Some(format!("Install {name} or add it to PATH")),
            },
        )))
    }

    pub fn check_version(&self, _executable: &Path) -> Result<String> {
        // Mock version check - in real implementation, this would run the executable
        Ok("1.0.0".to_string())
    }

    pub fn validate_installation(&self, env: &LanguageEnvironment) -> Result<ValidationReport> {
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        // Check if root path exists
        if !env.root_path.exists() {
            issues.push(super::environment::ValidationIssue {
                severity: super::environment::IssueSeverity::Error,
                category: super::environment::IssueCategory::PathConfiguration,
                description: "Environment root path does not exist".to_string(),
                fix_suggestion: Some("Recreate the environment".to_string()),
            });
        }

        // Check if executable exists
        if !env.executable_path.exists() && !env.executable_path.as_os_str().is_empty() {
            issues.push(super::environment::ValidationIssue {
                severity: super::environment::IssueSeverity::Warning,
                category: super::environment::IssueCategory::MissingDependency,
                description: format!("Executable not found: {}", env.executable_path.display()),
                fix_suggestion: Some("Install the required executable".to_string()),
            });
        }

        let is_healthy = issues.iter().all(|issue| {
            matches!(
                issue.severity,
                super::environment::IssueSeverity::Info
                    | super::environment::IssueSeverity::Warning
            )
        });

        if !is_healthy {
            recommendations.push("Consider recreating the environment".to_string());
        }

        Ok(ValidationReport {
            is_healthy,
            issues,
            recommendations,
            performance_score: if is_healthy { 1.0 } else { 0.5 },
        })
    }
}

/// Command construction utilities
pub struct CommandBuilder {
    base_command: String,
    arguments: Vec<String>,
    environment: HashMap<String, String>,
    working_directory: Option<PathBuf>,
}

impl CommandBuilder {
    pub fn new(executable: &str) -> Self {
        Self {
            base_command: executable.to_string(),
            arguments: Vec::new(),
            environment: HashMap::new(),
            working_directory: None,
        }
    }

    pub fn arg(&mut self, arg: &str) -> &mut Self {
        self.arguments.push(arg.to_string());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.arguments
            .extend(args.into_iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn env(&mut self, key: &str, value: &str) -> &mut Self {
        self.environment.insert(key.to_string(), value.to_string());
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (key, value) in vars {
            self.environment
                .insert(key.as_ref().to_string(), value.as_ref().to_string());
        }
        self
    }

    pub fn current_dir(&mut self, dir: &Path) -> &mut Self {
        self.working_directory = Some(dir.to_path_buf());
        self
    }

    pub fn build(&self) -> Command {
        Command {
            executable: self.base_command.clone(),
            arguments: self.arguments.clone(),
            environment: self.environment.clone(),
            working_directory: self.working_directory.clone(),
            timeout: Some(Duration::from_secs(300)), // Default 5 minute timeout
        }
    }
}

/// Macro for simplified language plugin implementation
#[macro_export]
macro_rules! language_plugin {
    (
        name: $name:expr,
        extensions: [$($ext:expr),*],
        executable: $executable:expr,
        dependency_manager: $dep_manager:expr
    ) => {
        pub struct Plugin {
            base: $crate::language::BaseLanguagePlugin,
        }

        impl Plugin {
            pub fn new() -> Self {
                Self {
                    base: $crate::language::BaseLanguagePlugin::new(
                        $name.to_string(),
                        vec![$($ext.to_string()),*],
                    ),
                }
            }
        }

        #[async_trait::async_trait]
        impl $crate::language::Language for Plugin {
            fn language_name(&self) -> &str {
                &self.base.name
            }

            fn supported_extensions(&self) -> &[&str] {
                static EXTENSIONS: &[&str] = &[$($ext),*];
                EXTENSIONS
            }

            fn detection_patterns(&self) -> &[&str] {
                &[] // Default: no special patterns
            }

            async fn setup_environment(&self, config: &$crate::language::EnvironmentConfig) -> $crate::error::Result<$crate::language::LanguageEnvironment> {
                let mut env = self.base.setup_base_environment(config)?;
                env.executable_path = self.base.find_executable($executable, &env)?;
                Ok(env)
            }

            async fn install_dependencies(&self, env: &$crate::language::LanguageEnvironment, dependencies: &[$crate::language::Dependency]) -> $crate::error::Result<()> {
                self.base.dependency_manager.install(dependencies, env).await?;
                Ok(())
            }

            async fn cleanup_environment(&self, _env: &$crate::language::LanguageEnvironment) -> $crate::error::Result<()> {
                Ok(())
            }

            async fn execute_hook(&self, hook: &$crate::core::Hook, env: &$crate::language::LanguageEnvironment, files: &[std::path::PathBuf]) -> $crate::error::Result<$crate::execution::HookExecutionResult> {
                use std::time::Instant;
                let start_time = Instant::now();
                let command = self.build_command(hook, env, files)?;

                // Mock execution
                Ok($crate::execution::HookExecutionResult {
                    hook_id: hook.id.clone(),
                    exit_code: 0,
                    stdout: "Mock execution".to_string(),
                    stderr: String::new(),
                    duration: start_time.elapsed(),
                    files_processed: files.to_vec(),
                })
            }

            fn build_command(&self, hook: &$crate::core::Hook, env: &$crate::language::LanguageEnvironment, files: &[std::path::PathBuf]) -> $crate::error::Result<$crate::language::Command> {
                let mut builder = $crate::language::CommandBuilder::new(&env.executable_path.to_string_lossy());
                builder.args(&hook.args);

                if hook.pass_filenames {
                    builder.args(files.iter().map(|f| f.to_string_lossy().to_string()));
                }

                builder.envs(&env.environment_variables);
                builder.current_dir(&env.root_path);

                Ok(builder.build())
            }

            async fn resolve_dependencies(&self, dependencies: &[String]) -> $crate::error::Result<Vec<$crate::language::Dependency>> {
                Ok(dependencies.iter().map(|dep| {
                    $crate::language::Dependency::parse_spec(dep).unwrap_or_else(|_| $crate::language::Dependency::new(dep))
                }).collect())
            }

            fn parse_dependency(&self, dep_spec: &str) -> $crate::error::Result<$crate::language::Dependency> {
                $crate::language::Dependency::parse_spec(dep_spec)
            }

            fn get_dependency_manager(&self) -> &dyn $crate::language::DependencyManager {
                self.base.dependency_manager.as_ref()
            }

            fn get_environment_info(&self, env: &$crate::language::LanguageEnvironment) -> $crate::language::EnvironmentInfo {
                $crate::language::EnvironmentInfo {
                    environment_id: env.environment_id.clone(),
                    language: env.language.clone(),
                    language_version: env.metadata.language_version.clone(),
                    created_at: env.metadata.created_at,
                    last_used: env.metadata.last_used,
                    usage_count: env.metadata.usage_count,
                    size_bytes: env.metadata.size_bytes,
                    dependency_count: env.metadata.dependency_count,
                    is_healthy: true,
                }
            }

            fn validate_environment(&self, env: &$crate::language::LanguageEnvironment) -> $crate::error::Result<$crate::language::ValidationReport> {
                self.base.validate_installation(env)
            }

            fn default_config(&self) -> $crate::language::LanguageConfig {
                $crate::language::LanguageConfig::default()
            }

            fn validate_config(&self, _config: &$crate::language::LanguageConfig) -> $crate::error::Result<()> {
                Ok(())
            }
        }
    };
}
