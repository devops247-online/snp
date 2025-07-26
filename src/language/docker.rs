// Docker language plugin for containerized hooks and tools
// This plugin provides comprehensive Docker container management, image handling,
// and hook execution for containerized pre-commit hooks and development tools.

#![allow(clippy::uninlined_format_args)]

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};
use tokio::process::Command as TokioCommand;

use crate::core::Hook;
use crate::error::{Result, SnpError};
use crate::execution::HookExecutionResult;

use super::dependency::{Dependency, DependencyManager, DependencyManagerConfig};
use super::environment::{
    EnvironmentConfig, EnvironmentInfo, LanguageEnvironment, ValidationReport,
};
use super::traits::{Command, Language, LanguageConfig, LanguageError};

/// Docker language plugin for containerized hooks and tools
pub struct DockerLanguagePlugin {
    config: DockerLanguageConfig,
    docker_info_cache: Option<DockerInfo>,
    image_cache: HashMap<String, DockerImage>,
    container_cache: HashMap<String, ContainerInfo>,
    dependency_manager: DockerDependencyManager,
}

#[derive(Debug, Clone)]
pub struct DockerLanguageConfig {
    pub docker_command: String,
    pub default_base_image: String,
    pub enable_buildkit: bool,
    pub container_config: ContainerConfig,
    pub image_config: ImageConfig,
    pub security_config: SecurityConfig,
}

#[derive(Debug, Clone)]
pub struct ContainerConfig {
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
    pub timeout: Duration,
    pub cleanup_on_exit: bool,
    pub network_mode: NetworkMode,
    pub user_mapping: UserMapping,
}

#[derive(Debug, Clone)]
pub enum NetworkMode {
    None,
    Bridge,
    Host,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct UserMapping {
    pub map_current_user: bool,
    pub container_user: Option<String>,
    pub uid_mapping: Option<(u32, u32)>,
    pub gid_mapping: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub struct ImageConfig {
    pub cache_images: bool,
    pub pull_policy: PullPolicy,
    pub build_cache: bool,
    pub registry_config: RegistryConfig,
}

#[derive(Debug, Clone)]
pub enum PullPolicy {
    Always,
    IfNotPresent,
    Never,
}

#[derive(Debug, Clone, Default)]
pub struct RegistryConfig {
    pub default_registry: Option<String>,
    pub authenticated_registries: HashMap<String, AuthConfig>,
    pub insecure_registries: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub username: String,
    pub password: String, // In practice, should be secured
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub allow_privileged: bool,
    pub security_opts: Vec<String>,
    pub cap_add: Vec<String>,
    pub cap_drop: Vec<String>,
    pub read_only_root: bool,
}

/// Docker dependency manager for image-based dependencies
pub struct DockerDependencyManager {
    config: DependencyManagerConfig,
}

impl Default for DockerLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: DockerLanguageConfig::default(),
            docker_info_cache: None,
            image_cache: HashMap::new(),
            container_cache: HashMap::new(),
            dependency_manager: DockerDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    pub fn with_config(config: DockerLanguageConfig) -> Self {
        Self {
            config,
            docker_info_cache: None,
            image_cache: HashMap::new(),
            container_cache: HashMap::new(),
            dependency_manager: DockerDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    /// Detect Docker installation and capabilities
    pub async fn detect_docker_installation(&mut self) -> Result<DockerInfo> {
        // Check cache first
        if let Some(ref cached) = self.docker_info_cache {
            return Ok(cached.clone());
        }

        let docker_path = which::which(&self.config.docker_command).map_err(|_| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: "Docker not found in PATH".to_string(),
                recovery_suggestion: Some(
                    "Install Docker and ensure it's in your PATH".to_string(),
                ),
            })
        })?;

        // Get Docker version
        let version_output = TokioCommand::new(&docker_path)
            .args(["version", "--format", "{{.Client.Version}}"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: format!("Failed to execute docker command: {e}"),
                    recovery_suggestion: Some("Ensure Docker daemon is running".to_string()),
                })
            })?;

        if !version_output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!(
                    "Docker not running: {}",
                    String::from_utf8_lossy(&version_output.stderr)
                ),
                recovery_suggestion: Some("Start Docker daemon".to_string()),
            }));
        }

        let version_str = String::from_utf8_lossy(&version_output.stdout)
            .trim()
            .to_string();
        let version = semver::Version::parse(&version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!("Failed to parse Docker version: {e}"),
                recovery_suggestion: None,
            })
        })?;

        // Get Docker system info
        let info_output = TokioCommand::new(&docker_path)
            .args(["system", "info", "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: format!("Failed to get Docker system info: {e}"),
                    recovery_suggestion: None,
                })
            })?;

        let docker_system_info = if info_output.status.success() {
            let info_str = String::from_utf8_lossy(&info_output.stdout);
            self.parse_docker_info(&info_str)?
        } else {
            DockerSystemInfo::default()
        };

        let docker_info = DockerInfo {
            docker_path,
            version,
            compose_available: self.check_docker_compose().await?,
            buildkit_available: self.check_buildkit_support().await?,
            system_info: docker_system_info,
        };

        // Cache the result
        self.docker_info_cache = Some(docker_info.clone());
        Ok(docker_info)
    }

    /// Parse Docker system info JSON
    fn parse_docker_info(&self, info_json: &str) -> Result<DockerSystemInfo> {
        let info: Value = serde_json::from_str(info_json).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!("Failed to parse Docker system info: {}", e),
                recovery_suggestion: None,
            })
        })?;

        Ok(DockerSystemInfo {
            containers_running: info["ContainersRunning"].as_u64().unwrap_or(0) as u32,
            containers_paused: info["ContainersPaused"].as_u64().unwrap_or(0) as u32,
            containers_stopped: info["ContainersStopped"].as_u64().unwrap_or(0) as u32,
            images: info["Images"].as_u64().unwrap_or(0) as u32,
            server_version: info["ServerVersion"]
                .as_str()
                .and_then(|v| semver::Version::parse(v).ok()),
            storage_driver: info["Driver"].as_str().map(String::from),
            cgroup_version: info["CgroupVersion"].as_str().map(String::from),
        })
    }

    /// Check if Docker Compose is available
    async fn check_docker_compose(&self) -> Result<bool> {
        // Try docker compose (newer syntax)
        if let Ok(output) = TokioCommand::new(&self.config.docker_command)
            .args(["compose", "version"])
            .output()
            .await
        {
            if output.status.success() {
                return Ok(true);
            }
        }

        // Try docker-compose (legacy)
        if let Ok(output) = TokioCommand::new("docker-compose")
            .args(["version"])
            .output()
            .await
        {
            if output.status.success() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if BuildKit is supported
    async fn check_buildkit_support(&self) -> Result<bool> {
        let output = TokioCommand::new(&self.config.docker_command)
            .args(["buildx", "version"])
            .output()
            .await
            .map_err(|_| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: "Failed to check BuildKit support".to_string(),
                    recovery_suggestion: None,
                })
            })?;

        Ok(output.status.success())
    }

    /// Find Dockerfile in the given directory
    pub fn find_dockerfile(&self, dir: &Path) -> Result<PathBuf> {
        let dockerfile_names = [
            "Dockerfile",
            "dockerfile",
            "Dockerfile.dev",
            "Dockerfile.prod",
        ];

        for name in &dockerfile_names {
            let dockerfile_path = dir.join(name);
            if dockerfile_path.exists() {
                return Ok(dockerfile_path);
            }
        }

        Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
            language: "docker".to_string(),
            error: format!("No Dockerfile found in {}", dir.display()),
            recovery_suggestion: Some("Create a Dockerfile in the working directory".to_string()),
        }))
    }

    /// Parse Dockerfile and create build context
    pub async fn prepare_docker_environment(
        &mut self,
        dockerfile_path: &Path,
        build_context: &Path,
    ) -> Result<DockerEnvironment> {
        let dockerfile = self.parse_dockerfile(dockerfile_path)?;
        let build_context_info = self.analyze_build_context(build_context)?;

        // Generate image tag
        let image_tag = self.generate_image_tag(&dockerfile, build_context)?;

        // Check if image exists or needs building
        let image_exists = self.check_image_exists(&image_tag).await?;

        let docker_image = if !image_exists || !self.config.image_config.cache_images {
            self.build_docker_image(&dockerfile, build_context, &image_tag)
                .await?
        } else {
            self.get_cached_image(&image_tag).await?
        };

        Ok(DockerEnvironment {
            dockerfile,
            build_context: build_context.to_path_buf(),
            docker_image,
            build_context_info,
        })
    }

    /// Parse Dockerfile
    fn parse_dockerfile(&self, dockerfile_path: &Path) -> Result<Dockerfile> {
        let content = std::fs::read_to_string(dockerfile_path).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!("Failed to read Dockerfile: {}", e),
                recovery_suggestion: None,
            })
        })?;

        let mut instructions = Vec::new();
        let mut base_image = String::new();
        let build_args = HashMap::new();
        let mut exposed_ports = Vec::new();
        let mut working_dir = None;
        let mut user = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let instruction = self.parse_dockerfile_instruction(line)?;

            match &instruction {
                DockerInstruction::From { image, .. } => {
                    if base_image.is_empty() {
                        base_image = image.clone();
                    }
                }
                DockerInstruction::Expose { port } => {
                    exposed_ports.push(*port);
                }
                DockerInstruction::Workdir { path } => {
                    working_dir = Some(path.clone());
                }
                DockerInstruction::User { user: u } => {
                    user = Some(u.clone());
                }
                _ => {}
            }

            instructions.push(instruction);
        }

        Ok(Dockerfile {
            path: dockerfile_path.to_path_buf(),
            base_image,
            instructions,
            build_args,
            exposed_ports,
            working_dir,
            user,
        })
    }

    /// Parse a single Dockerfile instruction
    fn parse_dockerfile_instruction(&self, line: &str) -> Result<DockerInstruction> {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            return Ok(DockerInstruction::Other {
                instruction: line.to_string(),
                args: String::new(),
            });
        }

        let instruction = parts[0].to_uppercase();
        let args = parts[1];

        match instruction.as_str() {
            "FROM" => {
                let from_parts: Vec<&str> = args.split(" AS ").collect();
                Ok(DockerInstruction::From {
                    image: from_parts[0].trim().to_string(),
                    alias: if from_parts.len() > 1 {
                        Some(from_parts[1].trim().to_string())
                    } else {
                        None
                    },
                })
            }
            "RUN" => Ok(DockerInstruction::Run {
                command: args.to_string(),
            }),
            "COPY" => {
                let copy_parts: Vec<&str> = args.split_whitespace().collect();
                if copy_parts.len() >= 2 {
                    Ok(DockerInstruction::Copy {
                        src: copy_parts[0].to_string(),
                        dest: copy_parts[copy_parts.len() - 1].to_string(),
                    })
                } else {
                    Ok(DockerInstruction::Other {
                        instruction: instruction.clone(),
                        args: args.to_string(),
                    })
                }
            }
            "WORKDIR" => Ok(DockerInstruction::Workdir {
                path: args.to_string(),
            }),
            "EXPOSE" => {
                if let Ok(port) = args.parse::<u16>() {
                    Ok(DockerInstruction::Expose { port })
                } else {
                    Ok(DockerInstruction::Other {
                        instruction: instruction.clone(),
                        args: args.to_string(),
                    })
                }
            }
            "USER" => Ok(DockerInstruction::User {
                user: args.to_string(),
            }),
            _ => Ok(DockerInstruction::Other {
                instruction: instruction.clone(),
                args: args.to_string(),
            }),
        }
    }

    /// Analyze build context
    fn analyze_build_context(&self, build_context: &Path) -> Result<BuildContextInfo> {
        let dockerignore_path = build_context.join(".dockerignore");
        let dockerignore_exists = dockerignore_path.exists();

        let ignored_patterns = if dockerignore_exists {
            std::fs::read_to_string(&dockerignore_path)
                .unwrap_or_default()
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .collect()
        } else {
            vec![]
        };

        // Calculate context size (simplified)
        let mut context_size = 0u64;
        let mut file_count = 0usize;

        if let Ok(entries) = std::fs::read_dir(build_context) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        context_size += metadata.len();
                        file_count += 1;
                    }
                }
            }
        }

        Ok(BuildContextInfo {
            root_path: build_context.to_path_buf(),
            dockerignore_path: if dockerignore_exists {
                Some(dockerignore_path)
            } else {
                None
            },
            ignored_patterns,
            context_size,
            file_count,
        })
    }

    /// Generate image tag based on context
    fn generate_image_tag(&self, dockerfile: &Dockerfile, build_context: &Path) -> Result<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        dockerfile.path.hash(&mut hasher);
        build_context.hash(&mut hasher);
        let hash = hasher.finish();

        Ok(format!("snp-docker-{:x}", hash))
    }

    /// Check if Docker image exists
    async fn check_image_exists(&self, image_tag: &str) -> Result<bool> {
        let output = TokioCommand::new(&self.config.docker_command)
            .args(["images", "-q", image_tag])
            .output()
            .await
            .map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: format!("Failed to check if image exists: {}", e),
                    recovery_suggestion: None,
                })
            })?;

        Ok(!output.stdout.is_empty())
    }

    /// Build Docker image
    async fn build_docker_image(
        &self,
        dockerfile: &Dockerfile,
        build_context: &Path,
        image_tag: &str,
    ) -> Result<DockerImage> {
        let mut build_args = vec!["build".to_string()];

        // Add BuildKit support if enabled
        if self.config.enable_buildkit {
            build_args.push("--progress=plain".to_string());
        }

        // Add build arguments
        for (key, value) in &dockerfile.build_args {
            build_args.extend(["--build-arg".to_string(), format!("{}={}", key, value)]);
        }

        // Add tag
        build_args.extend(["-t".to_string(), image_tag.to_string()]);

        // Add build context
        build_args.push(build_context.display().to_string());

        let output = TokioCommand::new(&self.config.docker_command)
            .args(&build_args)
            .output()
            .await
            .map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: format!("Failed to build Docker image: {}", e),
                    recovery_suggestion: None,
                })
            })?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!(
                    "Docker image build failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                recovery_suggestion: Some("Check Dockerfile syntax and dependencies".to_string()),
            }));
        }

        // Get image info
        self.get_image_info(image_tag).await
    }

    /// Get image information
    async fn get_image_info(&self, image_tag: &str) -> Result<DockerImage> {
        let inspect_output = TokioCommand::new(&self.config.docker_command)
            .args(["image", "inspect", image_tag, "--format", "json"])
            .output()
            .await
            .map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: format!("Failed to inspect Docker image: {}", e),
                    recovery_suggestion: None,
                })
            })?;

        if !inspect_output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!("Docker image not found: {}", image_tag),
                recovery_suggestion: None,
            }));
        }

        let inspect_str = String::from_utf8_lossy(&inspect_output.stdout);
        self.parse_image_info(&inspect_str)
    }

    /// Parse image information from Docker inspect output
    fn parse_image_info(&self, inspect_json: &str) -> Result<DockerImage> {
        let images: Vec<Value> = serde_json::from_str(inspect_json).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: format!("Failed to parse image info: {}", e),
                recovery_suggestion: None,
            })
        })?;

        if images.is_empty() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "docker".to_string(),
                error: "No image information found".to_string(),
                recovery_suggestion: None,
            }));
        }

        let image = &images[0];

        Ok(DockerImage {
            tag: image["RepoTags"][0]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            id: image["Id"].as_str().unwrap_or("unknown").to_string(),
            size: image["Size"].as_u64().unwrap_or(0),
            created: SystemTime::now(), // Simplified
            layers: vec![],             // Simplified
        })
    }

    /// Get cached image
    async fn get_cached_image(&self, image_tag: &str) -> Result<DockerImage> {
        self.get_image_info(image_tag).await
    }

    /// Build Docker command for execution
    pub fn build_docker_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        let mut docker_args = vec!["run".to_string()];

        // Add cleanup flag
        if self.config.container_config.cleanup_on_exit {
            docker_args.push("--rm".to_string());
        }

        // Add resource limits
        if let Some(ref memory) = self.config.container_config.memory_limit {
            docker_args.extend(["--memory".to_string(), memory.clone()]);
        }

        if let Some(ref cpu) = self.config.container_config.cpu_limit {
            docker_args.extend(["--cpus".to_string(), cpu.clone()]);
        }

        // Add security options
        if self.config.security_config.read_only_root {
            docker_args.push("--read-only".to_string());
        }

        for cap in &self.config.security_config.cap_drop {
            docker_args.extend(["--cap-drop".to_string(), cap.clone()]);
        }

        for cap in &self.config.security_config.cap_add {
            docker_args.extend(["--cap-add".to_string(), cap.clone()]);
        }

        // Add user mapping
        if self.config.container_config.user_mapping.map_current_user {
            #[cfg(unix)]
            {
                let current_uid = unsafe { libc::getuid() };
                let current_gid = unsafe { libc::getgid() };
                docker_args.extend([
                    "--user".to_string(),
                    format!("{}:{}", current_uid, current_gid),
                ]);
            }
        }

        // Add volume mounts
        docker_args.extend([
            "--volume".to_string(),
            format!("{}:/workspace", env.root_path.display()),
            "--workdir".to_string(),
            "/workspace".to_string(),
        ]);

        // Add environment variables
        for (key, value) in &env.environment_variables {
            docker_args.extend(["--env".to_string(), format!("{}={}", key, value)]);
        }

        // Add image tag (simplified - would need actual image management)
        docker_args.push("alpine:latest".to_string()); // Default image for now

        // Add hook entry point
        docker_args.push(hook.entry.clone());

        // Add hook arguments
        docker_args.extend(hook.args.iter().cloned());

        // Add file arguments if enabled
        if hook.pass_filenames {
            for file in files {
                docker_args.push(file.to_string_lossy().to_string());
            }
        }

        Ok(Command {
            executable: self.config.docker_command.clone(),
            arguments: docker_args,
            environment: HashMap::new(), // Docker handles environment passing
            working_directory: Some(env.root_path.clone()),
            timeout: Some(self.config.container_config.timeout),
        })
    }

    /// Execute Docker command
    pub async fn execute_docker_command(&self, command: Command) -> Result<HookExecutionResult> {
        let start_time = Instant::now();

        // Build tokio command
        let mut tokio_cmd = TokioCommand::new(&command.executable);
        tokio_cmd.args(&command.arguments);
        tokio_cmd.envs(&command.environment);

        if let Some(ref cwd) = command.working_directory {
            tokio_cmd.current_dir(cwd);
        }

        tokio_cmd.stdout(Stdio::piped());
        tokio_cmd.stderr(Stdio::piped());

        // Execute with timeout
        let output = if let Some(timeout) = command.timeout {
            match tokio::time::timeout(timeout, tokio_cmd.output()).await {
                Ok(output) => output.map_err(|e| {
                    SnpError::from(LanguageError::EnvironmentSetupFailed {
                        language: "docker".to_string(),
                        error: format!("Docker command execution failed: {}", e),
                        recovery_suggestion: None,
                    })
                })?,
                Err(_) => {
                    return Err(SnpError::from(Box::new(
                        crate::error::ProcessError::Timeout {
                            command: command.executable.clone(),
                            duration: timeout,
                        },
                    )));
                }
            }
        } else {
            tokio_cmd.output().await.map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "docker".to_string(),
                    error: format!("Docker command execution failed: {}", e),
                    recovery_suggestion: None,
                })
            })?
        };

        let duration = start_time.elapsed();

        Ok(HookExecutionResult {
            hook_id: "docker".to_string(), // Will be overridden by caller
            success: output.status.success(),
            skipped: false,
            skip_reason: None,
            exit_code: output.status.code(),
            duration,
            files_processed: vec![], // Will be set by caller
            files_modified: vec![],
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            error: if output.status.success() {
                None
            } else {
                Some(crate::error::HookExecutionError::ExecutionFailed {
                    hook_id: "docker".to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            },
        })
    }
}

// Data structures

#[derive(Debug, Clone)]
pub struct DockerInfo {
    pub docker_path: PathBuf,
    pub version: semver::Version,
    pub compose_available: bool,
    pub buildkit_available: bool,
    pub system_info: DockerSystemInfo,
}

#[derive(Debug, Clone, Default)]
pub struct DockerSystemInfo {
    pub containers_running: u32,
    pub containers_paused: u32,
    pub containers_stopped: u32,
    pub images: u32,
    pub server_version: Option<semver::Version>,
    pub storage_driver: Option<String>,
    pub cgroup_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Dockerfile {
    pub path: PathBuf,
    pub base_image: String,
    pub instructions: Vec<DockerInstruction>,
    pub build_args: HashMap<String, String>,
    pub exposed_ports: Vec<u16>,
    pub working_dir: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DockerInstruction {
    From {
        image: String,
        alias: Option<String>,
    },
    Run {
        command: String,
    },
    Copy {
        src: String,
        dest: String,
    },
    Add {
        src: String,
        dest: String,
    },
    Workdir {
        path: String,
    },
    Expose {
        port: u16,
    },
    Env {
        key: String,
        value: String,
    },
    User {
        user: String,
    },
    Entrypoint {
        command: Vec<String>,
    },
    Cmd {
        command: Vec<String>,
    },
    Other {
        instruction: String,
        args: String,
    },
}

#[derive(Debug, Clone)]
pub struct DockerImage {
    pub tag: String,
    pub id: String,
    pub size: u64,
    pub created: SystemTime,
    pub layers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BuildContextInfo {
    pub root_path: PathBuf,
    pub dockerignore_path: Option<PathBuf>,
    pub ignored_patterns: Vec<String>,
    pub context_size: u64,
    pub file_count: usize,
}

#[derive(Debug, Clone)]
pub struct DockerEnvironment {
    pub dockerfile: Dockerfile,
    pub build_context: PathBuf,
    pub docker_image: DockerImage,
    pub build_context_info: BuildContextInfo,
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: ContainerStatus,
    pub created: SystemTime,
    pub ports: Vec<PortMapping>,
    pub mounts: Vec<VolumeMount>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContainerStatus {
    Created,
    Running,
    Paused,
    Restarting,
    Removing,
    Exited(i32),
    Dead,
}

#[derive(Debug, Clone)]
pub struct PortMapping {
    pub container_port: u16,
    pub host_port: Option<u16>,
    pub protocol: String,
}

#[derive(Debug, Clone)]
pub struct VolumeMount {
    pub source: PathBuf,
    pub destination: String,
    pub mode: MountMode,
}

#[derive(Debug, Clone)]
pub enum MountMode {
    ReadOnly,
    ReadWrite,
    Bind,
    Volume,
    Tmpfs,
}

// Default implementations

impl Default for DockerLanguageConfig {
    fn default() -> Self {
        Self {
            docker_command: "docker".to_string(),
            default_base_image: "alpine:latest".to_string(),
            enable_buildkit: true,
            container_config: ContainerConfig::default(),
            image_config: ImageConfig::default(),
            security_config: SecurityConfig::default(),
        }
    }
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            memory_limit: Some("512m".to_string()),
            cpu_limit: Some("0.5".to_string()),
            timeout: Duration::from_secs(300), // 5 minutes
            cleanup_on_exit: true,
            network_mode: NetworkMode::Bridge,
            user_mapping: UserMapping::default(),
        }
    }
}

impl Default for UserMapping {
    fn default() -> Self {
        Self {
            map_current_user: true,
            container_user: None,
            uid_mapping: None,
            gid_mapping: None,
        }
    }
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            cache_images: true,
            pull_policy: PullPolicy::IfNotPresent,
            build_cache: true,
            registry_config: RegistryConfig::default(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allow_privileged: false,
            security_opts: vec![],
            cap_add: vec![],
            cap_drop: vec!["ALL".to_string()],
            read_only_root: false,
        }
    }
}

// Language trait implementation

#[async_trait]
impl Language for DockerLanguagePlugin {
    fn language_name(&self) -> &str {
        "docker"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["dockerfile", "Dockerfile"]
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            "Dockerfile",
            "dockerfile",
            "Dockerfile.*",
            "docker-compose.yml",
            "docker-compose.yaml",
            ".dockerignore",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        // Check Docker availability
        let mut plugin = self.clone();
        let docker_info = plugin.detect_docker_installation().await?;

        // Determine working directory
        let working_dir = config
            .working_directory
            .as_ref()
            .unwrap_or(&std::env::current_dir()?)
            .clone();

        // Find Dockerfile
        let dockerfile_path = self.find_dockerfile(&working_dir)?;
        let build_context = dockerfile_path.parent().unwrap_or(&working_dir);

        // Prepare Docker environment
        let docker_env = plugin
            .prepare_docker_environment(&dockerfile_path, build_context)
            .await?;

        let environment_id = format!("docker-{}", uuid::Uuid::new_v4());

        let mut environment_variables = config.environment_variables.clone();

        // Set platform-specific Docker host
        #[cfg(unix)]
        let docker_host = "unix:///var/run/docker.sock";
        #[cfg(windows)]
        let docker_host = "npipe:////./pipe/docker_engine";
        #[cfg(not(any(unix, windows)))]
        let docker_host = "tcp://localhost:2376";

        environment_variables.insert("DOCKER_HOST".to_string(), docker_host.to_string());

        Ok(LanguageEnvironment {
            language: "docker".to_string(),
            environment_id,
            root_path: working_dir.clone(),
            executable_path: docker_info.docker_path,
            environment_variables,
            installed_dependencies: vec![], // Docker manages its own dependencies
            metadata: super::environment::EnvironmentMetadata {
                created_at: SystemTime::now(),
                last_used: SystemTime::now(),
                usage_count: 0,
                size_bytes: docker_env.build_context_info.context_size,
                dependency_count: 0,
                language_version: docker_info.version.to_string(),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    async fn install_dependencies(
        &self,
        _env: &LanguageEnvironment,
        _dependencies: &[Dependency],
    ) -> Result<()> {
        // Docker language doesn't install dependencies - they should be in the image
        Ok(())
    }

    async fn cleanup_environment(&self, _env: &LanguageEnvironment) -> Result<()> {
        // Docker environment cleanup is handled by the --rm flag
        Ok(())
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        let command = self.build_command(hook, env, files)?;
        let mut result = self.execute_docker_command(command).await?;

        // Set the correct hook ID and files
        result.hook_id = hook.id.clone();
        result.files_processed = files.to_vec();

        Ok(result)
    }

    fn build_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        self.build_docker_command(hook, env, files)
    }

    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
        // For Docker language, dependencies are typically Docker images
        let mut resolved = Vec::new();
        for dep_str in dependencies {
            let dependency = Dependency::new(dep_str);
            resolved.push(dependency);
        }
        Ok(resolved)
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        // Docker dependencies are typically image specifications like "alpine:latest"
        Ok(Dependency::new(dep_spec))
    }

    fn get_dependency_manager(&self) -> &dyn DependencyManager {
        &self.dependency_manager
    }

    fn get_environment_info(&self, env: &LanguageEnvironment) -> EnvironmentInfo {
        EnvironmentInfo {
            environment_id: env.environment_id.clone(),
            language: env.language.clone(),
            language_version: env.metadata.language_version.clone(),
            created_at: env.metadata.created_at,
            last_used: env.metadata.last_used,
            usage_count: env.metadata.usage_count,
            size_bytes: env.metadata.size_bytes,
            dependency_count: env.installed_dependencies.len(),
            is_healthy: true, // Simplified health check
        }
    }

    fn validate_environment(&self, env: &LanguageEnvironment) -> Result<ValidationReport> {
        let mut issues = vec![];
        let recommendations = vec![];

        // Check if Docker is still available
        if !env.executable_path.exists() {
            issues.push(super::environment::ValidationIssue {
                severity: super::environment::IssueSeverity::Error,
                category: super::environment::IssueCategory::MissingDependency,
                description: "Docker executable not found".to_string(),
                fix_suggestion: Some("Reinstall Docker".to_string()),
            });
        }

        // Check if working directory still exists
        if !env.root_path.exists() {
            issues.push(super::environment::ValidationIssue {
                severity: super::environment::IssueSeverity::Error,
                category: super::environment::IssueCategory::PathConfiguration,
                description: "Working directory not found".to_string(),
                fix_suggestion: Some("Ensure working directory exists".to_string()),
            });
        }

        let performance_score = if issues.is_empty() { 1.0 } else { 0.5 };

        Ok(ValidationReport {
            is_healthy: issues.is_empty(),
            issues,
            recommendations,
            performance_score,
        })
    }

    fn default_config(&self) -> LanguageConfig {
        LanguageConfig::default()
    }

    fn validate_config(&self, _config: &LanguageConfig) -> Result<()> {
        // Basic config validation for Docker
        Ok(())
    }
}

// Dependency manager implementation

#[async_trait]
impl DependencyManager for DockerDependencyManager {
    async fn install(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<super::dependency::InstallationResult> {
        // Docker dependencies (images) are pulled automatically
        Ok(super::dependency::InstallationResult {
            installed: vec![],
            failed: vec![],
            skipped: vec![],
            duration: Duration::from_millis(0),
        })
    }

    async fn resolve(
        &self,
        dependencies: &[Dependency],
    ) -> Result<Vec<super::dependency::ResolvedDependency>> {
        let mut resolved = Vec::new();
        for dep in dependencies {
            resolved.push(super::dependency::ResolvedDependency {
                dependency: dep.clone(),
                resolved_version: dep.version_spec.to_string(),
                dependencies: vec![],
                conflicts: vec![],
            });
        }
        Ok(resolved)
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        // For Docker, assume images can be pulled when needed
        Ok(vec![true; dependencies.len()])
    }

    async fn update(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<super::dependency::UpdateResult> {
        Ok(super::dependency::UpdateResult {
            updated: vec![],
            failed: vec![],
            no_update_available: vec![],
            duration: Duration::from_millis(0),
        })
    }

    async fn uninstall(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<()> {
        Ok(()) // Docker images can be removed manually
    }

    async fn list_installed(
        &self,
        _env: &LanguageEnvironment,
    ) -> Result<Vec<super::dependency::InstalledPackage>> {
        Ok(vec![]) // Docker doesn't track installed packages this way
    }
}

// Clone implementation for the plugin

impl Clone for DockerLanguagePlugin {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            docker_info_cache: self.docker_info_cache.clone(),
            image_cache: self.image_cache.clone(),
            container_cache: self.container_cache.clone(),
            dependency_manager: DockerDependencyManager {
                config: self.dependency_manager.config.clone(),
            },
        }
    }
}
