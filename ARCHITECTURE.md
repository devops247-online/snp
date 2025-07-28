# SNP Architecture Documentation

This document provides an overview of SNP's architecture and the advanced performance optimizations implemented.

## Core Architecture

SNP is built with a focus on high-performance parallel execution while maintaining compatibility with Python pre-commit configurations.

### System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        SNP Framework                        │
├─────────────────────────────────────────────────────────────┤
│  CLI Interface & Command Processing                         │
├─────────────────────────────────────────────────────────────┤
│  Work-Stealing Scheduler                                    │
│  ├─── Task Queue Management                                 │
│  ├─── Load Balancing & Task Stealing                        │
│  └─── Parallel Execution Engine                             │
├─────────────────────────────────────────────────────────────┤
│  Multi-Tier Caching System                                 │
│  ├─── L1 Cache (Hot Data - DashMap)                        │
│  ├─── L2 Cache (Warm Data - LRU)                           │
│  └─── L3 Cache (Cold Data - SQLite)                        │
├─────────────────────────────────────────────────────────────┤
│  Hook Execution & Language Plugins                         │
│  ├─── Python │ Rust │ Node.js │ Go │ Ruby │ Docker         │
│  └─── System Commands & Environment Management             │
├─────────────────────────────────────────────────────────────┤
│  Lock-Free Data Structures & Concurrency                  │
│  ├─── Concurrent Collections                               │
│  ├─── Arena-Based Memory Management                        │
│  └─── Zero-Copy String Operations                          │
└─────────────────────────────────────────────────────────────┘
```

## Performance Optimizations

### 1. Work-Stealing Task Scheduler

SNP implements an advanced work-stealing scheduler that optimizes parallel execution:

**Key Features:**
- **Dynamic Load Balancing**: Tasks are distributed across workers with automatic load balancing
- **Work Stealing**: Idle workers steal tasks from busy workers to maximize CPU utilization
- **Priority-Based Scheduling**: Tasks can be prioritized for optimal execution order
- **Dependency Resolution**: Handles task dependencies with graph-based algorithms

**Implementation Details:**
- Uses `crossbeam` deques for lock-free task queues
- Implements round-robin stealing with backoff strategies
- Supports configurable steal ratios and timeouts
- Provides comprehensive metrics for performance monitoring

### 2. Multi-Tier Caching Architecture

SNP implements a sophisticated three-tier caching system for optimal performance:

#### L1 Cache (Hot Data)
- **Data Structure**: `DashMap` for concurrent access
- **Purpose**: Frequently accessed data with zero-lock contention
- **Size**: Configurable, typically 1000 entries
- **Eviction**: LRU-based with intelligent promotion

#### L2 Cache (Warm Data)
- **Data Structure**: `LruCache` with async read/write locks
- **Purpose**: Recently accessed data that may be promoted to L1
- **Size**: Configurable, typically 10,000 entries
- **Promotion**: Based on access frequency thresholds

#### L3 Cache (Cold Data)
- **Data Structure**: SQLite persistent storage
- **Purpose**: Long-term storage and persistence across runs
- **Size**: Unlimited (disk-based)
- **Management**: Automatic cleanup and garbage collection

**Cache Intelligence:**
- Automatic promotion/demotion based on access patterns
- Deadlock prevention through careful lock ordering
- Comprehensive metrics and monitoring
- Configurable cache policies and thresholds

### 3. Lock-Free Data Structures

SNP extensively uses lock-free data structures to minimize contention:

**Concurrent Collections:**
- `DashMap` for shared hash maps
- `AtomicU64`/`AtomicUsize` for counters and metrics
- `crossbeam` channels for message passing
- Lock-free queues for task distribution

**Benefits:**
- Zero lock contention in hot paths
- Improved scalability with multiple cores
- Reduced context switching overhead
- Enhanced performance under high concurrency

### 4. Memory Management Optimizations

#### Arena-Based Allocation
- Custom arena allocators for hot execution paths
- Reduced fragmentation and allocation overhead
- Bulk deallocation for improved performance
- Memory pool reuse across hook executions

#### Zero-Copy String Operations
- Efficient string handling without unnecessary copies
- `Cow<str>` usage for optimal memory management
- Shared string references where possible
- Reduced memory allocations in command generation

### 5. Bloom Filter Negative Caching

SNP implements Bloom filters for fast negative cache lookups:

**Purpose:**
- Quick determination of cache misses
- Reduces expensive disk I/O operations
- Improves cache hit ratio calculations

**Implementation:**
- Probabilistic data structure with configurable false positive rate
- Automatic sizing based on expected cache size
- Integrated with multi-tier cache system

### 6. Regex Processing Optimizations

#### Batch Compilation with RegexSet
- Compiles multiple regex patterns into a single optimized matcher
- Reduces compilation overhead for repeated pattern matching
- Improved performance for file classification and filtering

#### Pattern Caching
- Compiled regex patterns are cached and reused
- Automatic pattern optimization and simplification
- Support for regex compilation flags and options

### 7. File I/O Optimizations

#### Async-First Design
- All file operations use `tokio` async I/O
- Non-blocking file reads and writes
- Concurrent file processing capabilities

#### Intelligent Batching
- Groups related file operations for efficiency
- Reduces system call overhead
- Optimized buffer sizes for different file types

#### Incremental Change Detection
- Tracks file modifications to avoid unnecessary processing
- Content-based change detection using checksums
- Intelligent file system watching and monitoring

## Concurrency Model

SNP's concurrency model is designed for maximum performance and safety:

### Thread Safety
- All shared data structures are thread-safe
- Extensive use of lock-free algorithms
- Careful attention to memory ordering and synchronization

### Error Handling
- Comprehensive error propagation with `anyhow` and `thiserror`
- Graceful degradation under resource constraints
- Proper cleanup and resource management

### Resource Management
- Automatic resource pools for expensive operations
- Connection pooling for external services
- Memory pressure detection and adaptive behavior

## Testing Architecture

SNP's testing framework ensures reliability and performance:

### Test Categories
- **Unit Tests**: Individual component testing
- **Integration Tests**: End-to-end workflow validation
- **Performance Tests**: Benchmarking and regression detection
- **Concurrency Tests**: Thread safety and race condition detection
- **Load Tests**: High-throughput scenario validation

### Test Infrastructure
- 758+ comprehensive tests across 49 test files
- Automated performance regression detection
- Cross-platform test validation
- Real-world scenario simulation

## Configuration and Extensibility

### Language Plugin Architecture
- Modular plugin system for language support
- Standardized interface for new language implementations
- Runtime plugin loading and management
- Extensive configuration options per language

### Performance Tuning
- Comprehensive configuration options for performance tuning
- Runtime performance monitoring and metrics
- Adaptive algorithms that adjust based on workload
- Profiling integration for performance analysis

## Future Optimizations

SNP continues to evolve with planned improvements:

- GPU acceleration for compute-intensive operations
- Advanced machine learning for workload prediction
- Enhanced cross-platform optimizations
- Further memory management improvements
- Extended language plugin ecosystem

This architecture enables SNP to achieve 3-5x performance improvements over Python pre-commit while maintaining full compatibility and reliability.
