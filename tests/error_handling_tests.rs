/// TDD Red Phase: Error handling tests as specified in GitHub issue #6
/// These tests verify the comprehensive error handling framework
use snp::error::{CliError, ConfigError, GitError, HookExecutionError, SnpError};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_configuration_errors() {
    // Test YAML parsing errors
    let yaml = "invalid: yaml: [\ninvalid";
    let result = snp::config::Config::from_yaml(yaml);
    assert!(result.is_err());
    if let Err(SnpError::Config(ref config_err)) = result {
        if let ConfigError::InvalidYaml { message, .. } = config_err.as_ref() {
            // Just check that we got a YAML parsing error message
            assert!(!message.is_empty());
        } else {
            panic!(
                "Expected ConfigError::InvalidYaml, got: {:?}",
                result.unwrap_err()
            );
        }
    } else {
        panic!(
            "Expected ConfigError::InvalidYaml, got: {:?}",
            result.unwrap_err()
        );
    }

    // Test validation errors
    let yaml = r#"
repos:
  - repo: ""
    hooks:
      - id: test-hook
"#;
    let result = snp::config::Config::from_yaml(yaml);
    assert!(result.is_err());
    if let Err(SnpError::Config(ref config_err)) = result {
        if let ConfigError::MissingField { field, .. } = config_err.as_ref() {
            assert!(field.contains("repo"));
        } else {
            panic!("Expected ConfigError::MissingField for empty repo");
        }
    } else {
        panic!("Expected ConfigError::MissingField for empty repo");
    }

    // Test missing file errors
    let non_existent_path = PathBuf::from("/non/existent/config.yaml");
    let result = snp::config::Config::from_file(&non_existent_path);
    assert!(result.is_err());
    if let Err(SnpError::Config(ref config_err)) = result {
        if let ConfigError::NotFound { path, suggestion } = config_err.as_ref() {
            assert_eq!(*path, non_existent_path);
            assert!(suggestion.is_some());
        } else {
            panic!("Expected ConfigError::NotFound");
        }
    } else {
        panic!("Expected ConfigError::NotFound");
    }
}

#[test]
fn test_git_operation_errors() {
    // Test repository not found
    let git_error = GitError::RepositoryNotFound {
        path: PathBuf::from("/non/existent/repo"),
        suggestion: Some("Initialize a git repository first".to_string()),
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), 3); // GIT_ERROR

    // Test git command failures
    let git_error = GitError::CommandFailed {
        command: "git status".to_string(),
        exit_code: Some(128),
        stdout: "".to_string(),
        stderr: "fatal: not a git repository".to_string(),
        working_dir: Some(PathBuf::from("/tmp")),
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), 3);

    // Test permission errors
    let git_error = GitError::PermissionDenied {
        operation: "read repository".to_string(),
        path: PathBuf::from("/restricted/repo"),
        suggestion: Some("Check file permissions".to_string()),
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), 5); // PERMISSION_ERROR

    // Test timeout errors
    let git_error = GitError::Timeout {
        operation: "git clone".to_string(),
        duration_secs: 300,
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), 6); // TIMEOUT_ERROR

    // Test remote repository errors
    let git_error = GitError::RemoteError {
        message: "Repository not found".to_string(),
        repository: "https://github.com/nonexistent/repo".to_string(),
        operation: "clone".to_string(),
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), 3);
}

#[test]
fn test_hook_execution_errors() {
    // Test command not found
    let hook_error = HookExecutionError::CommandNotFound {
        command: "nonexistent-command".to_string(),
        hook_id: "test-hook".to_string(),
        language: "system".to_string(),
        suggestion: Some("Install the required command".to_string()),
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), 4); // HOOK_FAILURE

    // Test execution timeout
    let hook_error = HookExecutionError::ExecutionTimeout {
        hook_id: "slow-hook".to_string(),
        command: "sleep 1000".to_string(),
        timeout_secs: 60,
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), 6); // TIMEOUT_ERROR

    // Test permission denied
    let hook_error = HookExecutionError::PermissionDenied {
        hook_id: "restricted-hook".to_string(),
        command: "/usr/bin/restricted".to_string(),
        path: PathBuf::from("/usr/bin/restricted"),
        suggestion: Some("Check file permissions".to_string()),
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), 5); // PERMISSION_ERROR

    // Test non-zero exit code
    let hook_error = HookExecutionError::NonZeroExit {
        hook_id: "failing-hook".to_string(),
        command: "exit 1".to_string(),
        exit_code: 1,
        stdout: "".to_string(),
        stderr: "Hook failed".to_string(),
        files_processed: vec![PathBuf::from("test.py")],
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), 4); // HOOK_FAILURE

    // Test environment setup failure
    let hook_error = HookExecutionError::EnvironmentSetupFailed {
        language: "python".to_string(),
        hook_id: "python-hook".to_string(),
        message: "Failed to create virtual environment".to_string(),
        suggestion: Some("Install python-venv package".to_string()),
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), 4);

    // Test dependency installation failure
    let hook_error = HookExecutionError::DependencyInstallFailed {
        dependency: "black==22.0.0".to_string(),
        hook_id: "black-formatter".to_string(),
        language: "python".to_string(),
        error: "Package not found".to_string(),
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), 4);
}

#[test]
fn test_cli_errors() {
    // Test invalid argument
    let cli_error = CliError::InvalidArgument {
        argument: "--invalid-flag".to_string(),
        message: "Unknown flag".to_string(),
        suggestion: Some("Use --help to see available options".to_string()),
    };
    let snp_error = SnpError::Cli(Box::new(cli_error));
    assert_eq!(snp_error.exit_code(), 7); // CLI_ERROR

    // Test conflicting arguments
    let cli_error = CliError::ConflictingArguments {
        first: "--verbose".to_string(),
        second: "--quiet".to_string(),
        suggestion: "Use either --verbose or --quiet, not both".to_string(),
    };
    let snp_error = SnpError::Cli(Box::new(cli_error));
    assert_eq!(snp_error.exit_code(), 7);

    // Test missing required argument
    let cli_error = CliError::MissingArgument {
        argument: "hook-id".to_string(),
        context: "run command".to_string(),
    };
    let snp_error = SnpError::Cli(Box::new(cli_error));
    assert_eq!(snp_error.exit_code(), 7);

    // Test invalid subcommand
    let cli_error = CliError::InvalidSubcommand {
        subcommand: "invalid-cmd".to_string(),
        available: vec![
            "run".to_string(),
            "install".to_string(),
            "clean".to_string(),
        ],
    };
    let snp_error = SnpError::Cli(Box::new(cli_error));
    assert_eq!(snp_error.exit_code(), 7);
}

#[test]
fn test_error_message_formatting() {
    // Verify user-friendly error messages
    let config_error = ConfigError::InvalidYaml {
        message: "Invalid YAML syntax".to_string(),
        line: Some(15),
        column: Some(3),
        file_path: Some(PathBuf::from(".pre-commit-config.yaml")),
    };
    let snp_error = SnpError::Config(Box::new(config_error));
    let formatted = snp_error.user_message(false);
    assert!(formatted.contains("Configuration error"));
    assert!(formatted.contains("Invalid YAML syntax"));

    // Test error chain display
    let git_error = GitError::CommandFailed {
        command: "git status".to_string(),
        exit_code: Some(128),
        stdout: "".to_string(),
        stderr: "fatal: not a git repository".to_string(),
        working_dir: Some(PathBuf::from("/tmp")),
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    let formatted = snp_error.user_message(false);
    assert!(formatted.contains("Git operation failed"));
    assert!(formatted.contains("git status"));

    // Test colored output
    let cli_error = CliError::ConflictingArguments {
        first: "--verbose".to_string(),
        second: "--quiet".to_string(),
        suggestion: "Use either --verbose or --quiet".to_string(),
    };
    let snp_error = SnpError::Cli(Box::new(cli_error));

    // Test without colors
    let uncolored = snp_error.user_message(false);
    assert!(!uncolored.contains("\x1b["));

    // Test with colors
    let colored = snp_error.user_message(true);
    assert!(colored.contains("\x1b[31m")); // Red color for "Error:"
}

#[test]
fn test_config_error_variants() {
    // Test InvalidValue error
    let error = ConfigError::InvalidValue {
        message: "Invalid stage value".to_string(),
        field: "stages".to_string(),
        value: "invalid-stage".to_string(),
        expected: "pre-commit, pre-push, etc.".to_string(),
        file_path: Some(PathBuf::from("config.yaml")),
        line: Some(10),
    };
    assert!(error.to_string().contains("Invalid configuration value"));

    // Test ValidationFailed error
    let error = ConfigError::ValidationFailed {
        message: "Multiple validation errors".to_string(),
        file_path: Some(PathBuf::from("config.yaml")),
        errors: vec![
            "Missing required field 'id'".to_string(),
            "Invalid regex pattern".to_string(),
        ],
    };
    assert!(error
        .to_string()
        .contains("Configuration validation failed"));

    // Test InvalidRegex error
    let error = ConfigError::InvalidRegex {
        pattern: "[unclosed".to_string(),
        field: "files".to_string(),
        error: "unclosed character class".to_string(),
        file_path: Some(PathBuf::from("config.yaml")),
        line: Some(5),
    };
    assert!(error.to_string().contains("Invalid regex pattern"));
}

#[test]
fn test_error_conversion_from_std_types() {
    // Test IO error conversion
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let snp_error = SnpError::from(io_error);
    assert!(snp_error.to_string().contains("IO operation failed"));
    assert_eq!(snp_error.exit_code(), 1); // GENERAL_ERROR

    // Test serde_yaml error conversion
    let yaml_error = serde_yaml::from_str::<serde_yaml::Value>("invalid: yaml: [").unwrap_err();
    let config_error = *Box::<ConfigError>::from(yaml_error);
    if let ConfigError::InvalidYaml { message, line, .. } = config_error {
        assert!(!message.is_empty()); // Just check message exists
        assert!(line.is_some());
    } else {
        panic!("Expected InvalidYaml error");
    }
}

#[test]
fn test_error_context_information() {
    // Test that errors include helpful context
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("invalid-config.yaml");
    std::fs::write(&config_path, "invalid: yaml: [\nunclosed").unwrap();

    let result = snp::config::Config::from_file(&config_path);
    assert!(result.is_err());

    // The error should contain context about the file and line
    let error = result.unwrap_err();
    let formatted = error.user_message(false);
    assert!(formatted.contains("Configuration error"));

    // Test suggestion inclusion
    let git_error = GitError::RepositoryNotFound {
        path: PathBuf::from("/tmp/nonexistent"),
        suggestion: Some("Run 'git init' to create a repository".to_string()),
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    let formatted = snp_error.user_message(false);
    assert!(formatted.contains("git init") || formatted.contains("Initialize"));
}

#[test]
fn test_error_recovery_mechanisms() {
    // Test that error messages include actionable suggestions

    // Config file not found should suggest creating one
    let error = ConfigError::NotFound {
        path: PathBuf::from(".pre-commit-config.yaml"),
        suggestion: Some("Create a .pre-commit-config.yaml file".to_string()),
    };
    let snp_error = SnpError::Config(Box::new(error));
    let formatted = snp_error.user_message(false);
    assert!(formatted.contains("Create a .pre-commit-config.yaml"));

    // Permission denied should suggest checking permissions
    let error = GitError::PermissionDenied {
        operation: "clone repository".to_string(),
        path: PathBuf::from("/restricted"),
        suggestion: Some("Check directory permissions".to_string()),
    };
    let snp_error = SnpError::Git(Box::new(error));
    let formatted = snp_error.user_message(false);
    assert!(formatted.contains("Check directory permissions"));

    // Command not found should suggest installation
    let error = HookExecutionError::CommandNotFound {
        command: "black".to_string(),
        hook_id: "black-formatter".to_string(),
        language: "python".to_string(),
        suggestion: Some("Install black: pip install black".to_string()),
    };
    let snp_error = SnpError::HookExecution(Box::new(error));
    let formatted = snp_error.user_message(false);
    assert!(formatted.contains("pip install black"));
}

#[test]
fn test_exit_codes_match_precommit() {
    // Verify that exit codes match pre-commit behavior

    // Success
    use snp::error::exit_codes::*;
    assert_eq!(SUCCESS, 0);

    // General error
    let io_error = std::io::Error::other("general error");
    let snp_error = SnpError::from(io_error);
    assert_eq!(snp_error.exit_code(), GENERAL_ERROR);

    // Config error
    let config_error = ConfigError::NotFound {
        path: PathBuf::from("config.yaml"),
        suggestion: None,
    };
    let snp_error = SnpError::Config(Box::new(config_error));
    assert_eq!(snp_error.exit_code(), CONFIG_ERROR);

    // Git error
    let git_error = GitError::CommandFailed {
        command: "git".to_string(),
        exit_code: Some(1),
        stdout: "".to_string(),
        stderr: "error".to_string(),
        working_dir: None,
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), GIT_ERROR);

    // Hook failure
    let hook_error = HookExecutionError::NonZeroExit {
        hook_id: "test".to_string(),
        command: "test".to_string(),
        exit_code: 1,
        stdout: "".to_string(),
        stderr: "".to_string(),
        files_processed: vec![],
    };
    let snp_error = SnpError::HookExecution(Box::new(hook_error));
    assert_eq!(snp_error.exit_code(), HOOK_FAILURE);

    // Permission error
    let git_error = GitError::PermissionDenied {
        operation: "read".to_string(),
        path: PathBuf::from("/restricted"),
        suggestion: None,
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), PERMISSION_ERROR);

    // Timeout error
    let git_error = GitError::Timeout {
        operation: "clone".to_string(),
        duration_secs: 60,
    };
    let snp_error = SnpError::Git(Box::new(git_error));
    assert_eq!(snp_error.exit_code(), TIMEOUT_ERROR);

    // CLI error
    let cli_error = CliError::InvalidArgument {
        argument: "--invalid".to_string(),
        message: "Invalid".to_string(),
        suggestion: None,
    };
    let snp_error = SnpError::Cli(Box::new(cli_error));
    assert_eq!(snp_error.exit_code(), CLI_ERROR);
}
