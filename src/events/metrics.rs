// Event metrics and filtering for hook lifecycle management
use crate::core::Stage;
use crate::events::{HookEvent, HookEventType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

/// Event metrics collection and reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetrics {
    pub total_events: u64,
    pub events_by_type: HashMap<HookEventType, u64>,
    pub handler_execution_times: HashMap<String, Duration>,
    pub failed_handlers: u64,
    pub last_event_time: Option<SystemTime>,
    pub event_rate_per_second: f64,
}

impl EventMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self {
            total_events: 0,
            events_by_type: HashMap::new(),
            handler_execution_times: HashMap::new(),
            failed_handlers: 0,
            last_event_time: None,
            event_rate_per_second: 0.0,
        }
    }

    /// Record an event occurrence
    pub fn record_event(&mut self, event: &HookEvent) {
        self.total_events += 1;

        let event_type = event.event_type();
        *self.events_by_type.entry(event_type).or_insert(0) += 1;

        let current_time = event.timestamp();
        if let Some(last_time) = self.last_event_time {
            if let Ok(duration_since_last) = current_time.duration_since(last_time) {
                if duration_since_last.as_secs() > 0 {
                    self.event_rate_per_second = 1.0 / duration_since_last.as_secs_f64();
                }
            }
        }

        self.last_event_time = Some(current_time);
    }

    /// Record handler execution time
    pub fn record_handler_execution(&mut self, handler_id: String, duration: Duration) {
        self.handler_execution_times.insert(handler_id, duration);
    }

    /// Record handler failure
    pub fn record_handler_failure(&mut self) {
        self.failed_handlers += 1;
    }

    /// Get total number of events
    pub fn total_events(&self) -> u64 {
        self.total_events
    }

    /// Get count of events by type
    pub fn events_of_type(&self, event_type: &HookEventType) -> u64 {
        self.events_by_type.get(event_type).copied().unwrap_or(0)
    }

    /// Get the average handler execution time
    pub fn average_handler_execution_time(&self) -> Option<Duration> {
        if self.handler_execution_times.is_empty() {
            return None;
        }

        let total_nanos: u128 = self
            .handler_execution_times
            .values()
            .map(|d| d.as_nanos())
            .sum();

        let average_nanos = total_nanos / self.handler_execution_times.len() as u128;
        Some(Duration::from_nanos(average_nanos as u64))
    }

    /// Get event rate per second
    pub fn event_rate(&self) -> f64 {
        self.event_rate_per_second
    }

    /// Get failed handler count
    pub fn failed_handlers(&self) -> u64 {
        self.failed_handlers
    }

    /// Get handler success rate
    pub fn handler_success_rate(&self) -> f64 {
        let total_handlers = self.handler_execution_times.len() as u64 + self.failed_handlers;
        if total_handlers == 0 {
            return 1.0;
        }

        let successful_handlers = self.handler_execution_times.len() as u64;
        successful_handlers as f64 / total_handlers as f64
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        self.total_events = 0;
        self.events_by_type.clear();
        self.handler_execution_times.clear();
        self.failed_handlers = 0;
        self.last_event_time = None;
        self.event_rate_per_second = 0.0;
    }

    /// Get metrics summary as formatted string
    pub fn summary(&self) -> String {
        format!(
            "Event Metrics:\n\
             - Total events: {}\n\
             - Event types: {:?}\n\
             - Handler success rate: {:.2}%\n\
             - Average handler time: {:?}\n\
             - Event rate: {:.2}/sec\n\
             - Failed handlers: {}",
            self.total_events,
            self.events_by_type,
            self.handler_success_rate() * 100.0,
            self.average_handler_execution_time(),
            self.event_rate_per_second,
            self.failed_handlers
        )
    }
}

impl Default for EventMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Event filter for controlling which events should be emitted or processed
#[derive(Debug, Clone)]
pub struct EventFilter {
    pub event_types: HashSet<HookEventType>,
    pub hook_patterns: Vec<Regex>,
    pub stage_filter: Option<Stage>,
    pub min_duration: Option<Duration>,
    pub max_events_per_second: Option<f64>,
}

impl EventFilter {
    /// Create a new empty event filter (allows all events)
    pub fn new() -> Self {
        Self {
            event_types: HashSet::new(),
            hook_patterns: Vec::new(),
            stage_filter: None,
            min_duration: None,
            max_events_per_second: None,
        }
    }

    /// Create filter that only allows specific event types
    pub fn with_event_types(mut self, event_types: Vec<HookEventType>) -> Self {
        self.event_types = event_types.into_iter().collect();
        self
    }

    /// Add hook name patterns to filter by
    pub fn with_hook_patterns(mut self, patterns: Vec<String>) -> Result<Self, String> {
        for pattern in patterns {
            let regex = Regex::new(&pattern)
                .map_err(|e| format!("Invalid regex pattern '{pattern}': {e}"))?;
            self.hook_patterns.push(regex);
        }
        Ok(self)
    }

    /// Filter by specific stage
    pub fn with_stage(mut self, stage: Stage) -> Self {
        self.stage_filter = Some(stage);
        self
    }

    /// Filter events by minimum duration (for completed/failed events)
    pub fn with_min_duration(mut self, min_duration: Duration) -> Self {
        self.min_duration = Some(min_duration);
        self
    }

    /// Set maximum events per second rate limit
    pub fn with_rate_limit(mut self, max_events_per_second: f64) -> Self {
        self.max_events_per_second = Some(max_events_per_second);
        self
    }

    /// Check if an event should be processed according to this filter
    pub fn should_emit(&self, event: &HookEvent) -> bool {
        // Check event type filter
        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type()) {
            return false;
        }

        // Check hook pattern filter
        if !self.hook_patterns.is_empty() {
            if let Some(hook_id) = event.hook_id() {
                let matches_pattern = self
                    .hook_patterns
                    .iter()
                    .any(|pattern| pattern.is_match(hook_id));
                if !matches_pattern {
                    return false;
                }
            } else {
                // Non-hook events don't match hook patterns
                return false;
            }
        }

        // Check stage filter
        if let Some(filter_stage) = &self.stage_filter {
            match event {
                HookEvent::HookStarted { context, .. } => {
                    if &context.stage != filter_stage {
                        return false;
                    }
                }
                HookEvent::PipelineStarted { stage, .. }
                | HookEvent::PipelineCompleted { stage, .. } => {
                    if stage != filter_stage {
                        return false;
                    }
                }
                _ => {
                    // Other events don't have stage information, so they pass stage filter
                }
            }
        }

        // Check minimum duration filter
        if let Some(min_duration) = self.min_duration {
            match event {
                HookEvent::HookCompleted { duration, .. }
                | HookEvent::HookFailed { duration, .. } => {
                    if duration < &min_duration {
                        return false;
                    }
                }
                _ => {
                    // Events without duration pass the duration filter
                }
            }
        }

        // TODO: Implement rate limiting (requires state tracking)
        // For now, rate limiting would need to be implemented at the EventBus level

        true
    }

    /// Check if filter has any constraints
    pub fn is_empty(&self) -> bool {
        self.event_types.is_empty()
            && self.hook_patterns.is_empty()
            && self.stage_filter.is_none()
            && self.min_duration.is_none()
            && self.max_events_per_second.is_none()
    }

    /// Get summary of filter constraints
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.event_types.is_empty() {
            parts.push(format!("event_types: {:?}", self.event_types));
        }

        if !self.hook_patterns.is_empty() {
            let patterns: Vec<String> = self
                .hook_patterns
                .iter()
                .map(|r| r.as_str().to_string())
                .collect();
            parts.push(format!("hook_patterns: {patterns:?}"));
        }

        if let Some(stage) = &self.stage_filter {
            parts.push(format!("stage: {}", stage.as_str()));
        }

        if let Some(duration) = self.min_duration {
            parts.push(format!("min_duration: {duration:?}"));
        }

        if let Some(rate) = self.max_events_per_second {
            parts.push(format!("max_rate: {rate}/sec"));
        }

        if parts.is_empty() {
            "no filters".to_string()
        } else {
            parts.join(", ")
        }
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ExecutionContext, Stage};
    use crate::events::{event::EventHookExecutionResult, HookEvent};
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn test_event_metrics_creation() {
        let metrics = EventMetrics::new();

        assert_eq!(metrics.total_events(), 0);
        assert_eq!(metrics.events_of_type(&HookEventType::HookStarted), 0);
        assert_eq!(metrics.failed_handlers(), 0);
        assert_eq!(metrics.handler_success_rate(), 1.0);
    }

    #[test]
    fn test_event_metrics_recording() {
        let mut metrics = EventMetrics::new();

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        metrics.record_event(&event);

        assert_eq!(metrics.total_events(), 1);
        assert_eq!(metrics.events_of_type(&HookEventType::HookStarted), 1);
        assert_eq!(metrics.events_of_type(&HookEventType::HookCompleted), 0);
    }

    #[test]
    fn test_handler_execution_recording() {
        let mut metrics = EventMetrics::new();
        let duration = Duration::from_millis(100);

        metrics.record_handler_execution("test-handler".to_string(), duration);

        assert_eq!(metrics.average_handler_execution_time(), Some(duration));
        assert_eq!(metrics.handler_success_rate(), 1.0);
    }

    #[test]
    fn test_handler_failure_recording() {
        let mut metrics = EventMetrics::new();

        metrics.record_handler_failure();

        assert_eq!(metrics.failed_handlers(), 1);
        assert_eq!(metrics.handler_success_rate(), 0.0);
    }

    #[test]
    fn test_metrics_reset() {
        let mut metrics = EventMetrics::new();

        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        metrics.record_event(&event);
        metrics.record_handler_failure();

        assert_eq!(metrics.total_events(), 1);
        assert_eq!(metrics.failed_handlers(), 1);

        metrics.reset();

        assert_eq!(metrics.total_events(), 0);
        assert_eq!(metrics.failed_handlers(), 0);
    }

    #[test]
    fn test_event_filter_creation() {
        let filter = EventFilter::new();

        assert!(filter.is_empty());
        assert!(filter.should_emit(&HookEvent::hook_started(
            "test".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit)
        )));
    }

    #[test]
    fn test_event_filter_by_type() {
        let filter = EventFilter::new().with_event_types(vec![
            HookEventType::HookStarted,
            HookEventType::HookCompleted,
        ]);

        let started_event = HookEvent::hook_started(
            "test".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        let skipped_event =
            HookEvent::hook_skipped("test".to_string(), Uuid::new_v4(), "No files".to_string());

        assert!(filter.should_emit(&started_event));
        assert!(!filter.should_emit(&skipped_event));
    }

    #[test]
    fn test_event_filter_by_hook_pattern() {
        let filter = EventFilter::new()
            .with_hook_patterns(vec!["rust-.*".to_string(), "python-.*".to_string()])
            .unwrap();

        let rust_event = HookEvent::hook_started(
            "rust-fmt".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        let javascript_event = HookEvent::hook_started(
            "eslint".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        assert!(filter.should_emit(&rust_event));
        assert!(!filter.should_emit(&javascript_event));
    }

    #[test]
    fn test_event_filter_by_stage() {
        let filter = EventFilter::new().with_stage(Stage::PrePush);

        let pre_commit_event = HookEvent::hook_started(
            "test".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        let pre_push_event = HookEvent::hook_started(
            "test".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PrePush),
        );

        assert!(!filter.should_emit(&pre_commit_event));
        assert!(filter.should_emit(&pre_push_event));
    }

    #[test]
    fn test_event_filter_by_duration() {
        let filter = EventFilter::new().with_min_duration(Duration::from_millis(500));

        let fast_result = EventHookExecutionResult {
            hook_id: "test".to_string(),
            success: true,
            exit_code: Some(0),
            duration: Duration::from_millis(100),
            files_processed: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
        };

        let slow_result = EventHookExecutionResult {
            hook_id: "test".to_string(),
            success: true,
            exit_code: Some(0),
            duration: Duration::from_millis(1000),
            files_processed: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
        };

        let fast_event = HookEvent::hook_completed(
            "test".to_string(),
            Uuid::new_v4(),
            fast_result,
            Duration::from_millis(100),
        );

        let slow_event = HookEvent::hook_completed(
            "test".to_string(),
            Uuid::new_v4(),
            slow_result,
            Duration::from_millis(1000),
        );

        assert!(!filter.should_emit(&fast_event));
        assert!(filter.should_emit(&slow_event));
    }

    #[test]
    fn test_event_filter_invalid_pattern() {
        let result = EventFilter::new().with_hook_patterns(vec!["[invalid".to_string()]);

        assert!(result.is_err());
    }

    #[test]
    fn test_event_filter_summary() {
        let filter = EventFilter::new()
            .with_event_types(vec![HookEventType::HookStarted])
            .with_stage(Stage::PreCommit)
            .with_min_duration(Duration::from_millis(100));

        let summary = filter.summary();

        assert!(summary.contains("event_types"));
        assert!(summary.contains("stage"));
        assert!(summary.contains("min_duration"));
    }

    #[test]
    fn test_metrics_summary() {
        let mut metrics = EventMetrics::new();

        let event = HookEvent::hook_started(
            "test".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        metrics.record_event(&event);
        metrics.record_handler_execution("test".to_string(), Duration::from_millis(50));

        let summary = metrics.summary();

        assert!(summary.contains("Total events: 1"));
        assert!(summary.contains("Handler success rate"));
        assert!(summary.contains("Average handler time"));
    }

    #[test]
    fn test_multiple_event_types_metrics() {
        let mut metrics = EventMetrics::new();

        let events = vec![
            HookEvent::hook_started(
                "test1".to_string(),
                Uuid::new_v4(),
                &ExecutionContext::new(Stage::PreCommit),
            ),
            HookEvent::hook_started(
                "test2".to_string(),
                Uuid::new_v4(),
                &ExecutionContext::new(Stage::PreCommit),
            ),
            HookEvent::hook_skipped("test3".to_string(), Uuid::new_v4(), "No files".to_string()),
        ];

        for event in events {
            metrics.record_event(&event);
        }

        assert_eq!(metrics.total_events(), 3);
        assert_eq!(metrics.events_of_type(&HookEventType::HookStarted), 2);
        assert_eq!(metrics.events_of_type(&HookEventType::HookSkipped), 1);
    }
}
