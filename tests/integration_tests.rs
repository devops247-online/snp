use assert_cmd::Command;
use predicates::prelude::*;

/// TDD Red Phase: These tests should fail initially
/// Following issue #2 requirements for basic CLI functionality

#[test]
fn test_binary_builds_successfully() {
    // This test verifies that `cargo build` succeeds
    // The binary should compile without errors
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.assert().success();
}

#[test]
fn test_version_flag_works() {
    // Verify `snp --version` returns correct version information
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("snp"))
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn test_help_flag_works() {
    // Verify `snp --help` shows usage information
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("snp"))
        .stdout(predicate::str::contains(
            "framework for managing and maintaining multi-language pre-commit hooks",
        ));
}

#[test]
fn test_help_flag_short_works() {
    // Verify `snp -h` also shows help
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.arg("-h");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn test_no_args_defaults_to_run() {
    // When no arguments provided, should default to 'run' command
    // This matches pre-commit behavior
    let mut cmd = Command::cargo_bin("snp").unwrap();

    // Should succeed and show default run message
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Running hooks (default)"));
}

#[test]
fn test_invalid_command_shows_error() {
    // Invalid commands should show helpful error messages
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.arg("invalid-command");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("error"))
        .stderr(predicate::str::contains("invalid-command"));
}

#[test]
fn test_cli_structure_completeness() {
    // Verify that all main commands are available
    // This test will guide our CLI implementation
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["--help"]);

    let output = cmd.assert().success().get_output().stdout.clone();
    let help_text = String::from_utf8(output).unwrap();

    // These commands should be mentioned in help text
    let expected_commands = [
        "run",
        "install",
        "uninstall",
        "autoupdate",
        "clean",
        "gc",
        "validate-config",
    ];

    // Initially this will fail until we implement all commands
    for command in expected_commands {
        assert!(
            help_text.contains(command),
            "Help text should mention '{command}' command"
        );
    }
}

#[test]
fn test_shell_completion_command() {
    // Test that shell completion command exists and works
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["generate-completion", "bash"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_conflicting_verbose_quiet_flags() {
    // Test that conflicting --verbose and --quiet flags show error
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["--verbose", "--quiet", "run"]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Conflicting arguments: --verbose and --quiet",
    ));
}

#[test]
fn test_conflicting_run_options() {
    // Test that conflicting --all-files and --files show error
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["run", "--all-files", "--files", "test.py"]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Conflicting arguments: --all-files and --files",
    ));
}

#[test]
fn test_run_with_specific_hook() {
    // Test running a specific hook
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["run", "--hook", "black"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Running hook: black"));
}

#[test]
fn test_run_all_files() {
    // Test running on all files
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["run", "--all-files"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Running hooks on all files"));
}

#[test]
fn test_run_specific_files() {
    // Test running on specific files
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["run", "--files", "file1.py", "file2.rs"]);

    cmd.assert().success().stdout(predicate::str::contains(
        "Running hooks on 2 specific files",
    ));
}

#[test]
fn test_config_file_option() {
    // Test custom config file option
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["--config", "custom-config.yaml", "run"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Running hooks"));
}

#[test]
fn test_verbose_flag() {
    // Test verbose flag works
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["--verbose", "run"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Running hooks"));
}

#[test]
fn test_install_with_hook_types() {
    // Test install command with hook types
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args([
        "install",
        "--hook-type",
        "pre-commit",
        "--hook-type",
        "pre-push",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Installing pre-commit hooks"));
}

#[test]
fn test_autoupdate_with_options() {
    // Test autoupdate command with various options
    let mut cmd = Command::cargo_bin("snp").unwrap();
    cmd.args(["autoupdate", "--bleeding-edge", "--jobs", "4"]);

    cmd.assert().success();
}
