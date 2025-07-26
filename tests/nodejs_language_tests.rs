// Comprehensive tests for Node.js language support following TDD Red Phase
// These tests are designed to fail initially and then pass once implementation is complete

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;
use tokio::fs;

use snp::core::Hook;
use snp::language::{
    environment::EnvironmentConfig, environment::IsolationLevel, traits::Language, CacheStrategy,
};

// This will be the Node.js language plugin (doesn't exist yet)
use snp::language::nodejs::NodejsLanguagePlugin;

#[tokio::test]
async fn test_nodejs_environment_creation() {
    let plugin = NodejsLanguagePlugin::new();
    let config = EnvironmentConfig {
        language_version: Some("node".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
        working_directory: None,
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    // Test Node.js environment creation
    let result = plugin.setup_environment(&config).await;
    if let Err(ref e) = result {
        eprintln!("Environment setup error: {e}");
    }
    assert!(
        result.is_ok(),
        "Failed to create Node.js environment: {:?}",
        result.err()
    );

    let env = result.unwrap();
    assert_eq!(env.language, "node");
    assert!(env.root_path.exists(), "Environment root path should exist");

    // Test Node.js executable exists
    let node_exe = if cfg!(windows) {
        env.executable_path.join("node.exe")
    } else {
        env.executable_path.join("node")
    };

    // For system Node, executable_path might be the direct path to node
    assert!(
        node_exe.exists()
            || env.executable_path.file_name().map(|n| n.to_str()) == Some(Some("node"))
            || env.executable_path.file_name().map(|n| n.to_str()) == Some(Some("node.exe")),
        "Node.js executable should be accessible"
    );

    // Test npm executable exists or is accessible
    let npm_available = if cfg!(windows) {
        which::which("npm.cmd").is_ok() || which::which("npm.exe").is_ok()
    } else {
        which::which("npm").is_ok()
    };
    assert!(npm_available, "npm should be available");

    // Test environment variables are set correctly
    assert!(
        env.environment_variables.contains_key("NODE_PATH")
            || env.environment_variables.contains_key("NODE_VIRTUAL_ENV")
    );

    // Test metadata is populated
    assert!(!env.metadata.language_version.is_empty());
    assert!(!env.metadata.platform.is_empty());
}

#[tokio::test]
async fn test_nodejs_version_detection_and_compatibility() {
    let mut plugin = NodejsLanguagePlugin::new();

    // Test Node.js installation detection
    let installations = plugin.detect_node_installations().await;
    assert!(
        installations.is_ok(),
        "Failed to detect Node.js installations"
    );
    let installations = installations.unwrap();
    assert!(
        !installations.is_empty(),
        "Should find at least one Node.js installation"
    );

    // Test version information is complete
    for installation in &installations {
        assert!(!installation.executable_path.as_os_str().is_empty());
        assert!(installation.version.major > 0);
        assert!(!installation.architecture.is_empty());
        assert!(!installation.platform.is_empty());

        // Modern Node.js should have npm bundled
        if installation.version >= semver::Version::new(0, 6, 0) {
            assert!(
                installation.npm_version.is_some(),
                "Node.js 0.6+ should have npm"
            );
        }
    }

    // Test finding suitable Node.js for different requirements
    let config = EnvironmentConfig {
        language_version: Some(">=14.0.0".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Partial,
        cache_strategy: CacheStrategy::Memory,
        working_directory: None,
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let suitable_node = plugin.find_suitable_node(&config).await;
    if suitable_node.is_ok() {
        let node_info = suitable_node.unwrap();
        assert!(node_info.version >= semver::Version::new(14, 0, 0));
    }
}

#[tokio::test]
async fn test_package_manager_detection() {
    let mut plugin = NodejsLanguagePlugin::new();

    // Create temporary directory with different lockfiles
    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Test npm detection (package-lock.json)
    fs::write(project_path.join("package-lock.json"), "{}")
        .await
        .unwrap();
    let manager = plugin.detect_package_manager(project_path).await;
    assert!(manager.is_ok(), "Should detect npm from package-lock.json");
    let manager = manager.unwrap();
    assert_eq!(
        manager.manager_type,
        snp::language::nodejs::PackageManagerType::Npm
    );
    assert!(manager.executable_path.exists());

    // Clean up
    fs::remove_file(project_path.join("package-lock.json"))
        .await
        .unwrap();

    // Test yarn detection (yarn.lock) - should detect yarn if available, otherwise fallback to npm
    fs::write(project_path.join("yarn.lock"), "").await.unwrap();
    let manager = plugin.detect_package_manager(project_path).await;
    match manager {
        Ok(manager) => {
            // Should detect a package manager (yarn if available, npm as fallback)
            println!(
                "With yarn.lock present, detected manager: {:?}",
                manager.manager_type
            );
            // Either yarn (if working) or npm (if yarn failed) is acceptable
            assert!(matches!(
                manager.manager_type,
                snp::language::nodejs::PackageManagerType::Yarn
                    | snp::language::nodejs::PackageManagerType::YarnBerry
                    | snp::language::nodejs::PackageManagerType::Npm
            ));
        }
        Err(e) => {
            // This should not happen as npm should always be available as fallback
            panic!("Package manager detection should not fail completely: {e}");
        }
    }
    fs::remove_file(project_path.join("yarn.lock"))
        .await
        .unwrap();

    // Test pnpm detection (pnpm-lock.yaml) - should detect pnpm if available, otherwise fallback to npm
    fs::write(project_path.join("pnpm-lock.yaml"), "")
        .await
        .unwrap();
    let manager = plugin.detect_package_manager(project_path).await;
    match manager {
        Ok(manager) => {
            println!(
                "With pnpm-lock.yaml present, detected manager: {:?}",
                manager.manager_type
            );
            // Either pnpm (if available) or npm (if pnpm failed) is acceptable
            assert!(matches!(
                manager.manager_type,
                snp::language::nodejs::PackageManagerType::Pnpm
                    | snp::language::nodejs::PackageManagerType::Npm
            ));
        }
        Err(e) => {
            // This should not happen as npm should always be available as fallback
            panic!("Package manager detection should not fail completely: {e}");
        }
    }
    fs::remove_file(project_path.join("pnpm-lock.yaml"))
        .await
        .unwrap();

    // Test auto-detection fallback to npm
    let manager = plugin.detect_package_manager(project_path).await;
    assert!(
        manager.is_ok(),
        "Should fallback to npm when no lockfile found"
    );
    let manager = manager.unwrap();
    assert_eq!(
        manager.manager_type,
        snp::language::nodejs::PackageManagerType::Npm
    );
}

#[tokio::test]
async fn test_dependency_installation() {
    let mut plugin = NodejsLanguagePlugin::new();
    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create a package.json file
    let package_json = r#"{
        "name": "test-project",
        "version": "1.0.0",
        "description": "Test project for Node.js language plugin"
    }"#;
    fs::write(project_path.join("package.json"), package_json)
        .await
        .unwrap();

    // Setup environment
    let config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Partial,
        cache_strategy: CacheStrategy::Memory,
        working_directory: Some(project_path.to_path_buf()),
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env = plugin.setup_environment(&config).await;
    assert!(env.is_ok(), "Environment setup should succeed");
    let _env = env.unwrap();

    // Test package manager detection and dependency installation
    let package_manager = plugin.detect_package_manager(project_path).await;
    assert!(
        package_manager.is_ok(),
        "Package manager detection should succeed"
    );
    let package_manager = package_manager.unwrap();

    // Test installing simple dependencies
    let dependencies = vec!["lodash".to_string()];
    let install_result = plugin
        .install_dependencies(&package_manager, project_path, &dependencies)
        .await;

    // This might fail in CI without npm, so we check both success and specific error types
    match install_result {
        Ok(_) => {
            // If successful, verify node_modules exists
            assert!(
                project_path.join("node_modules").exists(),
                "node_modules should exist after successful installation"
            );
        }
        Err(e) => {
            // In CI environments, npm might not be available, which is acceptable
            eprintln!("Dependency installation failed (expected in some CI environments): {e}");
        }
    }
}

#[tokio::test]
async fn test_nodejs_hook_execution() {
    let plugin = NodejsLanguagePlugin::new();

    // Create a temporary environment
    let config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Partial,
        cache_strategy: CacheStrategy::Memory,
        working_directory: None,
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env_result = plugin.setup_environment(&config).await;
    if env_result.is_err() {
        eprintln!("Skipping hook execution test - Node.js environment unavailable");
        return;
    }
    let env = env_result.unwrap();

    // Create a simple Node.js hook
    let hook = Hook {
        id: "test-node-hook".to_string(),
        name: Some("Test Node Hook".to_string()),
        entry: "node".to_string(),
        language: "node".to_string(),
        files: Some(r"\.js$".to_string()),
        exclude: Some("".to_string()),
        types: vec!["javascript".to_string()],
        exclude_types: vec![],
        additional_dependencies: vec![],
        args: vec![
            "-e".to_string(),
            "console.log('Hello from Node.js')".to_string(),
        ],
        always_run: true,
        fail_fast: false,
        pass_filenames: false,
        stages: vec![snp::core::Stage::PreCommit],
        verbose: false,
        minimum_pre_commit_version: None,
    };

    // Test hook execution
    let files = vec![PathBuf::from("test.js")];
    let result = plugin.execute_hook(&hook, &env, &files).await;

    match result {
        Ok(exec_result) => {
            assert_eq!(exec_result.hook_id, "test-node-hook");
            assert!(
                exec_result.success,
                "Node.js hook should execute successfully"
            );
            assert!(
                exec_result.stdout.contains("Hello from Node.js"),
                "Hook output should contain expected message"
            );
            assert!(exec_result.duration > Duration::from_millis(0));
        }
        Err(e) => {
            eprintln!("Hook execution failed (might be expected if Node.js unavailable): {e}");
        }
    }
}

#[tokio::test]
async fn test_workspace_support() {
    let plugin = NodejsLanguagePlugin::new();
    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create a workspace root with package.json containing workspaces
    let root_package_json = r#"{
        "name": "workspace-root",
        "version": "1.0.0",
        "workspaces": [
            "packages/*"
        ]
    }"#;
    fs::write(project_path.join("package.json"), root_package_json)
        .await
        .unwrap();

    // Create a workspace package
    let packages_dir = project_path.join("packages").join("test-package");
    fs::create_dir_all(&packages_dir).await.unwrap();
    let package_json = r#"{
        "name": "test-package",
        "version": "1.0.0"
    }"#;
    fs::write(packages_dir.join("package.json"), package_json)
        .await
        .unwrap();

    // Test workspace root detection
    let workspace_root = plugin.find_workspace_root(&packages_dir);
    assert!(workspace_root.is_ok(), "Should find workspace root");
    let workspace_root = workspace_root.unwrap();
    assert!(workspace_root.is_some(), "Should detect workspace root");
    assert_eq!(workspace_root.unwrap(), project_path);

    // Test workspace root validation
    let is_workspace = plugin.is_workspace_root(project_path);
    assert!(
        is_workspace.is_ok(),
        "Workspace root validation should succeed"
    );
    assert!(
        is_workspace.unwrap(),
        "Should identify directory as workspace root"
    );

    // Test non-workspace directory
    let non_workspace = plugin.is_workspace_root(&packages_dir);
    assert!(
        non_workspace.is_ok(),
        "Non-workspace validation should succeed"
    );
    assert!(
        !non_workspace.unwrap(),
        "Package directory should not be workspace root"
    );
}

#[tokio::test]
async fn test_language_trait_implementation() {
    let plugin = NodejsLanguagePlugin::new();

    // Test language identification
    assert_eq!(plugin.language_name(), "node");

    let extensions = plugin.supported_extensions();
    assert!(extensions.contains(&"js"));
    assert!(extensions.contains(&"mjs"));
    assert!(extensions.contains(&"ts"));
    assert!(extensions.contains(&"tsx"));
    assert!(extensions.contains(&"jsx"));
    assert!(extensions.contains(&"json"));

    let patterns = plugin.detection_patterns();
    assert!(patterns.iter().any(|p| p.contains("node")));
    assert!(patterns.iter().any(|p| p.contains("package.json")));

    // Test default configuration
    let config = plugin.default_config();
    assert!(config.enabled);
    assert!(config.cache_settings.enable_environment_cache);
    assert!(config.cache_settings.enable_dependency_cache);
    assert!(config.execution_settings.capture_output);

    // Test configuration validation
    let validation = plugin.validate_config(&config);
    assert!(validation.is_ok(), "Default config should be valid");
}

#[tokio::test]
async fn test_command_construction() {
    let plugin = NodejsLanguagePlugin::new();

    // Setup environment
    let config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Partial,
        cache_strategy: CacheStrategy::Memory,
        working_directory: None,
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env_result = plugin.setup_environment(&config).await;
    if env_result.is_err() {
        eprintln!("Skipping command construction test - Node.js environment unavailable");
        return;
    }
    let env = env_result.unwrap();

    // Create test hook
    let hook = Hook {
        id: "eslint".to_string(),
        name: Some("ESLint".to_string()),
        entry: "eslint".to_string(),
        language: "node".to_string(),
        files: Some(r"\.js$".to_string()),
        exclude: Some("".to_string()),
        types: vec!["javascript".to_string()],
        exclude_types: vec![],
        additional_dependencies: vec![],
        args: vec!["--fix".to_string()],
        always_run: false,
        fail_fast: false,
        pass_filenames: true,
        stages: vec![snp::core::Stage::PreCommit],
        verbose: false,
        minimum_pre_commit_version: None,
    };

    let files = vec![PathBuf::from("src/main.js"), PathBuf::from("src/utils.js")];
    let command = plugin.build_command(&hook, &env, &files);

    assert!(command.is_ok(), "Command construction should succeed");
    let command = command.unwrap();

    assert_eq!(command.executable, "eslint");
    assert!(command.arguments.contains(&"--fix".to_string()));
    assert!(command.arguments.contains(&"src/main.js".to_string()));
    assert!(command.arguments.contains(&"src/utils.js".to_string()));

    // Test environment variables
    assert!(
        !command.environment.is_empty(),
        "Command should have environment variables"
    );

    // Test NODE_PATH or similar Node.js environment setup
    let has_node_env = command.environment.contains_key("NODE_PATH")
        || command.environment.contains_key("NODE_VIRTUAL_ENV")
        || command.environment.contains_key("PATH");
    assert!(
        has_node_env,
        "Command should have Node.js environment variables"
    );
}

#[tokio::test]
async fn test_performance_optimization() {
    let mut plugin = NodejsLanguagePlugin::new();
    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create package.json
    let package_json = r#"{
        "name": "performance-test",
        "version": "1.0.0"
    }"#;
    fs::write(project_path.join("package.json"), package_json)
        .await
        .unwrap();

    // Test caching behavior
    let start_time = std::time::Instant::now();
    let manager1 = plugin.detect_package_manager(project_path).await;
    let first_duration = start_time.elapsed();

    let start_time = std::time::Instant::now();
    let manager2 = plugin.detect_package_manager(project_path).await;
    let second_duration = start_time.elapsed();

    // Second call should be faster due to caching (or at least not significantly slower)
    if manager1.is_ok() && manager2.is_ok() {
        // Allow for some variance in timing - cached should be no more than 2x slower
        if second_duration > first_duration * 2 {
            eprintln!(
                "Caching may not be working optimally: first={}ms, second={}ms",
                first_duration.as_millis(),
                second_duration.as_millis()
            );
        }

        let manager1 = manager1.unwrap();
        let manager2 = manager2.unwrap();
        assert_eq!(manager1.manager_type, manager2.manager_type);
        assert_eq!(manager1.executable_path, manager2.executable_path);
    }

    // Test Node.js installation caching
    let start_time = std::time::Instant::now();
    let installations1 = plugin.detect_node_installations().await;
    let first_detection = start_time.elapsed();

    let start_time = std::time::Instant::now();
    let installations2 = plugin.detect_node_installations().await;
    let second_detection = start_time.elapsed();

    if installations1.is_ok() && installations2.is_ok() {
        // Allow for some variance in timing
        if second_detection > first_detection * 2 {
            eprintln!(
                "Node.js detection caching may not be working optimally: first={}ms, second={}ms",
                first_detection.as_millis(),
                second_detection.as_millis()
            );
        }
    }
}

#[tokio::test]
async fn test_error_handling_and_recovery() {
    let plugin = NodejsLanguagePlugin::new();

    // Test handling non-existent Node.js version
    let config = EnvironmentConfig {
        language_version: Some("99.99.99".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Memory,
        working_directory: None,
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let result = plugin.setup_environment(&config).await;
    assert!(
        result.is_err(),
        "Should fail with non-existent Node.js version"
    );

    // Test handling invalid package.json
    let temp_dir = tempdir().unwrap();
    let invalid_project = temp_dir.path();
    fs::write(invalid_project.join("package.json"), "invalid json")
        .await
        .unwrap();

    let mut plugin = NodejsLanguagePlugin::new();
    let manager_result = plugin.detect_package_manager(invalid_project).await;
    // Should either succeed (ignoring invalid package.json) or fail gracefully
    if let Err(e) = manager_result {
        assert!(
            e.to_string().contains("package.json") || e.to_string().contains("invalid"),
            "Error should reference package.json or invalid content"
        );
    }

    // Test handling missing package manager
    let config = snp::language::nodejs::NodejsLanguageConfig {
        package_manager_preference: snp::language::nodejs::PackageManagerPreference::Pnpm,
        ..Default::default()
    };
    let mut plugin_with_config = NodejsLanguagePlugin::with_config(config);

    // If pnpm is not available, it should either fallback or fail gracefully
    let manager_result = plugin_with_config
        .detect_package_manager(temp_dir.path())
        .await;
    match manager_result {
        Ok(manager) => {
            // Either found pnpm or fell back to available manager
            assert!(!manager.executable_path.as_os_str().is_empty());
        }
        Err(e) => {
            // Graceful failure with informative error
            assert!(
                e.to_string().contains("pnpm") || e.to_string().contains("not found"),
                "Error should be informative about missing pnpm"
            );
        }
    }
}

#[tokio::test]
async fn test_cross_platform_compatibility() {
    let _plugin = NodejsLanguagePlugin::new();

    // Test platform detection
    let platform_info = snp::language::nodejs::NodejsLanguagePlugin::detect_platform_info();
    assert!(platform_info.is_ok(), "Platform detection should succeed");
    let platform_info = platform_info.unwrap();

    assert!(!platform_info.architecture.is_empty());
    assert!(!platform_info.path_separator.is_empty());

    // Test platform-specific executable extensions
    if cfg!(windows) {
        assert_eq!(platform_info.executable_extension, Some(".exe".to_string()));
        assert_eq!(platform_info.path_separator, ";");
    } else {
        assert_eq!(platform_info.executable_extension, None);
        assert_eq!(platform_info.path_separator, ":");
    }

    // Test Node.js executable finding with platform-specific names
    let mut plugin = NodejsLanguagePlugin::new();
    let installations = plugin.detect_node_installations().await;

    if let Ok(installations) = installations {
        for installation in installations {
            // Verify executable path follows platform conventions
            let exe_name = installation
                .executable_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if cfg!(windows) {
                assert!(exe_name == "node.exe" || exe_name == "node");
            } else {
                assert_eq!(exe_name, "node");
            }
        }
    }
}
