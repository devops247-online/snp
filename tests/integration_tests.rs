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
    cmd.args(&["--help"]);

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
            "Help text should mention '{}' command",
            command
        );
    }
}
