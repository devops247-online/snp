// Event configuration integration tests
use snp::config::Config;
use snp::events::EventConfig;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_event_config_parsing() {
    let config_yaml = r#"
events:
  enabled: false
  channel_capacity: 500
  enable_persistence: true
  max_handler_timeout: 30
  retry_failed_handlers: false
  emit_progress_events: false

repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.4.0
    hooks:
      - id: trailing-whitespace
"#;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");
    fs::write(&config_path, config_yaml).unwrap();

    let config = Config::from_file(&config_path).unwrap();

    assert!(config.events.is_some());
    let event_config = config.events.unwrap();

    assert!(!event_config.enabled);
    assert_eq!(event_config.channel_capacity, 500);
    assert!(event_config.enable_persistence);
    assert_eq!(event_config.max_handler_timeout, Duration::from_secs(30));
    assert!(!event_config.retry_failed_handlers);
    assert!(!event_config.emit_progress_events);
}

#[tokio::test]
async fn test_default_event_config() {
    let config_yaml = r#"
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.4.0
    hooks:
      - id: trailing-whitespace
"#;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");
    fs::write(&config_path, config_yaml).unwrap();

    let config = Config::from_file(&config_path).unwrap();

    // When no events section is present, it should be None
    assert!(config.events.is_none());

    // But unwrap_or_default should provide sensible defaults
    let event_config = config.events.unwrap_or_default();

    assert!(event_config.enabled);
    assert_eq!(event_config.channel_capacity, 1000);
    assert!(!event_config.enable_persistence);
    assert_eq!(event_config.max_handler_timeout, Duration::from_secs(5));
    assert!(event_config.retry_failed_handlers);
    assert!(event_config.emit_progress_events);
}

#[tokio::test]
async fn test_event_config_serialization() {
    let event_config = EventConfig {
        enabled: true,
        channel_capacity: 2000,
        enable_persistence: false,
        max_handler_timeout: Duration::from_secs(15),
        retry_failed_handlers: true,
        emit_progress_events: false,
    };

    // Test serialization
    let serialized = serde_yaml::to_string(&event_config).unwrap();
    assert!(serialized.contains("enabled: true"));
    assert!(serialized.contains("channel_capacity: 2000"));
    assert!(serialized.contains("max_handler_timeout: 15"));

    // Test deserialization
    let deserialized: EventConfig = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.enabled, event_config.enabled);
    assert_eq!(deserialized.channel_capacity, event_config.channel_capacity);
    assert_eq!(
        deserialized.max_handler_timeout,
        event_config.max_handler_timeout
    );
    assert_eq!(
        deserialized.retry_failed_handlers,
        event_config.retry_failed_handlers
    );
    assert_eq!(
        deserialized.emit_progress_events,
        event_config.emit_progress_events
    );
}

#[tokio::test]
async fn test_partial_event_config() {
    let config_yaml = r#"
events:
  enabled: false
  channel_capacity: 1500

repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.4.0
    hooks:
      - id: trailing-whitespace
"#;

    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");
    fs::write(&config_path, config_yaml).unwrap();

    let config = Config::from_file(&config_path).unwrap();

    assert!(config.events.is_some());
    let event_config = config.events.unwrap();

    // Specified values should be used
    assert!(!event_config.enabled);
    assert_eq!(event_config.channel_capacity, 1500);

    // Unspecified values should use defaults
    assert!(!event_config.enable_persistence); // default
    assert_eq!(event_config.max_handler_timeout, Duration::from_secs(5)); // default
    assert!(event_config.retry_failed_handlers); // default
    assert!(event_config.emit_progress_events); // default
}
