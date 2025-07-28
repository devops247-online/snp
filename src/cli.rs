// CLI interface for SNP using clap
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use crate::cache::CacheConfig;
use crate::commands::run;
use crate::core::Stage;
use crate::error::Result;
use crate::execution::ExecutionConfig;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "snp",
    about = "SNP (Shell Not Pass) - A fast, reliable pre-commit framework written in Rust",
    version = crate::VERSION,
    long_about = "SNP is a framework for managing and maintaining multi-language pre-commit hooks, written in Rust for performance and reliability."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Configuration file path
    #[arg(short, long, global = true, default_value = ".pre-commit-config.yaml")]
    pub config: String,

    /// Control color output (auto, always, never)
    #[arg(long, global = true, value_name = "WHEN")]
    pub color: Option<String>,

    /// Disable multi-tier regex cache
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Set L1 cache size (hot patterns)
    #[arg(long, global = true, value_name = "SIZE")]
    pub cache_l1_size: Option<usize>,

    /// Set L2 cache size (warm patterns)
    #[arg(long, global = true, value_name = "SIZE")]
    pub cache_l2_size: Option<usize>,
}

impl Cli {
    /// Create cache configuration from CLI arguments
    pub fn cache_config(&self) -> CacheConfig {
        CacheConfig {
            l1_max_entries: self.cache_l1_size.unwrap_or(500),
            l2_max_entries: self.cache_l2_size.unwrap_or(2000),
            promotion_threshold: 2,
            enable_l3_persistence: !self.no_cache,
            cache_warming: !self.no_cache,
            metrics_enabled: true,
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run hooks (default command)
    Run {
        /// Hook ID to run
        #[arg(long)]
        hook: Option<String>,

        /// Run on all files in the repository
        #[arg(short, long)]
        all_files: bool,

        /// Specific filenames to run hooks on
        #[arg(long, num_args = 1..)]
        files: Vec<String>,

        /// Show git diff when hooks fail
        #[arg(long)]
        show_diff_on_failure: bool,

        /// Hook stage to run
        #[arg(long, default_value = "pre-commit")]
        hook_stage: String,

        /// Source ref to use for determining changed files
        #[arg(long)]
        from_ref: Option<String>,

        /// Target ref to use for determining changed files
        #[arg(long)]
        to_ref: Option<String>,

        /// Number of concurrent hook processes to run (default: number of CPU cores)
        #[arg(short = 'j', long, value_name = "N")]
        jobs: Option<usize>,

        /// Enable verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Fail fast - stop running hooks after first failure
        #[arg(long)]
        fail_fast: bool,

        /// Show hook dependencies and execution order
        #[arg(long)]
        show_deps: bool,

        /// Enable resource pooling for improved performance
        #[arg(long)]
        use_pools: bool,
    },

    /// Install the pre-commit script
    Install {
        /// Hook types to install
        #[arg(short = 't', long)]
        hook_type: Vec<String>,

        /// Overwrite existing hooks
        #[arg(short, long)]
        overwrite: bool,

        /// Install hook environments
        #[arg(long)]
        install_hooks: bool,

        /// Allow missing config file
        #[arg(long)]
        allow_missing_config: bool,
    },

    /// Uninstall the pre-commit script
    Uninstall {
        /// Hook types to uninstall
        #[arg(short = 't', long)]
        hook_type: Vec<String>,
    },

    /// Auto-update pre-commit config to the latest repos' versions
    Autoupdate {
        /// Update to bleeding edge instead of latest tag
        #[arg(long)]
        bleeding_edge: bool,

        /// Store frozen hashes in rev instead of tag names
        #[arg(long)]
        freeze: bool,

        /// Only update specific repositories
        #[arg(long)]
        repo: Vec<String>,

        /// Number of threads to use
        #[arg(short, long, default_value = "1")]
        jobs: u32,
    },

    /// Clean out pre-commit files
    Clean {
        /// Clean only repository caches
        #[arg(long)]
        repos: bool,

        /// Clean only language environments
        #[arg(long)]
        envs: bool,

        /// Clean only temporary files
        #[arg(long)]
        temp: bool,

        /// Clean items older than specified days
        #[arg(long)]
        older_than: Option<u32>,

        /// Show what would be cleaned without removing
        #[arg(long)]
        dry_run: bool,
    },

    /// Clean unused cached repos (garbage collection)
    Gc {
        /// More thorough cleanup including recent items
        #[arg(long)]
        aggressive: bool,

        /// Show what would be cleaned without removing
        #[arg(long)]
        dry_run: bool,
    },

    /// Install hook environments for all environments in config
    InstallHooks,

    /// Install hook script in a directory for git config init.templateDir
    InitTemplateDir {
        /// Directory to write hook script
        directory: String,

        /// Hook types to install
        #[arg(short = 't', long)]
        hook_type: Vec<String>,

        /// Allow missing config
        #[arg(long)]
        allow_missing_config: bool,
    },

    /// Try hooks in a repository
    TryRepo {
        /// Repository to source hooks from
        repo: String,

        /// Specific revision to use
        #[arg(long)]
        rev: Option<String>,

        /// Run on all files
        #[arg(short, long)]
        all_files: bool,

        /// Specific files to run on
        #[arg(long)]
        files: Vec<String>,
    },

    /// Validate .pre-commit-config.yaml files
    ValidateConfig {
        /// Config files to validate
        filenames: Vec<String>,
    },

    /// Validate .pre-commit-hooks.yaml files
    ValidateManifest {
        /// Manifest files to validate
        filenames: Vec<String>,
    },

    /// Migrate list configuration to new map configuration
    MigrateConfig,

    /// Produce a sample .pre-commit-config.yaml file
    SampleConfig,

    /// Generate shell completion scripts
    GenerateCompletion {
        /// Shell to generate completion for
        shell: Shell,
    },

    /// Show version with build information
    Version,
}

impl Cli {
    pub fn run(&self) -> Result<i32> {
        // Initialize logging based on verbosity
        self.init_logging();

        // Validate conflicting flags
        if self.verbose && self.quiet {
            return Err(crate::error::SnpError::Cli(Box::new(
                crate::error::CliError::ConflictingArguments {
                    first: "--verbose".to_string(),
                    second: "--quiet".to_string(),
                    suggestion: "Use either --verbose for more output or --quiet for less output, but not both".to_string(),
                },
            )));
        }

        // Use tokio runtime for async operations
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                message: format!("Failed to create async runtime: {e}"),
            }))
        })?;

        rt.block_on(self.run_async())
    }

    async fn run_async(&self) -> Result<i32> {
        // Create user output handler
        let user_output_config =
            crate::user_output::UserOutputConfig::new(self.verbose, self.quiet, self.color.clone());
        let user_output = crate::user_output::UserOutput::new(user_output_config);

        match &self.command {
            Some(Commands::Run {
                hook,
                all_files,
                files,
                from_ref,
                to_ref,
                jobs,
                verbose,
                fail_fast,
                show_diff_on_failure: _,
                hook_stage,
                show_deps,
                use_pools,
            }) => {
                // Validate run command arguments
                if *all_files && !files.is_empty() {
                    return Err(crate::error::SnpError::Cli(Box::new(
                        crate::error::CliError::ConflictingArguments {
                            first: "--all-files".to_string(),
                            second: "--files".to_string(),
                            suggestion: "Use either --all-files to run on all files or --files to specify particular files".to_string(),
                        },
                    )));
                }

                // Validate from-ref/to-ref arguments
                if from_ref.is_some() != to_ref.is_some() {
                    return Err(crate::error::SnpError::Cli(Box::new(
                        crate::error::CliError::ConflictingArguments {
                            first: "--from-ref".to_string(),
                            second: "--to-ref".to_string(),
                            suggestion: "Both --from-ref and --to-ref must be specified together"
                                .to_string(),
                        },
                    )));
                }

                // Parse stage
                let stage = match Stage::from_str(hook_stage) {
                    Ok(stage) => stage,
                    Err(_) => {
                        return Err(crate::error::SnpError::Cli(Box::new(
                            crate::error::CliError::InvalidArgument {
                                argument: "--hook-stage".to_string(),
                                value: hook_stage.clone(),
                                suggestion: "Valid stages are: pre-commit, pre-push, commit-msg, prepare-commit-msg, post-commit, post-checkout, post-merge, pre-rebase, post-rewrite, manual, merge-commit".to_string(),
                            },
                        )));
                    }
                };

                // Get current directory as repository path
                let repo_path = std::env::current_dir().map_err(|e| {
                    crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                        message: format!("Failed to get current directory: {e}"),
                    }))
                })?;

                // Create execution configuration
                let mut execution_config = ExecutionConfig::new(stage.clone())
                    .with_verbose(*verbose || self.verbose)
                    .with_fail_fast(*fail_fast)
                    .with_hook_timeout(Duration::from_secs(60))
                    .with_user_output(user_output.clone());

                // Set parallel execution if jobs flag is provided
                if let Some(max_jobs) = jobs {
                    execution_config = execution_config.with_max_parallel_hooks(*max_jobs);
                }

                // If --show-deps is requested, display dependencies and return
                if *show_deps {
                    return self
                        .show_hook_dependencies(&repo_path, &stage, hook.as_deref())
                        .await;
                }

                // Add pool status to output if enabled
                if *use_pools {
                    user_output.show_status("Resource pooling enabled for improved performance");
                }

                // Execute based on command type
                let execution_result = if let Some(hook_id) = hook {
                    user_output.show_status(&format!("Running hook: {hook_id}"));
                    run::execute_run_command_single_hook_with_pools(
                        &repo_path,
                        &self.config,
                        hook_id,
                        &execution_config,
                        *use_pools,
                    )
                    .await?
                } else if *all_files {
                    user_output.show_status("Running hooks on all files");
                    execution_config = execution_config.with_all_files(true);
                    run::execute_run_command_all_files_with_pools(
                        &repo_path,
                        &self.config,
                        &execution_config,
                        *use_pools,
                    )
                    .await?
                } else if !files.is_empty() {
                    user_output
                        .show_status(&format!("Running hooks on {} specific files", files.len()));
                    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
                    execution_config = execution_config.with_files(file_paths);
                    run::execute_run_command_with_files_with_pools(
                        &repo_path,
                        &self.config,
                        &execution_config,
                        *use_pools,
                    )
                    .await?
                } else if let (Some(from), Some(to)) = (from_ref, to_ref) {
                    user_output.show_status(&format!(
                        "Running hooks on files changed between {from} and {to}"
                    ));
                    run::execute_run_command_with_refs_with_pools(
                        &repo_path,
                        &self.config,
                        from,
                        to,
                        &execution_config,
                        *use_pools,
                    )
                    .await?
                } else {
                    user_output.show_status("Running hooks on staged files");
                    run::execute_run_command_with_pools(
                        &repo_path,
                        &self.config,
                        &execution_config,
                        *use_pools,
                    )
                    .await?
                };

                // Report results using user-friendly output
                user_output.show_execution_summary(&execution_result);

                if execution_result.success {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            Some(Commands::Install {
                hook_type,
                overwrite,
                install_hooks,
                allow_missing_config,
            }) => {
                use crate::install::{GitHookManager, InstallConfig};

                // Get current directory as repository path
                let repo_path = std::env::current_dir().map_err(|e| {
                    crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                        message: format!("Failed to get current directory: {e}"),
                    }))
                })?;

                // Discover git repository
                let git_repo = crate::git::GitRepository::discover_from_path(&repo_path)?;
                let hook_manager = GitHookManager::new(git_repo);

                // Create install configuration
                let install_config = InstallConfig {
                    hook_types: if hook_type.is_empty() {
                        vec!["pre-commit".to_string()]
                    } else {
                        hook_type.clone()
                    },
                    overwrite_existing: *overwrite,
                    allow_missing_config: *allow_missing_config,
                    backup_existing: !overwrite,
                    hook_args: Vec::new(),
                };

                // Install hooks
                let result = hook_manager.install_hooks(&install_config).await?;

                // Report results
                if !self.quiet {
                    for hook in &result.hooks_installed {
                        println!("✓ Installed {hook} hook");
                    }
                    for hook in &result.hooks_backed_up {
                        println!("📦 Backed up existing {hook} hook");
                    }
                    for hook in &result.hooks_overwritten {
                        println!("⚠️  Overwritten {hook} hook");
                    }
                    for warning in &result.warnings {
                        eprintln!("⚠️  {warning}");
                    }

                    // Always print completion message
                    println!("Hook installation completed");
                }

                // Install hook environments if requested
                if *install_hooks {
                    println!("Installing hook environments...");
                    // TODO: Implement hook environment installation
                }

                Ok(0)
            }
            Some(Commands::Clean {
                repos,
                envs,
                temp,
                older_than,
                dry_run,
            }) => {
                use crate::commands::clean::{execute_clean_command, CleanConfig};

                // Create clean configuration
                let clean_config = CleanConfig {
                    repos: *repos,
                    envs: *envs,
                    temp: *temp,
                    older_than: older_than
                        .map(|days| Duration::from_secs(days as u64 * 24 * 60 * 60)),
                    dry_run: *dry_run,
                };

                // Execute clean command
                match execute_clean_command(&clean_config).await {
                    Ok(_result) => {
                        if !self.quiet {
                            println!("Clean operation completed successfully");
                        }
                        Ok(0)
                    }
                    Err(e) => {
                        if !self.quiet {
                            eprintln!("Clean operation failed: {e}");
                        }
                        Ok(1)
                    }
                }
            }
            Some(Commands::Gc {
                aggressive,
                dry_run,
            }) => {
                use crate::commands::clean::{execute_gc_command, GcConfig};

                // Create gc configuration
                let gc_config = GcConfig {
                    aggressive: *aggressive,
                    dry_run: *dry_run,
                };

                // Execute gc command
                match execute_gc_command(&gc_config).await {
                    Ok(_result) => {
                        if !self.quiet {
                            println!("Garbage collection completed successfully");
                        }
                        Ok(0)
                    }
                    Err(e) => {
                        if !self.quiet {
                            eprintln!("Garbage collection failed: {e}");
                        }
                        Ok(1)
                    }
                }
            }
            Some(Commands::Uninstall { hook_type }) => {
                use crate::install::{GitHookManager, UninstallConfig};

                // Get current directory as repository path
                let repo_path = std::env::current_dir().map_err(|e| {
                    crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                        message: format!("Failed to get current directory: {e}"),
                    }))
                })?;

                // Discover git repository
                let git_repo = crate::git::GitRepository::discover_from_path(&repo_path)?;
                let hook_manager = GitHookManager::new(git_repo);

                // Create uninstall configuration
                let uninstall_config = UninstallConfig {
                    hook_types: hook_type.clone(),
                    restore_backups: true,
                    clean_backups: false,
                };

                // Uninstall hooks
                let result = hook_manager.uninstall_hooks(&uninstall_config).await?;

                // Report results
                if !self.quiet {
                    for hook in &result.hooks_removed {
                        println!("✓ Removed {hook} hook");
                    }
                    for hook in &result.hooks_restored {
                        println!("🔄 Restored backup for {hook} hook");
                    }
                    for cleaned in &result.backups_cleaned {
                        println!("🧹 Cleaned backup: {cleaned}");
                    }
                    for warning in &result.warnings {
                        eprintln!("⚠️  {warning}");
                    }

                    if result.hooks_removed.is_empty() && result.warnings.is_empty() {
                        println!("No SNP hooks found to uninstall");
                    }
                }

                Ok(0)
            }
            Some(Commands::GenerateCompletion { shell }) => {
                let mut cmd = Self::command();
                let name = cmd.get_name().to_string();
                generate(*shell, &mut cmd, name, &mut std::io::stdout());
                Ok(0)
            }
            Some(Commands::Version) => {
                println!("{}", crate::version_info());
                Ok(0)
            }
            Some(Commands::Autoupdate {
                bleeding_edge,
                freeze,
                repo,
                jobs,
            }) => {
                // Get current directory as repository path
                let repo_path = std::env::current_dir().map_err(|e| {
                    crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                        message: format!("Failed to get current directory: {e}"),
                    }))
                })?;

                // Create autoupdate configuration
                let autoupdate_config = crate::commands::autoupdate::AutoupdateConfig {
                    bleeding_edge: *bleeding_edge,
                    freeze: *freeze,
                    specific_repos: repo.clone(),
                    jobs: *jobs,
                    dry_run: false, // TODO: Add dry_run flag to CLI
                };

                // Execute autoupdate command
                let config_path = repo_path.join(&self.config);
                let result = crate::commands::autoupdate::execute_autoupdate_command(
                    &config_path,
                    &autoupdate_config,
                )
                .await?;

                // Report results
                if !self.quiet {
                    println!(
                        "Processed {} repositories, {} updates available",
                        result.repositories_processed, result.updates_available
                    );
                    println!(
                        "Successful updates: {}, Failed updates: {}",
                        result.successful_updates, result.failed_updates
                    );

                    if result.failed_updates > 0 {
                        eprintln!("Some repositories failed to update:");
                        for update in &result.update_results {
                            if !update.success {
                                if let Some(ref error) = update.error {
                                    eprintln!("  {}: {}", update.repository_url, error);
                                }
                            }
                        }
                        Ok(1)
                    } else {
                        Ok(0)
                    }
                } else if result.failed_updates > 0 {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            Some(Commands::ValidateConfig { filenames }) => {
                use crate::validation::{SchemaValidator, ValidationConfig};
                use std::path::Path;

                let mut validator = SchemaValidator::new(ValidationConfig::default());
                let mut exit_code = 0;

                // If no filenames provided, use default config file
                let files_to_validate = if filenames.is_empty() {
                    vec![self.config.clone()]
                } else {
                    filenames.clone()
                };

                for filename in files_to_validate {
                    let path = Path::new(&filename);

                    if !self.quiet {
                        println!("Validating config file: {filename}");
                    }

                    let result = validator.validate_file(path);

                    if result.is_valid {
                        if !self.quiet {
                            println!("✓ {filename} is valid");
                        }
                    } else {
                        exit_code = 1;
                        eprintln!("✗ {filename} has validation errors:");

                        for error in &result.errors {
                            eprintln!("  Error: {} (at {})", error.message, error.field_path);
                            if let Some(ref suggestion) = error.suggestion {
                                eprintln!("    Suggestion: {suggestion}");
                            }
                        }

                        for warning in &result.warnings {
                            if self.verbose {
                                eprintln!(
                                    "  Warning: {} (at {})",
                                    warning.message, warning.field_path
                                );
                                if let Some(ref suggestion) = warning.suggestion {
                                    eprintln!("    Suggestion: {suggestion}");
                                }
                            }
                        }
                    }
                }

                Ok(exit_code)
            }
            Some(Commands::ValidateManifest { filenames }) => {
                use crate::validation::validate_manifest_file;

                let mut exit_code = 0;

                for filename in filenames {
                    if !self.quiet {
                        println!("Validating manifest file: {filename}");
                    }

                    match validate_manifest_file(filename) {
                        Ok(_) => {
                            if self.verbose {
                                println!("✓ {filename} is valid");
                            }
                        }
                        Err(e) => {
                            exit_code = 1;
                            eprintln!("✗ {filename}: {e}");
                        }
                    }
                }

                Ok(exit_code)
            }
            _ => {
                // Default to run command when no subcommand provided
                user_output.show_status("Running hooks (default)...");

                let repo_path = std::env::current_dir().map_err(|e| {
                    crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                        message: format!("Failed to get current directory: {e}"),
                    }))
                })?;

                let execution_config = ExecutionConfig::new(Stage::PreCommit)
                    .with_verbose(self.verbose)
                    .with_user_output(user_output.clone());

                let execution_result =
                    run::execute_run_command(&repo_path, &self.config, &execution_config).await?;

                user_output.show_execution_summary(&execution_result);

                if execution_result.success {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
        }
    }

    fn init_logging(&self) {
        use crate::logging::{init_logging, LogConfig};

        let log_config = LogConfig::from_cli(self.verbose, self.quiet, self.color.clone());

        if let Err(e) = init_logging(log_config) {
            eprintln!("Failed to initialize logging: {e}");
            // Continue execution even if logging fails
        }
    }

    /// Display hook dependencies and execution order
    async fn show_hook_dependencies(
        &self,
        repo_path: &std::path::Path,
        stage: &crate::core::Stage,
        hook_filter: Option<&str>,
    ) -> Result<i32> {
        use crate::commands::run::get_hooks_for_stage;
        use crate::config::Config;
        use crate::core::Hook;

        // Load configuration
        let config_path = repo_path.join(&self.config);
        let config = Config::from_file_with_resolved_hooks(&config_path).await?;

        // Get hooks for the specified stage
        let hooks = get_hooks_for_stage(&config, stage)?;

        // Filter to specific hook if requested
        let filtered_hooks: Vec<Hook> = if let Some(hook_id) = hook_filter {
            hooks
                .into_iter()
                .filter(|hook| hook.id == hook_id)
                .collect()
        } else {
            hooks
        };

        if filtered_hooks.is_empty() {
            if let Some(hook_id) = hook_filter {
                eprintln!("Hook '{hook_id}' not found for stage '{stage:?}'");
            } else {
                eprintln!("No hooks found for stage '{stage:?}'");
            }
            return Ok(1);
        }

        // Resolve dependencies and get execution order
        let execution_engine = crate::execution::HookExecutionEngine::new(
            std::sync::Arc::new(crate::process::ProcessManager::new()),
            std::sync::Arc::new(crate::storage::Store::new()?),
        );

        let ordered_hooks = execution_engine.resolve_hook_dependencies(&filtered_hooks)?;

        // Display dependency information
        println!("Hook Dependencies and Execution Order:");
        println!("=====================================");

        for (index, hook) in ordered_hooks.iter().enumerate() {
            println!(
                "\n{}. {} ({})",
                index + 1,
                hook.id,
                hook.name.as_deref().unwrap_or("")
            );

            if hook.depends_on.is_empty() {
                println!("   Dependencies: None");
            } else {
                println!("   Dependencies: {}", hook.depends_on.join(", "));
            }

            println!("   Language: {}", hook.language);
            println!("   Entry: {}", hook.entry);
        }

        // Show execution summary
        println!("\nExecution Summary:");
        println!("==================");
        let has_dependencies = filtered_hooks
            .iter()
            .any(|hook| !hook.depends_on.is_empty());

        if has_dependencies {
            println!("• Hooks will be executed sequentially due to dependencies");
            println!("• Total execution order: {} hooks", ordered_hooks.len());
        } else {
            println!("• No dependencies found - hooks can run in parallel");
            println!("• Total hooks: {}", ordered_hooks.len());
        }

        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_parsing_version() {
        // Test that --version flag is properly handled by clap
        // This will be validated by integration tests
        let cli = Cli::try_parse_from(["snp", "--version"]);
        // clap handles --version internally, so this will error with exit code 0
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_parsing_help() {
        // Test that --help flag is properly handled
        let cli = Cli::try_parse_from(["snp", "--help"]);
        // clap handles --help internally, so this will error with exit code 0
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_default_config() {
        let cli = Cli::try_parse_from(["snp"]).unwrap();
        assert_eq!(cli.config, ".pre-commit-config.yaml");
        assert!(!cli.verbose);
        assert!(!cli.quiet);
        assert!(cli.color.is_none());
    }

    #[test]
    fn test_cli_run_command() {
        let cli = Cli::try_parse_from(["snp", "run", "--all-files"]).unwrap();
        match cli.command {
            Some(Commands::Run { all_files, .. }) => assert!(all_files),
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_install_command() {
        let cli = Cli::try_parse_from(["snp", "install", "--overwrite"]).unwrap();
        match cli.command {
            Some(Commands::Install { overwrite, .. }) => assert!(overwrite),
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_cli_color_options() {
        let cli_always = Cli::try_parse_from(["snp", "--color", "always"]).unwrap();
        assert_eq!(cli_always.color, Some("always".to_string()));

        let cli_never = Cli::try_parse_from(["snp", "--color", "never"]).unwrap();
        assert_eq!(cli_never.color, Some("never".to_string()));

        let cli_auto = Cli::try_parse_from(["snp", "--color", "auto"]).unwrap();
        assert_eq!(cli_auto.color, Some("auto".to_string()));
    }
}
