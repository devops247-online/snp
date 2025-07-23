// Comprehensive process management tests following TDD Red Phase
use snp::process::{
    BufferedOutputHandler, OutputHandler, ProcessConfig, ProcessEnvironment, ProcessManager,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

// Basic process execution tests
#[test]
fn test_process_execution_basic() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test successful command execution
    let config = ProcessConfig::new("echo").with_args(vec!["hello", "world"]);

    let result = manager.execute(config);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        "hello world"
    );
}

#[test]
fn test_process_execution_failure() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test command that fails
    let config = ProcessConfig::new("false"); // Always exits with 1

    let result = manager.execute(config);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(!result.success());
    assert_eq!(result.exit_code(), Some(1));
}

#[test]
fn test_process_execution_command_not_found() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test command that doesn't exist
    let config = ProcessConfig::new("this_command_does_not_exist_12345");

    let result = manager.execute(config);
    // Should return an error or a ProcessResult indicating command not found
    // This depends on the implementation - either way is valid
    match result {
        Ok(result) => {
            // If it returns a result, it should indicate failure
            assert!(!result.success());
        }
        Err(_) => {
            // If it returns an error, that's also acceptable
        }
    }
}

#[test]
fn test_process_execution_with_arguments() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test command with multiple arguments
    let config = ProcessConfig::new("echo").with_args(vec!["-n", "no", "newline"]);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(String::from_utf8_lossy(&result.stdout), "no newline");
}

#[test]
fn test_process_execution_working_directory() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));
    let temp_dir = TempDir::new().unwrap();

    // Create a file in the temp directory
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "test content").unwrap();

    // Test command in specific working directory
    let config = ProcessConfig::new("ls").with_working_dir(temp_dir.path().to_path_buf());

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("test.txt"));
}

#[test]
fn test_process_execution_environment_variables() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "echo $TEST_VAR"])
        .with_environment(env)
        .with_inherit_env(false);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "test_value");
}

// Timeout handling tests
#[test]
fn test_process_timeout_handling() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test command that times out
    let config = ProcessConfig::new("sleep")
        .with_args(vec!["5"])
        .with_timeout(Duration::from_millis(100));

    let start = std::time::Instant::now();
    let result = manager.execute(config);
    let elapsed = start.elapsed();

    // Should timeout within reasonable time
    assert!(elapsed < Duration::from_secs(1));

    match result {
        Ok(result) => {
            // If it returns a result, it should indicate timeout
            assert!(result.timed_out);
        }
        Err(_) => {
            // If it returns an error, that's also acceptable for timeout
        }
    }
}

#[test]
fn test_process_timeout_enforcement() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test that timeout is properly enforced
    let config = ProcessConfig::new("sleep")
        .with_args(vec!["2"])
        .with_timeout(Duration::from_millis(500));

    let start = std::time::Instant::now();
    let _result = manager.execute(config);
    let elapsed = start.elapsed();

    // Should not take longer than timeout + reasonable margin
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn test_process_no_timeout() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test command that should complete normally without timeout
    let config = ProcessConfig::new("echo")
        .with_args(vec!["hello"])
        .with_timeout(Duration::from_secs(10));

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert!(!result.timed_out);
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "hello");
}

// Output capture tests
#[test]
fn test_process_output_capture_stdout() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new("echo")
        .with_args(vec!["stdout test"])
        .with_capture_output(true);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        "stdout test"
    );
}

#[test]
fn test_process_output_capture_stderr() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "echo 'stderr test' >&2"])
        .with_capture_output(true);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr).trim(),
        "stderr test"
    );
}

#[test]
fn test_process_output_capture_both() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "echo 'stdout'; echo 'stderr' >&2"])
        .with_capture_output(true);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "stdout");
    assert_eq!(String::from_utf8_lossy(&result.stderr).trim(), "stderr");
}

#[test]
fn test_process_output_large() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Generate large output to test buffering
    let config = ProcessConfig::new("sh")
        .with_args(vec![
            "-c",
            "for i in $(seq 1 1000); do echo \"Line $i with some text to make it longer\"; done",
        ])
        .with_capture_output(true);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert!(result.stdout.len() > 10000); // Should be a large output

    let output_str = String::from_utf8_lossy(&result.stdout);
    assert!(output_str.contains("Line 1"));
    assert!(output_str.contains("Line 1000"));
}

#[test]
fn test_process_output_streaming_callback() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new("echo")
        .with_args(vec!["streaming test"])
        .with_capture_output(true);

    let mut callback_data = Vec::new();
    let result = manager.execute_with_callback(config, |data| {
        callback_data.extend_from_slice(data);
        Ok(())
    });

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&callback_data).trim(),
        "streaming test"
    );
}

// Signal handling and graceful shutdown tests
#[test]
fn test_process_signal_handling_termination() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test process that can be terminated
    let config = ProcessConfig::new("sleep")
        .with_args(vec!["10"])
        .with_timeout(Duration::from_millis(200));

    let result = manager.execute(config);

    // Should be terminated/timeout
    match result {
        Ok(result) => {
            assert!(result.timed_out || !result.success());
        }
        Err(_) => {
            // Error is also acceptable for terminated processes
        }
    }
}

#[test]
fn test_process_signal_handling_interrupt() {
    // This test would require more complex signal handling
    // For now, we'll test that the process manager can handle interrupted processes
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test with a command that might be interrupted
    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "trap 'exit 130' INT; sleep 1"])
        .with_timeout(Duration::from_millis(100));

    let result = manager.execute(config);

    // Should handle the signal gracefully
    match result {
        Ok(result) => {
            // Exit code 130 typically indicates SIGINT
            assert!(result.timed_out || result.exit_code() == Some(130) || !result.success());
        }
        Err(_) => {
            // Error handling is also acceptable
        }
    }
}

#[test]
fn test_process_signal_propagation() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Test that signals are properly propagated to child processes
    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "sh -c 'sleep 10' &; wait"])
        .with_timeout(Duration::from_millis(200));

    let start = std::time::Instant::now();
    let _result = manager.execute(config);
    let elapsed = start.elapsed();

    // Should terminate quickly due to timeout
    assert!(elapsed < Duration::from_secs(1));
}

// Environment isolation and management tests
#[test]
fn test_process_environment_isolation() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Set a unique environment variable for this process
    std::env::set_var("TEST_ISOLATION_VAR", "parent_value");

    let mut env = HashMap::new();
    env.insert("TEST_ISOLATION_VAR".to_string(), "child_value".to_string());

    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "echo $TEST_ISOLATION_VAR"])
        .with_environment(env)
        .with_inherit_env(false);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        "child_value"
    );

    // Parent environment should be unchanged
    assert_eq!(std::env::var("TEST_ISOLATION_VAR").unwrap(), "parent_value");

    // Clean up
    std::env::remove_var("TEST_ISOLATION_VAR");
}

#[test]
fn test_process_environment_inheritance() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Set a test variable in parent
    std::env::set_var("TEST_INHERIT_VAR", "inherited_value");

    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "echo $TEST_INHERIT_VAR"])
        .with_inherit_env(true);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        "inherited_value"
    );

    // Clean up
    std::env::remove_var("TEST_INHERIT_VAR");
}

#[test]
fn test_process_environment_path_setup() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let mut env = HashMap::new();
    env.insert("PATH".to_string(), "/custom/bin:/usr/bin".to_string());

    let config = ProcessConfig::new("sh")
        .with_args(vec!["-c", "echo $PATH"])
        .with_environment(env)
        .with_inherit_env(false);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        "/custom/bin:/usr/bin"
    );
}

#[test]
fn test_process_environment_builder() {
    let mut env_builder = ProcessEnvironment::new();
    env_builder
        .set_var("TEST_VAR", "test_value")
        .add_to_path(PathBuf::from("/custom/path"))
        .set_language_env("python", PathBuf::from("/python/env"));

    let env = env_builder.build();
    assert_eq!(env.get("TEST_VAR"), Some(&"test_value".to_string()));
    assert!(env.get("PATH").unwrap().contains("/custom/path"));
}

#[test]
fn test_process_environment_clean() {
    let env = ProcessEnvironment::clean().build();
    // Clean environment should not inherit system PATH
    assert!(!env.contains_key("PATH") || env.get("PATH") == Some(&String::new()));
}

#[test]
fn test_process_environment_inherit_system() {
    let env = ProcessEnvironment::inherit_system().build();
    // Should inherit system PATH
    assert!(env.contains_key("PATH"));
    // Should have multiple environment variables
    assert!(env.len() > 5);
}

// Parallel process execution tests
#[test]
fn test_parallel_process_execution() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let configs = vec![
        ProcessConfig::new("echo").with_args(vec!["test1"]),
        ProcessConfig::new("echo").with_args(vec!["test2"]),
        ProcessConfig::new("echo").with_args(vec!["test3"]),
    ];

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let results = runtime.block_on(manager.execute_parallel(configs)).unwrap();

    assert_eq!(results.len(), 3);
    for result in &results {
        assert!(result.success());
    }

    let outputs: Vec<String> = results
        .iter()
        .map(|r| String::from_utf8_lossy(&r.stdout).trim().to_string())
        .collect();

    assert!(outputs.contains(&"test1".to_string()));
    assert!(outputs.contains(&"test2".to_string()));
    assert!(outputs.contains(&"test3".to_string()));
}

#[test]
fn test_parallel_process_execution_with_failures() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let configs = vec![
        ProcessConfig::new("echo").with_args(vec!["success"]),
        ProcessConfig::new("false"), // This will fail
        ProcessConfig::new("echo").with_args(vec!["success2"]),
    ];

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let results = runtime.block_on(manager.execute_parallel(configs)).unwrap();

    assert_eq!(results.len(), 3);
    assert!(results[0].success());
    assert!(!results[1].success());
    assert!(results[2].success());
}

#[test]
fn test_parallel_process_resource_limits() {
    let manager = ProcessManager::with_config(2, Duration::from_secs(30)); // Limit to 2 concurrent

    let configs = vec![
        ProcessConfig::new("sleep").with_args(vec!["0.1"]),
        ProcessConfig::new("sleep").with_args(vec!["0.1"]),
        ProcessConfig::new("sleep").with_args(vec!["0.1"]),
        ProcessConfig::new("sleep").with_args(vec!["0.1"]),
    ];

    let start = std::time::Instant::now();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let results = runtime.block_on(manager.execute_parallel(configs)).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 4);
    for result in &results {
        assert!(result.success());
    }

    // With 2 concurrent processes and 4 tasks of 0.1s each,
    // it should take at least 0.2s (2 batches)
    assert!(elapsed >= Duration::from_millis(150));
}

#[test]
fn test_parallel_process_error_aggregation() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let configs = vec![
        ProcessConfig::new("sh").with_args(vec!["-c", "exit 1"]),
        ProcessConfig::new("sh").with_args(vec!["-c", "exit 2"]),
        ProcessConfig::new("echo").with_args(vec!["success"]),
    ];

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let results = runtime.block_on(manager.execute_parallel(configs)).unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].exit_code(), Some(1));
    assert_eq!(results[1].exit_code(), Some(2));
    assert!(results[2].success());
}

// Output handler tests
#[test]
fn test_buffered_output_handler() {
    let mut handler = BufferedOutputHandler::new();

    handler.handle_stdout(b"stdout data").unwrap();
    handler.handle_stderr(b"stderr data").unwrap();

    assert_eq!(handler.stdout, b"stdout data");
    assert_eq!(handler.stderr, b"stderr data");
}

#[test]
fn test_buffered_output_handler_multiple_writes() {
    let mut handler = BufferedOutputHandler::new();

    handler.handle_stdout(b"part1").unwrap();
    handler.handle_stdout(b"part2").unwrap();
    handler.handle_stderr(b"error1").unwrap();
    handler.handle_stderr(b"error2").unwrap();

    assert_eq!(handler.stdout, b"part1part2");
    assert_eq!(handler.stderr, b"error1error2");
}

// Process manager configuration tests
#[test]
fn test_process_manager_creation() {
    let _manager = ProcessManager::with_config(8, Duration::from_secs(60));
    // Just test that it can be created without panicking
    // The internal fields are private, so we can't test them directly
}

#[test]
fn test_process_manager_terminate_all() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // This should not panic even if no processes are running
    let result = manager.terminate_all();
    // Implementation will determine if this is Ok or Err
    if result.is_ok() {
        // Both outcomes are acceptable for now
    }
}

// Process result tests
#[test]
fn test_process_result_success_methods() {
    // We can't easily create a ProcessResult without running a process
    // These tests will be meaningful once the implementation is complete

    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new("true"); // Always succeeds
    let result = manager.execute(config).unwrap();

    assert!(result.success());
    assert_eq!(result.exit_code(), Some(0));
    assert!(!result.timed_out);
}

#[test]
fn test_process_result_failure_methods() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new("false"); // Always fails
    let result = manager.execute(config).unwrap();

    assert!(!result.success());
    assert_eq!(result.exit_code(), Some(1));
    assert!(!result.timed_out);
}

// Additional edge case tests
#[test]
fn test_process_execution_empty_command() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    let config = ProcessConfig::new(""); // Empty command
    let result = manager.execute(config);

    // Should handle empty command gracefully (likely with an error)
    assert!(result.is_err());
}

#[test]
fn test_process_execution_very_long_output() {
    let manager = ProcessManager::with_config(4, Duration::from_secs(30));

    // Generate very long output (1MB+)
    let config = ProcessConfig::new("sh")
        .with_args(vec![
            "-c",
            "dd if=/dev/zero bs=1024 count=1024 2>/dev/null | tr '\\0' 'a'",
        ])
        .with_capture_output(true);

    let result = manager.execute(config).unwrap();
    assert!(result.success());
    assert!(result.stdout.len() > 1000000); // Should be > 1MB
}

#[test]
fn test_process_config_defaults() {
    let config = ProcessConfig::new("test");

    assert_eq!(config.command, "test");
    assert!(config.args.is_empty());
    assert!(config.working_dir.is_none());
    assert!(config.environment.is_empty());
    assert!(config.timeout.is_none());
    assert!(config.capture_output);
    assert!(config.inherit_env);
}
