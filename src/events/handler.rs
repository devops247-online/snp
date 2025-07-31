// Event handlers for hook lifecycle management
use crate::error::Result;
use crate::events::{HookEvent, HookEventType};
use async_trait::async_trait;
use std::sync::Arc;

/// Priority levels for event handlers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventHandlerPriority {
    Critical = 4,
    High = 3,
    Normal = 2,
    Low = 1,
    Background = 0,
}

/// Trait for handling hook lifecycle events
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle a hook lifecycle event
    async fn handle_event(&self, event: &HookEvent) -> Result<()>;

    /// Get the list of event types this handler is interested in
    fn interested_events(&self) -> Vec<HookEventType>;

    /// Get the priority of this handler (higher priority handlers execute first)
    fn priority(&self) -> EventHandlerPriority {
        EventHandlerPriority::Normal
    }

    /// Get a unique identifier for this handler
    fn handler_id(&self) -> String {
        format!(
            "{}@{:p}",
            std::any::type_name::<Self>(),
            self as *const Self
        )
    }

    /// Check if this handler should process the given event
    fn should_handle(&self, event: &HookEvent) -> bool {
        self.interested_events().contains(&event.event_type())
    }
}

/// Built-in logging event handler
pub struct LoggingEventHandler {
    logger: Arc<dyn LoggerTrait>,
}

/// Trait for logging to allow for dependency injection and testing
#[async_trait]
pub trait LoggerTrait: Send + Sync {
    async fn info(&self, message: &str);
    async fn warn(&self, message: &str);
    async fn error(&self, message: &str);
    async fn debug(&self, message: &str);
}

/// Default logger implementation using tracing
pub struct TracingLogger;

#[async_trait]
impl LoggerTrait for TracingLogger {
    async fn info(&self, message: &str) {
        tracing::info!("{}", message);
    }

    async fn warn(&self, message: &str) {
        tracing::warn!("{}", message);
    }

    async fn error(&self, message: &str) {
        tracing::error!("{}", message);
    }

    async fn debug(&self, message: &str) {
        tracing::debug!("{}", message);
    }
}

impl LoggingEventHandler {
    /// Create a new logging event handler with default tracing logger
    pub fn new() -> Self {
        Self {
            logger: Arc::new(TracingLogger),
        }
    }

    /// Create a new logging event handler with custom logger
    pub fn with_logger(logger: Arc<dyn LoggerTrait>) -> Self {
        Self { logger }
    }
}

impl Default for LoggingEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for LoggingEventHandler {
    async fn handle_event(&self, event: &HookEvent) -> Result<()> {
        match event {
            HookEvent::HookStarted {
                hook_id,
                execution_id,
                ..
            } => {
                self.logger
                    .info(&format!(
                        "Hook started: {hook_id} (execution: {execution_id})"
                    ))
                    .await;
            }
            HookEvent::HookCompleted {
                hook_id,
                execution_id,
                duration,
                ..
            } => {
                self.logger
                    .info(&format!(
                        "Hook completed: {hook_id} (execution: {execution_id}) in {duration:?}"
                    ))
                    .await;
            }
            HookEvent::HookFailed {
                hook_id,
                execution_id,
                error,
                duration,
                ..
            } => {
                self.logger
                    .debug(&format!(
                        "Hook failed: {hook_id} (execution: {execution_id}) after {duration:?} - {error}"
                    ))
                    .await;
            }
            HookEvent::HookSkipped {
                hook_id,
                execution_id,
                reason,
                ..
            } => {
                self.logger
                    .info(&format!(
                        "Hook skipped: {hook_id} (execution: {execution_id}) - {reason}"
                    ))
                    .await;
            }
            HookEvent::HookProgress {
                hook_id,
                execution_id,
                progress,
                message,
                ..
            } => {
                let msg = if let Some(msg) = message {
                    format!(
                        "Hook progress: {} (execution: {}) - {:.1}% - {}",
                        hook_id,
                        execution_id,
                        progress * 100.0,
                        msg
                    )
                } else {
                    format!(
                        "Hook progress: {} (execution: {}) - {:.1}%",
                        hook_id,
                        execution_id,
                        progress * 100.0
                    )
                };
                self.logger.debug(&msg).await;
            }
            HookEvent::PipelineStarted {
                pipeline_id,
                stage,
                hook_count,
                ..
            } => {
                self.logger
                    .info(&format!(
                        "Pipeline started: {} for stage {} with {} hooks",
                        pipeline_id,
                        stage.as_str(),
                        hook_count
                    ))
                    .await;
            }
            HookEvent::PipelineCompleted {
                pipeline_id,
                stage,
                summary,
                ..
            } => {
                self.logger
                    .info(&format!(
                        "Pipeline completed: {} for stage {} - passed: {}, failed: {}",
                        pipeline_id,
                        stage.as_str(),
                        summary.hooks_passed,
                        summary.hooks_failed
                    ))
                    .await;
            }
            HookEvent::FilesProcessed {
                hook_id,
                execution_id,
                files,
                ..
            } => {
                self.logger
                    .debug(&format!(
                        "Files processed by hook: {} (execution: {}) - {} files",
                        hook_id,
                        execution_id,
                        files.len()
                    ))
                    .await;
            }
            HookEvent::Custom { event_type, .. } => {
                self.logger
                    .debug(&format!("Custom event: {event_type}"))
                    .await;
            }
        }
        Ok(())
    }

    fn interested_events(&self) -> Vec<HookEventType> {
        vec![
            HookEventType::HookStarted,
            HookEventType::HookCompleted,
            HookEventType::HookFailed,
            HookEventType::HookSkipped,
            HookEventType::HookProgress,
            HookEventType::PipelineStarted,
            HookEventType::PipelineCompleted,
            HookEventType::FilesProcessed,
            HookEventType::Custom,
        ]
    }

    fn priority(&self) -> EventHandlerPriority {
        EventHandlerPriority::Normal
    }
}

/// Trait for metrics collection to allow for dependency injection and testing
pub trait MetricsCollectorTrait: Send + Sync {
    fn record_hook_duration(&self, hook_id: &str, duration: std::time::Duration);
    fn record_pipeline_metrics(
        &self,
        stage: &crate::core::Stage,
        summary: &crate::events::event::EventExecutionSummary,
    );
    fn increment_counter(&self, name: &str, value: u64);
    fn set_gauge(&self, name: &str, value: f64);
}

/// Default no-op metrics collector
pub struct NoOpMetricsCollector;

impl MetricsCollectorTrait for NoOpMetricsCollector {
    fn record_hook_duration(&self, _hook_id: &str, _duration: std::time::Duration) {
        // No-op
    }

    fn record_pipeline_metrics(
        &self,
        _stage: &crate::core::Stage,
        _summary: &crate::events::event::EventExecutionSummary,
    ) {
        // No-op
    }

    fn increment_counter(&self, _name: &str, _value: u64) {
        // No-op
    }

    fn set_gauge(&self, _name: &str, _value: f64) {
        // No-op
    }
}

/// Built-in metrics collection event handler
pub struct MetricsEventHandler {
    collector: Arc<dyn MetricsCollectorTrait>,
}

impl MetricsEventHandler {
    /// Create a new metrics event handler with default no-op collector
    pub fn new() -> Self {
        Self {
            collector: Arc::new(NoOpMetricsCollector),
        }
    }

    /// Create a new metrics event handler with custom collector
    pub fn with_collector(collector: Arc<dyn MetricsCollectorTrait>) -> Self {
        Self { collector }
    }
}

impl Default for MetricsEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for MetricsEventHandler {
    async fn handle_event(&self, event: &HookEvent) -> Result<()> {
        match event {
            HookEvent::HookCompleted {
                hook_id, duration, ..
            } => {
                self.collector.record_hook_duration(hook_id, *duration);
                self.collector.increment_counter("hooks_completed", 1);
            }
            HookEvent::HookFailed {
                hook_id, duration, ..
            } => {
                self.collector.record_hook_duration(hook_id, *duration);
                self.collector.increment_counter("hooks_failed", 1);
            }
            HookEvent::HookSkipped { .. } => {
                self.collector.increment_counter("hooks_skipped", 1);
            }
            HookEvent::PipelineCompleted { stage, summary, .. } => {
                self.collector.record_pipeline_metrics(stage, summary);
                self.collector.set_gauge(
                    "pipeline_success_rate",
                    if summary.total_hooks > 0 {
                        summary.hooks_passed as f64 / summary.total_hooks as f64
                    } else {
                        1.0
                    },
                );
            }
            HookEvent::FilesProcessed { files, .. } => {
                self.collector
                    .set_gauge("files_processed", files.len() as f64);
            }
            _ => {
                // Other events don't need metrics collection
            }
        }
        Ok(())
    }

    fn interested_events(&self) -> Vec<HookEventType> {
        vec![
            HookEventType::HookCompleted,
            HookEventType::HookFailed,
            HookEventType::HookSkipped,
            HookEventType::PipelineCompleted,
            HookEventType::FilesProcessed,
        ]
    }

    fn priority(&self) -> EventHandlerPriority {
        EventHandlerPriority::High
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ExecutionContext, Stage};
    use crate::events::{
        event::{EventExecutionSummary, EventHookExecutionResult},
        HookEvent,
    };
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use uuid::Uuid;

    // Test logger that captures log messages
    struct TestLogger {
        messages: Arc<Mutex<Vec<String>>>,
    }

    impl TestLogger {
        fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LoggerTrait for TestLogger {
        async fn info(&self, message: &str) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("INFO: {message}"));
        }

        async fn warn(&self, message: &str) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("WARN: {message}"));
        }

        async fn error(&self, message: &str) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("ERROR: {message}"));
        }

        async fn debug(&self, message: &str) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("DEBUG: {message}"));
        }
    }

    // Test metrics collector that captures metrics
    struct TestMetricsCollector {
        hook_durations: Arc<Mutex<Vec<(String, Duration)>>>,
        counters: Arc<Mutex<std::collections::HashMap<String, u64>>>,
        gauges: Arc<Mutex<std::collections::HashMap<String, f64>>>,
    }

    impl TestMetricsCollector {
        fn new() -> Self {
            Self {
                hook_durations: Arc::new(Mutex::new(Vec::new())),
                counters: Arc::new(Mutex::new(std::collections::HashMap::new())),
                gauges: Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }

        fn get_hook_durations(&self) -> Vec<(String, Duration)> {
            self.hook_durations.lock().unwrap().clone()
        }

        fn get_counter(&self, name: &str) -> u64 {
            self.counters
                .lock()
                .unwrap()
                .get(name)
                .copied()
                .unwrap_or(0)
        }

        pub fn get_gauge(&self, name: &str) -> f64 {
            self.gauges
                .lock()
                .unwrap()
                .get(name)
                .copied()
                .unwrap_or(0.0)
        }
    }

    impl MetricsCollectorTrait for TestMetricsCollector {
        fn record_hook_duration(&self, hook_id: &str, duration: Duration) {
            self.hook_durations
                .lock()
                .unwrap()
                .push((hook_id.to_string(), duration));
        }

        fn record_pipeline_metrics(
            &self,
            _stage: &crate::core::Stage,
            _summary: &EventExecutionSummary,
        ) {
            // Test implementation - could capture more detailed metrics
        }

        fn increment_counter(&self, name: &str, value: u64) {
            let mut counters = self.counters.lock().unwrap();
            *counters.entry(name.to_string()).or_insert(0) += value;
        }

        fn set_gauge(&self, name: &str, value: f64) {
            self.gauges.lock().unwrap().insert(name.to_string(), value);
        }
    }

    #[tokio::test]
    async fn test_logging_event_handler() {
        let test_logger = Arc::new(TestLogger::new());
        let handler = LoggingEventHandler::with_logger(test_logger.clone());

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        handler.handle_event(&event).await.unwrap();

        let messages = test_logger.get_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("Hook started: test-hook"));
    }

    #[tokio::test]
    async fn test_logging_handler_interested_events() {
        let handler = LoggingEventHandler::new();
        let interested = handler.interested_events();

        assert!(interested.contains(&HookEventType::HookStarted));
        assert!(interested.contains(&HookEventType::HookCompleted));
        assert!(interested.contains(&HookEventType::HookFailed));
        assert!(interested.contains(&HookEventType::PipelineStarted));
    }

    #[tokio::test]
    async fn test_metrics_event_handler() {
        let test_collector = Arc::new(TestMetricsCollector::new());
        let handler = MetricsEventHandler::with_collector(test_collector.clone());

        let result = EventHookExecutionResult {
            hook_id: "test-hook".to_string(),
            success: true,
            exit_code: Some(0),
            duration: Duration::from_millis(100),
            files_processed: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
        };

        let event = HookEvent::hook_completed(
            "test-hook".to_string(),
            Uuid::new_v4(),
            result,
            Duration::from_millis(100),
        );

        handler.handle_event(&event).await.unwrap();

        let durations = test_collector.get_hook_durations();
        assert_eq!(durations.len(), 1);
        assert_eq!(durations[0].0, "test-hook");
        assert_eq!(durations[0].1, Duration::from_millis(100));

        assert_eq!(test_collector.get_counter("hooks_completed"), 1);
    }

    #[tokio::test]
    async fn test_pipeline_completed_sets_gauge() {
        let test_collector = Arc::new(TestMetricsCollector::new());
        let handler = MetricsEventHandler::with_collector(test_collector.clone());

        let summary = crate::events::event::EventExecutionSummary {
            total_hooks: 10,
            hooks_passed: 8,
            hooks_failed: 2,
            hooks_skipped: 0,
            total_duration: Duration::from_secs(5),
        };

        let event =
            HookEvent::pipeline_completed(Uuid::new_v4(), crate::core::Stage::PreCommit, summary);

        handler.handle_event(&event).await.unwrap();

        // Verify that the gauge was set correctly using get_gauge
        let success_rate = test_collector.get_gauge("pipeline_success_rate");
        assert_eq!(success_rate, 0.8); // 8/10 = 0.8
    }

    #[tokio::test]
    async fn test_metrics_handler_interested_events() {
        let handler = MetricsEventHandler::new();
        let interested = handler.interested_events();

        assert!(interested.contains(&HookEventType::HookCompleted));
        assert!(interested.contains(&HookEventType::HookFailed));
        assert!(interested.contains(&HookEventType::HookSkipped));
        assert!(!interested.contains(&HookEventType::HookStarted));
    }

    #[tokio::test]
    async fn test_event_handler_priority() {
        let logging_handler = LoggingEventHandler::new();
        let metrics_handler = MetricsEventHandler::new();

        assert_eq!(logging_handler.priority(), EventHandlerPriority::Normal);
        assert_eq!(metrics_handler.priority(), EventHandlerPriority::High);
    }

    #[tokio::test]
    async fn test_handler_should_handle() {
        let handler = LoggingEventHandler::new();

        let hook_started_event = HookEvent::hook_started(
            "test".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        assert!(handler.should_handle(&hook_started_event));
    }

    #[tokio::test]
    async fn test_handler_id_generation() {
        let handler1 = LoggingEventHandler::new();
        let handler2 = LoggingEventHandler::new();

        let id1 = handler1.handler_id();
        let id2 = handler2.handler_id();

        // IDs should be different for different instances
        assert_ne!(id1, id2);

        // IDs should contain the type name
        assert!(id1.contains("LoggingEventHandler"));
        assert!(id2.contains("LoggingEventHandler"));
    }

    #[tokio::test]
    async fn test_multiple_event_types_logging() {
        let test_logger = Arc::new(TestLogger::new());
        let handler = LoggingEventHandler::with_logger(test_logger.clone());

        // Test different event types
        let events = vec![
            HookEvent::hook_started(
                "test".to_string(),
                Uuid::new_v4(),
                &ExecutionContext::new(Stage::PreCommit),
            ),
            HookEvent::hook_skipped("test".to_string(), Uuid::new_v4(), "No files".to_string()),
            HookEvent::pipeline_started(Uuid::new_v4(), Stage::PreCommit, 3),
        ];

        for event in events {
            handler.handle_event(&event).await.unwrap();
        }

        let messages = test_logger.get_messages();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].contains("Hook started"));
        assert!(messages[1].contains("Hook skipped"));
        assert!(messages[2].contains("Pipeline started"));
    }
}
