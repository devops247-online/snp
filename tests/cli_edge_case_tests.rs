// Comprehensive tests for CLI edge cases and error handling
// Tests argument validation, error scenarios, and complex CLI workflows

use clap::Parser;
use snp::cli::{Cli, Commands};
use tempfile::TempDir;

#[cfg(test)]
#[allow(clippy::uninlined_format_args)]
mod cli_edge_case_tests {
    use super::*;

    #[test]
    fn test_conflicting_global_arguments() {
        // Test verbose and quiet flags together
        let cli = Cli::try_parse_from(["snp", "--verbose", "--quiet"]);
        assert!(cli.is_ok(), "Clap should parse conflicting flags");

        let cli = cli.unwrap();
        assert!(cli.verbose);
        assert!(cli.quiet);

        // The actual conflict validation happens in cli.run()
    }

    #[test]
    fn test_invalid_color_values() {
        // Valid color values should parse correctly
        let valid_colors = ["auto", "always", "never"];
        for color in &valid_colors {
            let cli = Cli::try_parse_from(["snp", "--color", color]).unwrap();
            assert_eq!(cli.color, Some(color.to_string()));
        }

        // Invalid color values should still parse (validation happens later)
        let cli = Cli::try_parse_from(["snp", "--color", "invalid"]).unwrap();
        assert_eq!(cli.color, Some("invalid".to_string()));
    }

    #[test]
    fn test_config_file_path_variations() {
        // Test different config file paths
        let test_paths = [
            "custom-config.yaml",
            "../parent-config.yaml",
            "/absolute/path/config.yaml",
            "config with spaces.yaml",
            "config-with-unicode-café.yaml",
        ];

        for path in &test_paths {
            let cli = Cli::try_parse_from(["snp", "--config", path]).unwrap();
            assert_eq!(cli.config, *path);
        }
    }

    #[test]
    fn test_run_command_conflicting_arguments() {
        // Test all-files with files argument
        let cli = Cli::try_parse_from([
            "snp",
            "run",
            "--all-files",
            "--files",
            "file1.py",
            "file2.py",
        ])
        .unwrap();

        match cli.command {
            Some(Commands::Run {
                all_files, files, ..
            }) => {
                assert!(all_files);
                assert_eq!(files, vec!["file1.py", "file2.py"]);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_run_command_ref_arguments() {
        // Test from-ref without to-ref
        let cli = Cli::try_parse_from(["snp", "run", "--from-ref", "main"]).unwrap();

        match cli.command {
            Some(Commands::Run {
                from_ref, to_ref, ..
            }) => {
                assert_eq!(from_ref, Some("main".to_string()));
                assert_eq!(to_ref, None);
            }
            _ => panic!("Expected Run command"),
        }

        // Test to-ref without from-ref
        let cli = Cli::try_parse_from(["snp", "run", "--to-ref", "develop"]).unwrap();

        match cli.command {
            Some(Commands::Run {
                from_ref, to_ref, ..
            }) => {
                assert_eq!(from_ref, None);
                assert_eq!(to_ref, Some("develop".to_string()));
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_run_command_jobs_parameter() {
        // Test valid jobs parameter
        let cli = Cli::try_parse_from(["snp", "run", "--jobs", "4"]).unwrap();
        match cli.command {
            Some(Commands::Run { jobs, .. }) => {
                assert_eq!(jobs, Some(4));
            }
            _ => panic!("Expected Run command"),
        }

        // Test zero jobs (should parse but might be invalid)
        let cli = Cli::try_parse_from(["snp", "run", "--jobs", "0"]).unwrap();
        match cli.command {
            Some(Commands::Run { jobs, .. }) => {
                assert_eq!(jobs, Some(0));
            }
            _ => panic!("Expected Run command"),
        }

        // Test very large jobs number
        let cli = Cli::try_parse_from(["snp", "run", "--jobs", "1000"]).unwrap();
        match cli.command {
            Some(Commands::Run { jobs, .. }) => {
                assert_eq!(jobs, Some(1000));
            }
            _ => panic!("Expected Run command"),
        }

        // Test invalid jobs parameter (non-numeric)
        let result = Cli::try_parse_from(["snp", "run", "--jobs", "invalid"]);
        assert!(result.is_err(), "Should fail to parse non-numeric jobs");
    }

    #[test]
    fn test_run_command_hook_stage_variations() {
        let stages = [
            "pre-commit",
            "pre-push",
            "commit-msg",
            "prepare-commit-msg",
            "post-commit",
            "post-checkout",
            "post-merge",
            "pre-rebase",
            "post-rewrite",
            "manual",
            "merge-commit",
        ];

        for stage in &stages {
            let cli = Cli::try_parse_from(["snp", "run", "--hook-stage", stage]).unwrap();
            match cli.command {
                Some(Commands::Run { hook_stage, .. }) => {
                    assert_eq!(hook_stage, *stage);
                }
                _ => panic!("Expected Run command"),
            }
        }

        // Test invalid hook stage
        let cli = Cli::try_parse_from(["snp", "run", "--hook-stage", "invalid-stage"]).unwrap();
        match cli.command {
            Some(Commands::Run { hook_stage, .. }) => {
                assert_eq!(hook_stage, "invalid-stage");
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_install_command_edge_cases() {
        // Test empty hook types (should use default)
        let cli = Cli::try_parse_from(["snp", "install"]).unwrap();
        match cli.command {
            Some(Commands::Install { hook_type, .. }) => {
                assert!(hook_type.is_empty());
            }
            _ => panic!("Expected Install command"),
        }

        // Test multiple hook types
        let cli = Cli::try_parse_from([
            "snp",
            "install",
            "-t",
            "pre-commit",
            "-t",
            "pre-push",
            "-t",
            "commit-msg",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Install { hook_type, .. }) => {
                assert_eq!(hook_type, vec!["pre-commit", "pre-push", "commit-msg"]);
            }
            _ => panic!("Expected Install command"),
        }

        // Test invalid hook types (should still parse)
        let cli = Cli::try_parse_from([
            "snp",
            "install",
            "-t",
            "invalid-hook",
            "-t",
            "another-invalid",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Install { hook_type, .. }) => {
                assert_eq!(hook_type, vec!["invalid-hook", "another-invalid"]);
            }
            _ => panic!("Expected Install command"),
        }
    }

    #[test]
    fn test_clean_command_flag_combinations() {
        // Test all clean flags together
        let cli = Cli::try_parse_from([
            "snp",
            "clean",
            "--repos",
            "--envs",
            "--temp",
            "--older-than",
            "30",
            "--dry-run",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Clean {
                repos,
                envs,
                temp,
                older_than,
                dry_run,
            }) => {
                assert!(repos);
                assert!(envs);
                assert!(temp);
                assert_eq!(older_than, Some(30));
                assert!(dry_run);
            }
            _ => panic!("Expected Clean command"),
        }

        // Test no flags (should clean everything)
        let cli = Cli::try_parse_from(["snp", "clean"]).unwrap();
        match cli.command {
            Some(Commands::Clean {
                repos,
                envs,
                temp,
                older_than,
                dry_run,
            }) => {
                assert!(!repos);
                assert!(!envs);
                assert!(!temp);
                assert_eq!(older_than, None);
                assert!(!dry_run);
            }
            _ => panic!("Expected Clean command"),
        }

        // Test invalid older-than value
        let result = Cli::try_parse_from(["snp", "clean", "--older-than", "invalid"]);
        assert!(
            result.is_err(),
            "Should fail to parse non-numeric older-than"
        );
    }

    #[test]
    fn test_autoupdate_command_parameters() {
        // Test all autoupdate flags
        let cli = Cli::try_parse_from([
            "snp",
            "autoupdate",
            "--bleeding-edge",
            "--freeze",
            "--repo",
            "repo1",
            "--repo",
            "repo2",
            "--jobs",
            "4",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Autoupdate {
                bleeding_edge,
                freeze,
                repo,
                jobs,
            }) => {
                assert!(bleeding_edge);
                assert!(freeze);
                assert_eq!(repo, vec!["repo1", "repo2"]);
                assert_eq!(jobs, 4);
            }
            _ => panic!("Expected Autoupdate command"),
        }

        // Test edge case jobs value (0 is actually valid for clap)
        let cli = Cli::try_parse_from(["snp", "autoupdate", "--jobs", "0"]).unwrap();
        match cli.command {
            Some(Commands::Autoupdate { jobs, .. }) => {
                assert_eq!(jobs, 0);
            }
            _ => panic!("Expected Autoupdate command"),
        }

        // Test very large jobs value
        let cli = Cli::try_parse_from(["snp", "autoupdate", "--jobs", "1000"]).unwrap();
        match cli.command {
            Some(Commands::Autoupdate { jobs, .. }) => {
                assert_eq!(jobs, 1000);
            }
            _ => panic!("Expected Autoupdate command"),
        }
    }

    #[test]
    fn test_try_repo_command_variations() {
        // Test basic try-repo
        let cli = Cli::try_parse_from(["snp", "try-repo", "https://github.com/user/repo"]).unwrap();
        match cli.command {
            Some(Commands::TryRepo {
                repo,
                rev,
                all_files,
                files,
            }) => {
                assert_eq!(repo, "https://github.com/user/repo");
                assert_eq!(rev, None);
                assert!(!all_files);
                assert!(files.is_empty());
            }
            _ => panic!("Expected TryRepo command"),
        }

        // Test with revision parameter
        let cli = Cli::try_parse_from([
            "snp",
            "try-repo",
            "local/path",
            "--rev",
            "v1.0.0",
            "--all-files",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::TryRepo {
                repo,
                rev,
                all_files,
                files,
            }) => {
                assert_eq!(repo, "local/path");
                assert_eq!(rev, Some("v1.0.0".to_string()));
                assert!(all_files);
                assert!(files.is_empty());
            }
            _ => panic!("Expected TryRepo command"),
        }
    }

    #[test]
    fn test_validate_config_command() {
        // Test without filenames (should use default)
        let cli = Cli::try_parse_from(["snp", "validate-config"]).unwrap();
        match cli.command {
            Some(Commands::ValidateConfig { filenames }) => {
                assert!(filenames.is_empty());
            }
            _ => panic!("Expected ValidateConfig command"),
        }

        // Test with multiple filenames
        let cli = Cli::try_parse_from([
            "snp",
            "validate-config",
            "config1.yaml",
            "config2.yaml",
            "config3.yaml",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::ValidateConfig { filenames }) => {
                assert_eq!(
                    filenames,
                    vec!["config1.yaml", "config2.yaml", "config3.yaml"]
                );
            }
            _ => panic!("Expected ValidateConfig command"),
        }
    }

    #[test]
    fn test_validate_manifest_command() {
        // Test with multiple manifest files
        let cli = Cli::try_parse_from([
            "snp",
            "validate-manifest",
            "manifest1.yaml",
            "manifest2.yaml",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::ValidateManifest { filenames }) => {
                assert_eq!(filenames, vec!["manifest1.yaml", "manifest2.yaml"]);
            }
            _ => panic!("Expected ValidateManifest command"),
        }
    }

    #[test]
    fn test_init_template_dir_command() {
        let cli = Cli::try_parse_from([
            "snp",
            "init-template-dir",
            "/path/to/template",
            "-t",
            "pre-commit",
            "-t",
            "pre-push",
            "--allow-missing-config",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::InitTemplateDir {
                directory,
                hook_type,
                allow_missing_config,
            }) => {
                assert_eq!(directory, "/path/to/template");
                assert_eq!(hook_type, vec!["pre-commit", "pre-push"]);
                assert!(allow_missing_config);
            }
            _ => panic!("Expected InitTemplateDir command"),
        }
    }

    #[test]
    fn test_generate_completion_command() {
        use clap_complete::Shell;

        let shells = [
            ("bash", Shell::Bash),
            ("zsh", Shell::Zsh),
            ("fish", Shell::Fish),
            ("powershell", Shell::PowerShell),
            ("elvish", Shell::Elvish),
        ];

        for (shell_name, expected_shell) in &shells {
            let cli = Cli::try_parse_from(["snp", "generate-completion", shell_name]).unwrap();
            match cli.command {
                Some(Commands::GenerateCompletion { shell }) => {
                    assert_eq!(shell, *expected_shell);
                }
                _ => panic!("Expected GenerateCompletion command"),
            }
        }
    }

    #[test]
    fn test_command_parsing_errors() {
        // Test unknown subcommand
        let result = Cli::try_parse_from(["snp", "unknown-command"]);
        assert!(result.is_err(), "Should fail with unknown subcommand");

        // Test missing required arguments
        let result = Cli::try_parse_from(["snp", "try-repo"]);
        assert!(result.is_err(), "Should fail without repo argument");

        let result = Cli::try_parse_from(["snp", "init-template-dir"]);
        assert!(result.is_err(), "Should fail without directory argument");

        let result = Cli::try_parse_from(["snp", "generate-completion"]);
        assert!(result.is_err(), "Should fail without shell argument");
    }

    #[test]
    fn test_argument_value_edge_cases() {
        // Test empty string arguments
        let cli = Cli::try_parse_from(["snp", "--config", ""]).unwrap();
        assert_eq!(cli.config, "");

        let cli = Cli::try_parse_from(["snp", "run", "--hook", ""]).unwrap();
        match cli.command {
            Some(Commands::Run { hook, .. }) => {
                assert_eq!(hook, Some("".to_string()));
            }
            _ => panic!("Expected Run command"),
        }

        // Test arguments with special characters
        let cli = Cli::try_parse_from([
            "snp",
            "run",
            "--hook",
            "hook-with-dashes_and_underscores.123",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Run { hook, .. }) => {
                assert_eq!(
                    hook,
                    Some("hook-with-dashes_and_underscores.123".to_string())
                );
            }
            _ => panic!("Expected Run command"),
        }

        // Test Unicode in arguments
        let cli = Cli::try_parse_from(["snp", "--config", "config-café-測試.yaml"]).unwrap();
        assert_eq!(cli.config, "config-café-測試.yaml");
    }

    #[test]
    fn test_boolean_flag_combinations() {
        // Test all possible combinations of boolean flags for run command
        let test_cases = [
            (vec!["snp", "run"], false, false, false, false),
            (vec!["snp", "run", "--all-files"], true, false, false, false),
            (vec!["snp", "run", "--verbose"], false, true, false, false),
            (vec!["snp", "run", "--fail-fast"], false, false, true, false),
            (
                vec!["snp", "run", "--show-diff-on-failure"],
                false,
                false,
                false,
                true,
            ),
            (
                vec!["snp", "run", "--all-files", "--verbose", "--fail-fast"],
                true,
                true,
                true,
                false,
            ),
        ];

        for (args, expected_all_files, expected_verbose, expected_fail_fast, expected_show_diff) in
            &test_cases
        {
            let cli = Cli::try_parse_from(args).unwrap();
            match cli.command {
                Some(Commands::Run {
                    all_files,
                    verbose,
                    fail_fast,
                    show_diff_on_failure,
                    ..
                }) => {
                    assert_eq!(all_files, *expected_all_files);
                    assert_eq!(verbose, *expected_verbose);
                    assert_eq!(fail_fast, *expected_fail_fast);
                    assert_eq!(show_diff_on_failure, *expected_show_diff);
                }
                _ => panic!("Expected Run command"),
            }
        }
    }

    #[test]
    fn test_files_argument_variations() {
        // Test single file
        let cli = Cli::try_parse_from(["snp", "run", "--files", "single.py"]).unwrap();
        match cli.command {
            Some(Commands::Run { files, .. }) => {
                assert_eq!(files, vec!["single.py"]);
            }
            _ => panic!("Expected Run command"),
        }

        // Test multiple files
        let cli =
            Cli::try_parse_from(["snp", "run", "--files", "file1.py", "file2.js", "file3.rs"])
                .unwrap();
        match cli.command {
            Some(Commands::Run { files, .. }) => {
                assert_eq!(files, vec!["file1.py", "file2.js", "file3.rs"]);
            }
            _ => panic!("Expected Run command"),
        }

        // Test files with spaces and special characters
        let cli = Cli::try_parse_from([
            "snp",
            "run",
            "--files",
            "file with spaces.py",
            "file-with-dashes.js",
            "file_with_underscores.rs",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Run { files, .. }) => {
                assert_eq!(
                    files,
                    vec![
                        "file with spaces.py",
                        "file-with-dashes.js",
                        "file_with_underscores.rs"
                    ]
                );
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_no_subcommand_handling() {
        // Test CLI with no subcommand (should be valid)
        let cli = Cli::try_parse_from(["snp"]).unwrap();
        assert!(cli.command.is_none());

        // Test CLI with only global flags
        let cli = Cli::try_parse_from(["snp", "--verbose", "--config", "custom.yaml"]).unwrap();
        assert!(cli.command.is_none());
        assert!(cli.verbose);
        assert_eq!(cli.config, "custom.yaml");
    }

    #[test]
    fn test_cli_runtime_error_scenarios() -> Result<(), Box<dyn std::error::Error>> {
        let _temp_dir = TempDir::new()?;

        // Test config validation with non-existent directory
        let cli = Cli {
            command: None,
            verbose: false,
            quiet: false,
            config: "/non/existent/directory/config.yaml".to_string(),
            color: None,
        };

        // The actual runtime validation would happen in cli.run()
        // We can't easily test the full CLI execution in unit tests
        // but we can test the structure is correct
        assert_eq!(cli.config, "/non/existent/directory/config.yaml");

        Ok(())
    }

    #[test]
    fn test_cli_argument_precedence() {
        // Test global flags vs command-specific flags
        let cli = Cli::try_parse_from(["snp", "--verbose", "run", "--verbose"]).unwrap();
        assert!(cli.verbose); // Global verbose
        match cli.command {
            Some(Commands::Run { verbose, .. }) => {
                assert!(verbose); // Command-specific verbose
            }
            _ => panic!("Expected Run command"),
        }

        // Test that command-specific flags don't affect global state
        let cli = Cli::try_parse_from(["snp", "run", "--verbose"]).unwrap();
        // Note: In clap, global flags can appear anywhere, so this might be true
        // We're testing the structure rather than enforcing specific behavior
        match cli.command {
            Some(Commands::Run { verbose, .. }) => {
                assert!(verbose); // Command-specific verbose should be true
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_complex_argument_combinations() {
        // Test a complex command with many arguments
        let cli = Cli::try_parse_from([
            "snp",
            "--verbose",
            "--config",
            "custom-config.yaml",
            "--color",
            "always",
            "run",
            "--hook",
            "specific-hook",
            "--jobs",
            "8",
            "--from-ref",
            "main",
            "--to-ref",
            "develop",
            "--verbose",
            "--fail-fast",
            "--show-diff-on-failure",
        ])
        .unwrap();

        // Verify global flags
        assert!(cli.verbose);
        assert_eq!(cli.config, "custom-config.yaml");
        assert_eq!(cli.color, Some("always".to_string()));

        // Verify command-specific flags
        match cli.command {
            Some(Commands::Run {
                hook,
                jobs,
                from_ref,
                to_ref,
                verbose,
                fail_fast,
                show_diff_on_failure,
                ..
            }) => {
                assert_eq!(hook, Some("specific-hook".to_string()));
                assert_eq!(jobs, Some(8));
                assert_eq!(from_ref, Some("main".to_string()));
                assert_eq!(to_ref, Some("develop".to_string()));
                assert!(verbose);
                assert!(fail_fast);
                assert!(show_diff_on_failure);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_numeric_argument_boundaries() {
        // Test boundary values for numeric arguments

        // Jobs parameter
        let cli = Cli::try_parse_from(["snp", "run", "--jobs", "1"]).unwrap();
        match cli.command {
            Some(Commands::Run { jobs, .. }) => assert_eq!(jobs, Some(1)),
            _ => panic!("Expected Run command"),
        }

        // Large jobs value
        let cli = Cli::try_parse_from(["snp", "run", "--jobs", "9999"]).unwrap();
        match cli.command {
            Some(Commands::Run { jobs, .. }) => assert_eq!(jobs, Some(9999)),
            _ => panic!("Expected Run command"),
        }

        // Autoupdate jobs
        let cli = Cli::try_parse_from(["snp", "autoupdate", "--jobs", "1"]).unwrap();
        match cli.command {
            Some(Commands::Autoupdate { jobs, .. }) => assert_eq!(jobs, 1),
            _ => panic!("Expected Autoupdate command"),
        }

        // Clean older-than
        let cli = Cli::try_parse_from(["snp", "clean", "--older-than", "1"]).unwrap();
        match cli.command {
            Some(Commands::Clean { older_than, .. }) => assert_eq!(older_than, Some(1)),
            _ => panic!("Expected Clean command"),
        }

        let cli = Cli::try_parse_from(["snp", "clean", "--older-than", "365"]).unwrap();
        match cli.command {
            Some(Commands::Clean { older_than, .. }) => assert_eq!(older_than, Some(365)),
            _ => panic!("Expected Clean command"),
        }
    }

    #[test]
    fn test_cli_help_and_version_detection() {
        // These commands will exit with code 0, but we can test they're detected
        let help_result = Cli::try_parse_from(["snp", "--help"]);
        assert!(help_result.is_err(), "Help should cause clap to exit");

        let version_result = Cli::try_parse_from(["snp", "--version"]);
        assert!(version_result.is_err(), "Version should cause clap to exit");

        let help_subcommand_result = Cli::try_parse_from(["snp", "run", "--help"]);
        assert!(
            help_subcommand_result.is_err(),
            "Subcommand help should cause clap to exit"
        );
    }

    #[test]
    fn test_path_argument_variations() {
        // Test different path formats
        let path_variations = [
            "simple.yaml",
            "./relative.yaml",
            "../parent.yaml",
            "/absolute/path.yaml",
            "~/home/path.yaml",
            "path with spaces.yaml",
            "path-with-dashes.yaml",
            "path_with_underscores.yaml",
            "path.with.dots.yaml",
        ];

        for path in &path_variations {
            let cli = Cli::try_parse_from(["snp", "--config", path]).unwrap();
            assert_eq!(cli.config, *path);

            let cli = Cli::try_parse_from(["snp", "validate-config", path]).unwrap();
            match cli.command {
                Some(Commands::ValidateConfig { filenames }) => {
                    assert_eq!(filenames, vec![*path]);
                }
                _ => panic!("Expected ValidateConfig command"),
            }
        }
    }

    #[test]
    fn test_command_aliases_and_variations() {
        // Test short flag variants where available
        let cli = Cli::try_parse_from(["snp", "-v"]).unwrap();
        assert!(cli.verbose);

        let cli = Cli::try_parse_from(["snp", "-q"]).unwrap();
        assert!(cli.quiet);

        let cli = Cli::try_parse_from(["snp", "-c", "config.yaml"]).unwrap();
        assert_eq!(cli.config, "config.yaml");

        // Test run command short flags
        let cli = Cli::try_parse_from(["snp", "run", "-a"]).unwrap();
        match cli.command {
            Some(Commands::Run { all_files, .. }) => assert!(all_files),
            _ => panic!("Expected Run command"),
        }

        let cli = Cli::try_parse_from(["snp", "run", "-j", "4"]).unwrap();
        match cli.command {
            Some(Commands::Run { jobs, .. }) => assert_eq!(jobs, Some(4)),
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_memory_efficiency_with_large_inputs() {
        // Test with many files
        let mut args = vec!["snp", "run", "--files"];
        let large_file_list: Vec<String> = (0..1000).map(|i| format!("file_{}.py", i)).collect();
        for file in &large_file_list {
            args.push(file);
        }

        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Commands::Run { files, .. }) => {
                assert_eq!(files.len(), 1000);
                assert_eq!(files[0], "file_0.py");
                assert_eq!(files[999], "file_999.py");
            }
            _ => panic!("Expected Run command"),
        }

        // Test with many repositories
        let repo_strings: Vec<String> = (0..100).map(|i| format!("repo_{}", i)).collect();
        let mut args = vec!["snp", "autoupdate"];

        // Add repo arguments
        for repo in &repo_strings {
            args.push("--repo");
            args.push(repo.as_str());
        }

        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Commands::Autoupdate { repo, .. }) => {
                assert_eq!(repo.len(), 100);
                assert_eq!(repo[0], "repo_0");
                assert_eq!(repo[99], "repo_99");
            }
            _ => panic!("Expected Autoupdate command"),
        }
    }
}
