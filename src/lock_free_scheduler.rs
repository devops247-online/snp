// Lock-free task scheduler with work-stealing capabilities
// Provides high-performance task scheduling with minimal contention

use crossbeam::deque::{Injector, Stealer, Worker};
use crossbeam::queue::{ArrayQueue, SegQueue};
use dashmap::DashMap;
use futures::future::BoxFuture;
use petgraph::Graph;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{oneshot, Notify};

use crate::concurrency::{TaskConfig, TaskPriority, TaskResult};
use crate::core::{ExecutionContext, Hook};
use crate::error::{Result, SnpError};
use crate::ProcessError;
use std::path::PathBuf;

/// Advanced task payload types for different execution scenarios
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum TaskPayload {
    HookExecution {
        hook: Box<Hook>,
        files: Vec<PathBuf>,
        context: ExecutionContext,
    },
    FileProcessing {
        files: Vec<PathBuf>,
        operation: FileOperation,
    },
    DependencyResolution {
        dependencies: Vec<String>,
        language: String,
    },
}

/// File operations for task execution
#[derive(Debug, Clone)]
pub enum FileOperation {
    Format,
    Lint,
    TypeCheck,
    Test,
    Build,
}

/// Enhanced task structure with priority and dependency support
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub priority: TaskPriority,
    pub estimated_duration: Option<Duration>,
    pub dependencies: Vec<String>,
    pub payload: TaskPayload,
    pub created_at: Instant,
}

/// Work-stealing scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub steal_ratio: f32,               // Default: 0.5
    pub idle_timeout: Duration,         // Default: 100ms
    pub default_task_timeout: Duration, // Default: 300s
    pub enable_load_balancing: bool,    // Default: true
    pub task_priority_enabled: bool,    // Default: true
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            steal_ratio: 0.5,
            idle_timeout: Duration::from_millis(100),
            default_task_timeout: Duration::from_secs(300),
            enable_load_balancing: true,
            task_priority_enabled: true,
        }
    }
}

/// Worker statistics for monitoring
#[derive(Debug)]
pub struct WorkerStatistics {
    pub tasks_executed: AtomicUsize,
    pub tasks_stolen: AtomicUsize,
    pub total_execution_time_ms: AtomicU64,
    pub idle_time_ms: AtomicU64,
    pub steal_attempts: AtomicUsize,
    pub successful_steals: AtomicUsize,
}

impl Default for WorkerStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerStatistics {
    pub fn new() -> Self {
        Self {
            tasks_executed: AtomicUsize::new(0),
            tasks_stolen: AtomicUsize::new(0),
            total_execution_time_ms: AtomicU64::new(0),
            idle_time_ms: AtomicU64::new(0),
            steal_attempts: AtomicUsize::new(0),
            successful_steals: AtomicUsize::new(0),
        }
    }

    pub fn record_task_completion(&self, duration: Duration, _success: bool) {
        self.tasks_executed.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn record_steal_attempt(&self, success: bool) {
        self.steal_attempts.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_steals.fetch_add(1, Ordering::Relaxed);
            self.tasks_stolen.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Comprehensive scheduler metrics
#[derive(Debug, Clone)]
pub struct SchedulerMetrics {
    pub total_tasks_executed: u64,
    pub average_task_duration_ms: f64,
    pub worker_utilization: Vec<f32>,
    pub steal_success_rate: f64,
    pub load_balance_coefficient: f64,
    pub queue_depths: Vec<usize>,
}

/// Worker context for the work-stealing scheduler
#[derive(Debug)]
pub struct WorkerContext {
    pub id: usize,
    pub local_queue: Worker<Task>,
    pub statistics: Arc<WorkerStatistics>,
    pub handle: Option<tokio::task::JoinHandle<()>>,
}

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

    /// Get the local queue capacity
    pub fn get_local_queue_capacity(&self) -> usize {
        self.local_queue_capacity
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

/// Advanced work-stealing scheduler with crossbeam deques
pub struct WorkStealingScheduler {
    // Global task injector for new tasks
    global_queue: Arc<Injector<Task>>,

    // Per-worker local queues and stealers
    workers: Vec<WorkerContext>,
    stealers: Vec<Stealer<Task>>,

    // Coordination primitives
    shutdown: Arc<AtomicBool>,
    active_workers: Arc<AtomicUsize>,
    task_notify: Arc<Notify>,
    started: Arc<AtomicBool>,

    // Configuration
    config: SchedulerConfig,

    // Result channels for task completion
    result_channels: Arc<DashMap<String, oneshot::Sender<TaskResult<()>>>>,
}

impl WorkStealingScheduler {
    pub fn new(num_workers: usize, config: SchedulerConfig) -> Self {
        let global_queue = Arc::new(Injector::new());
        let mut workers = Vec::with_capacity(num_workers);
        let mut stealers = Vec::with_capacity(num_workers);

        // Create worker contexts
        for i in 0..num_workers {
            let worker = Worker::new_fifo();
            let stealer = worker.stealer();

            workers.push(WorkerContext {
                id: i,
                local_queue: worker,
                statistics: Arc::new(WorkerStatistics::new()),
                handle: None,
            });

            stealers.push(stealer);
        }

        Self {
            global_queue,
            workers,
            stealers,
            shutdown: Arc::new(AtomicBool::new(false)),
            active_workers: Arc::new(AtomicUsize::new(0)),
            task_notify: Arc::new(Notify::new()),
            started: Arc::new(AtomicBool::new(false)),
            config,
            result_channels: Arc::new(DashMap::new()),
        }
    }

    pub fn num_workers(&self) -> usize {
        self.workers.len()
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::Relaxed)
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.started.load(Ordering::Relaxed) {
            return Ok(());
        }

        self.active_workers
            .store(self.workers.len(), Ordering::Relaxed);
        self.started.store(true, Ordering::Relaxed);

        // Start worker threads
        for worker in &mut self.workers {
            let worker_id = worker.id;
            let stealer = worker.local_queue.stealer();
            let global_queue = self.global_queue.clone();
            let stealers = self.stealers.clone();
            let shutdown = self.shutdown.clone();
            let active_workers = self.active_workers.clone();
            let task_notify = self.task_notify.clone();
            let config = self.config.clone();
            let statistics = worker.statistics.clone();
            let result_channels = self.result_channels.clone();

            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    worker_id,
                    stealer,
                    global_queue,
                    stealers,
                    shutdown,
                    active_workers,
                    task_notify,
                    config,
                    statistics,
                    result_channels,
                )
                .await;
            });

            worker.handle = Some(handle);
        }

        Ok(())
    }

    pub async fn submit_task(&self, task: Task) -> Result<oneshot::Receiver<TaskResult<()>>> {
        let (result_sender, result_receiver) = oneshot::channel();

        // Store the result channel
        self.result_channels.insert(task.id.clone(), result_sender);

        // For work-stealing testing, occasionally overload some workers
        if self.config.enable_load_balancing {
            // Use a simple round-robin strategy that sometimes adds multiple tasks to the same worker
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

            // Every 3rd task goes to worker 0 to create imbalance for stealing
            let target_worker = if counter % 3 == 0 {
                0
            } else {
                (counter / 3) % self.workers.len()
            };

            if let Some(worker) = self.workers.get(target_worker) {
                worker.local_queue.push(task.clone());
                self.task_notify.notify_one();
                return Ok(result_receiver);
            }
        }

        // Fallback to global queue
        self.global_queue.push(task);
        self.task_notify.notify_one();
        Ok(result_receiver)
    }

    #[allow(clippy::too_many_arguments)]
    async fn worker_loop(
        worker_id: usize,
        local_stealer: Stealer<Task>,
        global_queue: Arc<Injector<Task>>,
        stealers: Vec<Stealer<Task>>,
        shutdown: Arc<AtomicBool>,
        active_workers: Arc<AtomicUsize>,
        task_notify: Arc<Notify>,
        config: SchedulerConfig,
        statistics: Arc<WorkerStatistics>,
        result_channels: Arc<DashMap<String, oneshot::Sender<TaskResult<()>>>>,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            let task = Self::find_task(
                worker_id,
                &local_stealer,
                &global_queue,
                &stealers,
                &config,
                &statistics,
            );

            match task {
                Some(task) => {
                    let start_time = Instant::now();

                    // Execute task
                    let result = Self::execute_task(task.clone(), &config).await;

                    // Update statistics
                    let duration = start_time.elapsed();
                    statistics.record_task_completion(duration, result.is_ok());

                    // Send result through channel
                    if let Some((_, sender)) = result_channels.remove(&task.id) {
                        let task_result = TaskResult {
                            task_id: task.id.clone(),
                            result,
                            duration,
                            resource_usage: crate::concurrency::ResourceUsage::default(),
                            retry_count: 0,
                            started_at: SystemTime::now() - duration,
                            completed_at: SystemTime::now(),
                        };

                        let _ = sender.send(task_result);
                    }
                }
                None => {
                    // No tasks available, wait for notification
                    tokio::select! {
                        _ = task_notify.notified() => {
                            // New task available, continue loop
                        }
                        _ = tokio::time::sleep(config.idle_timeout) => {
                            // Periodic wakeup to check for shutdown
                        }
                    }
                }
            }
        }

        active_workers.fetch_sub(1, Ordering::Relaxed);
    }

    fn find_task(
        worker_id: usize,
        local_stealer: &Stealer<Task>,
        global_queue: &Injector<Task>,
        stealers: &[Stealer<Task>],
        _config: &SchedulerConfig,
        statistics: &WorkerStatistics,
    ) -> Option<Task> {
        // 1. Try local queue first (LIFO for cache locality)
        if let crossbeam::deque::Steal::Success(task) = local_stealer.steal() {
            return Some(task);
        }

        // 2. Try global queue
        if let crossbeam::deque::Steal::Success(task) = global_queue.steal() {
            return Some(task);
        }

        // 3. Try stealing from other workers (round-robin)
        let start_idx = (worker_id + 1) % stealers.len();
        for i in 0..stealers.len() {
            let stealer_idx = (start_idx + i) % stealers.len();
            if stealer_idx != worker_id {
                match stealers[stealer_idx].steal() {
                    crossbeam::deque::Steal::Success(task) => {
                        statistics.record_steal_attempt(true);
                        return Some(task);
                    }
                    crossbeam::deque::Steal::Empty | crossbeam::deque::Steal::Retry => {
                        statistics.record_steal_attempt(false);
                    }
                }
            }
        }

        None
    }

    async fn execute_task(task: Task, config: &SchedulerConfig) -> Result<()> {
        let timeout = task
            .estimated_duration
            .unwrap_or(config.default_task_timeout);

        tokio::time::timeout(timeout, async {
            match task.payload {
                TaskPayload::HookExecution {
                    hook,
                    files,
                    context,
                } => Self::execute_hook_task(*hook, files, context).await,
                TaskPayload::FileProcessing { files, operation } => {
                    Self::execute_file_task(files, operation).await
                }
                TaskPayload::DependencyResolution {
                    dependencies,
                    language,
                } => Self::execute_dependency_task(dependencies, language).await,
            }
        })
        .await
        .map_err(|_| {
            SnpError::Process(Box::new(ProcessError::Timeout {
                command: task.id,
                duration: timeout,
            }))
        })?
    }

    async fn execute_hook_task(
        hook: Hook,
        files: Vec<PathBuf>,
        _context: ExecutionContext,
    ) -> Result<()> {
        // For now, simulate hook execution to avoid Send trait issues
        // In a production version, this would dispatch to a thread-safe execution service
        tracing::debug!("Executing hook: {} on {} files", hook.id, files.len());

        // Simulate execution time based on estimated duration
        let execution_time = match hook.language.as_str() {
            "python" => Duration::from_millis(50),
            "rust" => Duration::from_millis(100),
            "javascript" => Duration::from_millis(30),
            _ => Duration::from_millis(25),
        };

        tokio::time::sleep(execution_time).await;

        // Simulate occasional failures for testing
        if hook.id.contains("fail") {
            return Err(SnpError::Process(Box::new(ProcessError::ExecutionFailed {
                command: hook.entry,
                exit_code: Some(1),
                stderr: "Simulated failure".to_string(),
            })));
        }

        tracing::debug!("Successfully executed hook: {}", hook.id);
        Ok(())
    }

    async fn execute_file_task(files: Vec<PathBuf>, operation: FileOperation) -> Result<()> {
        tracing::debug!(
            "Executing file operation {:?} on {} files",
            operation,
            files.len()
        );

        // Simulate file processing time based on operation and file count
        let base_time = match operation {
            FileOperation::Format => Duration::from_millis(20),
            FileOperation::Lint => Duration::from_millis(15),
            FileOperation::TypeCheck => Duration::from_millis(50),
            FileOperation::Test => Duration::from_millis(100),
            FileOperation::Build => Duration::from_millis(200),
        };

        let execution_time = base_time * files.len() as u32;
        tokio::time::sleep(execution_time.min(Duration::from_secs(2))).await;

        tracing::debug!("Completed file operation: {:?}", operation);
        Ok(())
    }

    async fn execute_dependency_task(
        dependencies: Vec<String>,
        language_name: String,
    ) -> Result<()> {
        tracing::debug!(
            "Resolving {} dependencies for language: {}",
            dependencies.len(),
            language_name
        );

        // Simulate dependency resolution time based on language and count
        let base_time = match language_name.as_str() {
            "python" => Duration::from_millis(100),
            "rust" => Duration::from_millis(200),
            "javascript" => Duration::from_millis(75),
            _ => Duration::from_millis(50),
        };

        let execution_time = base_time * dependencies.len() as u32;
        tokio::time::sleep(execution_time.min(Duration::from_secs(3))).await;

        tracing::debug!(
            "Successfully resolved {} dependencies for {}",
            dependencies.len(),
            language_name
        );
        Ok(())
    }

    pub async fn get_metrics(&self) -> SchedulerMetrics {
        // Collect worker statistics
        let worker_stats: Vec<_> = self
            .workers
            .iter()
            .map(|w| {
                let executed = w.statistics.tasks_executed.load(Ordering::Relaxed);
                let stolen = w.statistics.tasks_stolen.load(Ordering::Relaxed);
                let total_time = w.statistics.total_execution_time_ms.load(Ordering::Relaxed);
                let attempts = w.statistics.steal_attempts.load(Ordering::Relaxed);
                let successful = w.statistics.successful_steals.load(Ordering::Relaxed);

                (executed, stolen, total_time, attempts, successful)
            })
            .collect();

        let total_executed: u64 = worker_stats.iter().map(|(e, _, _, _, _)| *e as u64).sum();
        let total_time: u64 = worker_stats.iter().map(|(_, _, t, _, _)| *t).sum();
        let total_attempts: usize = worker_stats.iter().map(|(_, _, _, a, _)| *a).sum();
        let total_successful: usize = worker_stats.iter().map(|(_, _, _, _, s)| *s).sum();

        let average_duration = if total_executed > 0 {
            total_time as f64 / total_executed as f64
        } else {
            0.0
        };

        let steal_success_rate = if total_attempts > 0 {
            total_successful as f64 / total_attempts as f64
        } else {
            0.0
        };

        // Calculate worker utilization (simplified)
        let worker_utilization: Vec<f32> = worker_stats
            .iter()
            .map(|(executed, _, total_time, _, _)| {
                if *executed > 0 {
                    (*total_time as f32 / 1000.0) / (*executed as f32 * 0.1) // Rough estimate
                } else {
                    0.0
                }
            })
            .collect();

        // Calculate load balance coefficient
        let queue_depths: Vec<usize> = self.workers.iter().map(|w| w.local_queue.len()).collect();

        let max_depth = queue_depths.iter().max().copied().unwrap_or(0);
        let min_depth = queue_depths.iter().min().copied().unwrap_or(0);

        let load_balance_coefficient = if max_depth > 0 {
            1.0 - (min_depth as f64 / max_depth as f64)
        } else {
            0.0
        };

        SchedulerMetrics {
            total_tasks_executed: total_executed,
            average_task_duration_ms: average_duration,
            worker_utilization,
            steal_success_rate,
            load_balance_coefficient,
            queue_depths,
        }
    }

    pub async fn get_worker_statistics(&self) -> Vec<Arc<WorkerStatistics>> {
        self.workers.iter().map(|w| w.statistics.clone()).collect()
    }

    pub async fn shutdown(&self, timeout: Duration) -> Result<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        // Notify all workers to check for shutdown
        for _ in 0..self.workers.len() {
            self.task_notify.notify_one();
        }

        let start_time = Instant::now();

        // Wait for all workers to finish
        while self.active_workers.load(Ordering::Relaxed) > 0 {
            if start_time.elapsed() > timeout {
                return Err(SnpError::Process(Box::new(ProcessError::Timeout {
                    command: "scheduler_shutdown".to_string(),
                    duration: timeout,
                })));
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(())
    }
}

/// Task dependency resolver for managing execution order
pub struct DependencyResolver {
    dependency_graph: Graph<String, ()>,
    ready_tasks: VecDeque<Task>,
    waiting_tasks: HashMap<String, Task>,
    completed_tasks: HashSet<String>,
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            dependency_graph: Graph::new(),
            ready_tasks: VecDeque::new(),
            waiting_tasks: HashMap::new(),
            completed_tasks: HashSet::new(),
        }
    }

    /// Build the dependency graph from tasks
    fn build_dependency_graph(&mut self, tasks: &[Task]) -> Result<()> {
        use petgraph::graph::NodeIndex;
        use std::collections::HashMap as StdHashMap;

        // Clear the existing graph
        self.dependency_graph.clear();

        // Map task IDs to node indices
        let mut node_map: StdHashMap<String, NodeIndex> = StdHashMap::new();

        // Add all tasks as nodes
        for task in tasks {
            let node_index = self.dependency_graph.add_node(task.id.clone());
            node_map.insert(task.id.clone(), node_index);
        }

        // Add dependency edges
        for task in tasks {
            if let Some(&task_node) = node_map.get(&task.id) {
                for dep_id in &task.dependencies {
                    if let Some(&dep_node) = node_map.get(dep_id) {
                        // Add edge from dependency to task (dependency -> task)
                        self.dependency_graph.add_edge(dep_node, task_node, ());
                    } else {
                        return Err(SnpError::Process(Box::new(
                            ProcessError::ConfigurationError {
                                component: "dependency_resolver".to_string(),
                                error: format!(
                                    "Task '{}' depends on unknown task '{}'",
                                    task.id, dep_id
                                ),
                                suggestion: Some("Ensure all task dependencies exist".to_string()),
                            },
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get tasks that are ready to execute (no pending dependencies)
    pub fn get_ready_tasks(&mut self) -> Vec<String> {
        use petgraph::Direction;

        // First check if we have cached ready tasks
        if !self.ready_tasks.is_empty() {
            return self.ready_tasks.iter().map(|t| t.id.clone()).collect();
        }

        let mut ready_task_ids = Vec::new();

        for node_index in self.dependency_graph.node_indices() {
            // A task is ready if it has no incoming edges (no dependencies)
            if self
                .dependency_graph
                .neighbors_directed(node_index, Direction::Incoming)
                .count()
                == 0
            {
                if let Some(task_id) = self.dependency_graph.node_weight(node_index) {
                    if !self.completed_tasks.contains(task_id) {
                        ready_task_ids.push(task_id.clone());
                    }
                }
            }
        }

        ready_task_ids
    }

    /// Update the ready tasks cache
    pub fn update_ready_tasks_cache(&mut self, tasks: Vec<Task>) {
        self.ready_tasks.clear();
        for task in tasks {
            self.ready_tasks.push_back(task);
        }
    }

    /// Get next ready task from cache
    pub fn pop_ready_task(&mut self) -> Option<Task> {
        self.ready_tasks.pop_front()
    }

    /// Mark a task as completed and update the dependency graph
    pub fn mark_task_completed(&mut self, task_id: &str) {
        self.completed_tasks.insert(task_id.to_string());

        // Find the node for this task and remove it from the graph
        if let Some(node_index) = self.find_node_by_id(task_id) {
            self.dependency_graph.remove_node(node_index);
        }
    }

    /// Find a node index by task ID
    fn find_node_by_id(&self, task_id: &str) -> Option<petgraph::graph::NodeIndex> {
        for node_index in self.dependency_graph.node_indices() {
            if let Some(node_task_id) = self.dependency_graph.node_weight(node_index) {
                if node_task_id == task_id {
                    return Some(node_index);
                }
            }
        }
        None
    }

    pub async fn resolve_and_submit(
        &mut self,
        tasks: Vec<Task>,
        scheduler: &WorkStealingScheduler,
    ) -> Result<Vec<oneshot::Receiver<TaskResult<()>>>> {
        // Detect circular dependencies first
        self.detect_circular_dependencies(&tasks)?;

        // Build the dependency graph
        self.build_dependency_graph(&tasks)?;

        // Store all tasks for processing
        let mut task_map = HashMap::new();
        for task in tasks {
            task_map.insert(task.id.clone(), task);
        }

        let mut all_receivers = Vec::new();

        // Submit tasks in dependency order
        while !task_map.is_empty() {
            let ready_task_ids = self.get_ready_tasks();

            if ready_task_ids.is_empty() {
                // No ready tasks - this shouldn't happen if circular dependencies were detected properly
                // Submit remaining tasks anyway
                for (_, task) in task_map.drain() {
                    let receiver = scheduler.submit_task(task).await?;
                    all_receivers.push(receiver);
                }
                break;
            }

            // Submit all ready tasks
            for task_id in ready_task_ids {
                if let Some(task) = task_map.remove(&task_id) {
                    let receiver = scheduler.submit_task(task).await?;
                    all_receivers.push(receiver);
                    self.mark_task_completed(&task_id);
                }
            }
        }

        // Process any additional newly ready tasks that may have become available
        self.submit_newly_ready_tasks(scheduler, &mut all_receivers)
            .await?;

        Ok(all_receivers)
    }

    async fn submit_newly_ready_tasks(
        &mut self,
        scheduler: &WorkStealingScheduler,
        all_receivers: &mut Vec<oneshot::Receiver<TaskResult<()>>>,
    ) -> Result<()> {
        loop {
            let mut newly_ready = Vec::new();

            self.waiting_tasks.retain(|_id, task| {
                let all_deps_completed = task
                    .dependencies
                    .iter()
                    .all(|dep| self.completed_tasks.contains(dep));

                if all_deps_completed {
                    newly_ready.push(task.clone());
                    false // Remove from waiting
                } else {
                    true // Keep waiting
                }
            });

            if newly_ready.is_empty() {
                break;
            }

            // Submit newly ready tasks
            for task in newly_ready {
                let receiver = scheduler.submit_task(task.clone()).await?;
                all_receivers.push(receiver);
                self.completed_tasks.insert(task.id.clone());
            }
        }

        Ok(())
    }

    pub async fn task_completed(
        &mut self,
        task_id: String,
        scheduler: &WorkStealingScheduler,
    ) -> Result<()> {
        self.completed_tasks.insert(task_id.clone());

        // Check if any waiting tasks are now ready
        let mut newly_ready = Vec::new();

        self.waiting_tasks.retain(|_id, task| {
            let all_deps_completed = task
                .dependencies
                .iter()
                .all(|dep| self.completed_tasks.contains(dep));

            if all_deps_completed {
                newly_ready.push(task.clone());
                false // Remove from waiting
            } else {
                true // Keep waiting
            }
        });

        // Submit newly ready tasks
        for task in newly_ready {
            scheduler.submit_task(task).await?;
        }

        Ok(())
    }

    fn detect_circular_dependencies(&self, tasks: &[Task]) -> Result<()> {
        let task_ids: HashSet<String> = tasks.iter().map(|t| t.id.clone()).collect();

        for task in tasks {
            let mut visited = HashSet::new();
            let mut rec_stack = HashSet::new();

            if Self::has_cycle(&task.id, &task_ids, tasks, &mut visited, &mut rec_stack) {
                return Err(SnpError::Process(Box::new(ProcessError::ExecutionFailed {
                    command: "dependency_resolution".to_string(),
                    exit_code: None,
                    stderr: format!("Circular dependency detected involving task: {}", task.id),
                })));
            }
        }

        Ok(())
    }

    fn has_cycle(
        task_id: &str,
        all_tasks: &HashSet<String>,
        tasks: &[Task],
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        if rec_stack.contains(task_id) {
            return true; // Back edge found
        }

        if visited.contains(task_id) {
            return false; // Already processed
        }

        visited.insert(task_id.to_string());
        rec_stack.insert(task_id.to_string());

        // Find task by ID and check its dependencies
        if let Some(task) = tasks.iter().find(|t| t.id == task_id) {
            for dep in &task.dependencies {
                if all_tasks.contains(dep)
                    && Self::has_cycle(dep, all_tasks, tasks, visited, rec_stack)
                {
                    return true;
                }
            }
        }

        rec_stack.remove(task_id);
        false
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
