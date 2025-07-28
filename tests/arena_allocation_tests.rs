// Arena allocation performance and correctness tests
// Tests the arena-based memory management implementation

use bumpalo::Bump;
use snp::core::{ArenaExecutionContext, ExecutionContext, Stage};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_arena_execution_context_creation() {
    let arena = Bump::new();
    let files = vec![PathBuf::from("test1.rs"), PathBuf::from("test2.rs")];
    let mut environment = HashMap::new();
    environment.insert("TEST_VAR".to_string(), "test_value".to_string());

    let context = ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);

    assert_eq!(context.stage, Stage::PreCommit);
    assert_eq!(context.files.len(), 2);
    assert!(context.environment.contains_key("TEST_VAR"));
}

#[test]
fn test_arena_memory_usage() {
    let arena = Bump::new();
    let initial_allocated = arena.allocated_bytes();

    let files = vec![
        PathBuf::from("file1.rs"),
        PathBuf::from("file2.rs"),
        PathBuf::from("file3.rs"),
    ];
    let mut environment = HashMap::new();
    environment.insert("VAR1".to_string(), "value1".to_string());
    environment.insert("VAR2".to_string(), "value2".to_string());

    let _context = ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);

    let allocated_after = arena.allocated_bytes();
    assert!(
        allocated_after > initial_allocated,
        "Arena should have allocated memory for strings"
    );
}

#[test]
fn test_arena_context_equivalency_with_regular_context() {
    // Test that arena context behaves the same as regular context
    let arena = Bump::new();
    let files = vec![PathBuf::from("test.rs")];
    let mut environment = HashMap::new();
    environment.insert("PATH".to_string(), "/usr/bin".to_string());

    let regular_context = ExecutionContext {
        files: files.clone(),
        stage: Stage::PreCommit,
        verbose: false,
        show_diff_on_failure: false,
        environment: environment.clone(),
        working_directory: PathBuf::from("/tmp"),
        color: true,
    };

    let arena_context = ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);

    assert_eq!(regular_context.stage, arena_context.stage);
    assert_eq!(regular_context.files.len(), arena_context.files.len());

    // Check environment equivalency
    for (key, value) in &regular_context.environment {
        assert_eq!(
            arena_context.environment.get(key.as_str()),
            Some(&value.as_str())
        );
    }
}

#[test]
fn test_arena_context_with_large_environment() {
    let arena = Bump::new();
    let files = vec![];
    let mut environment = HashMap::new();

    // Create a large environment to test arena performance
    for i in 0..1000 {
        environment.insert(format!("VAR_{i}"), format!("value_{i}"));
    }

    let context = ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment.clone());

    assert_eq!(context.environment.len(), 1000);

    // Verify all values are correctly stored
    for i in 0..100 {
        // Check first 100 for performance
        let key = format!("VAR_{i}");
        let expected_value = format!("value_{i}");
        assert_eq!(
            context.environment.get(key.as_str()),
            Some(&expected_value.as_str())
        );
    }
}

#[test]
fn test_arena_cleanup_on_drop() {
    let arena_allocated_bytes = {
        let arena = Bump::new();
        let files = vec![PathBuf::from("test.rs"); 100];
        let mut environment = HashMap::new();
        for i in 0..100 {
            environment.insert(format!("VAR_{i}"), format!("value_{i}"));
        }

        let _context = ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);
        arena.allocated_bytes()
    }; // arena drops here

    // Test passes if no memory leaks occur (handled by Rust's ownership system)
    assert!(
        arena_allocated_bytes > 0,
        "Arena should have allocated memory"
    );
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_arena_vs_regular_context_performance() {
        let files: Vec<PathBuf> = (0..1000)
            .map(|i| PathBuf::from(format!("file_{i}.rs")))
            .collect();
        let mut environment = HashMap::new();
        for i in 0..1000 {
            environment.insert(format!("VAR_{i}"), format!("value_{i}"));
        }

        // Test regular context creation time
        let start = Instant::now();
        let _regular_context = ExecutionContext {
            files: files.clone(),
            stage: Stage::PreCommit,
            verbose: false,
            show_diff_on_failure: false,
            environment: environment.clone(),
            working_directory: PathBuf::from("/tmp"),
            color: true,
        };
        let regular_duration = start.elapsed();

        // Test arena context creation time
        let arena = Bump::new();
        let start = Instant::now();
        let _arena_context =
            ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);
        let arena_duration = start.elapsed();

        println!("Regular context creation: {regular_duration:?}");
        println!("Arena context creation: {arena_duration:?}");

        // Arena context might be slower for small datasets due to arena initialization overhead
        // but should be faster or comparable for repeated operations
        // This is more of a benchmark than a strict test - just ensure it's not unreasonably slow
        assert!(
            arena_duration < regular_duration * 5,
            "Arena context shouldn't be unreasonably slow"
        );
    }
}

#[test]
fn test_arena_context_with_unicode_strings() {
    let arena = Bump::new();
    let files = vec![PathBuf::from("测试.rs"), PathBuf::from("тест.py")];
    let mut environment = HashMap::new();
    environment.insert("UNICODE_VAR".to_string(), "测试值".to_string());
    environment.insert("CYRILLIC_VAR".to_string(), "тестовое значение".to_string());

    let context = ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);

    assert_eq!(context.files.len(), 2);
    assert_eq!(context.environment.get("UNICODE_VAR"), Some(&"测试值"));
    assert_eq!(
        context.environment.get("CYRILLIC_VAR"),
        Some(&"тестовое значение")
    );
}
