// Event system integration tests
use async_trait::async_trait;
use snp::core::{ExecutionContext, Stage};
use snp::events::event::{
    EventExecutionSummary, EventHookExecutionError, EventHookExecutionResult,
};
use snp::events::{
    EventBus, EventConfig, EventHandler, EventHandlerPriority, HookEvent, HookEventType,
    LoggingEventHandler, MetricsEventHandler,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
struct TestEventCollector {
    events: Arc<Mutex<Vec<HookEvent>>>,
}

impl TestEventCollector {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_events(&self) -> Vec<HookEvent> {
        self.events.lock().unwrap().clone()
    }

    fn event_count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[async_trait]
impl EventHandler for TestEventCollector {
    async fn handle_event(&self, event: &HookEvent) -> snp::error::Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    fn interested_events(&self) -> Vec<HookEventType> {
        vec![
            HookEventType::HookStarted,
            HookEventType::HookProgress,
            HookEventType::HookCompleted,
            HookEventType::HookFailed,
            HookEventType::HookSkipped,
            HookEventType::PipelineStarted,
            HookEventType::PipelineCompleted,
        ]
    }

    fn priority(&self) -> EventHandlerPriority {
        EventHandlerPriority::High
    }
}

#[tokio::test]
async fn test_event_bus_integration() {
    let mut bus = EventBus::new();
    let collector = Arc::new(TestEventCollector::new());

    bus.register_handler(collector.clone());

    let context = ExecutionContext::new(Stage::PreCommit);
    let hook_id = "test-hook".to_string();
    let execution_id = Uuid::new_v4();

    // Emit hook lifecycle events
    let start_event = HookEvent::hook_started(hook_id.clone(), execution_id, &context);
    bus.emit(start_event).await.unwrap();

    let progress_event = HookEvent::hook_progress(
        hook_id.clone(),
        execution_id,
        0.5,
        Some("Processing files".to_string()),
    );
    bus.emit(progress_event).await.unwrap();

    let result = EventHookExecutionResult {
        hook_id: hook_id.clone(),
        success: true,
        exit_code: Some(0),
        duration: Duration::from_millis(100),
        files_processed: vec![],
        stdout: "Success".to_string(),
        stderr: String::new(),
    };

    let complete_event = HookEvent::hook_completed(
        hook_id.clone(),
        execution_id,
        result,
        Duration::from_millis(100),
    );
    bus.emit(complete_event).await.unwrap();

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    let event_count = collector.event_count();
    let events = collector.get_events();
    println!(
        "Expected 3 events, got {}: {:?}",
        event_count,
        events.iter().map(|e| e.event_type()).collect::<Vec<_>>()
    );
    assert_eq!(event_count, 3);

    let events = collector.get_events();
    assert!(matches!(events[0], HookEvent::HookStarted { .. }));
    assert!(matches!(events[1], HookEvent::HookProgress { .. }));
    assert!(matches!(events[2], HookEvent::HookCompleted { .. }));
}

#[tokio::test]
async fn test_pipeline_events() {
    let mut bus = EventBus::new();
    let collector = Arc::new(TestEventCollector::new());

    bus.register_handler(collector.clone());

    let pipeline_id = Uuid::new_v4();
    let stage = Stage::PreCommit;

    // Pipeline started
    let start_event = HookEvent::pipeline_started(pipeline_id, stage.clone(), 3);
    bus.emit(start_event).await.unwrap();

    // Pipeline completed
    let summary = EventExecutionSummary {
        total_hooks: 3,
        hooks_passed: 2,
        hooks_failed: 1,
        hooks_skipped: 0,
        total_duration: Duration::from_secs(5),
    };

    let complete_event = HookEvent::pipeline_completed(pipeline_id, stage, summary);
    bus.emit(complete_event).await.unwrap();

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(collector.event_count(), 2);

    let events = collector.get_events();
    assert!(matches!(events[0], HookEvent::PipelineStarted { .. }));
    assert!(matches!(events[1], HookEvent::PipelineCompleted { .. }));
}

#[tokio::test]
async fn test_hook_failure_events() {
    let mut bus = EventBus::new();
    let collector = Arc::new(TestEventCollector::new());

    bus.register_handler(collector.clone());

    let hook_id = "failing-hook".to_string();
    let execution_id = Uuid::new_v4();

    let error = EventHookExecutionError {
        hook_id: hook_id.clone(),
        message: "Command failed".to_string(),
        exit_code: Some(1),
    };

    let fail_event =
        HookEvent::hook_failed(hook_id, execution_id, error, Duration::from_millis(50));

    bus.emit(fail_event).await.unwrap();

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(collector.event_count(), 1);

    let events = collector.get_events();
    if let HookEvent::HookFailed { error, .. } = &events[0] {
        assert_eq!(error.message, "Command failed");
        assert_eq!(error.exit_code, Some(1));
    } else {
        panic!("Expected HookFailed event");
    }
}

#[tokio::test]
async fn test_hook_skipped_events() {
    let mut bus = EventBus::new();
    let collector = Arc::new(TestEventCollector::new());

    bus.register_handler(collector.clone());

    let hook_id = "skipped-hook".to_string();
    let execution_id = Uuid::new_v4();

    let skip_event =
        HookEvent::hook_skipped(hook_id, execution_id, "No matching files".to_string());

    bus.emit(skip_event).await.unwrap();

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(collector.event_count(), 1);

    let events = collector.get_events();
    if let HookEvent::HookSkipped { reason, .. } = &events[0] {
        assert_eq!(reason, "No matching files");
    } else {
        panic!("Expected HookSkipped event");
    }
}

#[tokio::test]
async fn test_custom_events() {
    let mut bus = EventBus::new();
    let collector = Arc::new(TestEventCollector::new());

    // Test collector doesn't handle custom events by default
    bus.register_handler(collector.clone());

    let custom_data = serde_json::json!({
        "webhook_url": "https://example.com/webhook",
        "payload": {"status": "completed"}
    });

    let custom_event = HookEvent::custom("webhook-notification".to_string(), custom_data);
    bus.emit(custom_event).await.unwrap();

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Custom events are not handled by our test collector
    assert_eq!(collector.event_count(), 0);
}

#[tokio::test]
async fn test_multiple_handlers() {
    let mut bus = EventBus::new();
    let collector1 = Arc::new(TestEventCollector::new());
    let collector2 = Arc::new(TestEventCollector::new());

    bus.register_handler(collector1.clone());
    bus.register_handler(collector2.clone());

    let context = ExecutionContext::new(Stage::PreCommit);
    let start_event = HookEvent::hook_started("test".to_string(), Uuid::new_v4(), &context);

    bus.emit(start_event).await.unwrap();

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(collector1.event_count(), 1);
    assert_eq!(collector2.event_count(), 1);
}

#[tokio::test]
async fn test_built_in_logging_handler() {
    let mut bus = EventBus::new();
    let logging_handler = Arc::new(LoggingEventHandler::new());

    bus.register_handler(logging_handler);

    let context = ExecutionContext::new(Stage::PreCommit);
    let events = vec![
        HookEvent::hook_started("test-hook".to_string(), Uuid::new_v4(), &context),
        HookEvent::hook_progress(
            "test-hook".to_string(),
            Uuid::new_v4(),
            0.75,
            Some("Almost done".to_string()),
        ),
        HookEvent::hook_skipped(
            "test-hook".to_string(),
            Uuid::new_v4(),
            "No files".to_string(),
        ),
    ];

    for event in events {
        bus.emit(event).await.unwrap();
    }

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Test passes if no panics occur - logging output is checked in unit tests
}

#[tokio::test]
async fn test_built_in_metrics_handler() {
    let mut bus = EventBus::new();
    let metrics_handler = Arc::new(MetricsEventHandler::new());

    bus.register_handler(metrics_handler);

    let result = EventHookExecutionResult {
        hook_id: "test-hook".to_string(),
        success: true,
        exit_code: Some(0),
        duration: Duration::from_millis(100),
        files_processed: vec![],
        stdout: String::new(),
        stderr: String::new(),
    };

    let events = vec![
        HookEvent::hook_completed(
            "test-hook".to_string(),
            Uuid::new_v4(),
            result,
            Duration::from_millis(100),
        ),
        HookEvent::hook_skipped(
            "skipped-hook".to_string(),
            Uuid::new_v4(),
            "No files".to_string(),
        ),
    ];

    for event in events {
        bus.emit(event).await.unwrap();
    }

    // Give handlers time to process
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Test passes if no panics occur - metrics collection is checked in unit tests
}

#[tokio::test]
async fn test_event_bus_configuration() {
    let config = EventConfig {
        enabled: false,
        channel_capacity: 500,
        enable_persistence: false,
        max_handler_timeout: Duration::from_secs(10),
        retry_failed_handlers: false,
        emit_progress_events: false,
    };

    let mut bus = EventBus::with_config(config);
    let collector = Arc::new(TestEventCollector::new());

    bus.register_handler(collector.clone());

    let context = ExecutionContext::new(Stage::PreCommit);
    let event = HookEvent::hook_started("test".to_string(), Uuid::new_v4(), &context);

    // Bus is disabled, so events should not be processed
    bus.emit(event).await.unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(collector.event_count(), 0);

    // Enable the bus
    bus.set_enabled(true);

    let event2 = HookEvent::hook_started("test2".to_string(), Uuid::new_v4(), &context);
    bus.emit(event2).await.unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(collector.event_count(), 1);
}

#[tokio::test]
async fn test_event_subscription() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();

    let context = ExecutionContext::new(Stage::PreCommit);
    let event = HookEvent::hook_started("test".to_string(), Uuid::new_v4(), &context);

    // Emit event and check subscription
    bus.emit(event.clone()).await.unwrap();

    let received_event = receiver.recv().await.unwrap();
    assert_eq!(received_event.hook_id(), event.hook_id());
    assert_eq!(received_event.event_type(), event.event_type());
}

#[tokio::test]
async fn test_event_metrics_collection() {
    let bus = EventBus::new();

    let context = ExecutionContext::new(Stage::PreCommit);
    let events = vec![
        HookEvent::hook_started("hook1".to_string(), Uuid::new_v4(), &context),
        HookEvent::hook_started("hook2".to_string(), Uuid::new_v4(), &context),
        HookEvent::hook_skipped("hook3".to_string(), Uuid::new_v4(), "No files".to_string()),
    ];

    for event in events {
        bus.emit(event).await.unwrap();
    }

    let metrics = bus.metrics().await;
    assert_eq!(metrics.total_events(), 3);
    assert_eq!(metrics.events_of_type(&HookEventType::HookStarted), 2);
    assert_eq!(metrics.events_of_type(&HookEventType::HookSkipped), 1);
}

#[tokio::test]
async fn test_event_serialization_roundtrip() {
    let context = ExecutionContext::new(Stage::PreCommit);
    let original_event = HookEvent::hook_started("test-hook".to_string(), Uuid::new_v4(), &context);

    // Serialize to JSON
    let json = serde_json::to_string(&original_event).unwrap();

    // Deserialize back
    let deserialized_event: HookEvent = serde_json::from_str(&json).unwrap();

    // Verify key properties are preserved
    assert_eq!(original_event.hook_id(), deserialized_event.hook_id());
    assert_eq!(original_event.event_type(), deserialized_event.event_type());
    assert_eq!(
        original_event.execution_id(),
        deserialized_event.execution_id()
    );
}
