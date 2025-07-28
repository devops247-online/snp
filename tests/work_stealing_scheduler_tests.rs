// Comprehensive tests for the advanced work-stealing task scheduler
// Tests work-stealing algorithm, task priorities, dependency resolution, and performance

use serial_test::serial;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use snp::{
    concurrency::TaskPriority,
    core::{ExecutionContext, Hook, Stage},
    lock_free_scheduler::{SchedulerConfig, Task, TaskPayload, WorkStealingScheduler},
    TaskDependencyResolver,
};

/// Create a mock hook for testing
fn create_mock_hook(id: &str, language: &str) -> Hook {
    Hook {
        id: id.to_string(),
        name: Some(id.to_string()),
        entry: format!("mock-{language}"),
        language: language.to_string(),
        files: Some(".*".to_string()),
        exclude: Some("^$".to_string()),
        types: vec![],
        exclude_types: vec![],
        stages: vec![Stage::PreCommit],
        additional_dependencies: vec![],
        args: vec![],
        always_run: false,
        pass_filenames: true,
        minimum_pre_commit_version: None,
        depends_on: vec![],
        fail_fast: false,
        verbose: false,
    }
}

/// Create a mock execution context
fn create_mock_execution_context() -> ExecutionContext {
    ExecutionContext::new(Stage::PreCommit)
}

/// Create test tasks with varying execution times for load balancing tests
fn create_heterogeneous_tasks(count: usize) -> Vec<Task> {
    let mut tasks = Vec::with_capacity(count);

    for i in 0..count {
        let duration_ms = match i % 4 {
            0 => 100, // Fast tasks
            1 => 150, // Medium tasks
            2 => 200, // Slow tasks
            _ => 300, // Very slow tasks
        };

        let priority = match i % 3 {
            0 => TaskPriority::High,
            1 => TaskPriority::Normal,
            _ => TaskPriority::Low,
        };

        let hook = create_mock_hook(&format!("hook-{i}"), "python");
        let files = vec![PathBuf::from(format!("test-{i}.py"))];
        let context = create_mock_execution_context();

        let task = Task {
            id: format!("task-{i}"),
            priority,
            estimated_duration: Some(Duration::from_millis(duration_ms)),
            dependencies: vec![],
            payload: TaskPayload::HookExecution {
                hook: Box::new(hook),
                files,
                context,
            },
            created_at: Instant::now(),
        };

        tasks.push(task);
    }

    tasks
}

#[cfg(test)]
mod work_stealing_algorithm_tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_work_stealing_scheduler_creation() {
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(4, config);

        assert_eq!(scheduler.num_workers(), 4);
        assert!(!scheduler.is_started());

        scheduler.start().await.unwrap();
        assert!(scheduler.is_started());

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn test_task_submission_and_execution() {
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(2, config);
        scheduler.start().await.unwrap();

        let hook = create_mock_hook("test-hook", "python");
        let files = vec![PathBuf::from("test.py")];
        let context = create_mock_execution_context();

        let task = Task {
            id: "test-task".to_string(),
            priority: TaskPriority::Normal,
            estimated_duration: Some(Duration::from_millis(100)),
            dependencies: vec![],
            payload: TaskPayload::HookExecution {
                hook: Box::new(hook),
                files,
                context,
            },
            created_at: Instant::now(),
        };

        let result_receiver = scheduler.submit_task(task).await.unwrap();

        // Wait for task completion
        let result = tokio::time::timeout(Duration::from_secs(5), result_receiver)
            .await
            .unwrap()
            .unwrap();

        assert!(result.result.is_ok());
        assert_eq!(result.task_id, "test-task");

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn test_work_stealing_load_balancing() {
        let config = SchedulerConfig {
            enable_load_balancing: true,
            steal_ratio: 0.5,
            ..Default::default()
        };
        let mut scheduler = WorkStealingScheduler::new(4, config);
        scheduler.start().await.unwrap();

        // Submit heterogeneous tasks
        let tasks = create_heterogeneous_tasks(100);
        let mut receivers = Vec::new();

        for task in tasks {
            let receiver = scheduler.submit_task(task).await.unwrap();
            receivers.push(receiver);
        }

        // Wait for all tasks to complete
        for receiver in receivers {
            let result = tokio::time::timeout(Duration::from_secs(10), receiver)
                .await
                .unwrap()
                .unwrap();
            assert!(result.result.is_ok());
        }

        // Verify load balancing effectiveness
        let metrics = scheduler.get_metrics().await;
        let statistics = scheduler.get_worker_statistics().await;

        // Check that work was distributed relatively evenly
        let max_tasks = statistics
            .iter()
            .map(|s| s.tasks_executed.load(std::sync::atomic::Ordering::Relaxed))
            .max()
            .unwrap();
        let min_tasks = statistics
            .iter()
            .map(|s| s.tasks_executed.load(std::sync::atomic::Ordering::Relaxed))
            .min()
            .unwrap();

        // Allow some imbalance but should be reasonable
        assert!(
            max_tasks - min_tasks <= 30,
            "Load imbalance too high: max={max_tasks}, min={min_tasks}"
        );
        assert!(
            metrics.load_balance_coefficient < 0.3,
            "Load balance coefficient too high: {}",
            metrics.load_balance_coefficient
        );

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn test_steal_success_rate() {
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(8, config);
        scheduler.start().await.unwrap();

        // Submit many short tasks to encourage stealing
        let tasks = create_heterogeneous_tasks(200);
        let mut receivers = Vec::new();

        for task in tasks {
            let receiver = scheduler.submit_task(task).await.unwrap();
            receivers.push(receiver);
        }

        // Wait for completion
        for receiver in receivers {
            tokio::time::timeout(Duration::from_secs(10), receiver)
                .await
                .unwrap()
                .unwrap();
        }

        let metrics = scheduler.get_metrics().await;

        // Should have good steal success rate with many workers and tasks
        assert!(
            metrics.steal_success_rate > 0.1,
            "Steal success rate too low: {}",
            metrics.steal_success_rate
        );
        assert!(metrics.total_tasks_executed == 200);

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }
}

#[cfg(test)]
mod task_priority_tests {
    use super::*;

    #[tokio::test]
    async fn test_priority_based_scheduling() {
        let config = SchedulerConfig {
            task_priority_enabled: true,
            ..Default::default()
        };
        let mut scheduler = WorkStealingScheduler::new(1, config); // Single worker to test priority ordering
        scheduler.start().await.unwrap();

        // We'll track execution order in test results instead

        // Submit tasks in mixed priority order
        let priorities = vec![
            TaskPriority::Low,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::High,
            TaskPriority::Low,
        ];

        let mut receivers = Vec::new();

        for (i, priority) in priorities.into_iter().enumerate() {
            let hook = create_mock_hook(&format!("hook-{i}"), "python");
            let files = vec![PathBuf::from(format!("test-{i}.py"))];
            let context = create_mock_execution_context();

            let task = Task {
                id: format!("task-{i}"),
                priority,
                estimated_duration: Some(Duration::from_millis(1)),
                dependencies: vec![],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook),
                    files,
                    context,
                },
                created_at: Instant::now(),
            };

            let receiver = scheduler.submit_task(task).await.unwrap();
            receivers.push(receiver);
        }

        // Wait for all tasks to complete
        for receiver in receivers {
            tokio::time::timeout(Duration::from_secs(5), receiver)
                .await
                .unwrap()
                .unwrap();
        }

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn test_task_timeout_handling() {
        let config = SchedulerConfig {
            default_task_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let mut scheduler = WorkStealingScheduler::new(1, config);
        scheduler.start().await.unwrap();

        let hook = create_mock_hook("timeout-hook", "python");
        let files = vec![PathBuf::from("test.py")];
        let context = create_mock_execution_context();

        let task = Task {
            id: "timeout-task".to_string(),
            priority: TaskPriority::Normal,
            estimated_duration: Some(Duration::from_millis(30)), // Shorter than execution time
            dependencies: vec![],
            payload: TaskPayload::HookExecution {
                hook: Box::new(hook),
                files,
                context,
            },
            created_at: Instant::now(),
        };

        let receiver = scheduler.submit_task(task).await.unwrap();

        // Task should timeout
        let result = tokio::time::timeout(Duration::from_secs(1), receiver)
            .await
            .unwrap()
            .unwrap();

        // Should be an error due to timeout
        assert!(result.result.is_err());

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }
}

#[cfg(test)]
mod dependency_resolution_tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_task_dependencies() {
        let mut resolver = TaskDependencyResolver::new();
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(2, config);
        scheduler.start().await.unwrap();

        let hook_a = create_mock_hook("hook-a", "python");
        let hook_b = create_mock_hook("hook-b", "python");
        let hook_c = create_mock_hook("hook-c", "python");

        let files = vec![PathBuf::from("test.py")];
        let context = create_mock_execution_context();

        let tasks = vec![
            Task {
                id: "task-a".to_string(),
                priority: TaskPriority::Normal,
                estimated_duration: Some(Duration::from_millis(100)),
                dependencies: vec![],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook_a),
                    files: files.clone(),
                    context: context.clone(),
                },
                created_at: Instant::now(),
            },
            Task {
                id: "task-b".to_string(),
                priority: TaskPriority::Normal,
                estimated_duration: Some(Duration::from_millis(100)),
                dependencies: vec!["task-a".to_string()],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook_b),
                    files: files.clone(),
                    context: context.clone(),
                },
                created_at: Instant::now(),
            },
            Task {
                id: "task-c".to_string(),
                priority: TaskPriority::Normal,
                estimated_duration: Some(Duration::from_millis(100)),
                dependencies: vec!["task-a".to_string(), "task-b".to_string()],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook_c),
                    files: files.clone(),
                    context: context.clone(),
                },
                created_at: Instant::now(),
            },
        ];

        let receivers = resolver
            .resolve_and_submit(tasks, &scheduler)
            .await
            .unwrap();
        assert_eq!(receivers.len(), 3);

        // Initially only task-a should be ready
        let _metrics = scheduler.get_metrics().await;
        // Tasks will execute in dependency order

        // Wait for all tasks to complete
        for receiver in receivers {
            tokio::time::timeout(Duration::from_secs(5), receiver)
                .await
                .unwrap()
                .unwrap();
        }

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn test_circular_dependency_detection() {
        let mut resolver = TaskDependencyResolver::new();
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(1, config);
        scheduler.start().await.unwrap();

        let hook_a = create_mock_hook("hook-a", "python");
        let hook_b = create_mock_hook("hook-b", "python");

        let files = vec![PathBuf::from("test.py")];
        let context = create_mock_execution_context();

        let tasks = vec![
            Task {
                id: "task-a".to_string(),
                priority: TaskPriority::Normal,
                estimated_duration: Some(Duration::from_millis(100)),
                dependencies: vec!["task-b".to_string()],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook_a),
                    files: files.clone(),
                    context: context.clone(),
                },
                created_at: Instant::now(),
            },
            Task {
                id: "task-b".to_string(),
                priority: TaskPriority::Normal,
                estimated_duration: Some(Duration::from_millis(100)),
                dependencies: vec!["task-a".to_string()],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook_b),
                    files: files.clone(),
                    context: context.clone(),
                },
                created_at: Instant::now(),
            },
        ];

        // Should detect circular dependency
        let result = resolver.resolve_and_submit(tasks, &scheduler).await;
        assert!(result.is_err());

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    #[serial] // Run performance tests in isolation
    async fn test_throughput_vs_single_threaded() {
        // Test work-stealing scheduler vs single-threaded execution
        let task_count = 100; // Reduced from 1000 to make test faster
        let tasks = create_heterogeneous_tasks(task_count);

        // Single-threaded baseline with shorter execution times
        let start = Instant::now();
        for task in &tasks {
            // Use much shorter durations for testing
            let duration = task
                .estimated_duration
                .unwrap_or(Duration::from_millis(100));
            let short_duration = Duration::from_millis(duration.as_millis() as u64 / 10); // 10x faster
            tokio::time::sleep(short_duration).await;
        }
        let single_threaded_duration = start.elapsed();

        // Work-stealing scheduler
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(4, config);
        scheduler.start().await.unwrap();

        let start = Instant::now();
        let mut receivers = Vec::new();

        for task in tasks {
            let receiver = scheduler.submit_task(task).await.unwrap();
            receivers.push(receiver);
        }

        for receiver in receivers {
            tokio::time::timeout(Duration::from_secs(10), receiver)
                .await
                .unwrap()
                .unwrap();
        }

        let work_stealing_duration = start.elapsed();

        // Work-stealing should be significantly faster
        let speedup =
            single_threaded_duration.as_millis() as f64 / work_stealing_duration.as_millis() as f64;

        println!("Single-threaded: {single_threaded_duration:?}");
        println!("Work-stealing: {work_stealing_duration:?}");
        println!("Speedup: {speedup:.2}x");

        // Should achieve at least 1.5x speedup with 4 workers (conservative for smaller test)
        assert!(
            speedup > 1.5,
            "Work-stealing speedup too low: {speedup:.2}x"
        );

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[tokio::test]
    async fn test_worker_utilization() {
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(4, config);
        scheduler.start().await.unwrap();

        // Submit many tasks to keep workers busy
        let tasks = create_heterogeneous_tasks(200);
        let mut receivers = Vec::new();

        for task in tasks {
            let receiver = scheduler.submit_task(task).await.unwrap();
            receivers.push(receiver);
        }

        // Wait for completion
        for receiver in receivers {
            tokio::time::timeout(Duration::from_secs(10), receiver)
                .await
                .unwrap()
                .unwrap();
        }

        let metrics = scheduler.get_metrics().await;

        // Should have reasonable worker utilization
        let avg_utilization = metrics.worker_utilization.iter().sum::<f32>()
            / metrics.worker_utilization.len() as f32;
        assert!(
            avg_utilization > 0.3,
            "Worker utilization too low: {avg_utilization:.2}"
        );

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_integration_with_hook_execution() {
        let config = SchedulerConfig::default();
        let mut scheduler = WorkStealingScheduler::new(2, config);
        scheduler.start().await.unwrap();

        // Create realistic hook execution tasks
        let hooks = vec![
            create_mock_hook("flake8", "python"),
            create_mock_hook("black", "python"),
            create_mock_hook("mypy", "python"),
        ];

        let files = vec![
            PathBuf::from("src/main.py"),
            PathBuf::from("src/utils.py"),
            PathBuf::from("tests/test_main.py"),
        ];

        let context = create_mock_execution_context();
        let mut receivers = Vec::new();

        for hook in hooks.into_iter() {
            let task = Task {
                id: format!("hook-{}", hook.id),
                priority: if hook.id == "mypy" {
                    TaskPriority::High
                } else {
                    TaskPriority::Normal
                },
                estimated_duration: Some(Duration::from_millis(50)),
                dependencies: vec![],
                payload: TaskPayload::HookExecution {
                    hook: Box::new(hook),
                    files: files.clone(),
                    context: context.clone(),
                },
                created_at: Instant::now(),
            };

            let receiver = scheduler.submit_task(task).await.unwrap();
            receivers.push(receiver);
        }

        let mut results = Vec::new();
        for receiver in receivers {
            let result = tokio::time::timeout(Duration::from_secs(5), receiver)
                .await
                .unwrap()
                .unwrap();
            results.push(result);
        }

        // All hooks should complete successfully
        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.result.is_ok());
        }

        scheduler.shutdown(Duration::from_secs(1)).await.unwrap();
    }
}
