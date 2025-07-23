// CLI interface for SNP using clap
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

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

        /// Enable verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Fail fast - stop running hooks after first failure
        #[arg(long)]
        fail_fast: bool,
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
    Clean,

    /// Clean unused cached repos
    Gc,

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
        match &self.command {
            Some(Commands::Run {
                hook,
                all_files,
                files,
                from_ref,
                to_ref,
                verbose,
                fail_fast,
                show_diff_on_failure: _,
                hook_stage,
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
                let stage = match hook_stage.as_str() {
                    "pre-commit" | "commit" => Stage::PreCommit,
                    "pre-push" | "push" => Stage::PrePush,
                    _ => {
                        return Err(crate::error::SnpError::Cli(Box::new(
                            crate::error::CliError::InvalidArgument {
                                argument: "--hook-stage".to_string(),
                                value: hook_stage.clone(),
                                suggestion: "Valid stages are: pre-commit, pre-push".to_string(),
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
                let mut execution_config = ExecutionConfig::new(stage)
                    .with_verbose(*verbose || self.verbose)
                    .with_fail_fast(*fail_fast)
                    .with_hook_timeout(Duration::from_secs(60));

                // Execute based on command type
                let execution_result = if let Some(hook_id) = hook {
                    println!("Running hook: {hook_id}");
                    run::execute_run_command_single_hook(
                        &repo_path,
                        &self.config,
                        hook_id,
                        &execution_config,
                    )
                    .await?
                } else if *all_files {
                    println!("Running hooks on all files");
                    execution_config = execution_config.with_all_files(true);
                    run::execute_run_command_all_files(&repo_path, &self.config, &execution_config)
                        .await?
                } else if !files.is_empty() {
                    println!("Running hooks on {} specific files", files.len());
                    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
                    execution_config = execution_config.with_files(file_paths);
                    run::execute_run_command_with_files(&repo_path, &self.config, &execution_config)
                        .await?
                } else if let (Some(from), Some(to)) = (from_ref, to_ref) {
                    if self.verbose || *verbose {
                        println!("Running hooks on files changed between {from} and {to}");
                    }
                    run::execute_run_command_with_refs(
                        &repo_path,
                        &self.config,
                        from,
                        to,
                        &execution_config,
                    )
                    .await?
                } else {
                    println!("Running hooks on staged files");
                    run::execute_run_command(&repo_path, &self.config, &execution_config).await?
                };

                // Report results
                if execution_result.success {
                    if self.verbose || *verbose {
                        println!(
                            "All hooks passed! Executed {} hooks in {:.2}s",
                            execution_result.hooks_executed,
                            execution_result.total_duration.as_secs_f64()
                        );
                    }
                    Ok(0)
                } else {
                    if !self.quiet {
                        eprintln!("Hook execution failed!");
                        eprintln!(
                            "Executed: {}, Passed: {}, Failed: {}",
                            execution_result.hooks_executed,
                            execution_result.hooks_passed.len(),
                            execution_result.hooks_failed.len()
                        );

                        if *verbose || self.verbose {
                            for failed_hook in &execution_result.hooks_failed {
                                eprintln!(
                                    "Failed hook: {} (exit code: {:?})",
                                    failed_hook.hook_id, failed_hook.exit_code
                                );
                                if !failed_hook.stderr.is_empty() {
                                    eprintln!("  stderr: {}", failed_hook.stderr);
                                }
                            }
                        }
                    }
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
                }

                // Install hook environments if requested
                if *install_hooks {
                    println!("Installing hook environments...");
                    // TODO: Implement hook environment installation
                }

                Ok(0)
            }
            Some(Commands::Clean) => {
                println!("Cleaning pre-commit files...");
                // TODO: Implement clean command
                Ok(0)
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
            // TODO: Implement all other commands
            _ => {
                // Default to run command when no subcommand provided
                println!("Running hooks (default)...");
                let repo_path = std::env::current_dir().map_err(|e| {
                    crate::error::SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
                        message: format!("Failed to get current directory: {e}"),
                    }))
                })?;

                let execution_config =
                    ExecutionConfig::new(Stage::PreCommit).with_verbose(self.verbose);

                let execution_result =
                    run::execute_run_command(&repo_path, &self.config, &execution_config).await?;

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
