// Lock-free task scheduler with work-stealing capabilities
// Provides high-performance task scheduling with minimal contention

use crossbeam::queue::{ArrayQueue, SegQueue};
use dashmap::DashMap;
use futures::future::BoxFuture;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::oneshot;

use crate::concurrency::{TaskConfig, TaskPriority, TaskResult};
use crate::error::Result;

/// Task wrapper for internal scheduling
pub struct ScheduledTask {
    pub id: String,
    pub priority: TaskPriority,
    pub config: TaskConfig,
    pub task: Option<BoxFuture<'static, Result<()>>>,
    pub result_sender: Option<oneshot::Sender<TaskResult<()>>>,
    pub created_at: SystemTime,
}

impl std::fmt::Debug for ScheduledTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScheduledTask")
            .field("id", &self.id)
            .field("priority", &self.priority)
            .field("config", &self.config)
            .field("task", &"<Future>")
            .field("result_sender", &"<Sender>")
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Worker state for monitoring
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerState {
    Idle,
    Working {
        task_id: String,
        started_at: SystemTime,
    },
    Stealing,
}

/// Statistics for the task scheduler
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub total_tasks_submitted: u64,
    pub total_tasks_completed: u64,
    pub total_tasks_failed: u64,
    pub total_tasks_stolen: u64,
    pub pending_tasks: usize,
    pub active_workers: usize,
    pub average_task_duration_ms: f64,
}

/// Lock-free task scheduler with work-stealing queues
pub struct LockFreeTaskScheduler {
    // Global task queue for initial task distribution
    global_queue: SegQueue<ScheduledTask>,

    // Per-worker local queues for work-stealing
    local_queues: Vec<ArrayQueue<ScheduledTask>>,

    // Atomic task counters
    pending_tasks: AtomicUsize,
    completed_tasks: AtomicU64,
    failed_tasks: AtomicU64,
    stolen_tasks: AtomicU64,
    submitted_tasks: AtomicU64,

    // Worker state tracking
    worker_states: DashMap<usize, WorkerState>,

    // Configuration
    num_workers: usize,
    #[allow(dead_code)]
    local_queue_capacity: usize,

    // Task duration tracking for statistics
    total_task_duration_ms: AtomicU64,
}

impl LockFreeTaskScheduler {
    /// Create a new lock-free task scheduler
    pub fn new(num_workers: usize, local_queue_capacity: usize) -> Self {
        let local_queues = (0..num_workers)
            .map(|_| ArrayQueue::new(local_queue_capacity))
            .collect();

        Self {
            global_queue: SegQueue::new(),
            local_queues,
            pending_tasks: AtomicUsize::new(0),
            completed_tasks: AtomicU64::new(0),
            failed_tasks: AtomicU64::new(0),
            stolen_tasks: AtomicU64::new(0),
            submitted_tasks: AtomicU64::new(0),
            worker_states: DashMap::new(),
            num_workers,
            local_queue_capacity,
            total_task_duration_ms: AtomicU64::new(0),
        }
    }

    /// Submit a task for execution
    pub fn submit_task<F>(
        &self,
        config: TaskConfig,
        task: F,
    ) -> Result<oneshot::Receiver<TaskResult<()>>>
    where
        F: FnOnce() -> BoxFuture<'static, Result<()>> + Send + 'static,
    {
        let (result_sender, result_receiver) = oneshot::channel();

        let scheduled_task = ScheduledTask {
            id: config.id.clone(),
            priority: config.priority.clone(),
            config,
            task: Some(task()),
            result_sender: Some(result_sender),
            created_at: SystemTime::now(),
        };

        // Try to submit to a worker's local queue first (round-robin)
        let worker_id = self.current_worker_id();

        let queue_result = if let Some(local_queue) = self.local_queues.get(worker_id) {
            local_queue.push(scheduled_task)
        } else {
            Err(scheduled_task) // Return the task if no local queue
        };

        // If local queue push failed, push to global queue
        if let Err(scheduled_task) = queue_result {
            self.global_queue.push(scheduled_task);
        }

        self.pending_tasks.fetch_add(1, Ordering::Relaxed);
        self.submitted_tasks.fetch_add(1, Ordering::Relaxed);

        Ok(result_receiver)
    }

    /// Submit multiple tasks in batch
    pub fn submit_batch<F>(
        &self,
        tasks: Vec<(TaskConfig, F)>,
    ) -> Result<Vec<oneshot::Receiver<TaskResult<()>>>>
    where
        F: FnOnce() -> BoxFuture<'static, Result<()>> + Send + 'static,
    {
        let mut receivers = Vec::with_capacity(tasks.len());

        for (config, task) in tasks {
            let receiver = self.submit_task(config, task)?;
            receivers.push(receiver);
        }

        Ok(receivers)
    }

    /// Try to steal a task from queues (used by workers)
    pub fn steal_task(&self, worker_id: usize) -> Option<ScheduledTask> {
        // Mark worker as stealing
        self.worker_states.insert(worker_id, WorkerState::Stealing);

        // Try local queue first
        if let Some(local_queue) = self.local_queues.get(worker_id) {
            if let Some(task) = local_queue.pop() {
                self.update_worker_working(worker_id, &task.id);
                return Some(task);
            }
        }

        // Try to steal from other workers' queues
        for (i, queue) in self.local_queues.iter().enumerate() {
            if i != worker_id {
                if let Some(task) = queue.pop() {
                    self.stolen_tasks.fetch_add(1, Ordering::Relaxed);
                    self.update_worker_working(worker_id, &task.id);
                    return Some(task);
                }
            }
        }

        // Try global queue as last resort
        if let Some(task) = self.global_queue.pop() {
            self.update_worker_working(worker_id, &task.id);
            return Some(task);
        }

        // No task available, mark worker as idle
        self.worker_states.insert(worker_id, WorkerState::Idle);
        None
    }

    /// Mark a task as completed
    pub fn mark_task_completed(&self, worker_id: usize, _task_id: &str, duration: Duration) {
        self.pending_tasks.fetch_sub(1, Ordering::Relaxed);
        self.completed_tasks.fetch_add(1, Ordering::Relaxed);
        self.total_task_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.worker_states.insert(worker_id, WorkerState::Idle);
    }

    /// Mark a task as failed
    pub fn mark_task_failed(&self, worker_id: usize, _task_id: &str) {
        self.pending_tasks.fetch_sub(1, Ordering::Relaxed);
        self.failed_tasks.fetch_add(1, Ordering::Relaxed);
        self.worker_states.insert(worker_id, WorkerState::Idle);
    }

    /// Get current scheduler statistics
    pub fn get_stats(&self) -> SchedulerStats {
        let completed = self.completed_tasks.load(Ordering::Relaxed);
        let total_duration = self.total_task_duration_ms.load(Ordering::Relaxed);

        let average_duration = if completed > 0 {
            total_duration as f64 / completed as f64
        } else {
            0.0
        };

        let active_workers = self
            .worker_states
            .iter()
            .filter(|entry| matches!(entry.value(), WorkerState::Working { .. }))
            .count();

        SchedulerStats {
            total_tasks_submitted: self.submitted_tasks.load(Ordering::Relaxed),
            total_tasks_completed: completed,
            total_tasks_failed: self.failed_tasks.load(Ordering::Relaxed),
            total_tasks_stolen: self.stolen_tasks.load(Ordering::Relaxed),
            pending_tasks: self.pending_tasks.load(Ordering::Relaxed),
            active_workers,
            average_task_duration_ms: average_duration,
        }
    }

    /// Get worker state
    pub fn get_worker_state(&self, worker_id: usize) -> Option<WorkerState> {
        self.worker_states
            .get(&worker_id)
            .map(|entry| entry.value().clone())
    }

    /// Get all worker states
    pub fn get_all_worker_states(&self) -> Vec<(usize, WorkerState)> {
        self.worker_states
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }

    /// Check if scheduler has pending tasks
    pub fn has_pending_tasks(&self) -> bool {
        self.pending_tasks.load(Ordering::Relaxed) > 0
    }

    /// Get the number of pending tasks
    pub fn pending_task_count(&self) -> usize {
        self.pending_tasks.load(Ordering::Relaxed)
    }

    /// Reset all statistics
    pub fn reset_stats(&self) {
        self.completed_tasks.store(0, Ordering::Relaxed);
        self.failed_tasks.store(0, Ordering::Relaxed);
        self.stolen_tasks.store(0, Ordering::Relaxed);
        self.submitted_tasks.store(0, Ordering::Relaxed);
        self.total_task_duration_ms.store(0, Ordering::Relaxed);
    }

    /// Shutdown the scheduler gracefully
    pub async fn shutdown(&self, timeout: Duration) -> Result<()> {
        let start_time = SystemTime::now();

        // Wait for all pending tasks to complete or timeout
        loop {
            if !self.has_pending_tasks() {
                return Ok(());
            }

            // Check if we've exceeded timeout
            if start_time.elapsed().unwrap_or_default() > timeout {
                return Err(crate::error::SnpError::Process(Box::new(
                    crate::ProcessError::Timeout {
                        command: "scheduler_shutdown".to_string(),
                        duration: timeout,
                    },
                )));
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Get queue lengths for monitoring
    pub fn get_queue_lengths(&self) -> Vec<usize> {
        let mut lengths = vec![self.global_queue.len()]; // Global queue first

        for queue in &self.local_queues {
            lengths.push(queue.len());
        }

        lengths
    }

    /// Get load balancing metrics
    pub fn get_load_balance_metrics(&self) -> LoadBalanceMetrics {
        let queue_lengths = self.get_queue_lengths();
        let total_queued: usize = queue_lengths.iter().sum();

        let max_queue_length = queue_lengths.iter().max().copied().unwrap_or(0);
        let min_queue_length = queue_lengths.iter().min().copied().unwrap_or(0);

        let balance_ratio = if max_queue_length > 0 {
            min_queue_length as f64 / max_queue_length as f64
        } else {
            1.0
        };

        LoadBalanceMetrics {
            total_queued_tasks: total_queued,
            max_queue_length,
            min_queue_length,
            balance_ratio,
            queue_lengths,
        }
    }

    // Helper methods

    fn current_worker_id(&self) -> usize {
        // Simple round-robin distribution based on current time
        // In a real implementation, this would use thread-local storage
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as usize;
        now % self.num_workers
    }

    fn update_worker_working(&self, worker_id: usize, task_id: &str) {
        self.worker_states.insert(
            worker_id,
            WorkerState::Working {
                task_id: task_id.to_string(),
                started_at: SystemTime::now(),
            },
        );
    }
}

/// Load balancing metrics
#[derive(Debug, Clone)]
pub struct LoadBalanceMetrics {
    pub total_queued_tasks: usize,
    pub max_queue_length: usize,
    pub min_queue_length: usize,
    pub balance_ratio: f64, // Ratio of min/max queue length (1.0 = perfectly balanced)
    pub queue_lengths: Vec<usize>,
}

impl LoadBalanceMetrics {
    /// Check if the queues are well balanced (balance ratio > 0.8)
    pub fn is_well_balanced(&self) -> bool {
        self.balance_ratio > 0.8
    }

    /// Get the imbalance score (0.0 = perfectly balanced, 1.0 = maximally imbalanced)
    pub fn imbalance_score(&self) -> f64 {
        1.0 - self.balance_ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_basic_task_submission() {
        let scheduler = LockFreeTaskScheduler::new(4, 1024);

        let config = TaskConfig::new("test_task");
        let _receiver = scheduler
            .submit_task(config, || {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(())
                })
            })
            .unwrap();

        // Simulate worker stealing and executing the task
        let task = scheduler.steal_task(0).unwrap();
        assert_eq!(task.id, "test_task");

        // Execute the task
        if let Some(task_future) = task.task {
            let start = SystemTime::now();
            let result = task_future.await;
            let duration = start.elapsed().unwrap_or_default();

            assert!(result.is_ok());
            scheduler.mark_task_completed(0, &task.id, duration);
        }

        let stats = scheduler.get_stats();
        assert_eq!(stats.total_tasks_submitted, 1);
        assert_eq!(stats.total_tasks_completed, 1);
        assert_eq!(stats.pending_tasks, 0);
    }

    #[tokio::test]
    async fn test_work_stealing() {
        let scheduler = Arc::new(LockFreeTaskScheduler::new(4, 2));

        // Submit multiple tasks
        let mut receivers = vec![];
        for i in 0..8 {
            let config = TaskConfig::new(format!("task_{i}"));
            let receiver = scheduler
                .submit_task(config, move || {
                    Box::pin(async move {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        Ok(())
                    })
                })
                .unwrap();
            receivers.push(receiver);
        }

        assert_eq!(scheduler.pending_task_count(), 8);

        // Simulate multiple workers stealing tasks
        let mut handles = vec![];
        for worker_id in 0..4 {
            let scheduler_clone = Arc::clone(&scheduler);
            let handle = tokio::spawn(async move {
                while let Some(task) = scheduler_clone.steal_task(worker_id) {
                    let start = SystemTime::now();
                    if let Some(task_future) = task.task {
                        let result = task_future.await;
                        let duration = start.elapsed().unwrap_or_default();

                        if result.is_ok() {
                            scheduler_clone.mark_task_completed(worker_id, &task.id, duration);
                        } else {
                            scheduler_clone.mark_task_failed(worker_id, &task.id);
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        let stats = scheduler.get_stats();
        assert_eq!(stats.total_tasks_submitted, 8);
        assert_eq!(stats.total_tasks_completed, 8);
        assert_eq!(stats.pending_tasks, 0);
    }

    #[tokio::test]
    async fn test_batch_submission() {
        let scheduler = LockFreeTaskScheduler::new(2, 1024);

        let tasks = (0..5)
            .map(|i| {
                let config = TaskConfig::new(format!("batch_task_{i}"));
                (config, move || {
                    Box::pin(async move {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        Ok(())
                    }) as BoxFuture<'static, Result<()>>
                })
            })
            .collect();

        let receivers = scheduler.submit_batch(tasks).unwrap();
        assert_eq!(receivers.len(), 5);
        assert_eq!(scheduler.pending_task_count(), 5);
    }

    #[test]
    fn test_worker_state_tracking() {
        let scheduler = LockFreeTaskScheduler::new(2, 1024);

        // Initially, no worker states
        assert!(scheduler.get_worker_state(0).is_none());

        // Submit a task and steal it
        let config = TaskConfig::new("test_task");
        scheduler
            .submit_task(config, || Box::pin(async move { Ok(()) }))
            .unwrap();

        let task = scheduler.steal_task(0).unwrap();

        // Worker should be in working state
        if let Some(WorkerState::Working { task_id, .. }) = scheduler.get_worker_state(0) {
            assert_eq!(task_id, "test_task");
        } else {
            panic!("Expected worker to be in working state");
        }

        // Mark task as completed
        scheduler.mark_task_completed(0, &task.id, Duration::from_millis(10));

        // Worker should be idle
        assert_eq!(scheduler.get_worker_state(0), Some(WorkerState::Idle));
    }

    #[test]
    fn test_load_balance_metrics() {
        let scheduler = LockFreeTaskScheduler::new(3, 1024);

        // Submit tasks unevenly
        for i in 0..6 {
            let config = TaskConfig::new(format!("task_{i}"));
            scheduler
                .submit_task(config, || Box::pin(async move { Ok(()) }))
                .unwrap();
        }

        let metrics = scheduler.get_load_balance_metrics();
        assert_eq!(metrics.total_queued_tasks, 6);
        assert!(!metrics.queue_lengths.is_empty());
    }

    #[tokio::test]
    async fn test_scheduler_shutdown() {
        let scheduler = Arc::new(LockFreeTaskScheduler::new(2, 1024));

        // Submit a long-running task
        let config = TaskConfig::new("long_task");
        scheduler
            .submit_task(config, || {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok(())
                })
            })
            .unwrap();

        // Start a worker to process the task
        let scheduler_clone = Arc::clone(&scheduler);
        tokio::spawn(async move {
            if let Some(task) = scheduler_clone.steal_task(0) {
                if let Some(task_future) = task.task {
                    let start = SystemTime::now();
                    let result = task_future.await;
                    let duration = start.elapsed().unwrap_or_default();

                    if result.is_ok() {
                        scheduler_clone.mark_task_completed(0, &task.id, duration);
                    } else {
                        scheduler_clone.mark_task_failed(0, &task.id);
                    }
                }
            }
        });

        // Shutdown should complete successfully
        let result = scheduler.shutdown(Duration::from_secs(1)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_scheduler_shutdown_timeout() {
        let scheduler = LockFreeTaskScheduler::new(1, 1024);

        // Submit a task but don't process it
        let config = TaskConfig::new("unprocessed_task");
        scheduler
            .submit_task(config, || Box::pin(async move { Ok(()) }))
            .unwrap();

        // Shutdown should timeout
        let result = scheduler.shutdown(Duration::from_millis(10)).await;
        assert!(result.is_err());
    }
}
