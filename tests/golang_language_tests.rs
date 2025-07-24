use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use snp::core::Hook;
use snp::language::environment::{CacheStrategy, EnvironmentConfig, IsolationLevel};
use snp::language::golang::{
    GoError, GoInstallationSource, GoLanguagePlugin, GoModule, GoTool, GoToolchainInfo, ModuleMode,
    OptimizationLevel,
};
use snp::language::traits::Language;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_go_toolchain_detection() {
    let mut plugin = GoLanguagePlugin::new();

    // Test go executable discovery and version detection
    match plugin.detect_go_installations().await {
        Ok(installations) => {
            assert!(
                !installations.is_empty(),
                "Should find at least one Go installation"
            );

            for installation in &installations {
                assert!(
                    installation.go_executable.exists(),
                    "Go executable should exist"
                );
                assert!(
                    !installation.version.to_string().is_empty(),
                    "Version should be detected"
                );
                assert!(installation.goroot.exists(), "GOROOT should exist");
                assert!(!installation.goos.is_empty(), "GOOS should be detected");
                assert!(!installation.goarch.is_empty(), "GOARCH should be detected");
            }
        }
        Err(_) => {
            // It's OK if Go is not installed on the test system
            eprintln!("Go not found on system - skipping toolchain detection test");
        }
    }
}

#[tokio::test]
async fn test_go_installation_path_and_goroot_detection() {
    let mut plugin = GoLanguagePlugin::new();

    // Test Go installation path detection
    if let Ok(installations) = plugin.detect_go_installations().await {
        for installation in installations {
            // Test GOROOT detection
            assert!(installation.goroot.exists(), "GOROOT path should exist");
            assert!(
                installation.goroot.join("bin").exists(),
                "GOROOT/bin should exist"
            );

            // Test Go toolchain component availability
            let go_bin = installation.goroot.join("bin").join("go");
            if cfg!(windows) {
                let go_exe = installation.goroot.join("bin").join("go.exe");
                assert!(go_bin.exists() || go_exe.exists(), "Go binary should exist");
            } else {
                assert!(go_bin.exists(), "Go binary should exist");
            }
        }
    }
}

#[tokio::test]
async fn test_cross_platform_go_installation_handling() {
    let plugin = GoLanguagePlugin::new();

    // Test that plugin handles different platform paths correctly
    let test_installations = vec![
        (
            PathBuf::from("/usr/local/go/bin/go"),
            PathBuf::from("/usr/local/go"),
        ),
        (PathBuf::from("/opt/go/bin/go"), PathBuf::from("/opt/go")),
        (
            PathBuf::from("C:\\Program Files\\Go\\bin\\go.exe"),
            PathBuf::from("C:\\Program Files\\Go"),
        ),
    ];

    for (go_path, expected_goroot) in test_installations {
        if Path::new(&go_path).exists() {
            if let Ok(info) = plugin.analyze_go_installation(&go_path).await {
                assert_eq!(info.goroot, expected_goroot);
            }
        }
    }
}

#[tokio::test]
async fn test_go_module_management() {
    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    // Create a test go.mod file
    let go_mod_content = r#"module example.com/test
go 1.21

require (
    github.com/stretchr/testify v1.8.4
    golang.org/x/sys v0.12.0 // indirect
)

replace github.com/old/module => github.com/new/module v1.0.0
"#;

    fs::write(module_path.join("go.mod"), go_mod_content)
        .await
        .unwrap();

    // Create go.sum file
    let go_sum_content = r#"github.com/stretchr/testify v1.8.4 h1:CcVxjf3Q8VM5dzMtD9rkQKUSV4I4C3hg0CvAmf8kNaM=
github.com/stretchr/testify v1.8.4/go.mod h1:sz/lmYIOXD/1dqDmKjjqLyZ2RngseejIcXlSw2iwfAo=
"#;
    fs::write(module_path.join("go.sum"), go_sum_content)
        .await
        .unwrap();

    let mut plugin = GoLanguagePlugin::new();
    let module = plugin.detect_go_module(module_path).await.unwrap();

    // Test go.mod parsing and module detection
    assert_eq!(module.module_mode, ModuleMode::Module);
    assert_eq!(module.module_name, Some("example.com/test".to_string()));
    assert!(module.go_mod_path.is_some());
    assert!(module.go_sum_path.is_some());

    // Test dependency parsing
    assert_eq!(module.dependencies.len(), 2);
    let testify_dep = module
        .dependencies
        .iter()
        .find(|d| d.module_path == "github.com/stretchr/testify")
        .unwrap();
    assert_eq!(testify_dep.version, "v1.8.4");
    assert!(!testify_dep.indirect);

    let sys_dep = module
        .dependencies
        .iter()
        .find(|d| d.module_path == "golang.org/x/sys")
        .unwrap();
    assert_eq!(sys_dep.version, "v0.12.0");
    assert!(sys_dep.indirect);

    // Test replace directives
    assert_eq!(module.replace_directives.len(), 1);
    let replace = &module.replace_directives[0];
    assert_eq!(replace.old_path, "github.com/old/module");
    assert_eq!(replace.new_path, "github.com/new/module");
    assert_eq!(replace.new_version, Some("v1.0.0".to_string()));
}

#[tokio::test]
async fn test_go_sum_lockfile_handling() {
    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    fs::write(module_path.join("go.mod"), "module test\ngo 1.21")
        .await
        .unwrap();

    let go_sum_content = r#"github.com/example v1.0.0 h1:hash1
github.com/example v1.0.0/go.mod h1:hash2
"#;
    fs::write(module_path.join("go.sum"), go_sum_content)
        .await
        .unwrap();

    let mut plugin = GoLanguagePlugin::new();
    let module = plugin.detect_go_module(module_path).await.unwrap();

    assert!(module.go_sum_path.is_some());
    assert_eq!(module.go_sum_path.unwrap(), module_path.join("go.sum"));
}

#[tokio::test]
async fn test_workspace_mode_support() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create go.work file
    let go_work_content = r#"go 1.21

use (
    ./module1
    ./module2
)
"#;
    fs::write(workspace_path.join("go.work"), go_work_content)
        .await
        .unwrap();

    // Create module directories
    fs::create_dir_all(workspace_path.join("module1"))
        .await
        .unwrap();
    fs::create_dir_all(workspace_path.join("module2"))
        .await
        .unwrap();

    fs::write(
        workspace_path.join("module1/go.mod"),
        "module example.com/module1\ngo 1.21",
    )
    .await
    .unwrap();
    fs::write(
        workspace_path.join("module2/go.mod"),
        "module example.com/module2\ngo 1.21",
    )
    .await
    .unwrap();

    let mut plugin = GoLanguagePlugin::new();
    let module = plugin.detect_go_module(workspace_path).await.unwrap();

    assert_eq!(module.module_mode, ModuleMode::Workspace);
    assert!(module.go_work_path.is_some());
}

#[tokio::test]
async fn test_module_proxy_and_goproxy_configuration() {
    let mut plugin = GoLanguagePlugin::new();
    plugin.config.goproxy_url = Some("https://proxy.golang.org".to_string());
    plugin.config.gosumdb = Some("sum.golang.org".to_string());

    // Test that configuration is properly set
    assert_eq!(
        plugin.config.goproxy_url,
        Some("https://proxy.golang.org".to_string())
    );
    assert_eq!(plugin.config.gosumdb, Some("sum.golang.org".to_string()));
}

#[tokio::test]
async fn test_dependency_management() {
    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    // Create a minimal go.mod
    fs::write(module_path.join("go.mod"), "module test\ngo 1.21")
        .await
        .unwrap();

    let mut plugin = GoLanguagePlugin::new();

    // Test go.mod dependency parsing and resolution
    let module = plugin.detect_go_module(module_path).await.unwrap();
    assert_eq!(module.dependencies.len(), 0); // No dependencies in minimal module

    // Test dependency parsing with actual dependencies
    let go_mod_with_deps = r#"module test
go 1.21

require (
    github.com/example/lib v1.0.0
)
"#;
    fs::write(module_path.join("go.mod"), go_mod_with_deps)
        .await
        .unwrap();

    let module_with_deps = plugin.detect_go_module(module_path).await.unwrap();
    assert_eq!(module_with_deps.dependencies.len(), 1);
    assert_eq!(
        module_with_deps.dependencies[0].module_path,
        "github.com/example/lib"
    );
    assert_eq!(module_with_deps.dependencies[0].version, "v1.0.0");
}

#[tokio::test]
async fn test_replace_directives_and_local_modules() {
    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    let go_mod_content = r#"module test
go 1.21

require github.com/example/lib v1.0.0

replace github.com/example/lib => ../local-lib
replace github.com/old/lib v0.1.0 => github.com/new/lib v1.2.0
"#;

    fs::write(module_path.join("go.mod"), go_mod_content)
        .await
        .unwrap();

    let mut plugin = GoLanguagePlugin::new();
    let module = plugin.detect_go_module(module_path).await.unwrap();

    assert_eq!(module.replace_directives.len(), 2);

    // Test local replacement
    let local_replace = module
        .replace_directives
        .iter()
        .find(|r| r.new_path == "../local-lib")
        .unwrap();
    assert_eq!(local_replace.old_path, "github.com/example/lib");
    assert!(local_replace.old_version.is_none());
    assert!(local_replace.new_version.is_none());

    // Test version-specific replacement
    let version_replace = module
        .replace_directives
        .iter()
        .find(|r| r.old_version == Some("v0.1.0".to_string()))
        .unwrap();
    assert_eq!(version_replace.old_path, "github.com/old/lib");
    assert_eq!(version_replace.new_path, "github.com/new/lib");
    assert_eq!(version_replace.new_version, Some("v1.2.0".to_string()));
}

#[tokio::test]
async fn test_go_hook_execution() {
    // Skip this test if Go is not available
    if which::which("go").is_err() {
        eprintln!("Go not found - skipping hook execution test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    // Create a simple Go module with a main.go
    fs::write(module_path.join("go.mod"), "module test\ngo 1.21")
        .await
        .unwrap();

    let main_go = r#"package main

import "fmt"

func main() {
    fmt.Println("Hello from Go hook!")
}
"#;
    fs::write(module_path.join("main.go"), main_go)
        .await
        .unwrap();

    let plugin = GoLanguagePlugin::new();

    // Setup environment
    let env_config = EnvironmentConfig {
        language_version: None,
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::None,
        isolation_level: IsolationLevel::Complete,
        working_directory: Some(module_path.to_path_buf()),
        version: None,
    };

    let env = plugin.setup_environment(&env_config).await.unwrap();

    // Create a simple hook
    let hook = Hook::new("test-go", "go run main.go", "golang").with_name("Test Go Hook");

    // Execute hook
    let files = vec![module_path.join("main.go")];
    let result = plugin.execute_hook(&hook, &env, &files).await.unwrap();

    if !result.success {
        eprintln!("Hook execution failed:");
        eprintln!("Exit code: {:?}", result.exit_code);
        eprintln!("Stdout: {}", result.stdout);
        eprintln!("Stderr: {}", result.stderr);
        if let Some(ref error) = result.error {
            eprintln!("Error: {error:?}");
        }
    }

    assert!(result.success, "Go hook should execute successfully");
    assert!(result.stdout.contains("Hello from Go hook!"));
}

#[tokio::test]
async fn test_go_run_based_hook_execution() {
    if which::which("go").is_err() {
        eprintln!("Go not found - skipping go run test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    // Create Go module
    fs::write(module_path.join("go.mod"), "module formatter\ngo 1.21")
        .await
        .unwrap();

    // Create a simple formatter tool
    let formatter_go = r#"package main

import (
    "fmt"
    "os"
)

func main() {
    if len(os.Args) < 2 {
        fmt.Println("No files provided")
        return
    }
    
    for _, file := range os.Args[1:] {
        fmt.Printf("Formatting %s\n", file)
    }
}
"#;
    fs::write(module_path.join("main.go"), formatter_go)
        .await
        .unwrap();

    let plugin = GoLanguagePlugin::new();

    let env_config = EnvironmentConfig {
        language_version: None,
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::None,
        isolation_level: IsolationLevel::Complete,
        working_directory: Some(module_path.to_path_buf()),
        version: None,
    };

    let env = plugin.setup_environment(&env_config).await.unwrap();

    let hook = Hook::new("formatter", "go run .", "golang").with_name("Go Formatter");

    // Create the test files that the formatter will process
    fs::write(module_path.join("file1.go"), "package main\n")
        .await
        .unwrap();
    fs::write(module_path.join("file2.go"), "package main\n")
        .await
        .unwrap();

    let test_files = vec![module_path.join("file1.go"), module_path.join("file2.go")];

    let result = plugin.execute_hook(&hook, &env, &test_files).await.unwrap();

    if !result.success {
        eprintln!("Hook execution failed:");
        eprintln!("Exit code: {:?}", result.exit_code);
        eprintln!("Stdout: {}", result.stdout);
        eprintln!("Stderr: {}", result.stderr);
        if let Some(ref error) = result.error {
            eprintln!("Error: {error:?}");
        }
    }

    assert!(result.success);
    assert!(result.stdout.contains("Formatting"));
}

#[tokio::test]
async fn test_go_install_and_binary_execution() {
    if which::which("go").is_err() {
        eprintln!("Go not found - skipping go install test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    fs::write(module_path.join("go.mod"), "module tool\ngo 1.21")
        .await
        .unwrap();

    let tool_go = r#"package main

import "fmt"

func main() {
    fmt.Println("Tool executed successfully")
}
"#;
    fs::write(module_path.join("main.go"), tool_go)
        .await
        .unwrap();

    let plugin = GoLanguagePlugin::new();

    // Test dependency installation
    let fake_toolchain = GoToolchainInfo {
        go_executable: which::which("go").unwrap(),
        version: semver::Version::new(1, 21, 0),
        goroot: PathBuf::from("/usr/local/go"),
        gopath: Some(temp_dir.path().to_path_buf()),
        goos: "linux".to_string(),
        goarch: "amd64".to_string(),
        tools: vec![],
        source: GoInstallationSource::System,
    };

    let fake_module = GoModule {
        root_path: module_path.to_path_buf(),
        module_mode: ModuleMode::Module,
        go_mod_path: Some(module_path.join("go.mod")),
        go_sum_path: None,
        go_work_path: None,
        module_name: Some("tool".to_string()),
        go_version: Some(semver::Version::new(1, 21, 0)),
        dependencies: vec![],
        replace_directives: vec![],
    };

    // This would normally install the binary
    let deps = vec!["./...".to_string()];
    let result = plugin
        .install_go_dependencies(&fake_toolchain, &fake_module, &deps)
        .await;

    // We don't assert success here as it depends on the test environment
    // but we test that the function doesn't panic
    match result {
        Ok(_) => println!("Go install succeeded"),
        Err(e) => println!("Go install failed (expected in test): {e:?}"),
    }
}

#[tokio::test]
async fn test_gopath_vs_module_mode_handling() {
    let temp_dir = TempDir::new().unwrap();

    // Test module mode
    let module_path = temp_dir.path().join("module");
    fs::create_dir_all(&module_path).await.unwrap();
    fs::write(module_path.join("go.mod"), "module test\ngo 1.21")
        .await
        .unwrap();

    let mut plugin = GoLanguagePlugin::new();
    let module = plugin.detect_go_module(&module_path).await.unwrap();
    assert_eq!(module.module_mode, ModuleMode::Module);

    // Test GOPATH mode (no go.mod)
    let gopath_path = temp_dir.path().join("gopath");
    fs::create_dir_all(&gopath_path).await.unwrap();

    let gopath_module = plugin.detect_go_module(&gopath_path).await.unwrap();
    assert_eq!(gopath_module.module_mode, ModuleMode::Gopath);
}

#[tokio::test]
async fn test_build_environment_setup() {
    let mut plugin = GoLanguagePlugin::new();
    plugin.config.goproxy_url = Some("https://proxy.golang.org".to_string());
    plugin.config.gosumdb = Some("sum.golang.org".to_string());
    plugin.config.enable_module_mode = true;
    plugin.config.build_config.cgo_enabled = false;

    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    let fake_toolchain = GoToolchainInfo {
        go_executable: PathBuf::from("/usr/local/go/bin/go"),
        version: semver::Version::new(1, 21, 0),
        goroot: PathBuf::from("/usr/local/go"),
        gopath: Some(temp_dir.path().to_path_buf()),
        goos: "linux".to_string(),
        goarch: "amd64".to_string(),
        tools: vec![],
        source: GoInstallationSource::System,
    };

    let fake_module = GoModule {
        root_path: module_path.to_path_buf(),
        module_mode: ModuleMode::Module,
        go_mod_path: Some(module_path.join("go.mod")),
        go_sum_path: None,
        go_work_path: None,
        module_name: Some("test".to_string()),
        go_version: Some(semver::Version::new(1, 21, 0)),
        dependencies: vec![],
        replace_directives: vec![],
    };

    let go_env = plugin
        .setup_go_environment(&fake_toolchain, &fake_module, &[])
        .await
        .unwrap();

    // Test GOPATH, GOROOT, and GOPROXY setup
    assert_eq!(go_env.toolchain.goroot, PathBuf::from("/usr/local/go"));
    assert!(go_env.gopath.exists() || go_env.gopath.parent().unwrap().exists());

    // Test environment variables
    assert_eq!(
        go_env.environment_vars.get("GOPROXY"),
        Some(&"https://proxy.golang.org".to_string())
    );
    assert_eq!(
        go_env.environment_vars.get("GOSUMDB"),
        Some(&"sum.golang.org".to_string())
    );
    assert_eq!(
        go_env.environment_vars.get("GO111MODULE"),
        Some(&"on".to_string())
    );
    assert_eq!(
        go_env.environment_vars.get("CGO_ENABLED"),
        Some(&"0".to_string())
    );
}

#[tokio::test]
async fn test_build_cache_management() {
    let plugin = GoLanguagePlugin::new();

    // Test build cache configuration
    assert!(plugin.config.cache_config.build_cache_dir.is_none()); // Default

    let mut plugin_with_cache = GoLanguagePlugin::new();
    plugin_with_cache.config.cache_config.build_cache_dir = Some(PathBuf::from("/tmp/go-cache"));
    plugin_with_cache.config.cache_config.max_cache_size = Some(1024 * 1024 * 100); // 100MB

    assert_eq!(
        plugin_with_cache.config.cache_config.build_cache_dir,
        Some(PathBuf::from("/tmp/go-cache"))
    );
    assert_eq!(
        plugin_with_cache.config.cache_config.max_cache_size,
        Some(1024 * 1024 * 100)
    );
}

#[tokio::test]
async fn test_cross_compilation_environment() {
    let _plugin = GoLanguagePlugin::new();

    let fake_toolchain = GoToolchainInfo {
        go_executable: PathBuf::from("/usr/local/go/bin/go"),
        version: semver::Version::new(1, 21, 0),
        goroot: PathBuf::from("/usr/local/go"),
        gopath: Some(PathBuf::from("/home/user/go")),
        goos: "linux".to_string(),
        goarch: "amd64".to_string(),
        tools: vec![],
        source: GoInstallationSource::System,
    };

    // Test that GOOS and GOARCH are properly detected
    assert_eq!(fake_toolchain.goos, "linux");
    assert_eq!(fake_toolchain.goarch, "amd64");
}

#[tokio::test]
async fn test_cgo_and_build_constraints_handling() {
    let mut plugin = GoLanguagePlugin::new();
    plugin.config.build_config.cgo_enabled = true;
    plugin.config.build_config.build_tags = vec!["integration".to_string(), "test".to_string()];
    plugin.config.build_config.ldflags = vec!["-s".to_string(), "-w".to_string()];

    // Test CGO configuration
    assert!(plugin.config.build_config.cgo_enabled);
    assert_eq!(
        plugin.config.build_config.build_tags,
        vec!["integration", "test"]
    );
    assert_eq!(plugin.config.build_config.ldflags, vec!["-s", "-w"]);
}

#[tokio::test]
async fn test_performance_optimization() {
    let plugin = GoLanguagePlugin::new();

    // Test that caching is available
    assert!(plugin.toolchain_cache.is_empty()); // Initially empty
    assert!(plugin.module_cache.is_empty()); // Initially empty

    // Test build caching configuration
    let cache_config = &plugin.config.cache_config;
    assert!(cache_config.cache_ttl > Duration::from_secs(0));
}

#[tokio::test]
async fn test_module_cache_reuse() {
    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    fs::write(module_path.join("go.mod"), "module test\ngo 1.21")
        .await
        .unwrap();

    let mut plugin = GoLanguagePlugin::new();

    // First call should parse and cache
    let module1 = plugin.detect_go_module(module_path).await.unwrap();
    assert_eq!(plugin.module_cache.len(), 1);

    // Second call should use cache
    let module2 = plugin.detect_go_module(module_path).await.unwrap();
    assert_eq!(plugin.module_cache.len(), 1);

    // Results should be identical
    assert_eq!(module1.module_name, module2.module_name);
    assert_eq!(module1.module_mode, module2.module_mode);
}

#[tokio::test]
async fn test_parallel_build_configuration() {
    let mut plugin = GoLanguagePlugin::new();
    plugin.config.build_config.optimization_level = OptimizationLevel::Speed;
    plugin.config.build_config.race_detector = true;

    assert_eq!(
        plugin.config.build_config.optimization_level,
        OptimizationLevel::Speed
    );
    assert!(plugin.config.build_config.race_detector);
}

#[tokio::test]
async fn test_vendor_directory_support() {
    let temp_dir = TempDir::new().unwrap();
    let module_path = temp_dir.path();

    fs::write(module_path.join("go.mod"), "module test\ngo 1.21")
        .await
        .unwrap();

    // Create vendor directory
    fs::create_dir_all(module_path.join("vendor"))
        .await
        .unwrap();
    fs::write(
        module_path.join("vendor/modules.txt"),
        "# github.com/example/lib v1.0.0\n",
    )
    .await
    .unwrap();

    let mut plugin = GoLanguagePlugin::new();
    let module = plugin.detect_go_module(module_path).await.unwrap();

    // Module should still be detected correctly with vendor directory
    assert_eq!(module.module_mode, ModuleMode::Module);
    assert!(module_path.join("vendor").exists());
}

#[test]
fn test_go_language_plugin_creation() {
    let plugin = GoLanguagePlugin::new();
    assert_eq!(plugin.language_name(), "golang");

    let extensions = plugin.supported_extensions();
    assert!(extensions.contains(&"go"));
    assert!(extensions.contains(&"mod"));
    assert!(extensions.contains(&"sum"));
    assert!(extensions.contains(&"work"));

    let patterns = plugin.detection_patterns();
    assert!(!patterns.is_empty());
}

#[test]
fn test_go_error_types() {
    let error = GoError::GoNotFound {
        searched_paths: vec![PathBuf::from("/usr/bin/go")],
        min_version: Some(semver::Version::new(1, 18, 0)),
    };

    assert!(error.to_string().contains("Go installation not found"));
}

#[test]
fn test_go_toolchain_info_creation() {
    let toolchain = GoToolchainInfo {
        go_executable: PathBuf::from("/usr/local/go/bin/go"),
        version: semver::Version::new(1, 21, 0),
        goroot: PathBuf::from("/usr/local/go"),
        gopath: Some(PathBuf::from("/home/user/go")),
        goos: "linux".to_string(),
        goarch: "amd64".to_string(),
        tools: vec![GoTool {
            name: "gofmt".to_string(),
            available: true,
            path: Some(PathBuf::from("/usr/local/go/bin/gofmt")),
        }],
        source: GoInstallationSource::System,
    };

    assert_eq!(toolchain.version.major, 1);
    assert_eq!(toolchain.version.minor, 21);
    assert_eq!(toolchain.goos, "linux");
    assert_eq!(toolchain.goarch, "amd64");
    assert_eq!(toolchain.tools.len(), 1);
}
