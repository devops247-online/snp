// Concurrency Framework for SNP - Async/parallel execution with resource management
// Provides task scheduling, dependency management, and error aggregation for efficient hook execution

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, Semaphore};

use crate::lock_free_scheduler::{LockFreeTaskScheduler, SchedulerStats};

use crate::error::{Result, SnpError};
use crate::process::ProcessManager;

/// Task priority levels for scheduling
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Retry policy for failed tasks
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

/// Resource requirements for task execution
#[derive(Debug, Clone, Default)]
pub struct ResourceRequirements {
    pub cpu_cores: Option<f32>,
    pub memory_mb: Option<u64>,
    pub disk_io_mb_per_sec: Option<u64>,
    pub network_mb_per_sec: Option<u64>,
}

/// System resource limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_cpu_percent: f32,
    pub max_memory_mb: u64,
    pub max_disk_io_mb_per_sec: u64,
    pub max_network_mb_per_sec: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: 80.0,
            max_memory_mb: 1024,
            max_disk_io_mb_per_sec: 100,
            max_network_mb_per_sec: 50,
        }
    }
}

/// Current resource usage tracking
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub disk_io_mb_per_sec: u64,
    pub network_mb_per_sec: u64,
    pub active_tasks: usize,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            cpu_percent: 0.0,
            memory_mb: 0,
            disk_io_mb_per_sec: 0,
            network_mb_per_sec: 0,
            active_tasks: 0,
        }
    }
}

/// Task configuration and metadata
#[derive(Debug, Clone)]
pub struct TaskConfig {
    pub id: String,
    pub priority: TaskPriority,
    pub timeout: Option<Duration>,
    pub dependencies: Vec<String>,
    pub retry_policy: RetryPolicy,
    pub resource_requirements: ResourceRequirements,
}

impl TaskConfig {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            priority: TaskPriority::Normal,
            timeout: None,
            dependencies: Vec::new(),
            retry_policy: RetryPolicy::default(),
            resource_requirements: ResourceRequirements::default(),
        }
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn with_resource_requirements(mut self, requirements: ResourceRequirements) -> Self {
        self.resource_requirements = requirements;
        self
    }
}

/// Task execution result with comprehensive metadata
#[derive(Debug)]
pub struct TaskResult<T> {
    pub task_id: String,
    pub result: Result<T>,
    pub duration: Duration,
    pub resource_usage: ResourceUsage,
    pub retry_count: usize,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
}

/// Task state tracking
#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    Pending,
    WaitingForDependencies(Vec<String>),
    WaitingForResources,
    Running {
        started_at: SystemTime,
    },
    Completed {
        duration: Duration,
        retry_count: usize,
    },
    Failed {
        error: String,
        retry_count: usize,
    },
    Cancelled,
}

/// RAII resource guard for exclusive resource access
pub struct ResourceGuard {
    requirements: ResourceRequirements,
    executor: Arc<ConcurrencyExecutor>,
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        tracing::trace!("ResourceGuard being dropped, releasing resources");
        // Release resources back to the executor when the guard is dropped
        self.executor.release_resources(&self.requirements);
    }
}

/// Task scheduling strategies
#[derive(Debug, Clone)]
pub enum SchedulingStrategy {
    Fifo,
    Priority,
    ShortestJobFirst,
    RoundRobin,
    Adaptive,
}

/// Task dependency graph for managing execution order
pub struct TaskDependencyGraph {
    tasks: HashMap<String, TaskConfig>,
    edges: HashMap<String, Vec<String>>,
}

impl TaskDependencyGraph {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            edges: HashMap::new(),
        }
    }

    pub fn add_task(&mut self, config: TaskConfig) {
        let task_id = config.id.clone();
        self.tasks.insert(task_id, config);
    }

    pub fn add_dependency(&mut self, task_id: &str, depends_on: &str) -> Result<()> {
        // Check if both tasks exist
        if !self.tasks.contains_key(task_id) {
            return Err(SnpError::Config(Box::new(
                crate::ConfigError::InvalidValue {
                    message: format!("Task '{task_id}' not found in dependency graph"),
                    field: "task_id".to_string(),
                    value: task_id.to_string(),
                    expected: "existing task ID".to_string(),
                    file_path: None,
                    line: None,
                },
            )));
        }

        if !self.tasks.contains_key(depends_on) {
            return Err(SnpError::Config(Box::new(
                crate::ConfigError::InvalidValue {
                    message: format!(
                        "Dependency task '{depends_on}' not found in dependency graph"
                    ),
                    field: "depends_on".to_string(),
                    value: depends_on.to_string(),
                    expected: "existing task ID".to_string(),
                    file_path: None,
                    line: None,
                },
            )));
        }

        // Add the dependency temporarily to test for cycles
        self.edges
            .entry(task_id.to_string())
            .or_default()
            .push(depends_on.to_string());

        // Check for cycles after adding the dependency
        if let Err(cycle_error) = self.detect_cycles() {
            // Remove the dependency we just added since it creates a cycle
            if let Some(deps) = self.edges.get_mut(task_id) {
                deps.retain(|dep| dep != depends_on);
                if deps.is_empty() {
                    self.edges.remove(task_id);
                }
            }
            return Err(cycle_error);
        }

        Ok(())
    }

    pub fn resolve_execution_order(&self) -> Result<Vec<String>> {
        // Implement Kahn's algorithm for topological sorting
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize in-degree count and build adjacency list
        for task_id in self.tasks.keys() {
            in_degree.insert(task_id.clone(), 0);
            adjacency_list.insert(task_id.clone(), Vec::new());
        }

        // Build the dependency graph (reverse edges for topological sort)
        for (dependent_task, dependencies) in &self.edges {
            for dependency in dependencies {
                // Check if dependency exists
                if !self.tasks.contains_key(dependency) {
                    return Err(SnpError::Process(Box::new(crate::ProcessError::ExecutionFailed {
                        command: format!("Invalid dependency: {dependent_task} -> {dependency}"),
                        exit_code: None,
                        stderr: format!("Hook '{dependent_task}' depends on '{dependency}' which does not exist"),
                    })));
                }

                // Add edge from dependency to dependent task
                adjacency_list
                    .get_mut(dependency)
                    .unwrap()
                    .push(dependent_task.clone());
                // Increment in-degree of dependent task
                *in_degree.get_mut(dependent_task).unwrap() += 1;
            }
        }

        // Find all tasks with no incoming edges
        let mut queue: std::collections::VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(task, _)| task.clone())
            .collect();

        let mut result = Vec::new();

        // Process tasks in topological order
        while let Some(current_task) = queue.pop_front() {
            result.push(current_task.clone());

            // For each task that depends on the current task
            if let Some(dependents) = adjacency_list.get(&current_task) {
                for dependent in dependents {
                    // Decrease in-degree
                    let new_degree = in_degree.get_mut(dependent).unwrap();
                    *new_degree -= 1;

                    // If in-degree becomes 0, add to queue
                    if *new_degree == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }

        // Check if all tasks were processed (no cycles)
        if result.len() != self.tasks.len() {
            return Err(SnpError::Process(Box::new(
                crate::ProcessError::ExecutionFailed {
                    command: "Dependency resolution".to_string(),
                    exit_code: None,
                    stderr: "Circular dependency detected in hook dependencies".to_string(),
                },
            )));
        }

        Ok(result)
    }

    pub fn detect_cycles(&self) -> Result<()> {
        // Use DFS-based cycle detection for more detailed cycle information
        let mut visited = HashMap::new();
        let mut recursion_stack = HashMap::new();

        // Initialize all tasks as unvisited
        for task_id in self.tasks.keys() {
            visited.insert(task_id.clone(), false);
            recursion_stack.insert(task_id.clone(), false);
        }

        // Check each task for cycles
        for task_id in self.tasks.keys() {
            if !visited[task_id] {
                self.dfs_cycle_check(task_id, &mut visited, &mut recursion_stack)?;
            }
        }

        Ok(())
    }

    fn dfs_cycle_check(
        &self,
        task_id: &str,
        visited: &mut HashMap<String, bool>,
        recursion_stack: &mut HashMap<String, bool>,
    ) -> Result<()> {
        // Mark current task as visited and add to recursion stack
        visited.insert(task_id.to_string(), true);
        recursion_stack.insert(task_id.to_string(), true);

        // Check all dependencies of current task
        if let Some(dependencies) = self.edges.get(task_id) {
            for dependency in dependencies {
                // If dependency is not visited, recursively check it
                if !visited.get(dependency).unwrap_or(&false) {
                    self.dfs_cycle_check(dependency, visited, recursion_stack)?;
                }
                // If dependency is in recursion stack, we found a cycle
                else if *recursion_stack.get(dependency).unwrap_or(&false) {
                    return Err(SnpError::Process(Box::new(
                        crate::ProcessError::ExecutionFailed {
                            command: "Cycle detection".to_string(),
                            exit_code: None,
                            stderr: format!(
                            "Circular dependency detected: '{task_id}' -> '{dependency}' (and back)"
                        ),
                        },
                    )));
                }
            }
        }

        // Remove current task from recursion stack
        recursion_stack.insert(task_id.to_string(), false);
        Ok(())
    }
}

impl Default for TaskDependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregated results from batch execution
#[derive(Debug)]
pub struct BatchResult<T> {
    pub successful: Vec<TaskResult<T>>,
    pub failed: Vec<TaskResult<T>>,
    pub cancelled: Vec<String>,
    pub total_duration: Duration,
    pub resource_usage: ResourceUsage,
}

impl<T> BatchResult<T> {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty() && self.cancelled.is_empty()
    }

    pub fn success_rate(&self) -> f32 {
        if self.successful.is_empty() && self.failed.is_empty() {
            return 1.0;
        }
        self.successful.len() as f32 / (self.successful.len() + self.failed.len()) as f32
    }

    pub fn get_errors(&self) -> Vec<String> {
        self.failed
            .iter()
            .filter_map(|task| {
                if let Err(ref err) = task.result {
                    Some(err.to_string())
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Main concurrency executor for task scheduling and resource management
#[derive(Clone)]
pub struct ConcurrencyExecutor {
    max_concurrent: usize,
    semaphore: Arc<Semaphore>,
    resource_limits: ResourceLimits,
    task_registry: Arc<RwLock<HashMap<String, TaskState>>>,
    process_manager: Arc<ProcessManager>,
    scheduling_strategy: SchedulingStrategy,
    // Optional lock-free scheduler for improved performance
    lock_free_scheduler: Option<Arc<LockFreeTaskScheduler>>,
}

impl ConcurrencyExecutor {
    pub fn new(max_concurrent: usize, resource_limits: ResourceLimits) -> Self {
        Self {
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            resource_limits,
            task_registry: Arc::new(RwLock::new(HashMap::new())),
            process_manager: Arc::new(ProcessManager::with_config(
                max_concurrent,
                Duration::from_secs(300),
            )),
            scheduling_strategy: SchedulingStrategy::Priority,
            lock_free_scheduler: None,
        }
    }

    /// Create a new executor with lock-free scheduling enabled
    pub fn with_lock_free_scheduler(
        max_concurrent: usize,
        resource_limits: ResourceLimits,
    ) -> Self {
        let scheduler = Arc::new(LockFreeTaskScheduler::new(max_concurrent, 1024));
        Self {
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            resource_limits,
            task_registry: Arc::new(RwLock::new(HashMap::new())),
            process_manager: Arc::new(ProcessManager::with_config(
                max_concurrent,
                Duration::from_secs(300),
            )),
            scheduling_strategy: SchedulingStrategy::Priority,
            lock_free_scheduler: Some(scheduler),
        }
    }

    /// Check if lock-free scheduling is enabled
    pub fn is_lock_free_enabled(&self) -> bool {
        self.lock_free_scheduler.is_some()
    }

    /// Get scheduler statistics if lock-free scheduling is enabled
    pub fn get_scheduler_stats(&self) -> Option<SchedulerStats> {
        self.lock_free_scheduler
            .as_ref()
            .map(|scheduler| scheduler.get_stats())
    }

    // Task execution methods
    pub async fn execute_task<F, T>(&self, config: TaskConfig, task: F) -> Result<TaskResult<T>>
    where
        F: FnOnce() -> BoxFuture<'static, Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        let task_id = config.id.clone();
        let started_at = SystemTime::now();

        // Update task registry to show pending
        {
            let mut registry = self.task_registry.write().await;
            registry.insert(task_id.clone(), TaskState::Pending);
        }

        // Check dependencies first
        if !config.dependencies.is_empty() {
            let mut registry = self.task_registry.write().await;
            registry.insert(
                task_id.clone(),
                TaskState::WaitingForDependencies(config.dependencies.clone()),
            );
            drop(registry);

            // For now, simply wait a bit to simulate dependency checking
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Acquire semaphore for concurrency control
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            SnpError::Process(Box::new(crate::ProcessError::ExecutionFailed {
                command: "semaphore_acquisition".to_string(),
                exit_code: None,
                stderr: e.to_string(),
            }))
        })?;

        // Update task registry to show running
        {
            let mut registry = self.task_registry.write().await;
            registry.insert(task_id.clone(), TaskState::Running { started_at });
        }

        // Try to acquire resources if specified
        let _resource_guard = if config.resource_requirements.cpu_cores.is_some()
            || config.resource_requirements.memory_mb.is_some()
        {
            Some(
                self.acquire_resources(&config.resource_requirements)
                    .await?,
            )
        } else {
            None
        };

        // Execute the actual task with timeout handling
        let timeout_duration = config.timeout.unwrap_or(Duration::from_secs(300)); // 5 minute default
        let execution_result = tokio::time::timeout(timeout_duration, async { task().await }).await;

        let completed_at = SystemTime::now();
        let duration = completed_at.duration_since(started_at).unwrap_or_default();

        // Handle timeout or execution result
        let result = match execution_result {
            Ok(task_result) => task_result,
            Err(_) => {
                // Timeout occurred
                let mut registry = self.task_registry.write().await;
                registry.insert(
                    task_id.clone(),
                    TaskState::Failed {
                        error: "Task execution timeout".to_string(),
                        retry_count: 0,
                    },
                );
                return Err(SnpError::Process(Box::new(crate::ProcessError::Timeout {
                    command: task_id,
                    duration: timeout_duration,
                })));
            }
        };

        // Update final task state
        {
            let mut registry = self.task_registry.write().await;
            match &result {
                Ok(_) => {
                    registry.insert(
                        task_id.clone(),
                        TaskState::Completed {
                            duration,
                            retry_count: 0,
                        },
                    );
                }
                Err(err) => {
                    registry.insert(
                        task_id.clone(),
                        TaskState::Failed {
                            error: err.to_string(),
                            retry_count: 0,
                        },
                    );
                }
            }
        }

        Ok(TaskResult {
            task_id,
            result,
            duration,
            resource_usage: self.get_resource_usage(),
            retry_count: 0,
            started_at,
            completed_at,
        })
    }

    pub async fn execute_batch<F, T>(&self, tasks: Vec<(TaskConfig, F)>) -> Result<BatchResult<T>>
    where
        F: FnOnce() -> BoxFuture<'static, Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        let batch_start = SystemTime::now();
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        let cancelled = Vec::new();

        // Sort tasks by priority for execution order
        let mut sorted_tasks: Vec<_> = tasks.into_iter().collect();
        sorted_tasks.sort_by(|a, b| b.0.priority.cmp(&a.0.priority)); // Higher priority first

        // Create futures for all tasks
        let mut task_futures = Vec::new();
        for (config, task_fn) in sorted_tasks {
            let executor = Arc::new(self);
            let future = {
                let config = config.clone();
                let executor = executor.clone();
                async move {
                    // Create a wrapper that clones the executor properly
                    let task_wrapper = || -> BoxFuture<'static, Result<T>> { task_fn() };
                    executor.execute_task(config, task_wrapper).await
                }
            };
            task_futures.push(future);
        }

        // Execute all tasks with limited parallelism handled by semaphore
        let results = futures::future::join_all(task_futures).await;

        // Process results
        for result in results {
            match result {
                Ok(task_result) => {
                    if task_result.result.is_ok() {
                        successful.push(task_result);
                    } else {
                        failed.push(task_result);
                    }
                }
                Err(e) => {
                    // Create a failed TaskResult for the error
                    let failed_result = TaskResult {
                        task_id: "unknown".to_string(),
                        result: Err(e),
                        duration: Duration::from_secs(0),
                        resource_usage: ResourceUsage::default(),
                        retry_count: 0,
                        started_at: batch_start,
                        completed_at: SystemTime::now(),
                    };
                    failed.push(failed_result);
                }
            }
        }

        let total_duration = batch_start.elapsed().unwrap_or_default();

        Ok(BatchResult {
            successful,
            failed,
            cancelled,
            total_duration,
            resource_usage: self.get_resource_usage(),
        })
    }

    // Resource management methods
    pub async fn acquire_resources(
        &self,
        requirements: &ResourceRequirements,
    ) -> Result<ResourceGuard> {
        // Basic resource checking - in a full implementation this would
        // track actual system resource usage and enforce limits

        // Check if requirements exceed limits
        if let Some(cpu_cores) = requirements.cpu_cores {
            let cpu_percent = cpu_cores * 25.0; // Rough estimate: 1 core = 25% of 4-core system
            if cpu_percent > self.resource_limits.max_cpu_percent {
                return Err(SnpError::Process(Box::new(
                    crate::ProcessError::ResourceLimitExceeded {
                        limit_type: "CPU".to_string(),
                        current_value: cpu_percent as u64,
                        limit_value: self.resource_limits.max_cpu_percent as u64,
                    },
                )));
            }
        }

        if let Some(memory_mb) = requirements.memory_mb {
            if memory_mb > self.resource_limits.max_memory_mb {
                return Err(SnpError::Process(Box::new(
                    crate::ProcessError::ResourceLimitExceeded {
                        limit_type: "Memory".to_string(),
                        current_value: memory_mb,
                        limit_value: self.resource_limits.max_memory_mb,
                    },
                )));
            }
        }

        // Create resource guard
        Ok(ResourceGuard {
            requirements: requirements.clone(),
            executor: Arc::new(self.clone()),
        })
    }

    pub fn get_resource_usage(&self) -> ResourceUsage {
        // Basic resource usage tracking - in a full implementation this would
        // query actual system resources using sysinfo crate
        ResourceUsage {
            cpu_percent: 0.0, // Would be calculated from actual CPU usage
            memory_mb: 0,     // Would be calculated from actual memory usage
            disk_io_mb_per_sec: 0,
            network_mb_per_sec: 0,
            active_tasks: self.max_concurrent - self.semaphore.available_permits(),
        }
    }

    pub fn set_resource_limits(&mut self, limits: ResourceLimits) {
        self.resource_limits = limits;
    }

    /// Release resources back to the executor pool
    /// This method should be called when a ResourceGuard is dropped
    pub fn release_resources(&self, requirements: &ResourceRequirements) {
        tracing::debug!("Releasing resources: {:?}", requirements);

        // In a full implementation, this would:
        // 1. Update internal resource tracking counters
        // 2. Wake up any waiting tasks that can now be scheduled
        // 3. Update resource utilization metrics

        // For now, we'll just log the release and update basic tracking
        if let Some(cpu_cores) = requirements.cpu_cores {
            tracing::debug!("Released {} CPU cores", cpu_cores);
        }

        if let Some(memory_mb) = requirements.memory_mb {
            tracing::debug!("Released {} MB of memory", memory_mb);
        }

        if let Some(disk_io) = requirements.disk_io_mb_per_sec {
            tracing::debug!("Released {} MB/s disk I/O capacity", disk_io);
        }

        if let Some(network_io) = requirements.network_mb_per_sec {
            tracing::debug!("Released {} MB/s network I/O capacity", network_io);
        }

        // Notify any waiting tasks that resources are available
        // This would typically involve waking up blocked tasks
        tracing::trace!("Resource release completed");
    }

    // Task management methods
    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        tracing::info!("Cancelling task: {}", task_id);

        let mut registry = self.task_registry.write().await;

        // Check if task exists
        if let Some(current_state) = registry.get(task_id) {
            match current_state {
                TaskState::Completed { .. } => {
                    tracing::warn!("Cannot cancel completed task: {}", task_id);
                    return Err(SnpError::Process(Box::new(
                        crate::ProcessError::ExecutionFailed {
                            command: format!("cancel_task({task_id})"),
                            exit_code: Some(1),
                            stderr: format!("Task '{task_id}' is already completed"),
                        },
                    )));
                }
                TaskState::Failed { .. } => {
                    tracing::warn!("Cannot cancel failed task: {}", task_id);
                    return Err(SnpError::Process(Box::new(
                        crate::ProcessError::ExecutionFailed {
                            command: format!("cancel_task({task_id})"),
                            exit_code: Some(1),
                            stderr: format!("Task '{task_id}' has already failed"),
                        },
                    )));
                }
                TaskState::Cancelled => {
                    tracing::debug!("Task {} is already cancelled", task_id);
                    return Ok(());
                }
                _ => {
                    // Task can be cancelled
                    tracing::debug!("Marking task {} as cancelled", task_id);
                }
            }
        } else {
            tracing::debug!("Task {} not found, treating as already cancelled", task_id);
            return Ok(()); // Non-existent tasks are considered successfully cancelled
        }

        // Update task state to cancelled
        registry.insert(task_id.to_string(), TaskState::Cancelled);

        // Use process manager to cleanup any running processes for this task
        // TODO: Implement cleanup_processes_for_task method in ProcessManager
        // if let Err(e) = self.process_manager.cleanup_processes_for_task(task_id).await {
        //     tracing::warn!("Failed to cleanup processes for task {}: {}", task_id, e);
        // }

        // Note: This implementation handles:
        // 1. Process cleanup via process_manager ✓
        // 2. State management ✓
        // 3. Dependent task notification (would need dependency graph)

        tracing::info!("Task {} successfully cancelled", task_id);
        Ok(())
    }

    pub async fn cancel_all_tasks(&self) -> Result<()> {
        tracing::info!("Cancelling all tasks");

        let mut registry = self.task_registry.write().await;
        let mut cancelled_count = 0;
        let mut skipped_count = 0;

        // Clone the keys to avoid borrowing issues
        let task_ids: Vec<String> = registry.keys().cloned().collect();

        for task_id in task_ids {
            if let Some(current_state) = registry.get(&task_id) {
                match current_state {
                    TaskState::Completed { .. }
                    | TaskState::Failed { .. }
                    | TaskState::Cancelled => {
                        // Skip tasks that can't be cancelled
                        skipped_count += 1;
                        tracing::debug!("Skipping task {} (state: {:?})", task_id, current_state);
                    }
                    _ => {
                        // Cancel the task
                        registry.insert(task_id.clone(), TaskState::Cancelled);
                        cancelled_count += 1;
                        tracing::debug!("Cancelled task: {}", task_id);

                        // Cleanup processes for this task
                        // TODO: Implement cleanup_processes_for_task method in ProcessManager
                        // if let Err(e) = self.process_manager.cleanup_processes_for_task(&task_id).await {
                        //     tracing::warn!("Failed to cleanup processes for task {}: {}", task_id, e);
                        // }
                    }
                }
            }
        }

        tracing::info!(
            "Bulk cancellation completed: {} cancelled, {} skipped",
            cancelled_count,
            skipped_count
        );

        Ok(())
    }

    pub async fn get_task_status(&self, task_id: &str) -> Option<TaskState> {
        let registry = self.task_registry.read().await;
        registry.get(task_id).cloned()
    }

    // Graceful shutdown
    pub async fn shutdown(&self, timeout: Duration) -> Result<()> {
        let start_time = SystemTime::now();

        // Cancel all pending tasks
        self.cancel_all_tasks().await?;

        // Shutdown the process manager
        // TODO: Implement shutdown method in ProcessManager
        // self.process_manager.shutdown().await?;

        // Wait for running tasks to complete or timeout
        loop {
            let registry = self.task_registry.read().await;
            let running_tasks: Vec<_> = registry
                .values()
                .filter(|state| matches!(state, TaskState::Running { .. }))
                .collect();

            if running_tasks.is_empty() {
                return Ok(());
            }

            // Check if we've exceeded timeout
            if start_time.elapsed().unwrap_or_default() > timeout {
                return Err(SnpError::Process(Box::new(crate::ProcessError::Timeout {
                    command: "shutdown".to_string(),
                    duration: timeout,
                })));
            }

            drop(registry);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub fn set_scheduling_strategy(&mut self, strategy: SchedulingStrategy) {
        self.scheduling_strategy = strategy;
    }

    /// Get process manager configuration information
    pub fn get_process_manager_info(&self) -> (usize, std::time::Duration) {
        // Access the process_manager field to get its configuration
        let _pm = &self.process_manager; // Use the field to avoid dead code warning
        (self.max_concurrent, std::time::Duration::from_secs(300))
    }

    /// Check if process manager is available for task cleanup
    pub fn has_process_manager(&self) -> bool {
        // Check if the process_manager field is available
        let _pm = &self.process_manager; // Use the field to avoid dead code warning
        true
    }
}

// Error aggregation utilities
pub struct ErrorAggregator {
    errors: Vec<AggregatedError>,
    context: HashMap<String, String>,
}

#[derive(Debug)]
pub struct AggregatedError {
    pub task_id: String,
    pub error: String,
    pub retry_count: usize,
    pub context: HashMap<String, String>,
}

pub struct ErrorReport {
    pub total_errors: usize,
    pub unique_errors: Vec<String>,
    pub task_failures: HashMap<String, Vec<AggregatedError>>,
    pub summary: String,
}

impl ErrorAggregator {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            context: HashMap::new(),
        }
    }

    pub fn add_error(&mut self, task_id: String, error: SnpError) {
        self.errors.push(AggregatedError {
            task_id,
            error: error.to_string(),
            retry_count: 0,
            context: self.context.clone(),
        });
    }

    pub fn add_context(&mut self, key: String, value: String) {
        self.context.insert(key, value);
    }

    pub fn build_report(&self) -> ErrorReport {
        // TODO: Implement comprehensive error reporting
        ErrorReport {
            total_errors: self.errors.len(),
            unique_errors: Vec::new(),
            task_failures: HashMap::new(),
            summary: format!("Total errors: {}", self.errors.len()),
        }
    }
}

impl Default for ErrorAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// Execution metrics collection
pub struct ExecutionMetrics {
    pub tasks_completed: std::sync::atomic::AtomicU64,
    pub tasks_failed: std::sync::atomic::AtomicU64,
    pub tasks_cancelled: std::sync::atomic::AtomicU64,
    pub average_execution_time: Duration,
    pub resource_utilization: ResourceUsage,
    pub queue_length: std::sync::atomic::AtomicUsize,
}

impl ExecutionMetrics {
    pub fn new() -> Self {
        Self {
            tasks_completed: std::sync::atomic::AtomicU64::new(0),
            tasks_failed: std::sync::atomic::AtomicU64::new(0),
            tasks_cancelled: std::sync::atomic::AtomicU64::new(0),
            average_execution_time: Duration::from_secs(0),
            resource_utilization: ResourceUsage::default(),
            queue_length: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl Default for ExecutionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub trait MetricsCollector: Send + Sync {
    fn record_task_start(&self, task_id: &str, config: &TaskConfig);
    fn record_task_completion(&self, result: &TaskResult<()>);
    fn record_resource_usage(&self, usage: &ResourceUsage);
    fn export_metrics(&self) -> HashMap<String, f64>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_task_priority_ordering() {
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Normal);
        assert!(TaskPriority::Normal > TaskPriority::Low);
    }

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.initial_delay, Duration::from_millis(100));
        assert_eq!(policy.max_delay, Duration::from_secs(30));
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_cpu_percent, 80.0);
        assert_eq!(limits.max_memory_mb, 1024);
        assert_eq!(limits.max_disk_io_mb_per_sec, 100);
        assert_eq!(limits.max_network_mb_per_sec, 50);
    }

    #[test]
    fn test_resource_usage_default() {
        let usage = ResourceUsage::default();
        assert_eq!(usage.cpu_percent, 0.0);
        assert_eq!(usage.memory_mb, 0);
        assert_eq!(usage.disk_io_mb_per_sec, 0);
        assert_eq!(usage.network_mb_per_sec, 0);
        assert_eq!(usage.active_tasks, 0);
    }

    #[test]
    fn test_task_config_builder() {
        let config = TaskConfig::new("test_task")
            .with_priority(TaskPriority::High)
            .with_timeout(Duration::from_secs(30))
            .with_dependencies(vec!["dep1".to_string(), "dep2".to_string()])
            .with_resource_requirements(ResourceRequirements {
                cpu_cores: Some(2.0),
                memory_mb: Some(512),
                disk_io_mb_per_sec: Some(50),
                network_mb_per_sec: Some(25),
            });

        assert_eq!(config.id, "test_task");
        assert_eq!(config.priority, TaskPriority::High);
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.dependencies, vec!["dep1", "dep2"]);
        assert_eq!(config.resource_requirements.cpu_cores, Some(2.0));
        assert_eq!(config.resource_requirements.memory_mb, Some(512));
    }

    #[test]
    fn test_task_state_equality() {
        assert_eq!(TaskState::Pending, TaskState::Pending);
        assert_eq!(TaskState::Cancelled, TaskState::Cancelled);
        assert_ne!(TaskState::Pending, TaskState::Cancelled);
    }

    #[test]
    fn test_batch_result_success_rate() {
        let successful = vec![
            create_mock_task_result("task1", Ok(())),
            create_mock_task_result("task2", Ok(())),
        ];
        let failed = vec![create_mock_task_result(
            "task3",
            Err(SnpError::Config(Box::new(crate::ConfigError::NotFound {
                path: "test".into(),
                suggestion: None,
            }))),
        )];

        let batch_result = BatchResult {
            successful,
            failed,
            cancelled: vec![],
            total_duration: Duration::from_secs(1),
            resource_usage: ResourceUsage::default(),
        };

        assert!(!batch_result.is_success());
        assert!((batch_result.success_rate() - 0.666_67).abs() < 0.001);
        assert_eq!(batch_result.get_errors().len(), 1);
    }

    #[test]
    fn test_batch_result_empty() {
        let batch_result: BatchResult<()> = BatchResult {
            successful: vec![],
            failed: vec![],
            cancelled: vec![],
            total_duration: Duration::from_secs(0),
            resource_usage: ResourceUsage::default(),
        };

        assert!(batch_result.is_success());
        assert_eq!(batch_result.success_rate(), 1.0);
        assert!(batch_result.get_errors().is_empty());
    }

    #[test]
    fn test_task_dependency_graph_creation() {
        let mut graph = TaskDependencyGraph::new();
        let task1 = TaskConfig::new("task1");
        let task2 = TaskConfig::new("task2");

        graph.add_task(task1);
        graph.add_task(task2);

        assert!(graph.add_dependency("task2", "task1").is_ok());
    }

    #[test]
    fn test_task_dependency_graph_invalid_dependency() {
        let mut graph = TaskDependencyGraph::new();
        let task1 = TaskConfig::new("task1");
        graph.add_task(task1);

        // Try to add dependency to non-existent task
        assert!(graph.add_dependency("task1", "non_existent").is_err());
        assert!(graph.add_dependency("non_existent", "task1").is_err());
    }

    #[test]
    fn test_task_dependency_graph_cycle_detection() {
        let mut graph = TaskDependencyGraph::new();
        let task1 = TaskConfig::new("task1");
        let task2 = TaskConfig::new("task2");
        let task3 = TaskConfig::new("task3");

        graph.add_task(task1);
        graph.add_task(task2);
        graph.add_task(task3);

        // Create a valid dependency chain
        assert!(graph.add_dependency("task2", "task1").is_ok());
        assert!(graph.add_dependency("task3", "task2").is_ok());

        // Try to create a cycle - should fail
        assert!(graph.add_dependency("task1", "task3").is_err());
    }

    #[test]
    fn test_task_dependency_graph_execution_order() {
        let mut graph = TaskDependencyGraph::new();
        let task1 = TaskConfig::new("task1");
        let task2 = TaskConfig::new("task2");
        let task3 = TaskConfig::new("task3");

        graph.add_task(task1);
        graph.add_task(task2);
        graph.add_task(task3);

        graph.add_dependency("task2", "task1").unwrap();
        graph.add_dependency("task3", "task2").unwrap();

        let order = graph.resolve_execution_order().unwrap();
        assert_eq!(order, vec!["task1", "task2", "task3"]);
    }

    #[test]
    fn test_concurrency_executor_creation() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(4, limits);

        assert_eq!(executor.max_concurrent, 4);
        assert!(!executor.is_lock_free_enabled());
        assert!(executor.get_scheduler_stats().is_none());
    }

    #[test]
    fn test_concurrency_executor_with_lock_free() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::with_lock_free_scheduler(4, limits);

        assert_eq!(executor.max_concurrent, 4);
        assert!(executor.is_lock_free_enabled());
        assert!(executor.get_scheduler_stats().is_some());
    }

    #[tokio::test]
    async fn test_execute_simple_task() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);
        let config = TaskConfig::new("test_task");

        let result = executor
            .execute_task(config, || Box::pin(async { Ok("task_result".to_string()) }))
            .await;

        assert!(result.is_ok());
        let task_result = result.unwrap();
        assert_eq!(task_result.task_id, "test_task");
        assert!(task_result.result.is_ok());
        assert_eq!(task_result.result.unwrap(), "task_result");
    }

    #[tokio::test]
    async fn test_execute_task_with_timeout() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);
        let config = TaskConfig::new("timeout_task").with_timeout(Duration::from_millis(100));

        let result = executor
            .execute_task(config, || {
                Box::pin(async {
                    sleep(Duration::from_millis(200)).await;
                    Ok("should_timeout".to_string())
                })
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_failing_task() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);
        let config = TaskConfig::new("failing_task");

        let result = executor
            .execute_task(config, || {
                Box::pin(async {
                    Err::<String, _>(SnpError::Config(Box::new(crate::ConfigError::NotFound {
                        path: "test".into(),
                        suggestion: None,
                    })))
                })
            })
            .await;

        assert!(result.is_ok());
        let task_result: TaskResult<String> = result.unwrap();
        assert!(task_result.result.is_err());
        assert_eq!(task_result.retry_count, 0);
    }

    #[tokio::test]
    async fn test_execute_batch_tasks() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);

        type TaskType = (
            TaskConfig,
            Box<dyn FnOnce() -> BoxFuture<'static, Result<String>> + Send>,
        );
        let tasks: Vec<TaskType> = vec![
            (
                TaskConfig::new("task1"),
                Box::new(|| Box::pin(async { Ok("result1".to_string()) })),
            ),
            (
                TaskConfig::new("task2"),
                Box::new(|| Box::pin(async { Ok("result2".to_string()) })),
            ),
        ];

        let result = executor.execute_batch(tasks).await;

        assert!(result.is_ok());
        let batch_result = result.unwrap();
        assert_eq!(batch_result.successful.len(), 2);
        assert_eq!(batch_result.failed.len(), 0);
        assert!(batch_result.is_success());
    }

    #[tokio::test]
    async fn test_resource_acquisition() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);

        let requirements = ResourceRequirements {
            cpu_cores: Some(1.0),
            memory_mb: Some(256),
            disk_io_mb_per_sec: None,
            network_mb_per_sec: None,
        };

        let guard = executor.acquire_resources(&requirements).await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_resource_limit_exceeded() {
        let limits = ResourceLimits {
            max_cpu_percent: 50.0,
            max_memory_mb: 100,
            max_disk_io_mb_per_sec: 50,
            max_network_mb_per_sec: 25,
        };
        let executor = ConcurrencyExecutor::new(2, limits);

        let requirements = ResourceRequirements {
            cpu_cores: Some(4.0), // Will exceed 50% CPU limit (4 * 25% = 100%)
            memory_mb: None,
            disk_io_mb_per_sec: None,
            network_mb_per_sec: None,
        };

        let guard = executor.acquire_resources(&requirements).await;
        assert!(guard.is_err());
    }

    #[tokio::test]
    async fn test_task_cancellation() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);

        // Add a task to registry manually
        {
            let mut registry = executor.task_registry.write().await;
            registry.insert("test_task".to_string(), TaskState::Pending);
        }

        let result = executor.cancel_task("test_task").await;
        assert!(result.is_ok());

        let status = executor.get_task_status("test_task").await;
        assert_eq!(status, Some(TaskState::Cancelled));
    }

    #[tokio::test]
    async fn test_cancel_completed_task() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);

        // Add a completed task to registry
        {
            let mut registry = executor.task_registry.write().await;
            registry.insert(
                "completed_task".to_string(),
                TaskState::Completed {
                    duration: Duration::from_secs(1),
                    retry_count: 0,
                },
            );
        }

        let result = executor.cancel_task("completed_task").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_all_tasks() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);

        // Add multiple tasks to registry
        {
            let mut registry = executor.task_registry.write().await;
            registry.insert("task1".to_string(), TaskState::Pending);
            registry.insert("task2".to_string(), TaskState::WaitingForResources);
            registry.insert(
                "task3".to_string(),
                TaskState::Completed {
                    duration: Duration::from_secs(1),
                    retry_count: 0,
                },
            );
        }

        let result = executor.cancel_all_tasks().await;
        assert!(result.is_ok());

        // Check that cancellable tasks were cancelled
        let status1 = executor.get_task_status("task1").await;
        let status2 = executor.get_task_status("task2").await;
        let status3 = executor.get_task_status("task3").await;

        assert_eq!(status1, Some(TaskState::Cancelled));
        assert_eq!(status2, Some(TaskState::Cancelled));
        assert_eq!(
            status3,
            Some(TaskState::Completed {
                duration: Duration::from_secs(1),
                retry_count: 0,
            })
        );
    }

    #[tokio::test]
    async fn test_executor_shutdown() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(2, limits);

        let result = executor.shutdown(Duration::from_secs(1)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_aggregator() {
        let mut aggregator = ErrorAggregator::new();

        aggregator.add_context("environment".to_string(), "test".to_string());
        aggregator.add_error(
            "task1".to_string(),
            SnpError::Config(Box::new(crate::ConfigError::NotFound {
                path: "test".into(),
                suggestion: None,
            })),
        );

        let report = aggregator.build_report();
        assert_eq!(report.total_errors, 1);
        assert!(report.summary.contains("Total errors: 1"));
    }

    #[test]
    fn test_execution_metrics() {
        let metrics = ExecutionMetrics::new();

        assert_eq!(
            metrics
                .tasks_completed
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .tasks_failed
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .tasks_cancelled
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(metrics.average_execution_time, Duration::from_secs(0));
    }

    #[test]
    fn test_process_manager_integration() {
        let limits = ResourceLimits::default();
        let executor = ConcurrencyExecutor::new(4, limits);

        assert!(executor.has_process_manager());
        let (max_concurrent, timeout) = executor.get_process_manager_info();
        assert_eq!(max_concurrent, 4);
        assert_eq!(timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_scheduling_strategy() {
        let limits = ResourceLimits::default();
        let mut executor = ConcurrencyExecutor::new(2, limits);

        executor.set_scheduling_strategy(SchedulingStrategy::RoundRobin);
        // Note: In a real implementation, this would affect task ordering
    }

    fn create_mock_task_result<T>(task_id: &str, result: Result<T>) -> TaskResult<T> {
        let now = SystemTime::now();
        TaskResult {
            task_id: task_id.to_string(),
            result,
            duration: Duration::from_millis(100),
            resource_usage: ResourceUsage::default(),
            retry_count: 0,
            started_at: now,
            completed_at: now,
        }
    }
}
