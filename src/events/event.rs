// Event types for hook lifecycle management
use crate::core::{ExecutionContext, Stage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Simplified execution result for events (avoids circular dependencies)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHookExecutionResult {
    pub hook_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub files_processed: Vec<PathBuf>,
    pub stdout: String,
    pub stderr: String,
}

/// Simplified execution summary for events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventExecutionSummary {
    pub total_hooks: usize,
    pub hooks_passed: usize,
    pub hooks_failed: usize,
    pub hooks_skipped: usize,
    pub total_duration: Duration,
}

/// Simplified error information for events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHookExecutionError {
    pub hook_id: String,
    pub message: String,
    pub exit_code: Option<i32>,
}

impl std::fmt::Display for EventHookExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(exit_code) = self.exit_code {
            write!(f, "{} (exit code: {})", self.message, exit_code)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

/// Represents different types of hook lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    // Lifecycle events
    HookStarted {
        hook_id: String,
        execution_id: Uuid,
        timestamp: SystemTime,
        context: ExecutionContextData,
    },

    HookProgress {
        hook_id: String,
        execution_id: Uuid,
        progress: f32,
        message: Option<String>,
        timestamp: SystemTime,
    },

    HookCompleted {
        hook_id: String,
        execution_id: Uuid,
        result: EventHookExecutionResult,
        duration: Duration,
        timestamp: SystemTime,
    },

    HookFailed {
        hook_id: String,
        execution_id: Uuid,
        error: EventHookExecutionError,
        duration: Duration,
        timestamp: SystemTime,
    },

    HookSkipped {
        hook_id: String,
        execution_id: Uuid,
        reason: String,
        timestamp: SystemTime,
    },

    // Pipeline events
    PipelineStarted {
        pipeline_id: Uuid,
        stage: Stage,
        hook_count: usize,
        timestamp: SystemTime,
    },

    PipelineCompleted {
        pipeline_id: Uuid,
        stage: Stage,
        summary: EventExecutionSummary,
        timestamp: SystemTime,
    },

    // File events
    FilesProcessed {
        hook_id: String,
        execution_id: Uuid,
        files: Vec<PathBuf>,
        timestamp: SystemTime,
    },

    // Custom events for extensibility
    Custom {
        event_type: String,
        data: serde_json::Value,
        timestamp: SystemTime,
    },
}

/// Simplified execution context data for event serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContextData {
    pub stage: Stage,
    pub files_count: usize,
    pub verbose: bool,
    pub show_diff_on_failure: bool,
    pub color: bool,
}

impl From<&ExecutionContext> for ExecutionContextData {
    fn from(ctx: &ExecutionContext) -> Self {
        Self {
            stage: ctx.stage.clone(),
            files_count: ctx.files.len(),
            verbose: ctx.verbose,
            show_diff_on_failure: ctx.show_diff_on_failure,
            color: ctx.color,
        }
    }
}

/// Enum representing different types of hook events for filtering
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEventType {
    HookStarted,
    HookProgress,
    HookCompleted,
    HookFailed,
    HookSkipped,
    PipelineStarted,
    PipelineCompleted,
    FilesProcessed,
    Custom,
}

impl HookEvent {
    /// Get the event type for this event
    pub fn event_type(&self) -> HookEventType {
        match self {
            HookEvent::HookStarted { .. } => HookEventType::HookStarted,
            HookEvent::HookProgress { .. } => HookEventType::HookProgress,
            HookEvent::HookCompleted { .. } => HookEventType::HookCompleted,
            HookEvent::HookFailed { .. } => HookEventType::HookFailed,
            HookEvent::HookSkipped { .. } => HookEventType::HookSkipped,
            HookEvent::PipelineStarted { .. } => HookEventType::PipelineStarted,
            HookEvent::PipelineCompleted { .. } => HookEventType::PipelineCompleted,
            HookEvent::FilesProcessed { .. } => HookEventType::FilesProcessed,
            HookEvent::Custom { .. } => HookEventType::Custom,
        }
    }

    /// Get the timestamp for this event
    pub fn timestamp(&self) -> SystemTime {
        match self {
            HookEvent::HookStarted { timestamp, .. }
            | HookEvent::HookProgress { timestamp, .. }
            | HookEvent::HookCompleted { timestamp, .. }
            | HookEvent::HookFailed { timestamp, .. }
            | HookEvent::HookSkipped { timestamp, .. }
            | HookEvent::PipelineStarted { timestamp, .. }
            | HookEvent::PipelineCompleted { timestamp, .. }
            | HookEvent::FilesProcessed { timestamp, .. }
            | HookEvent::Custom { timestamp, .. } => *timestamp,
        }
    }

    /// Get the hook ID if this is a hook-specific event
    pub fn hook_id(&self) -> Option<&str> {
        match self {
            HookEvent::HookStarted { hook_id, .. }
            | HookEvent::HookProgress { hook_id, .. }
            | HookEvent::HookCompleted { hook_id, .. }
            | HookEvent::HookFailed { hook_id, .. }
            | HookEvent::HookSkipped { hook_id, .. }
            | HookEvent::FilesProcessed { hook_id, .. } => Some(hook_id),
            _ => None,
        }
    }

    /// Get the execution ID if this is an execution-specific event
    pub fn execution_id(&self) -> Option<Uuid> {
        match self {
            HookEvent::HookStarted { execution_id, .. }
            | HookEvent::HookProgress { execution_id, .. }
            | HookEvent::HookCompleted { execution_id, .. }
            | HookEvent::HookFailed { execution_id, .. }
            | HookEvent::HookSkipped { execution_id, .. }
            | HookEvent::FilesProcessed { execution_id, .. } => Some(*execution_id),
            _ => None,
        }
    }

    /// Create a HookStarted event
    pub fn hook_started(hook_id: String, execution_id: Uuid, context: &ExecutionContext) -> Self {
        Self::HookStarted {
            hook_id,
            execution_id,
            timestamp: SystemTime::now(),
            context: ExecutionContextData::from(context),
        }
    }

    /// Create a HookProgress event
    pub fn hook_progress(
        hook_id: String,
        execution_id: Uuid,
        progress: f32,
        message: Option<String>,
    ) -> Self {
        Self::HookProgress {
            hook_id,
            execution_id,
            progress,
            message,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a HookCompleted event
    pub fn hook_completed(
        hook_id: String,
        execution_id: Uuid,
        result: EventHookExecutionResult,
        duration: Duration,
    ) -> Self {
        Self::HookCompleted {
            hook_id,
            execution_id,
            result,
            duration,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a HookFailed event
    pub fn hook_failed(
        hook_id: String,
        execution_id: Uuid,
        error: EventHookExecutionError,
        duration: Duration,
    ) -> Self {
        Self::HookFailed {
            hook_id,
            execution_id,
            error,
            duration,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a HookSkipped event
    pub fn hook_skipped(hook_id: String, execution_id: Uuid, reason: String) -> Self {
        Self::HookSkipped {
            hook_id,
            execution_id,
            reason,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a PipelineStarted event
    pub fn pipeline_started(pipeline_id: Uuid, stage: Stage, hook_count: usize) -> Self {
        Self::PipelineStarted {
            pipeline_id,
            stage,
            hook_count,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a PipelineCompleted event
    pub fn pipeline_completed(
        pipeline_id: Uuid,
        stage: Stage,
        summary: EventExecutionSummary,
    ) -> Self {
        Self::PipelineCompleted {
            pipeline_id,
            stage,
            summary,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a FilesProcessed event
    pub fn files_processed(hook_id: String, execution_id: Uuid, files: Vec<PathBuf>) -> Self {
        Self::FilesProcessed {
            hook_id,
            execution_id,
            files,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a Custom event
    pub fn custom(event_type: String, data: serde_json::Value) -> Self {
        Self::Custom {
            event_type,
            data,
            timestamp: SystemTime::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ExecutionContext, Stage};

    #[test]
    fn test_hook_event_creation() {
        let hook_id = "test-hook".to_string();
        let execution_id = Uuid::new_v4();
        let context = ExecutionContext::new(Stage::PreCommit);

        let event = HookEvent::hook_started(hook_id.clone(), execution_id, &context);

        match event {
            HookEvent::HookStarted {
                hook_id: id,
                execution_id: exec_id,
                context: ctx,
                ..
            } => {
                assert_eq!(id, hook_id);
                assert_eq!(exec_id, execution_id);
                assert_eq!(ctx.stage, Stage::PreCommit);
            }
            _ => panic!("Expected HookStarted event"),
        }
    }

    #[test]
    fn test_event_type_extraction() {
        let event = HookEvent::hook_progress(
            "test".to_string(),
            Uuid::new_v4(),
            0.5,
            Some("Processing files".to_string()),
        );

        assert_eq!(event.event_type(), HookEventType::HookProgress);
    }

    #[test]
    fn test_hook_id_extraction() {
        let hook_id = "test-hook".to_string();
        let event = HookEvent::hook_skipped(
            hook_id.clone(),
            Uuid::new_v4(),
            "No files to process".to_string(),
        );

        assert_eq!(event.hook_id(), Some(hook_id.as_str()));
    }

    #[test]
    fn test_execution_id_extraction() {
        let execution_id = Uuid::new_v4();
        let event = HookEvent::hook_progress("test".to_string(), execution_id, 0.75, None);

        assert_eq!(event.execution_id(), Some(execution_id));
    }

    #[test]
    fn test_pipeline_events() {
        let pipeline_id = Uuid::new_v4();
        let stage = Stage::PreCommit;
        let hook_count = 5;

        let event = HookEvent::pipeline_started(pipeline_id, stage.clone(), hook_count);

        match event {
            HookEvent::PipelineStarted {
                pipeline_id: id,
                stage: s,
                hook_count: count,
                ..
            } => {
                assert_eq!(id, pipeline_id);
                assert_eq!(s, stage);
                assert_eq!(count, hook_count);
            }
            _ => panic!("Expected PipelineStarted event"),
        }
    }

    #[test]
    fn test_custom_event() {
        let event_type = "webhook-notification".to_string();
        let data = serde_json::json!({
            "url": "https://example.com/webhook",
            "payload": {"status": "completed"}
        });

        let event = HookEvent::custom(event_type.clone(), data.clone());

        match event {
            HookEvent::Custom {
                event_type: t,
                data: d,
                ..
            } => {
                assert_eq!(t, event_type);
                assert_eq!(d, data);
            }
            _ => panic!("Expected Custom event"),
        }
    }

    #[test]
    fn test_execution_context_data_conversion() {
        let mut context = ExecutionContext::new(Stage::PrePush)
            .with_verbose(true)
            .with_show_diff(true)
            .with_color(false);

        context.files = vec![PathBuf::from("file1.rs"), PathBuf::from("file2.py")];

        let ctx_data = ExecutionContextData::from(&context);

        assert_eq!(ctx_data.stage, Stage::PrePush);
        assert_eq!(ctx_data.files_count, 2);
        assert!(ctx_data.verbose);
        assert!(ctx_data.show_diff_on_failure);
        assert!(!ctx_data.color);
    }

    #[test]
    fn test_event_serialization() {
        let event = HookEvent::hook_started(
            "test-hook".to_string(),
            Uuid::new_v4(),
            &ExecutionContext::new(Stage::PreCommit),
        );

        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: HookEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event.event_type(), deserialized.event_type());
        assert_eq!(event.hook_id(), deserialized.hook_id());
    }
}
