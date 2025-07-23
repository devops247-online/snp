/// TDD tests for logging system implementation
use snp::logging::{utils, ColorConfig, LogConfig, LogFormat};
use std::path::PathBuf;
use tracing::{debug, error, info, warn, Level};

#[test]
fn test_log_level_filtering() {
    // Test that log levels are filtered correctly
    let config = LogConfig {
        level: Level::WARN,
        format: LogFormat::Pretty,
        color: ColorConfig::Never,
        show_targets: false,
        show_timestamps: false,
    };

    // This is mainly testing that the configuration is created correctly
    // since actual log filtering requires runtime setup
    assert_eq!(config.level, Level::WARN);
}

#[test]
fn test_verbose_log_level() {
    let config = LogConfig::from_cli(true, false, None);
    assert_eq!(config.level, Level::DEBUG);
}

#[test]
fn test_quiet_log_level() {
    let config = LogConfig::from_cli(false, true, None);
    assert_eq!(config.level, Level::ERROR);
}

#[test]
fn test_normal_log_level() {
    let config = LogConfig::from_cli(false, false, None);
    assert_eq!(config.level, Level::INFO);
}

#[test]
fn test_structured_logging_config() {
    // Test structured fields are properly configured
    let config = LogConfig {
        level: Level::INFO,
        format: LogFormat::Json,
        color: ColorConfig::Never,
        show_targets: true,
        show_timestamps: true,
    };

    assert_eq!(config.format, LogFormat::Json);
    assert!(config.show_targets);
    assert!(config.show_timestamps);
}

#[test]
fn test_log_output_formats() {
    // Test different output formats can be configured
    let pretty_config = LogConfig {
        level: Level::INFO,
        format: LogFormat::Pretty,
        color: ColorConfig::Auto,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(pretty_config.format, LogFormat::Pretty);

    let json_config = LogConfig {
        level: Level::INFO,
        format: LogFormat::Json,
        color: ColorConfig::Never,
        show_targets: true,
        show_timestamps: true,
    };
    assert_eq!(json_config.format, LogFormat::Json);

    let compact_config = LogConfig {
        level: Level::INFO,
        format: LogFormat::Compact,
        color: ColorConfig::Auto,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(compact_config.format, LogFormat::Compact);
}

#[test]
fn test_color_configuration() {
    // Test color configuration options
    let always_config = LogConfig::from_cli(false, false, Some("always".to_string()));
    assert_eq!(always_config.color, ColorConfig::Always);
    assert!(always_config.should_use_colors());

    let never_config = LogConfig::from_cli(false, false, Some("never".to_string()));
    assert_eq!(never_config.color, ColorConfig::Never);
    assert!(!never_config.should_use_colors());

    let auto_config = LogConfig::from_cli(false, false, Some("auto".to_string()));
    assert_eq!(auto_config.color, ColorConfig::Auto);

    // Invalid color should default to auto
    let invalid_config = LogConfig::from_cli(false, false, Some("invalid".to_string()));
    assert_eq!(invalid_config.color, ColorConfig::Auto);
}

#[test]
fn test_environment_color_detection() {
    let config = LogConfig {
        level: Level::INFO,
        format: LogFormat::Pretty,
        color: ColorConfig::Auto,
        show_targets: false,
        show_timestamps: false,
    };

    // This test is environment-dependent, so we just verify the method doesn't panic
    let _uses_colors = config.should_use_colors();
}

#[test]
fn test_logging_utility_functions() {
    // Test that logging utility functions don't panic
    let files = [PathBuf::from("test.py"), PathBuf::from("test.rs")];

    // These should not panic and should create valid spans
    let _hook_span = utils::hook_execution_span("test-hook", files.len());
    let _config_span = utils::config_loading_span(&PathBuf::from("config.yaml"));
    let _git_span = utils::git_operation_span("clone", Some("https://github.com/test/repo"));
}

#[test]
fn test_hook_logging_utilities() {
    let files = vec![PathBuf::from("test.py")];

    // These functions should not panic
    utils::log_hook_start("test-hook", &files);
    utils::log_hook_completion("test-hook", true, 150);
    utils::log_hook_completion("test-hook", false, 200);
}

#[test]
fn test_config_validation_logging() {
    let config_path = PathBuf::from(".pre-commit-config.yaml");

    // These should not panic
    utils::log_config_validation(&config_path, true);
    utils::log_config_validation(&config_path, false);
}

#[test]
fn test_warning_logging() {
    // Test warning logging with context
    utils::log_warning_with_context("Test warning message", "test_context");
}

#[test]
fn test_repository_operation_logging() {
    // Test repository operation logging
    utils::log_repository_operation("clone", "https://github.com/test/repo", true);
    utils::log_repository_operation("update", "https://github.com/test/repo", false);
}

#[test]
fn test_span_creation() {
    // Test that spans can be created and entered
    let span = utils::hook_execution_span("test-hook", 5);
    let _enter = span.enter();

    info!("Test message within span");
}

#[test]
fn test_structured_logging_fields() {
    // Test that structured logging fields are properly set
    let span = utils::git_operation_span("fetch", Some("origin"));
    let _enter = span.enter();

    info!(
        operation = "fetch",
        repository = "origin",
        "Git operation started"
    );
    debug!(file_count = 10, "Processing files");
    warn!(hook_id = "test", "Hook execution took longer than expected");
    error!(error_code = 1, "Hook execution failed");
}

#[test]
fn test_performance_impact() {
    // Basic performance test to ensure logging doesn't have significant overhead
    use std::time::Instant;

    let start = Instant::now();

    // Simulate logging in a loop
    for i in 0..1000 {
        let span = utils::hook_execution_span(&format!("hook-{i}"), 1);
        let _enter = span.enter();
        debug!("Processing iteration {}", i);
    }

    let duration = start.elapsed();

    // Logging 1000 operations should complete quickly (under 100ms is reasonable)
    assert!(
        duration.as_millis() < 100,
        "Logging performance test failed: took {}ms",
        duration.as_millis()
    );
}

#[test]
fn test_nested_spans() {
    // Test nested span creation and context propagation
    let outer_span = utils::config_loading_span(&PathBuf::from("config.yaml"));
    let _outer_enter = outer_span.enter();

    info!("Loading configuration");

    let inner_span = utils::git_operation_span("validate", None);
    let _inner_enter = inner_span.enter();

    debug!("Validating git repository");

    // Both spans should be active and context should propagate
}

#[test]
fn test_log_level_hierarchy() {
    // Test that log levels work in the expected hierarchy
    let trace_config = LogConfig {
        level: Level::TRACE,
        format: LogFormat::Pretty,
        color: ColorConfig::Never,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(trace_config.level, Level::TRACE);

    let debug_config = LogConfig {
        level: Level::DEBUG,
        format: LogFormat::Pretty,
        color: ColorConfig::Never,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(debug_config.level, Level::DEBUG);

    let info_config = LogConfig {
        level: Level::INFO,
        format: LogFormat::Pretty,
        color: ColorConfig::Never,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(info_config.level, Level::INFO);

    let warn_config = LogConfig {
        level: Level::WARN,
        format: LogFormat::Pretty,
        color: ColorConfig::Never,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(warn_config.level, Level::WARN);

    let error_config = LogConfig {
        level: Level::ERROR,
        format: LogFormat::Pretty,
        color: ColorConfig::Never,
        show_targets: false,
        show_timestamps: false,
    };
    assert_eq!(error_config.level, Level::ERROR);
}

#[test]
fn test_default_configuration() {
    let default_config = LogConfig::default();

    assert_eq!(default_config.level, Level::INFO);
    assert_eq!(default_config.format, LogFormat::Pretty);
    assert_eq!(default_config.color, ColorConfig::Auto);
    assert!(!default_config.show_targets);
    assert!(!default_config.show_timestamps);
}

#[test]
fn test_cli_configuration_combinations() {
    // Test various CLI flag combinations
    let verbose_always = LogConfig::from_cli(true, false, Some("always".to_string()));
    assert_eq!(verbose_always.level, Level::DEBUG);
    assert_eq!(verbose_always.color, ColorConfig::Always);

    let quiet_never = LogConfig::from_cli(false, true, Some("never".to_string()));
    assert_eq!(quiet_never.level, Level::ERROR);
    assert_eq!(quiet_never.color, ColorConfig::Never);

    // Quiet should override verbose (quiet wins)
    let quiet_verbose = LogConfig::from_cli(true, true, None);
    assert_eq!(quiet_verbose.level, Level::ERROR);
}

#[test]
fn test_async_context_propagation() {
    // Test that logging works in async contexts
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let span = utils::hook_execution_span("async-hook", 3);
        let _enter = span.enter();

        info!("Starting async operation");

        // Simulate async work
        tokio::task::yield_now().await;

        debug!("Async operation completed");
    });
}

#[test]
fn test_concurrent_logging() {
    // Test that concurrent logging doesn't cause issues
    use std::thread;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                let span = utils::hook_execution_span(&format!("concurrent-hook-{i}"), 1);
                let _enter = span.enter();

                for j in 0..10 {
                    debug!("Thread {} iteration {}", i, j);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_error_integration_logging() {
    // Test integration with error handling
    use snp::error::{ConfigError, SnpError};
    use std::path::PathBuf;

    let error = SnpError::Config(Box::new(ConfigError::NotFound {
        path: PathBuf::from("missing.yaml"),
        suggestion: Some("Create the configuration file".to_string()),
    }));

    // Log the error
    error!("Configuration error occurred: {}", error);
    warn!("Error details: {:?}", error);
}
