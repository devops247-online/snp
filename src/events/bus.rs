// Event bus for broadcasting and handling hook lifecycle events
use crate::error::Result;
use crate::events::handler::EventHandler;
use crate::events::metrics::EventMetrics;
use crate::events::{HookEvent, HookEventType};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};

/// Configuration for event bus behavior
#[derive(Debug, Clone)]
pub struct EventConfig {
    pub enabled: bool,                 // Default: true
    pub channel_capacity: usize,       // Default: 1000
    pub enable_persistence: bool,      // Default: false
    pub max_handler_timeout: Duration, // Default: 5s
    pub retry_failed_handlers: bool,   // Default: true
    pub emit_progress_events: bool,    // Default: true
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            channel_capacity: 1000,
            enable_persistence: false,
            max_handler_timeout: Duration::from_secs(5),
            retry_failed_handlers: true,
            emit_progress_events: true,
        }
    }
}

/// Event bus for managing event distribution and handler registration
pub struct EventBus {
    config: EventConfig,
    handlers: Vec<Arc<dyn EventHandler>>,
    channel: broadcast::Sender<HookEvent>,
    metrics: Arc<Mutex<EventMetrics>>,
}

impl EventBus {
    /// Create a new event bus with default configuration
    pub fn new() -> Self {
        Self::with_config(EventConfig::default())
    }

    /// Create a new event bus with custom configuration
    pub fn with_config(config: EventConfig) -> Self {
        let (sender, _) = broadcast::channel(config.channel_capacity);

        Self {
            config,
            handlers: Vec::new(),
            channel: sender,
            metrics: Arc::new(Mutex::new(EventMetrics::new())),
        }
    }

    /// Emit an event to all registered handlers and subscribers
    pub async fn emit(&self, event: HookEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Update metrics
        self.metrics.lock().await.record_event(&event);

        // Broadcast to channel subscribers
        if let Err(e) = self.channel.send(event.clone()) {
            tracing::warn!("Failed to broadcast event to channel: {}", e);
        }

        // Call registered handlers
        for handler in &self.handlers {
            if handler.interested_events().contains(&event.event_type()) {
                let handler_clone = Arc::clone(handler);
                let event_clone = event.clone();
                let timeout = self.config.max_handler_timeout;

                // Execute handler with timeout
                let result = tokio::time::timeout(timeout, async move {
                    handler_clone.handle_event(&event_clone).await
                })
                .await;

                match result {
                    Ok(Ok(())) => {
                        // Handler succeeded
                        tracing::trace!(
                            "Event handler executed successfully for event: {:?}",
                            event.event_type()
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Event handler failed: {}", e);
                        if self.config.retry_failed_handlers {
                            // TODO: Implement retry logic
                            tracing::debug!("Event handler retry not yet implemented");
                        }
                    }
                    Err(_) => {
                        tracing::warn!("Event handler timed out after {:?}", timeout);
                    }
                }
            }
        }

        Ok(())
    }

    /// Register an event handler
    pub fn register_handler(&mut self, handler: Arc<dyn EventHandler>) {
        self.handlers.push(handler);
        // Sort handlers by priority (higher priority first)
        self.handlers
            .sort_by_key(|h| std::cmp::Reverse(h.priority()));
    }

    /// Subscribe to events via a broadcast receiver
    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.channel.subscribe()
    }

    /// Get current metrics
    pub async fn metrics(&self) -> EventMetrics {
        self.metrics.lock().await.clone()
    }

    /// Get the number of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Get the channel capacity
    pub fn channel_capacity(&self) -> usize {
        self.config.channel_capacity
    }

    /// Check if events are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Enable or disable event emission
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// Get handlers filtered by event type interest
    pub fn handlers_for_event(&self, event_type: &HookEventType) -> Vec<Arc<dyn EventHandler>> {
        self.handlers
            .iter()
            .filter(|handler| handler.interested_events().contains(event_type))
            .cloned()
            .collect()
    }

    /// Clear all registered handlers
    pub fn clear_handlers(&mut self) {
        self.handlers.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// Implement Clone for EventBus by creating a new instance with same config
impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self::with_config(self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ExecutionContext, Stage};
    use crate::events::handler::{EventHandler, EventHandlerPriority};
    use crate::events::{HookEvent, HookEventType};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    // Test event handler that counts events
    struct TestEventHandler {
        counter: Arc<AtomicUsize>,
        interested_events: Vec<HookEventType>,
    }

    impl TestEventHandler {
        fn new(interested_events: Vec<HookEventType>) -> Self {
            Self {
                counter: Arc::new(AtomicUsize::new(0)),
                interested_events,
            }
        }

        fn event_count(&self) -> usize {
            self.counter.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl EventHandler for TestEventHandler {
        async fn handle_event(&self, _event: &HookEvent) -> Result<()> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn interested_events(&self) -> Vec<HookEventType> {
            self.interested_events.clone()
        }

        fn priority(&self) -> EventHandlerPriority {
            EventHandlerPriority::Normal
        }
    }

    #[tokio::test]
    async fn test_event_bus_creation() {
        let bus = EventBus::new();

        assert!(bus.is_enabled());
        assert_eq!(bus.handler_count(), 0);
        assert_eq!(bus.channel_capacity(), 1000);
    }

    #[tokio::test]
    async fn test_event_bus_with_config() {
        let config = EventConfig {
            enabled: false,
            channel_capacity: 500,
            ..Default::default()
        };

        let bus = EventBus::with_config(config);

        assert!(!bus.is_enabled());
        assert_eq!(bus.channel_capacity(), 500);
    }

    #[tokio::test]
    async fn test_handler_registration() {
        let mut bus = EventBus::new();
        let handler = Arc::new(TestEventHandler::new(vec![HookEventType::HookStarted]));

        bus.register_handler(handler);

        assert_eq!(bus.handler_count(), 1);
    }

    #[tokio::test]
    async fn test_event_emission_to_handlers() {
        let mut bus = EventBus::new();
        let handler = Arc::new(TestEventHandler::new(vec![HookEventType::HookStarted]));
        let handler_ref = Arc::clone(&handler);

        bus.register_handler(handler);

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        bus.emit(event).await.unwrap();

        // Give handler time to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(handler_ref.event_count(), 1);
    }

    #[tokio::test]
    async fn test_handler_filtering_by_interest() {
        let mut bus = EventBus::new();
        let handler1 = Arc::new(TestEventHandler::new(vec![HookEventType::HookStarted]));
        let handler2 = Arc::new(TestEventHandler::new(vec![HookEventType::HookCompleted]));
        let handler1_ref = Arc::clone(&handler1);
        let handler2_ref = Arc::clone(&handler2);

        bus.register_handler(handler1);
        bus.register_handler(handler2);

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        bus.emit(event).await.unwrap();

        // Give handlers time to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(handler1_ref.event_count(), 1);
        assert_eq!(handler2_ref.event_count(), 0);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        bus.emit(event.clone()).await.unwrap();

        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.hook_id(), event.hook_id());
        assert_eq!(received_event.event_type(), event.event_type());
    }

    #[tokio::test]
    async fn test_disabled_event_bus() {
        let config = EventConfig {
            enabled: false,
            ..Default::default()
        };

        let mut bus = EventBus::with_config(config);
        let handler = Arc::new(TestEventHandler::new(vec![HookEventType::HookStarted]));
        let handler_ref = Arc::clone(&handler);

        bus.register_handler(handler);

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        bus.emit(event).await.unwrap();

        // Give handler time to process (it shouldn't)
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(handler_ref.event_count(), 0);
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let bus = EventBus::new();

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        bus.emit(event).await.unwrap();

        let metrics = bus.metrics().await;
        assert_eq!(metrics.total_events(), 1);
    }

    #[tokio::test]
    async fn test_handlers_for_event() {
        let mut bus = EventBus::new();
        let handler1 = Arc::new(TestEventHandler::new(vec![
            HookEventType::HookStarted,
            HookEventType::HookCompleted,
        ]));
        let handler2 = Arc::new(TestEventHandler::new(vec![HookEventType::HookCompleted]));

        bus.register_handler(handler1);
        bus.register_handler(handler2);

        let handlers = bus.handlers_for_event(&HookEventType::HookStarted);
        assert_eq!(handlers.len(), 1);

        let handlers = bus.handlers_for_event(&HookEventType::HookCompleted);
        assert_eq!(handlers.len(), 2);

        let handlers = bus.handlers_for_event(&HookEventType::HookFailed);
        assert_eq!(handlers.len(), 0);
    }

    #[tokio::test]
    async fn test_clear_handlers() {
        let mut bus = EventBus::new();
        let handler = Arc::new(TestEventHandler::new(vec![HookEventType::HookStarted]));

        bus.register_handler(handler);
        assert_eq!(bus.handler_count(), 1);

        bus.clear_handlers();
        assert_eq!(bus.handler_count(), 0);
    }
}
