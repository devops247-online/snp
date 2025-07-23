// Comprehensive tests for hook chaining functionality
// These tests should initially fail to demonstrate TDD Red phase

use snp::hook_chaining::{
    ChainedHook, DependencyResolver, ExecutionCondition, ExecutionStrategy, FailureBehavior,
    FailureStrategy, HookChain, HookChainExecutor,
};
use snp::Hook;
use std::time::Duration;

#[tokio::test]
async fn test_basic_hook_dependencies_execution_order() {
    // Test that hooks execute in correct dependency order
    let hook_build = Hook::new("build", "echo 'Building...'", "system");
    let hook_test = Hook::new("test", "echo 'Testing...'", "system");
    let hook_deploy = Hook::new("deploy", "echo 'Deploying...'", "system");

    let chained_hooks = vec![
        ChainedHook::new(hook_deploy).with_dependencies(vec!["test".to_string()]),
        ChainedHook::new(hook_build), // No dependencies - should run first
        ChainedHook::new(hook_test).with_dependencies(vec!["build".to_string()]),
    ];

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;
    chain.execution_strategy = ExecutionStrategy::Sequential;

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    // Should execute in order: build -> test -> deploy
    assert!(result.success, "Chain execution should succeed");
    assert_eq!(result.executed_hooks.len(), 3);

    // Check execution order
    let execution_order: Vec<String> = result
        .executed_hooks
        .iter()
        .map(|r| r.hook_id.clone())
        .collect();
    assert_eq!(execution_order, vec!["build", "test", "deploy"]);
}

#[tokio::test]
async fn test_circular_dependency_detection_fails() {
    // Test that circular dependencies are properly detected and rejected
    let mut resolver = DependencyResolver::new();

    let hook_a = Hook::new("a", "echo a", "system");
    let hook_b = Hook::new("b", "echo b", "system");
    let hook_c = Hook::new("c", "echo c", "system");

    // Create A->B->C->A circular dependency
    let chained_hooks = vec![
        ChainedHook::new(hook_a).with_dependencies(vec!["c".to_string()]),
        ChainedHook::new(hook_b).with_dependencies(vec!["a".to_string()]),
        ChainedHook::new(hook_c).with_dependencies(vec!["b".to_string()]),
    ];

    let graph = resolver.build_graph(&chained_hooks).unwrap();
    let cycles = resolver.detect_cycles(&graph).unwrap();

    // Should detect exactly one cycle containing all three hooks
    assert!(!cycles.is_empty(), "Should detect circular dependency");
    assert_eq!(cycles.len(), 1, "Should detect exactly one cycle");
    assert_eq!(cycles[0].len(), 3, "Cycle should contain all three hooks");

    // Validation should fail due to circular dependency
    let validation = resolver.validate_dependencies(&graph);
    assert!(
        validation.is_err(),
        "Validation should fail for circular dependencies"
    );
}

#[tokio::test]
async fn test_missing_dependency_error_handling() {
    // Test proper error handling for missing dependencies
    let mut resolver = DependencyResolver::new();

    let hook_test = Hook::new("test", "cargo test", "rust");
    let chained_hooks =
        vec![ChainedHook::new(hook_test).with_dependencies(vec!["nonexistent_hook".to_string()])];

    let graph = resolver.build_graph(&chained_hooks).unwrap();

    // Validation should detect missing dependency
    let validation_result = resolver.validate_dependencies(&graph);
    assert!(
        validation_result.is_err(),
        "Should fail with missing dependency error"
    );

    // Error should contain information about the missing dependency
    let error = validation_result.unwrap_err();
    let error_msg = format!("{error}");
    assert!(
        error_msg.contains("nonexistent_hook"),
        "Error should mention missing hook"
    );
}

#[tokio::test]
async fn test_parallel_execution_optimization() {
    // Test that independent hooks can execute in parallel
    let hook_lint = Hook::new("lint", "echo 'Linting...'", "system");
    let hook_format = Hook::new("format", "echo 'Formatting...'", "system");
    let hook_build = Hook::new("build", "echo 'Building...'", "system");
    let hook_test = Hook::new("test", "echo 'Testing...'", "system");

    let chained_hooks = vec![
        ChainedHook::new(hook_lint),   // Independent
        ChainedHook::new(hook_format), // Independent
        ChainedHook::new(hook_build)
            .with_dependencies(vec!["lint".to_string(), "format".to_string()]),
        ChainedHook::new(hook_test).with_dependencies(vec!["build".to_string()]),
    ];

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;
    chain.execution_strategy = ExecutionStrategy::MaxParallel;
    chain.max_parallel_chains = 2;

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    assert!(result.success);

    // Should have multiple execution phases
    assert!(
        !result.execution_phases.is_empty(),
        "Should have execution phases"
    );

    // First phase should run lint and format in parallel
    let first_phase = &result.execution_phases[0];
    assert_eq!(first_phase.hooks_executed.len(), 2);
    assert!(first_phase.hooks_executed.contains(&"lint".to_string()));
    assert!(first_phase.hooks_executed.contains(&"format".to_string()));

    // Should demonstrate parallel efficiency > 0.5 (running 2 hooks concurrently)
    assert!(
        first_phase.parallel_efficiency > 0.5,
        "Parallel efficiency should be > 0.5 when running hooks concurrently"
    );
}

#[tokio::test]
async fn test_failure_propagation_with_dependencies() {
    // Test that hook failures properly propagate to dependent hooks
    let hook_build = Hook::new("build", "false", "system"); // Will fail
    let hook_test = Hook::new("test", "echo 'Testing...'", "system");
    let hook_deploy = Hook::new("deploy", "echo 'Deploying...'", "system");

    let chained_hooks = vec![
        ChainedHook::new(hook_build).with_failure_behavior(FailureBehavior::Critical),
        ChainedHook::new(hook_test).with_dependencies(vec!["build".to_string()]),
        ChainedHook::new(hook_deploy).with_dependencies(vec!["test".to_string()]),
    ];

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;
    chain.failure_strategy = FailureStrategy::ContinueIndependent;

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    // Chain should fail due to build failure
    assert!(
        !result.success,
        "Chain should fail when critical hook fails"
    );

    // Build should fail, test and deploy should be skipped
    assert_eq!(
        result.executed_hooks.len(),
        1,
        "Only build hook should execute"
    );
    assert_eq!(
        result.skipped_hooks.len(),
        2,
        "Test and deploy should be skipped"
    );
    assert!(result.skipped_hooks.contains(&"test".to_string()));
    assert!(result.skipped_hooks.contains(&"deploy".to_string()));

    // Should track dependency failures
    assert!(
        !result.failed_dependencies.is_empty(),
        "Should track failed dependencies"
    );
    assert!(result.failed_dependencies.contains_key("test"));
    assert!(result.failed_dependencies.contains_key("deploy"));
}

#[tokio::test]
async fn test_conditional_execution_based_on_file_existence() {
    // Test conditional execution based on file existence
    let temp_dir = tempfile::TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "test content").unwrap();

    let hook_conditional = Hook::new("conditional", "echo 'Running conditionally'", "system");
    let chained_hook =
        ChainedHook::new(hook_conditional).with_conditions(vec![ExecutionCondition::FileExists {
            path: test_file.clone(),
        }]);

    let mut chain = HookChain::new();
    chain.hooks = vec![chained_hook];

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    // Hook should execute because file exists
    assert!(result.success);
    assert_eq!(result.executed_hooks.len(), 1);

    // Test with non-existent file
    let nonexistent_file = temp_dir.path().join("nonexistent.txt");
    let hook_conditional2 = Hook::new("conditional2", "echo 'Should not run'", "system");
    let chained_hook2 =
        ChainedHook::new(hook_conditional2).with_conditions(vec![ExecutionCondition::FileExists {
            path: nonexistent_file,
        }]);

    let mut chain2 = HookChain::new();
    chain2.hooks = vec![chained_hook2];

    let result2 = executor.execute_chain(chain2).await.unwrap();

    // Hook should be skipped because file doesn't exist
    assert!(result2.success); // Chain succeeds but hook is skipped
    assert_eq!(result2.executed_hooks.len(), 0, "Hook should be skipped");
    assert_eq!(
        result2.skipped_hooks.len(),
        1,
        "Hook should be in skipped list"
    );
}

#[tokio::test]
async fn test_hook_communication_and_data_sharing() {
    // Test inter-hook communication and data sharing
    let hook_producer = Hook::new("producer", "echo 'data' > /tmp/hook_data.txt", "system");
    let hook_consumer = Hook::new("consumer", "cat /tmp/hook_data.txt", "system");

    let chained_hooks = vec![
        ChainedHook::new(hook_producer),
        ChainedHook::new(hook_consumer).with_dependencies(vec!["producer".to_string()]),
    ];

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    assert!(result.success);

    // Should have communication log entries
    assert!(
        !result.communication_log.is_empty(),
        "Should have communication logs"
    );

    // Consumer hook should have received data from producer
    let consumer_result = result
        .executed_hooks
        .iter()
        .find(|r| r.hook_id == "consumer")
        .expect("Consumer hook should have executed");

    assert!(
        consumer_result.stdout.contains("data"),
        "Consumer should have received data from producer"
    );
}

#[tokio::test]
async fn test_timeout_handling_in_hook_chains() {
    // Test timeout handling at the chain level
    let hook_slow = Hook::new("slow", "sleep 2", "system");
    let hook_fast = Hook::new("fast", "echo 'done'", "system");

    let chained_hooks = vec![
        ChainedHook::new(hook_slow).with_timeout_override(Some(Duration::from_millis(100))),
        ChainedHook::new(hook_fast).with_dependencies(vec!["slow".to_string()]),
    ];

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;
    chain.timeout = Some(Duration::from_secs(1));

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    // Chain should fail due to timeout
    assert!(!result.success, "Chain should fail due to timeout");

    // Slow hook should have timed out
    let slow_result = result.executed_hooks.iter().find(|r| r.hook_id == "slow");

    if let Some(slow_result) = slow_result {
        assert!(!slow_result.success, "Slow hook should have failed");
        // Should have timeout-related error
        assert!(slow_result.error.is_some(), "Should have timeout error");
    }

    // Fast hook should be skipped due to dependency failure
    assert!(result.skipped_hooks.contains(&"fast".to_string()));
}

#[tokio::test]
async fn test_execution_plan_optimization() {
    // Test execution plan creation and optimization
    let hooks = vec![
        ("setup", Vec::<String>::new()),
        ("lint", vec!["setup".to_string()]),
        ("format", vec!["setup".to_string()]),
        ("test", vec!["lint".to_string(), "format".to_string()]),
        ("build", vec!["test".to_string()]),
        ("deploy", vec!["build".to_string()]),
    ];

    let chained_hooks: Vec<ChainedHook> = hooks
        .into_iter()
        .map(|(id, deps)| {
            let hook = Hook::new(id, format!("echo '{id}'"), "system");
            ChainedHook::new(hook).with_dependencies(deps)
        })
        .collect();

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;
    chain.execution_strategy = ExecutionStrategy::TimeOptimized;

    let mut executor = HookChainExecutor::new();

    // Test execution plan creation
    let plan = executor.plan_execution(&chain).unwrap();

    // Should have multiple execution phases for parallel execution
    assert!(
        !plan.execution_phases.is_empty(),
        "Should have execution phases"
    );
    assert!(
        plan.max_parallelism > 1,
        "Should support parallel execution"
    );

    // Critical path should be identified
    assert!(
        !plan.critical_path.is_empty(),
        "Should identify critical path"
    );

    // Should estimate reasonable duration
    assert!(
        plan.estimated_duration > Duration::from_secs(0),
        "Should estimate duration"
    );
}

#[tokio::test]
async fn test_retry_behavior_on_failure() {
    // Test retry behavior for hooks with RetryOnFailure
    let hook_flaky = Hook::new("flaky", "test $RANDOM -gt 16383", "system"); // ~50% failure rate

    let chained_hook =
        ChainedHook::new(hook_flaky).with_failure_behavior(FailureBehavior::RetryOnFailure {
            max_retries: 3,
            delay: Duration::from_millis(10),
        });

    let mut chain = HookChain::new();
    chain.hooks = vec![chained_hook];

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    // Should attempt retries (may succeed or fail, but should show retry attempts)
    // This is hard to test deterministically, so we mainly check structure
    assert_eq!(result.executed_hooks.len(), 1);

    let hook_result = &result.executed_hooks[0];
    // Should have some execution time due to retries and delays
    assert!(hook_result.duration >= Duration::from_millis(5));
}

#[tokio::test]
async fn test_complex_dependency_graph_diamond_pattern() {
    // Test complex diamond dependency pattern: A -> B,C -> D
    let hook_a = Hook::new("setup", "echo 'setup'", "system");
    let hook_b = Hook::new("frontend", "echo 'frontend build'", "system");
    let hook_c = Hook::new("backend", "echo 'backend build'", "system");
    let hook_d = Hook::new("integration", "echo 'integration test'", "system");

    let chained_hooks = vec![
        ChainedHook::new(hook_a),
        ChainedHook::new(hook_b).with_dependencies(vec!["setup".to_string()]),
        ChainedHook::new(hook_c).with_dependencies(vec!["setup".to_string()]),
        ChainedHook::new(hook_d)
            .with_dependencies(vec!["frontend".to_string(), "backend".to_string()]),
    ];

    let mut chain = HookChain::new();
    chain.hooks = chained_hooks;
    chain.execution_strategy = ExecutionStrategy::MaxParallel;

    let mut executor = HookChainExecutor::new();
    let result = executor.execute_chain(chain).await.unwrap();

    assert!(result.success);
    assert_eq!(result.executed_hooks.len(), 4);

    // Should have at least 3 execution phases:
    // Phase 1: setup
    // Phase 2: frontend, backend (parallel)
    // Phase 3: integration
    assert!(
        result.execution_phases.len() >= 3,
        "Should have multiple execution phases"
    );

    // Verify execution order respects dependencies
    let execution_order: Vec<String> = result
        .executed_hooks
        .iter()
        .map(|r| r.hook_id.clone())
        .collect();

    // Setup should be first
    assert_eq!(execution_order[0], "setup");
    // Integration should be last
    assert_eq!(execution_order[3], "integration");
    // Frontend and backend should come before integration
    let frontend_pos = execution_order
        .iter()
        .position(|x| x == "frontend")
        .unwrap();
    let backend_pos = execution_order.iter().position(|x| x == "backend").unwrap();
    let integration_pos = execution_order
        .iter()
        .position(|x| x == "integration")
        .unwrap();
    assert!(frontend_pos < integration_pos);
    assert!(backend_pos < integration_pos);
}
