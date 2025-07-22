// CLI interface for SNP using clap
use clap::{Parser, Subcommand};

use crate::error::Result;

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
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run hooks (default command)
    Run {
        /// Hook ID to run
        hook: Option<String>,

        /// Run on all files in the repository
        #[arg(short, long)]
        all_files: bool,

        /// Specific filenames to run hooks on
        #[arg(long)]
        files: Vec<String>,

        /// Show git diff when hooks fail
        #[arg(long)]
        show_diff_on_failure: bool,

        /// Hook stage to run
        #[arg(long, default_value = "pre-commit")]
        hook_stage: String,
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
}

impl Cli {
    pub fn run(&self) -> Result<i32> {
        // Initialize logging based on verbosity
        self.init_logging();

        match &self.command {
            Some(Commands::Run { .. }) => {
                println!("Running hooks...");
                // TODO: Implement run command
                Ok(0)
            }
            Some(Commands::Install { .. }) => {
                println!("Installing pre-commit hooks...");
                // TODO: Implement install command
                Ok(0)
            }
            Some(Commands::Clean) => {
                println!("Cleaning pre-commit files...");
                // TODO: Implement clean command
                Ok(0)
            }
            // TODO: Implement all other commands
            _ => {
                // Default to run command when no subcommand provided
                println!("Running hooks (default)...");
                Ok(0)
            }
        }
    }

    fn init_logging(&self) {
        use tracing_subscriber::{fmt, EnvFilter};

        let level = if self.quiet {
            "error"
        } else if self.verbose {
            "debug"
        } else {
            "info"
        };

        let filter = EnvFilter::new(format!("snp={level}"));

        fmt().with_env_filter(filter).with_target(false).init();
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
        let cli = Cli::try_parse_from(&["snp", "--version"]);
        // clap handles --version internally, so this will error with exit code 0
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_parsing_help() {
        // Test that --help flag is properly handled
        let cli = Cli::try_parse_from(&["snp", "--help"]);
        // clap handles --help internally, so this will error with exit code 0
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_default_config() {
        let cli = Cli::try_parse_from(&["snp"]).unwrap();
        assert_eq!(cli.config, ".pre-commit-config.yaml");
        assert!(!cli.verbose);
        assert!(!cli.quiet);
    }

    #[test]
    fn test_cli_run_command() {
        let cli = Cli::try_parse_from(&["snp", "run", "--all-files"]).unwrap();
        match cli.command {
            Some(Commands::Run { all_files, .. }) => assert!(all_files),
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_install_command() {
        let cli = Cli::try_parse_from(&["snp", "install", "--overwrite"]).unwrap();
        match cli.command {
            Some(Commands::Install { overwrite, .. }) => assert!(overwrite),
            _ => panic!("Expected Install command"),
        }
    }
}
