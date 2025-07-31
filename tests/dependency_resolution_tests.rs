// Tests for hook dependency resolution functionality
// Tests the core dependency graph, topological sorting, and cycle detection

use snp::concurrency::TaskDependencyGraph;
use snp::config::Config;
use snp::core::{Hook, Stage};
use snp::execution::{ExecutionConfig, HookExecutionEngine};
use snp::process::ProcessManager;
use snp::storage::Store;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_task_dependency_graph_basic_operations() {
    let mut graph = TaskDependencyGraph::new();

    // Add tasks
    let config_a = snp::concurrency::TaskConfig::new("task_a");
    let config_b = snp::concurrency::TaskConfig::new("task_b");
    let config_c = snp::concurrency::TaskConfig::new("task_c");

    graph.add_task(config_a);
    graph.add_task(config_b);
    graph.add_task(config_c);

    // Add dependencies: c depends on b, b depends on a
    graph.add_dependency("task_c", "task_b").unwrap();
    graph.add_dependency("task_b", "task_a").unwrap();

    // Test topological sort
    let execution_order = graph.resolve_execution_order().unwrap();
    assert_eq!(execution_order, vec!["task_a", "task_b", "task_c"]);

    // Test cycle detection (should pass)
    graph.detect_cycles().unwrap();
}

#[tokio::test]
async fn test_dependency_graph_cycle_detection() {
    let mut graph = TaskDependencyGraph::new();

    // Add tasks that will form a cycle
    let config_a = snp::concurrency::TaskConfig::new("task_a");
    let config_b = snp::concurrency::TaskConfig::new("task_b");
    let config_c = snp::concurrency::TaskConfig::new("task_c");

    graph.add_task(config_a);
    graph.add_task(config_b);
    graph.add_task(config_c);

    // Create partial cycle: a -> b -> c
    graph.add_dependency("task_b", "task_a").unwrap();
    graph.add_dependency("task_c", "task_b").unwrap();

    // Attempting to complete the cycle (c -> a) should fail
    let cycle_addition = graph.add_dependency("task_a", "task_c");
    assert!(
        cycle_addition.is_err(),
        "Should prevent circular dependency creation"
    );

    // Verify the error indicates a circular dependency was detected
    if let Err(error) = cycle_addition {
        let error_msg = error.to_string();
        // The error should indicate cycle detection (either directly or through nested errors)
        assert!(
            error_msg.contains("Circular")
                || error_msg.contains("cycle")
                || error_msg.contains("Cycle"),
            "Error should indicate cycle detection: {error_msg}"
        );
    }

    // Since the cycle was prevented, topological sort should succeed
    let topo_result = graph.resolve_execution_order();
    assert!(
        topo_result.is_ok(),
        "Topological sort should succeed without cycles"
    );

    // Verify the valid execution order
    if let Ok(execution_order) = topo_result {
        assert_eq!(execution_order.len(), 3, "Should have all three tasks");
        // task_a should come before task_b, and task_b should come before task_c
        let a_pos = execution_order.iter().position(|t| t == "task_a").unwrap();
        let b_pos = execution_order.iter().position(|t| t == "task_b").unwrap();
        let c_pos = execution_order.iter().position(|t| t == "task_c").unwrap();
        assert!(a_pos < b_pos, "task_a should come before task_b");
        assert!(b_pos < c_pos, "task_b should come before task_c");
    }
}

#[tokio::test]
async fn test_dependency_graph_missing_dependency() {
    let mut graph = TaskDependencyGraph::new();

    // Add only one task
    let config_a = snp::concurrency::TaskConfig::new("task_a");
    graph.add_task(config_a);

    // Try to add dependency to non-existent task
    let result = graph.add_dependency("task_a", "nonexistent_task");
    // Note: The current implementation doesn't check for missing tasks in add_dependency,
    // but it will be caught in resolve_execution_order

    if result.is_ok() {
        // If add_dependency doesn't catch it, resolve_execution_order should
        let execution_result = graph.resolve_execution_order();
        assert!(
            execution_result.is_err(),
            "Should fail with missing dependency"
        );
    } else {
        // If add_dependency catches it, that's also fine
        assert!(result.is_err(), "Should fail with missing dependency");
    }
}

#[tokio::test]
async fn test_hook_dependency_yaml_parsing() {
    // Create a temporary config file with hook dependencies
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: lint
        name: Lint Code
        entry: echo
        language: system
        args: ["linting..."]
      - id: test
        name: Run Tests
        entry: echo
        language: system
        args: ["testing..."]
        depends_on: ["lint"]
      - id: build
        name: Build Project
        entry: echo
        language: system
        args: ["building..."]
        depends_on: ["lint", "test"]
"#;

    std::fs::write(&config_path, config_content).unwrap();

    // Parse configuration
    let config = Config::from_file(&config_path).unwrap();

    // Verify hooks were parsed with dependencies
    assert_eq!(config.repos.len(), 1);
    assert_eq!(config.repos[0].hooks.len(), 3);

    let lint_hook = &config.repos[0].hooks[0];
    let test_hook = &config.repos[0].hooks[1];
    let build_hook = &config.repos[0].hooks[2];

    assert_eq!(lint_hook.id, "lint");
    assert!(lint_hook.depends_on.is_none() || lint_hook.depends_on.as_ref().unwrap().is_empty());

    assert_eq!(test_hook.id, "test");
    assert_eq!(test_hook.depends_on.as_ref().unwrap(), &vec!["lint"]);

    assert_eq!(build_hook.id, "build");
    assert_eq!(
        build_hook.depends_on.as_ref().unwrap(),
        &vec!["lint", "test"]
    );
}

#[tokio::test]
async fn test_hook_dependency_conversion_to_core() {
    // Test that config::Hook with depends_on converts properly to core::Hook
    let config_hook = snp::config::Hook {
        id: "test_hook".to_string(),
        name: Some("Test Hook".to_string()),
        entry: "echo test".to_string(),
        language: "system".to_string(),
        files: None,
        exclude: None,
        types: None,
        types_or: None,
        exclude_types: None,
        additional_dependencies: None,
        args: None,
        always_run: None,
        fail_fast: None,
        pass_filenames: None,
        stages: None,
        verbose: None,
        depends_on: Some(vec!["dependency1".to_string(), "dependency2".to_string()]),
        concurrent: None,
    };

    // Convert to core Hook
    let core_hook = Hook::from_config(&config_hook, "local").unwrap();

    // Verify depends_on field was properly transferred
    assert_eq!(core_hook.depends_on, vec!["dependency1", "dependency2"]);
    assert_eq!(core_hook.id, "test_hook");
    assert_eq!(core_hook.entry, "echo test");
}

#[tokio::test]
async fn test_execution_engine_dependency_resolution() {
    // Test that the execution engine properly resolves dependencies
    // Use test-specific storage to avoid database locking
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Create hooks with dependencies
    let hook1 = Hook::new("hook1", "echo first", "system");
    let hook2 =
        Hook::new("hook2", "echo second", "system").with_depends_on(vec!["hook1".to_string()]);
    let hook3 = Hook::new("hook3", "echo third", "system")
        .with_depends_on(vec!["hook1".to_string(), "hook2".to_string()]);

    let hooks = vec![hook3.clone(), hook1.clone(), hook2.clone()]; // Intentionally out of order

    // Resolve dependencies
    let ordered_hooks = execution_engine.resolve_hook_dependencies(&hooks).unwrap();

    // Verify correct order
    assert_eq!(ordered_hooks.len(), 3);
    assert_eq!(ordered_hooks[0].id, "hook1");
    assert_eq!(ordered_hooks[1].id, "hook2");
    assert_eq!(ordered_hooks[2].id, "hook3");
}

#[tokio::test]
async fn test_execution_engine_cycle_detection_integration() {
    // Test that execution engine detects cycles
    // Use test-specific storage to avoid database locking
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Create hooks with circular dependencies
    let hook1 =
        Hook::new("hook1", "echo first", "system").with_depends_on(vec!["hook3".to_string()]);
    let hook2 =
        Hook::new("hook2", "echo second", "system").with_depends_on(vec!["hook1".to_string()]);
    let hook3 =
        Hook::new("hook3", "echo third", "system").with_depends_on(vec!["hook2".to_string()]);

    let hooks = vec![hook1, hook2, hook3];

    // Should detect cycle and fail
    let result = execution_engine.resolve_hook_dependencies(&hooks);
    assert!(result.is_err(), "Should detect circular dependency");
}

#[tokio::test]
async fn test_diamond_dependency_pattern() {
    // Test diamond dependency pattern: A -> B,C -> D
    let mut graph = TaskDependencyGraph::new();

    let tasks = ["a", "b", "c", "d"];
    for task in &tasks {
        let config = snp::concurrency::TaskConfig::new(*task);
        graph.add_task(config);
    }

    // Create diamond: a -> b, a -> c, b -> d, c -> d
    graph.add_dependency("b", "a").unwrap();
    graph.add_dependency("c", "a").unwrap();
    graph.add_dependency("d", "b").unwrap();
    graph.add_dependency("d", "c").unwrap();

    let execution_order = graph.resolve_execution_order().unwrap();

    // 'a' should be first
    assert_eq!(execution_order[0], "a");
    // 'd' should be last
    assert_eq!(execution_order[3], "d");

    // 'b' and 'c' should come after 'a' and before 'd'
    let a_pos = execution_order.iter().position(|x| x == "a").unwrap();
    let b_pos = execution_order.iter().position(|x| x == "b").unwrap();
    let c_pos = execution_order.iter().position(|x| x == "c").unwrap();
    let d_pos = execution_order.iter().position(|x| x == "d").unwrap();

    assert!(a_pos < b_pos);
    assert!(a_pos < c_pos);
    assert!(b_pos < d_pos);
    assert!(c_pos < d_pos);
}

#[tokio::test]
async fn test_parallel_vs_sequential_execution_decision() {
    // Test that hooks with dependencies force sequential execution
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to init git");

    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to config git");

    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to config git");

    // Create config file with dependencies
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: first
        name: First Hook
        entry: echo
        language: system
        args: ["first"]
        always_run: true
      - id: second
        name: Second Hook
        entry: echo
        language: system
        args: ["second"]
        depends_on: ["first"]
        always_run: true
"#;

    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).unwrap();

    // Test execution
    let execution_config = ExecutionConfig::new(Stage::PreCommit).with_max_parallel_hooks(4); // Normally would allow parallel execution

    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &execution_config,
    )
    .await;

    // Should succeed (basic smoke test)
    assert!(result.is_ok(), "Execution should succeed");
    let exec_result = result.unwrap();
    assert!(exec_result.success, "All hooks should pass");

    // Verify both hooks executed successfully
    assert_eq!(exec_result.hooks_passed.len(), 2);
    assert_eq!(exec_result.hooks_failed.len(), 0);

    // Verify hooks executed
    let hook_ids: Vec<String> = exec_result
        .hooks_passed
        .iter()
        .map(|r| r.hook_id.clone())
        .collect();
    assert!(hook_ids.contains(&"first".to_string()));
    assert!(hook_ids.contains(&"second".to_string()));

    // In a more sophisticated test, we would verify that they executed sequentially
    // rather than in parallel, but that would require timing analysis
}

#[tokio::test]
async fn test_complex_dependency_chain() {
    // Test a more complex dependency chain
    let mut graph = TaskDependencyGraph::new();

    // Create chain: setup -> [lint, format] -> test -> build -> deploy
    let tasks = ["setup", "lint", "format", "test", "build", "deploy"];
    for task in &tasks {
        let config = snp::concurrency::TaskConfig::new(*task);
        graph.add_task(config);
    }

    // Add dependencies
    graph.add_dependency("lint", "setup").unwrap();
    graph.add_dependency("format", "setup").unwrap();
    graph.add_dependency("test", "lint").unwrap();
    graph.add_dependency("test", "format").unwrap();
    graph.add_dependency("build", "test").unwrap();
    graph.add_dependency("deploy", "build").unwrap();

    let execution_order = graph.resolve_execution_order().unwrap();

    // Verify constraints
    let positions: std::collections::HashMap<&str, usize> = execution_order
        .iter()
        .enumerate()
        .map(|(i, task)| (task.as_str(), i))
        .collect();

    // setup should be first
    assert_eq!(positions["setup"], 0);

    // lint and format should come after setup
    assert!(positions["lint"] > positions["setup"]);
    assert!(positions["format"] > positions["setup"]);

    // test should come after both lint and format
    assert!(positions["test"] > positions["lint"]);
    assert!(positions["test"] > positions["format"]);

    // build should come after test
    assert!(positions["build"] > positions["test"]);

    // deploy should come after build (and be last)
    assert!(positions["deploy"] > positions["build"]);
    assert_eq!(positions["deploy"], tasks.len() - 1);
}
