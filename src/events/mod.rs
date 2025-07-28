// Event-driven hook lifecycle management system
// Provides comprehensive observability, extensibility, and integration capabilities

pub mod bus;
pub mod event;
pub mod handler;
pub mod metrics;

// Re-export main types for easier access
pub use bus::{EventBus, EventConfig};
pub use event::{HookEvent, HookEventType};
pub use handler::{EventHandler, EventHandlerPriority, LoggingEventHandler, MetricsEventHandler};
pub use metrics::{EventFilter, EventMetrics};
