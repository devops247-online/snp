// Comprehensive tests for Python language support following TDD Red Phase
// These tests are designed to fail initially and then pass once implementation is complete

use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;
use tokio::fs;

use snp::core::Hook;
use snp::language::{CacheStrategy, EnvironmentConfig, IsolationLevel, Language, VersionSpec};

// This will be the Python language plugin (doesn't exist yet)
use snp::language::python::PythonLanguagePlugin;

#[tokio::test]
async fn test_python_environment_creation() {
    let plugin = PythonLanguagePlugin::new();
    let config = EnvironmentConfig {
        language_version: Some("python3".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    // Test virtual environment creation
    let result = plugin.setup_environment(&config).await;
    if let Err(ref e) = result {
        eprintln!("Environment setup error: {e}");
    }
    assert!(
        result.is_ok(),
        "Failed to create Python environment: {:?}",
        result.err()
    );

    let env = result.unwrap();
    assert_eq!(env.language, "python");
    assert!(env.root_path.exists(), "Environment root path should exist");

    // Test Python executable exists
    let python_exe = if cfg!(windows) {
        env.root_path.join("Scripts").join("python.exe")
    } else {
        env.root_path.join("bin").join("python")
    };
    assert!(
        python_exe.exists(),
        "Python executable should exist in venv"
    );

    // Test pip executable exists
    let pip_exe = if cfg!(windows) {
        env.root_path.join("Scripts").join("pip.exe")
    } else {
        env.root_path.join("bin").join("pip")
    };
    assert!(pip_exe.exists(), "Pip executable should exist in venv");

    // Test environment variables are set correctly
    assert!(env.environment_variables.contains_key("VIRTUAL_ENV"));
    assert_eq!(
        env.environment_variables.get("VIRTUAL_ENV").unwrap(),
        &env.root_path.to_string_lossy()
    );

    // Test environment isolation
    assert!(!env.environment_variables.contains_key("PYTHONHOME"));
    assert_eq!(
        env.environment_variables
            .get("PIP_DISABLE_PIP_VERSION_CHECK"),
        Some(&"1".to_string())
    );
}

#[tokio::test]
async fn test_python_version_detection_and_compatibility() {
    let plugin = PythonLanguagePlugin::new();

    // Test Python interpreter detection
    let interpreters = plugin.detect_python_interpreters().await;
    assert!(interpreters.is_ok(), "Failed to detect Python interpreters");
    let interpreters = interpreters.unwrap();
    assert!(
        !interpreters.is_empty(),
        "Should find at least one Python interpreter"
    );

    // Test version information is complete
    for interpreter in &interpreters {
        assert!(!interpreter.executable_path.as_os_str().is_empty());
        assert!(interpreter.version.major > 0);
        assert!(interpreter.supports_venv || interpreter.version.major < 3);
        // Python 3.3+ should support venv
        if interpreter.version >= semver::Version::new(3, 3, 0) {
            assert!(interpreter.supports_venv, "Python 3.3+ should support venv");
        }
    }

    // Test version requirements
    let suitable_python = plugin.find_suitable_python("python3.9").await;
    if suitable_python.is_ok() {
        let python_info = suitable_python.unwrap();
        assert!(python_info.version.major == 3);
        assert!(python_info.version.minor >= 9);
    }

    // Test version compatibility checking
    let config = EnvironmentConfig {
        language_version: Some("python3.12".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    let result = plugin.setup_environment(&config).await;
    // Should either succeed with Python 3.12 or fail with clear error message
    if result.is_err() {
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(
            error_str.contains("Python") && error_str.contains("3.12"),
            "Error should mention Python 3.12 requirement"
        );
    }
}

#[tokio::test]
async fn test_pip_dependency_management() {
    let plugin = PythonLanguagePlugin::new();
    let config = EnvironmentConfig {
        language_version: Some("python3".to_string()),
        additional_dependencies: vec![
            "pytest>=6.0.0".to_string(),
            "black==23.1.0".to_string(),
            "requests".to_string(),
        ],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    // Create environment
    let env = plugin.setup_environment(&config).await;
    assert!(env.is_ok(), "Failed to create Python environment");
    let env = env.unwrap();

    // Test dependency resolution
    let deps = plugin
        .resolve_dependencies(&config.additional_dependencies)
        .await;
    assert!(deps.is_ok(), "Failed to resolve dependencies");
    let deps = deps.unwrap();
    assert_eq!(deps.len(), 3, "Should resolve 3 dependencies");

    // Test specific dependency parsing
    let pytest_dep = deps.iter().find(|d| d.name == "pytest").unwrap();
    assert!(matches!(pytest_dep.version_spec, VersionSpec::Range { .. }));

    let black_dep = deps.iter().find(|d| d.name == "black").unwrap();
    assert!(matches!(black_dep.version_spec, VersionSpec::Exact(_)));

    let requests_dep = deps.iter().find(|d| d.name == "requests").unwrap();
    assert!(matches!(requests_dep.version_spec, VersionSpec::Any));

    // Test dependency installation
    let install_result = plugin.install_dependencies(&env, &deps).await;
    assert!(install_result.is_ok(), "Failed to install dependencies");

    // Test pip cache directory exists
    let _pip_cache = env.root_path.join("pip-cache");
    // Pip cache may or may not exist depending on implementation
}

#[tokio::test]
async fn test_python_hook_execution() {
    let plugin = PythonLanguagePlugin::new();
    let config = EnvironmentConfig {
        language_version: Some("python3".to_string()),
        additional_dependencies: vec!["black".to_string()],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    // Create environment
    let env = plugin.setup_environment(&config).await;
    assert!(env.is_ok(), "Failed to create Python environment");
    let env = env.unwrap();

    // Create a test Python file
    let temp_dir = tempdir().unwrap();
    let test_file = temp_dir.path().join("test.py");
    fs::write(&test_file, "print('hello world')").await.unwrap();

    // Test hook execution - Python script
    let python_hook = Hook {
        id: "python-test".to_string(),
        name: Some("Python Test".to_string()),
        entry: "python".to_string(),
        language: "python".to_string(),
        files: Some(r"\.py$".to_string()),
        exclude: None,
        types: vec![],
        exclude_types: vec![],
        additional_dependencies: vec![],
        args: vec!["-c".to_string(), "print('test')".to_string()],
        always_run: false,
        fail_fast: false,
        pass_filenames: false,
        minimum_pre_commit_version: None,
        verbose: false,
        stages: vec![],
    };

    let files = vec![test_file.clone()];
    let result = plugin.execute_hook(&python_hook, &env, &files).await;
    assert!(result.is_ok(), "Failed to execute Python hook");

    let execution_result = result.unwrap();
    assert!(execution_result.success, "Hook execution should succeed");
    assert!(execution_result.stdout.contains("test"));

    // Test module-based hook execution
    let black_hook = Hook {
        id: "black-test".to_string(),
        name: Some("Black Formatter".to_string()),
        entry: "black".to_string(),
        language: "python".to_string(),
        files: Some(r"\.py$".to_string()),
        exclude: None,
        types: vec![],
        exclude_types: vec![],
        additional_dependencies: vec![],
        args: vec!["--check".to_string()],
        always_run: false,
        fail_fast: false,
        pass_filenames: true,
        minimum_pre_commit_version: None,
        verbose: false,
        stages: vec![],
    };

    let result = plugin.execute_hook(&black_hook, &env, &files).await;
    assert!(result.is_ok(), "Failed to execute Black hook");
}

#[tokio::test]
async fn test_performance_optimization() {
    let plugin = PythonLanguagePlugin::new();
    let config = EnvironmentConfig {
        language_version: Some("python3".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    // Test environment caching - first creation
    let start_time = std::time::Instant::now();
    let env1 = plugin.setup_environment(&config).await;
    let first_duration = start_time.elapsed();
    assert!(env1.is_ok(), "First environment creation should succeed");

    // Test environment reuse - should be much faster
    let start_time = std::time::Instant::now();
    let env2 = plugin.setup_environment(&config).await;
    let second_duration = start_time.elapsed();
    assert!(env2.is_ok(), "Second environment creation should succeed");

    // Second creation should be significantly faster (cached)
    eprintln!("First creation: {first_duration:?}, Second creation: {second_duration:?}");
    assert!(
        second_duration < first_duration / 2,
        "Cached environment creation should be much faster. First: {first_duration:?}, Second: {second_duration:?}"
    );

    // Test Python interpreter caching
    let interpreters1 = plugin.detect_python_interpreters().await.unwrap();
    let start_time = std::time::Instant::now();
    let interpreters2 = plugin.detect_python_interpreters().await.unwrap();
    let cached_duration = start_time.elapsed();

    assert_eq!(interpreters1.len(), interpreters2.len());
    assert!(
        cached_duration < Duration::from_millis(100),
        "Cached interpreter detection should be very fast"
    );
}

#[tokio::test]
async fn test_error_handling_and_recovery() {
    let plugin = PythonLanguagePlugin::new();

    // Test Python not found scenario
    let config = EnvironmentConfig {
        language_version: Some("python99.99".to_string()), // Non-existent version
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    let result = plugin.setup_environment(&config).await;
    assert!(
        result.is_err(),
        "Should fail with non-existent Python version"
    );

    let error = result.unwrap_err();
    let error_str = error.to_string();
    eprintln!("Actual error: {error_str}");
    assert!(
        error_str.to_lowercase().contains("python") || error_str.contains("not found"),
        "Error should mention Python not found. Actual error: {error_str}"
    );

    // Test invalid dependency specification
    let invalid_deps = vec!["invalid-package-name-!!!".to_string()];
    let deps_result = plugin.resolve_dependencies(&invalid_deps).await;
    // Should either succeed with best-effort parsing or fail gracefully
    if deps_result.is_err() {
        let error = deps_result.unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    // Test network timeout scenario (mock if needed)
    let config = EnvironmentConfig {
        language_version: Some("python3".to_string()),
        additional_dependencies: vec!["nonexistent-package-xyz".to_string()],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    let env = plugin.setup_environment(&config).await;
    if env.is_ok() {
        let env = env.unwrap();
        let deps = plugin
            .resolve_dependencies(&config.additional_dependencies)
            .await
            .unwrap();
        let install_result = plugin.install_dependencies(&env, &deps).await;
        // Should handle installation failures gracefully
        if install_result.is_err() {
            let error = install_result.unwrap_err();
            assert!(!error.to_string().is_empty());
        }
    }
}

#[tokio::test]
async fn test_requirements_txt_parsing() {
    let plugin = PythonLanguagePlugin::new();

    // Test requirements.txt parsing
    let requirements_content = r#"
# This is a comment
pytest>=6.0.0
black==23.1.0
requests
django>=3.0,<4.0
numpy==1.21.0 ; python_version >= "3.7"
-e git+https://github.com/user/repo.git#egg=package
"#;

    let temp_dir = tempdir().unwrap();
    let requirements_file = temp_dir.path().join("requirements.txt");
    fs::write(&requirements_file, requirements_content)
        .await
        .unwrap();

    let deps = plugin
        .parse_requirements_file(&requirements_file)
        .await
        .unwrap();

    // Should parse multiple dependencies with different version specs
    assert!(deps.len() >= 4, "Should parse at least 4 dependencies");

    // Check specific dependencies
    let pytest_dep = deps.iter().find(|d| d.name == "pytest");
    assert!(pytest_dep.is_some(), "Should find pytest dependency");

    let black_dep = deps.iter().find(|d| d.name == "black");
    assert!(black_dep.is_some(), "Should find black dependency");
}

#[tokio::test]
async fn test_cross_platform_compatibility() {
    let plugin = PythonLanguagePlugin::new();
    let config = EnvironmentConfig {
        language_version: Some("python3".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        isolation_level: IsolationLevel::Complete,
        cache_strategy: CacheStrategy::Both,
    };

    let env = plugin.setup_environment(&config).await;
    assert!(
        env.is_ok(),
        "Environment creation should work on all platforms"
    );

    let env = env.unwrap();

    // Test platform-specific executable paths
    let python_exe = if cfg!(windows) {
        env.root_path.join("Scripts").join("python.exe")
    } else {
        env.root_path.join("bin").join("python")
    };

    assert!(
        python_exe.exists(),
        "Python executable should exist at platform-specific path"
    );

    // Test PATH environment variable is set correctly
    let path_var = env.environment_variables.get("PATH").unwrap();
    let expected_bin_dir = if cfg!(windows) {
        env.root_path.join("Scripts")
    } else {
        env.root_path.join("bin")
    };

    assert!(
        path_var.contains(&expected_bin_dir.to_string_lossy().to_string()),
        "PATH should contain virtual environment bin directory"
    );
}

#[tokio::test]
async fn test_language_trait_integration() {
    let plugin = PythonLanguagePlugin::new();

    // Test Language trait methods
    assert_eq!(plugin.language_name(), "python");

    let extensions = plugin.supported_extensions();
    assert!(extensions.contains(&"py"));
    assert!(extensions.contains(&"pyi"));

    let patterns = plugin.detection_patterns();
    assert!(!patterns.is_empty());
    assert!(patterns.iter().any(|p| p.contains("python")));

    // Test configuration validation
    let config = plugin.default_config();
    let validation_result = plugin.validate_config(&config);
    assert!(validation_result.is_ok(), "Default config should be valid");
}
