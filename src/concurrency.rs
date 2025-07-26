// Concurrency Framework for SNP - Async/parallel execution with resource management
// Provides task scheduling, dependency management, and error aggregation for efficient hook execution

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, Semaphore};

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
#[allow(dead_code)]
pub struct ResourceGuard {
    requirements: ResourceRequirements,
    executor: Arc<ConcurrencyExecutor>,
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        // TODO: Release resources back to the executor
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
        // TODO: Implement dependency addition with cycle detection
        self.edges
            .entry(task_id.to_string())
            .or_default()
            .push(depends_on.to_string());
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
    #[allow(dead_code)]
    process_manager: Arc<ProcessManager>,
    scheduling_strategy: SchedulingStrategy,
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
        }
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

    // Task management methods - to be implemented
    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        // TODO: Implement task cancellation
        let mut registry = self.task_registry.write().await;
        registry.insert(task_id.to_string(), TaskState::Cancelled);
        Ok(())
    }

    pub async fn cancel_all_tasks(&self) -> Result<()> {
        // TODO: Implement cancellation of all tasks
        let mut registry = self.task_registry.write().await;
        for (_, state) in registry.iter_mut() {
            *state = TaskState::Cancelled;
        }
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
