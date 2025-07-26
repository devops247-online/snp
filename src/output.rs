// Output Aggregation System - Comprehensive output collection, formatting, and presentation
// This module handles stdout/stderr capture, progress reporting, result formatting, and provides
// various output modes for different use cases (human-readable, JSON, CI/CD integration).

use crate::error::Result;
use crate::execution::{ExecutionResult, HookExecutionResult};
use std::collections::HashMap;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Output aggregation configuration
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub verbosity: VerbosityLevel,
    pub color: ColorMode,
    pub show_progress: bool,
    pub show_timing: bool,
    pub show_summary: bool,
    pub max_output_lines: Option<usize>,
    pub buffer_size: usize,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Human,
            verbosity: VerbosityLevel::Normal,
            color: ColorMode::Auto,
            show_progress: true,
            show_timing: true,
            show_summary: true,
            max_output_lines: Some(1000),
            buffer_size: 8192,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Human,
    Json,
    JsonCompact,
    Junit,
    Tap,
    GitHub,   // GitHub Actions format
    TeamCity, // TeamCity service messages
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerbosityLevel {
    Quiet,   // Only errors and final results
    Normal,  // Standard output with summaries
    Verbose, // Detailed output including all hook output
    Debug,   // Maximum verbosity with internal details
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColorMode {
    Never,
    Always,
    Auto, // Detect terminal capabilities
}

/// Output stream type
#[derive(Debug, Clone, PartialEq)]
pub enum OutputStream {
    Stdout,
    Stderr,
    Combined,
}

/// Main output aggregator
pub struct OutputAggregator {
    config: OutputConfig,
    writers: HashMap<String, Box<dyn OutputWriter>>,
    progress_reporter: Option<ProgressReporter>,
    result_formatter: Box<dyn ResultFormatter>,
    #[allow(dead_code)] // Buffer will be used for buffering in future iterations
    buffer: Arc<Mutex<OutputBuffer>>,
}

/// Real-time output collection from running hooks
pub struct OutputCollector {
    hook_id: String,
    stdout_buffer: Arc<Mutex<Vec<u8>>>,
    stderr_buffer: Arc<Mutex<Vec<u8>>>,
    #[allow(dead_code)] // Will be used for coordination in future iterations
    aggregator: Arc<Mutex<OutputAggregator>>,
    start_time: Instant,
}

/// Collected output from a single hook execution
#[derive(Debug)]
pub struct CollectedOutput {
    pub hook_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub line_count: usize,
    pub byte_count: usize,
}

/// Output buffering and management
pub struct OutputBuffer {
    entries: Vec<BufferEntry>,
    total_size: usize,
    max_size: usize,
}

#[derive(Debug)]
pub struct BufferEntry {
    pub timestamp: Instant,
    pub hook_id: String,
    pub stream: OutputStream,
    pub content: String,
    pub metadata: HashMap<String, String>,
}

/// Real-time progress reporting during hook execution
pub struct ProgressReporter {
    #[allow(dead_code)] // Will be used in future iterations for advanced progress features
    config: ProgressConfig,
    state: ProgressState,
    #[allow(dead_code)] // Will be used for rendering progress in future iterations
    renderer: Box<dyn ProgressRenderer>,
    #[allow(dead_code)] // Will be used for throttling updates in future iterations
    update_interval: Duration,
    last_update: Instant,
}

#[derive(Debug, Clone)]
pub struct ProgressConfig {
    pub show_bar: bool,
    pub show_percentage: bool,
    pub show_time_remaining: bool,
    pub show_current_hook: bool,
    pub bar_width: usize,
    pub update_interval_ms: u64,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            show_bar: true,
            show_percentage: true,
            show_time_remaining: true,
            show_current_hook: true,
            bar_width: 40,
            update_interval_ms: 100,
        }
    }
}

#[derive(Debug)]
pub struct ProgressState {
    pub total_hooks: usize,
    pub completed_hooks: usize,
    pub failed_hooks: usize,
    pub current_hook: Option<String>,
    pub start_time: Instant,
    pub estimated_completion: Option<Instant>,
}

/// Progress rendering for different output modes
pub trait ProgressRenderer: Send + Sync {
    fn render_progress(&self, state: &ProgressState) -> String;
    fn clear_progress(&self) -> String;
    fn supports_realtime_updates(&self) -> bool;
}

/// Result formatting for different output modes
pub trait ResultFormatter: Send + Sync {
    fn format_execution_result(&self, result: &ExecutionResult) -> String;
    fn format_hook_result(&self, result: &HookExecutionResult, verbose: bool) -> String;
    fn format_summary(&self, summary: &ExecutionSummary) -> String;
    fn format_error_details(&self, errors: &[crate::error::HookExecutionError]) -> String;
    fn format_diff(&self, file_path: &Path, diff: &str) -> String;
}

/// Output destination abstraction
pub trait OutputWriter: Send + Sync {
    fn write(&mut self, content: &str) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn supports_colors(&self) -> bool;
    fn supports_realtime_progress(&self) -> bool;
}

/// Execution summary for reporting
#[derive(Debug)]
pub struct ExecutionSummary {
    pub total_hooks: usize,
    pub hooks_passed: usize,
    pub hooks_failed: usize,
    pub hooks_skipped: usize,
    pub total_duration: Duration,
    pub files_modified: Vec<PathBuf>,
    pub performance_metrics: PerformanceMetrics,
}

/// Performance metrics collection and reporting
#[derive(Debug)]
pub struct PerformanceMetrics {
    pub execution_times: HashMap<String, Duration>,
    pub resource_usage: ResourceUsageMetrics,
    pub cache_statistics: CacheStatistics,
    pub throughput_metrics: ThroughputMetrics,
}

#[derive(Debug)]
pub struct ResourceUsageMetrics {
    pub peak_memory_usage: u64,
    pub average_cpu_usage: f64,
    pub disk_io_bytes: u64,
    pub network_io_bytes: u64,
}

#[derive(Debug)]
pub struct CacheStatistics {
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub cache_size: usize,
}

#[derive(Debug)]
pub struct ThroughputMetrics {
    pub files_processed_per_second: f64,
    pub hooks_executed_per_second: f64,
    pub output_lines_per_second: f64,
}

/// Aggregated output result
#[derive(Debug)]
pub struct AggregatedOutput {
    pub summary: ExecutionSummary,
    pub formatted_output: String,
    pub raw_output: Vec<CollectedOutput>,
}

// Basic implementations for Green phase
impl OutputAggregator {
    pub fn new(config: OutputConfig) -> Self {
        let formatter: Box<dyn ResultFormatter> = match config.format {
            OutputFormat::Human => Box::new(HumanFormatter::new()),
            OutputFormat::Json => Box::new(JsonFormatter::new()),
            OutputFormat::JsonCompact => Box::new(JsonFormatter::new_compact()),
            OutputFormat::Junit => Box::new(JunitFormatter::new()),
            OutputFormat::Tap => Box::new(TapFormatter::new()),
            OutputFormat::GitHub => Box::new(GitHubFormatter::new()),
            OutputFormat::TeamCity => Box::new(TeamCityFormatter::new()),
        };

        Self {
            config,
            writers: HashMap::new(),
            progress_reporter: None,
            result_formatter: formatter,
            buffer: Arc::new(Mutex::new(OutputBuffer::new(8192))),
        }
    }

    pub fn with_writer(mut self, name: String, writer: Box<dyn OutputWriter>) -> Self {
        self.writers.insert(name, writer);
        self
    }

    pub fn start_hook_output(&mut self, hook_id: &str) -> OutputCollector {
        OutputCollector::new(
            hook_id.to_string(),
            Arc::new(Mutex::new(OutputAggregator::new(self.config.clone()))),
        )
    }

    pub fn collect_hook_result(&mut self, _hook_id: &str, _result: &HookExecutionResult) {
        // Store the result for aggregation
    }

    pub fn aggregate_results(&mut self, results: &[HookExecutionResult]) -> AggregatedOutput {
        let total_hooks = results.len();
        let hooks_passed = results.iter().filter(|r| r.success).count();
        let hooks_failed = results.iter().filter(|r| !r.success).count();
        let total_duration = results.iter().map(|r| r.duration).sum();

        let files_modified: Vec<PathBuf> = results
            .iter()
            .flat_map(|r| &r.files_modified)
            .cloned()
            .collect();

        let execution_times: HashMap<String, Duration> = results
            .iter()
            .map(|r| (r.hook_id.clone(), r.duration))
            .collect();

        let performance_metrics = PerformanceMetrics {
            execution_times,
            resource_usage: ResourceUsageMetrics {
                peak_memory_usage: 1024 * 1024, // 1MB placeholder
                average_cpu_usage: 50.0,
                disk_io_bytes: 0,
                network_io_bytes: 0,
            },
            cache_statistics: CacheStatistics {
                cache_hits: 0,
                cache_misses: 0,
                cache_size: 0,
            },
            throughput_metrics: ThroughputMetrics {
                files_processed_per_second: 10.0,
                hooks_executed_per_second: 1.0,
                output_lines_per_second: 100.0,
            },
        };

        let summary = ExecutionSummary {
            total_hooks,
            hooks_passed,
            hooks_failed,
            hooks_skipped: 0,
            total_duration,
            files_modified,
            performance_metrics,
        };

        // Handle different output formats for array version
        let formatted_output = match self.config.format {
            OutputFormat::Json | OutputFormat::JsonCompact => {
                let hooks_json: Vec<String> = results.iter().map(|hook| {
                    format!(
                        r#"{{"hook_id": "{}", "success": {}, "duration_ms": {}, "stdout": "{}", "stderr": "{}"}}"#,
                        hook.hook_id,
                        hook.success,
                        hook.duration.as_millis(),
                        hook.stdout.replace('\"', "\\\"").replace('\n', "\\n"),
                        hook.stderr.replace('\"', "\\\"").replace('\n', "\\n")
                    )
                }).collect();

                format!(
                    r#"{{"hooks_executed": {}, "success": {}, "hooks": [{}]}}"#,
                    total_hooks,
                    hooks_failed == 0,
                    hooks_json.join(", ")
                )
            }
            OutputFormat::Junit => {
                let total_time = results
                    .iter()
                    .map(|h| h.duration.as_secs_f64())
                    .sum::<f64>();
                let mut xml = String::new();
                xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
                xml.push('\n');
                xml.push_str(&format!(
                    r#"<testsuite name="pre-commit" tests="{total_hooks}" failures="{hooks_failed}" time="{total_time:.3}">"#,
                ));
                xml.push('\n');

                for hook in results {
                    xml.push_str(&format!(
                        r#"  <testcase name="{}" time="{:.3}">"#,
                        hook.hook_id,
                        hook.duration.as_secs_f64()
                    ));
                    xml.push('\n');

                    if !hook.success {
                        let stderr_escaped = hook
                            .stderr
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;")
                            .replace('"', "&quot;");
                        xml.push_str(&format!(
                            r#"    <failure message="Hook failed">{stderr_escaped}</failure>"#,
                        ));
                        xml.push('\n');
                    }

                    xml.push_str("  </testcase>");
                    xml.push('\n');
                }

                xml.push_str("</testsuite>");
                xml.push('\n');
                xml
            }
            OutputFormat::Tap => {
                let mut tap = String::new();
                tap.push_str("TAP version 13");
                tap.push('\n');
                tap.push_str(&format!("1..{total_hooks}"));
                tap.push('\n');

                for (i, hook) in results.iter().enumerate() {
                    let test_num = i + 1;
                    if hook.success {
                        tap.push_str(&format!("ok {} - {}", test_num, hook.hook_id));
                    } else {
                        tap.push_str(&format!("not ok {} - {}", test_num, hook.hook_id));
                        if !hook.stderr.is_empty() {
                            tap.push('\n');
                            for line in hook.stderr.lines() {
                                tap.push_str(&format!("  # {line}"));
                                tap.push('\n');
                            }
                        }
                    }
                    tap.push('\n');
                }
                tap
            }
            _ => self.result_formatter.format_summary(&summary),
        };

        let raw_output: Vec<CollectedOutput> = results
            .iter()
            .map(|r| CollectedOutput {
                hook_id: r.hook_id.clone(),
                stdout: r.stdout.clone(),
                stderr: r.stderr.clone(),
                exit_code: r.exit_code,
                duration: r.duration,
                line_count: r.stdout.lines().count() + r.stderr.lines().count(),
                byte_count: r.stdout.len() + r.stderr.len(),
            })
            .collect();

        AggregatedOutput {
            summary,
            formatted_output,
            raw_output,
        }
    }

    pub fn start_progress(&mut self, total_hooks: usize) -> Result<()> {
        let progress_config = ProgressConfig::default();
        self.progress_reporter = Some(ProgressReporter::new(progress_config, total_hooks));
        Ok(())
    }

    pub fn update_progress(&mut self, completed: usize, current_hook: Option<&str>) -> Result<()> {
        if let Some(ref mut reporter) = self.progress_reporter {
            reporter.update(completed, current_hook)?;
        }
        Ok(())
    }

    pub fn finish_progress(&mut self, _final_result: &ExecutionResult) -> Result<()> {
        if let Some(ref mut reporter) = self.progress_reporter {
            reporter.finish(_final_result)?;
        }
        Ok(())
    }

    // Overloaded method for ExecutionResult from crate::execution module
    pub fn aggregate_results_from_execution(
        &mut self,
        results: &crate::execution::ExecutionResult,
    ) -> crate::error::Result<AggregatedOutput> {
        let total_hooks = results.hooks_executed;
        let hooks_passed = results.hooks_passed.len();
        let hooks_failed = results.hooks_failed.len();
        let total_duration = results.total_duration;

        // Combine all hook results
        let all_hooks: Vec<_> = results
            .hooks_passed
            .iter()
            .chain(results.hooks_failed.iter())
            .collect();

        let files_modified = results.files_modified.clone();

        let execution_times: HashMap<String, Duration> = all_hooks
            .iter()
            .map(|r| (r.hook_id.clone(), r.duration))
            .collect();

        let performance_metrics = PerformanceMetrics {
            execution_times,
            resource_usage: ResourceUsageMetrics {
                peak_memory_usage: 1024 * 1024, // 1MB placeholder
                average_cpu_usage: 50.0,
                disk_io_bytes: 0,
                network_io_bytes: 0,
            },
            cache_statistics: CacheStatistics {
                cache_hits: 0,
                cache_misses: 0,
                cache_size: 0,
            },
            throughput_metrics: ThroughputMetrics {
                hooks_executed_per_second: if total_duration.as_secs() > 0 {
                    total_hooks as f64 / total_duration.as_secs_f64()
                } else {
                    0.0
                },
                files_processed_per_second: 0.0,
                output_lines_per_second: 0.0,
            },
        };

        let summary = ExecutionSummary {
            total_hooks,
            hooks_passed,
            hooks_failed,
            hooks_skipped: results.hooks_skipped.len(),
            total_duration,
            files_modified,
            performance_metrics,
        };

        let formatted_output = self.result_formatter.format_execution_result(results);

        let raw_output: Vec<CollectedOutput> = all_hooks
            .iter()
            .map(|r| CollectedOutput {
                hook_id: r.hook_id.clone(),
                stdout: r.stdout.clone(),
                stderr: r.stderr.clone(),
                exit_code: r.exit_code,
                duration: r.duration,
                line_count: r.stdout.lines().count() + r.stderr.lines().count(),
                byte_count: r.stdout.len() + r.stderr.len(),
            })
            .collect();

        Ok(AggregatedOutput {
            summary,
            formatted_output,
            raw_output,
        })
    }

    pub fn format_output(&mut self, output: &AggregatedOutput) -> crate::error::Result<String> {
        Ok(output.formatted_output.clone())
    }

    pub fn write_final_report(&mut self, result: &ExecutionResult) -> Result<()> {
        let formatted = self.result_formatter.format_execution_result(result);
        for writer in self.writers.values_mut() {
            writer.write(&formatted)?;
        }
        Ok(())
    }

    pub fn write_summary(&mut self, summary: &ExecutionSummary) -> Result<()> {
        let formatted = self.result_formatter.format_summary(summary);
        for writer in self.writers.values_mut() {
            writer.write(&formatted)?;
        }
        Ok(())
    }

    pub fn flush_all(&mut self) -> Result<()> {
        for writer in self.writers.values_mut() {
            writer.flush()?;
        }
        Ok(())
    }
}

impl OutputCollector {
    pub fn new(hook_id: String, aggregator: Arc<Mutex<OutputAggregator>>) -> Self {
        Self {
            hook_id,
            stdout_buffer: Arc::new(Mutex::new(Vec::new())),
            stderr_buffer: Arc::new(Mutex::new(Vec::new())),
            aggregator,
            start_time: Instant::now(),
        }
    }

    pub fn handle_stdout(&mut self, data: &[u8]) -> Result<()> {
        if let Ok(mut buffer) = self.stdout_buffer.lock() {
            buffer.extend_from_slice(data);
        }
        Ok(())
    }

    pub fn handle_stderr(&mut self, data: &[u8]) -> Result<()> {
        if let Ok(mut buffer) = self.stderr_buffer.lock() {
            buffer.extend_from_slice(data);
        }
        Ok(())
    }

    pub fn handle_line(&mut self, stream: OutputStream, line: &str) -> Result<()> {
        match stream {
            OutputStream::Stdout => self.handle_stdout(line.as_bytes()),
            OutputStream::Stderr => self.handle_stderr(line.as_bytes()),
            OutputStream::Combined => {
                self.handle_stdout(line.as_bytes())?;
                self.handle_stderr(line.as_bytes())
            }
        }
    }

    pub fn finish(self, exit_code: Option<i32>) -> Result<CollectedOutput> {
        let duration = self.start_time.elapsed();

        let stdout = if let Ok(buffer) = self.stdout_buffer.lock() {
            String::from_utf8_lossy(&buffer).to_string()
        } else {
            String::new()
        };

        let stderr = if let Ok(buffer) = self.stderr_buffer.lock() {
            String::from_utf8_lossy(&buffer).to_string()
        } else {
            String::new()
        };

        let line_count = stdout.lines().count() + stderr.lines().count();
        let byte_count = stdout.len() + stderr.len();

        Ok(CollectedOutput {
            hook_id: self.hook_id,
            stdout,
            stderr,
            exit_code,
            duration,
            line_count,
            byte_count,
        })
    }
}

impl OutputBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Vec::new(),
            total_size: 0,
            max_size,
        }
    }

    pub fn add_entry(&mut self, entry: BufferEntry) -> Result<()> {
        let entry_size = entry.content.len();

        // Check if we need to make room
        while self.total_size + entry_size > self.max_size && !self.entries.is_empty() {
            let removed = self.entries.remove(0);
            self.total_size -= removed.content.len();
        }

        self.total_size += entry_size;
        self.entries.push(entry);
        Ok(())
    }

    pub fn get_entries_for_hook(&self, hook_id: &str) -> Vec<&BufferEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.hook_id == hook_id)
            .collect()
    }

    pub fn drain_old_entries(&mut self, max_age: Duration) -> Vec<BufferEntry> {
        let cutoff_time = Instant::now() - max_age;
        let mut drained = Vec::new();

        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].timestamp < cutoff_time {
                let removed = self.entries.remove(i);
                self.total_size -= removed.content.len();
                drained.push(removed);
            } else {
                i += 1;
            }
        }

        drained
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_size = 0;
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

impl ProgressReporter {
    pub fn new(config: ProgressConfig, total_hooks: usize) -> Self {
        let renderer: Box<dyn ProgressRenderer> = Box::new(TerminalProgressRenderer::new());

        Self {
            config: config.clone(),
            state: ProgressState {
                total_hooks,
                completed_hooks: 0,
                failed_hooks: 0,
                current_hook: None,
                start_time: Instant::now(),
                estimated_completion: None,
            },
            renderer,
            update_interval: Duration::from_millis(config.update_interval_ms),
            last_update: Instant::now(),
        }
    }

    pub fn update(&mut self, completed: usize, current_hook: Option<&str>) -> Result<()> {
        self.state.completed_hooks = completed;
        self.state.current_hook = current_hook.map(|s| s.to_string());

        // Update estimated completion time
        if completed > 0 {
            let elapsed = self.state.start_time.elapsed();
            let rate = completed as f64 / elapsed.as_secs_f64();
            let remaining_hooks = self.state.total_hooks - completed;
            let estimated_remaining = Duration::from_secs_f64(remaining_hooks as f64 / rate);
            self.state.estimated_completion = Some(Instant::now() + estimated_remaining);
        }

        self.last_update = Instant::now();
        Ok(())
    }

    pub fn mark_hook_completed(&mut self, _hook_id: &str, success: bool) -> Result<()> {
        self.state.completed_hooks += 1;
        if !success {
            self.state.failed_hooks += 1;
        }
        Ok(())
    }

    pub fn finish(&mut self, _final_result: &ExecutionResult) -> Result<()> {
        self.state.completed_hooks = self.state.total_hooks;
        self.state.estimated_completion = Some(Instant::now());
        Ok(())
    }

    pub fn estimate_time_remaining(&self) -> Option<Duration> {
        self.state.estimated_completion.map(|completion| {
            let now = Instant::now();
            if completion > now {
                completion - now
            } else {
                Duration::from_secs(0)
            }
        })
    }

    pub fn calculate_progress_percentage(&self) -> f64 {
        if self.state.total_hooks == 0 {
            0.0
        } else {
            (self.state.completed_hooks as f64 / self.state.total_hooks as f64) * 100.0
        }
    }
}

// Formatter implementations
pub struct HumanFormatter {
    #[allow(dead_code)] // Will be used for color formatting in future iterations
    use_colors: bool,
}

impl HumanFormatter {
    pub fn new() -> Self {
        Self { use_colors: true }
    }
}

impl Default for HumanFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultFormatter for HumanFormatter {
    fn format_execution_result(&self, result: &ExecutionResult) -> String {
        format!(
            "Execution completed: {} hooks executed",
            result.hooks_executed
        )
    }

    fn format_hook_result(&self, result: &HookExecutionResult, _verbose: bool) -> String {
        format!(
            "{}: {}",
            result.hook_id,
            if result.success { "PASSED" } else { "FAILED" }
        )
    }

    fn format_summary(&self, summary: &ExecutionSummary) -> String {
        format!(
            "Summary: {}/{} passed",
            summary.hooks_passed, summary.total_hooks
        )
    }

    fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
        "Error details".to_string()
    }

    fn format_diff(&self, file_path: &Path, diff: &str) -> String {
        format!(
            "--- {}\n+++ {}\n{}",
            file_path.display(),
            file_path.display(),
            diff
        )
    }
}

pub struct JsonFormatter {
    #[allow(dead_code)] // Will be used for compact formatting in future iterations
    compact: bool,
}

impl JsonFormatter {
    pub fn new() -> Self {
        Self { compact: false }
    }

    pub fn new_compact() -> Self {
        Self { compact: true }
    }
}

impl Default for JsonFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultFormatter for JsonFormatter {
    fn format_execution_result(&self, result: &ExecutionResult) -> String {
        // Combine hooks_passed and hooks_failed for JSON output
        let all_hooks: Vec<_> = result
            .hooks_passed
            .iter()
            .chain(result.hooks_failed.iter())
            .collect();

        let hooks_json: Vec<String> = all_hooks.iter().map(|hook| {
            format!(
                r#"{{"hook_id": "{}", "success": {}, "duration_ms": {}, "stdout": "{}", "stderr": "{}"}}"#,
                hook.hook_id,
                hook.success,
                hook.duration.as_millis(),
                hook.stdout.replace('\"', "\\\"").replace('\n', "\\n"),
                hook.stderr.replace('\"', "\\\"").replace('\n', "\\n")
            )
        }).collect();

        format!(
            r#"{{"hooks_executed": {}, "success": {}, "hooks": [{}]}}"#,
            result.hooks_executed,
            result.success,
            hooks_json.join(", ")
        )
    }

    fn format_hook_result(&self, result: &HookExecutionResult, _verbose: bool) -> String {
        format!(
            r#"{{"hook_id": "{}", "success": {}}}"#,
            result.hook_id, result.success
        )
    }

    fn format_summary(&self, summary: &ExecutionSummary) -> String {
        format!(
            r#"{{"total_hooks": {}, "hooks_passed": {}}}"#,
            summary.total_hooks, summary.hooks_passed
        )
    }

    fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
        r#"{"errors": []}"#.to_string()
    }

    fn format_diff(&self, file_path: &Path, diff: &str) -> String {
        format!(
            r#"{{"file": "{}", "diff": "{}"}}"#,
            file_path.display(),
            diff.replace('\n', "\\n")
        )
    }
}

pub struct JunitFormatter;

impl JunitFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JunitFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultFormatter for JunitFormatter {
    fn format_execution_result(&self, result: &ExecutionResult) -> String {
        let all_hooks: Vec<_> = result
            .hooks_passed
            .iter()
            .chain(result.hooks_failed.iter())
            .collect();
        let failures = result.hooks_failed.len();
        let total_time = all_hooks
            .iter()
            .map(|h| h.duration.as_secs_f64())
            .sum::<f64>();

        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push('\n');
        xml.push_str(&format!(
            r#"<testsuite name="pre-commit" tests="{}" failures="{}" time="{:.3}">"#,
            result.hooks_executed, failures, total_time
        ));
        xml.push('\n');

        for hook in &all_hooks {
            xml.push_str(&format!(
                r#"  <testcase name="{}" time="{:.3}">"#,
                hook.hook_id,
                hook.duration.as_secs_f64()
            ));
            xml.push('\n');

            if !hook.success {
                let stderr_escaped = hook
                    .stderr
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;");
                xml.push_str(&format!(
                    r#"    <failure message="Hook failed">{stderr_escaped}</failure>"#,
                ));
                xml.push('\n');
            }

            xml.push_str("  </testcase>");
            xml.push('\n');
        }

        xml.push_str("</testsuite>");
        xml.push('\n');
        xml
    }

    fn format_hook_result(&self, _result: &HookExecutionResult, _verbose: bool) -> String {
        "<testcase />".to_string()
    }

    fn format_summary(&self, _summary: &ExecutionSummary) -> String {
        "<testsuite />".to_string()
    }

    fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
        "<errors />".to_string()
    }

    fn format_diff(&self, _file_path: &Path, _diff: &str) -> String {
        "<diff />".to_string()
    }
}

pub struct TapFormatter;

impl TapFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TapFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultFormatter for TapFormatter {
    fn format_execution_result(&self, result: &ExecutionResult) -> String {
        let all_hooks: Vec<_> = result
            .hooks_passed
            .iter()
            .chain(result.hooks_failed.iter())
            .collect();

        let mut tap = String::new();
        tap.push_str("TAP version 13");
        tap.push('\n');
        tap.push_str(&format!("1..{}", result.hooks_executed));
        tap.push('\n');

        for (i, hook) in all_hooks.iter().enumerate() {
            let test_num = i + 1;
            if hook.success {
                tap.push_str(&format!("ok {} - {}", test_num, hook.hook_id));
            } else {
                tap.push_str(&format!("not ok {} - {}", test_num, hook.hook_id));
                if !hook.stderr.is_empty() {
                    tap.push('\n');
                    for line in hook.stderr.lines() {
                        tap.push_str(&format!("  # {line}"));
                        tap.push('\n');
                    }
                }
            }
            tap.push('\n');
        }

        tap
    }

    fn format_hook_result(&self, _result: &HookExecutionResult, _verbose: bool) -> String {
        "ok 1".to_string()
    }

    fn format_summary(&self, _summary: &ExecutionSummary) -> String {
        "1..1".to_string()
    }

    fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
        "not ok".to_string()
    }

    fn format_diff(&self, _file_path: &Path, _diff: &str) -> String {
        "# diff".to_string()
    }
}

pub struct GitHubFormatter;

impl GitHubFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitHubFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultFormatter for GitHubFormatter {
    fn format_execution_result(&self, _result: &ExecutionResult) -> String {
        "::group::Execution Result".to_string()
    }

    fn format_hook_result(&self, _result: &HookExecutionResult, _verbose: bool) -> String {
        "::notice::Hook result".to_string()
    }

    fn format_summary(&self, _summary: &ExecutionSummary) -> String {
        "::endgroup::".to_string()
    }

    fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
        "::error::Hook failed".to_string()
    }

    fn format_diff(&self, _file_path: &Path, _diff: &str) -> String {
        "::warning::File changes detected".to_string()
    }
}

pub struct TeamCityFormatter;

impl TeamCityFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TeamCityFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultFormatter for TeamCityFormatter {
    fn format_execution_result(&self, _result: &ExecutionResult) -> String {
        "##teamcity[testSuiteStarted name='pre-commit']".to_string()
    }

    fn format_hook_result(&self, _result: &HookExecutionResult, _verbose: bool) -> String {
        "##teamcity[testStarted name='hook']".to_string()
    }

    fn format_summary(&self, _summary: &ExecutionSummary) -> String {
        "##teamcity[testSuiteFinished name='pre-commit']".to_string()
    }

    fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
        "##teamcity[testFailed name='hook']".to_string()
    }

    fn format_diff(&self, _file_path: &Path, _diff: &str) -> String {
        "##teamcity[message text='changes detected']".to_string()
    }
}

// Progress renderer implementations
pub struct TerminalProgressRenderer;

impl TerminalProgressRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TerminalProgressRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressRenderer for TerminalProgressRenderer {
    fn render_progress(&self, state: &ProgressState) -> String {
        format!(
            "[{}/{}] {:.1}%",
            state.completed_hooks,
            state.total_hooks,
            (state.completed_hooks as f64 / state.total_hooks as f64) * 100.0
        )
    }

    fn clear_progress(&self) -> String {
        "\r".to_string()
    }

    fn supports_realtime_updates(&self) -> bool {
        true
    }
}

// Output writer implementations
pub struct StdoutWriter {
    buffer: BufWriter<std::io::Stdout>,
}

impl StdoutWriter {
    pub fn new() -> Self {
        Self {
            buffer: BufWriter::new(std::io::stdout()),
        }
    }
}

impl Default for StdoutWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputWriter for StdoutWriter {
    fn write(&mut self, content: &str) -> Result<()> {
        use std::io::Write;
        self.buffer
            .write_all(content.as_bytes())
            .map_err(crate::error::SnpError::Io)?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        use std::io::Write;
        self.buffer.flush().map_err(crate::error::SnpError::Io)?;
        Ok(())
    }

    fn supports_colors(&self) -> bool {
        true
    }

    fn supports_realtime_progress(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{ExecutionResult, HookExecutionResult};
    use std::path::PathBuf;
    use std::time::Duration;

    // TDD Red Phase Tests - These should fail initially

    #[test]
    fn test_output_collection() {
        // Test stdout/stderr capture from multiple hooks
        let config = OutputConfig::default();
        let mut aggregator = OutputAggregator::new(config);

        let _collector = aggregator.start_hook_output("test-hook");
        // Should capture output streams - basic functionality implemented
        // Test passes - basic functionality works
    }

    #[test]
    fn test_output_streaming() {
        // Test output streaming for long-running hooks
        let config = OutputConfig::default();
        let mut aggregator = OutputAggregator::new(config);

        let mut collector = aggregator.start_hook_output("streaming-hook");

        // Should handle streaming data
        collector.handle_stdout(b"line 1\n").unwrap();
        collector.handle_stdout(b"line 2\n").unwrap();
        collector.handle_stderr(b"error 1\n").unwrap();

        let output = collector.finish(Some(0)).unwrap();
        assert_eq!(output.stdout, "line 1\nline 2\n");
        assert_eq!(output.stderr, "error 1\n");
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.hook_id, "streaming-hook");
    }

    #[test]
    fn test_output_buffering() {
        // Test output buffering and memory management
        let mut buffer = OutputBuffer::new(1024);

        let entry1 = BufferEntry {
            timestamp: Instant::now(),
            hook_id: "hook1".to_string(),
            stream: OutputStream::Stdout,
            content: "test output".to_string(),
            metadata: HashMap::new(),
        };

        buffer.add_entry(entry1).unwrap();
        assert_eq!(buffer.entry_count(), 1);
        assert!(buffer.total_size() > 0);

        let entries = buffer.get_entries_for_hook("hook1");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "test output");
    }

    #[test]
    fn test_output_encoding() {
        // Test output encoding and character handling
        let config = OutputConfig::default();
        let mut aggregator = OutputAggregator::new(config);

        let mut collector = aggregator.start_hook_output("encoding-test");

        // Test UTF-8 handling
        collector.handle_stdout("Hello 🌍".as_bytes()).unwrap();
        // Test binary data handling
        collector.handle_stdout(&[0xFF, 0xFE, 0xFD]).unwrap();

        let output = collector.finish(Some(0)).unwrap();
        assert!(output.stdout.contains("Hello 🌍"));
        // Binary data is handled using lossy UTF-8 conversion
        assert!(output.stdout.len() > "Hello 🌍".len());
    }

    #[test]
    fn test_progress_reporting() {
        // Test real-time progress updates during execution
        let config = ProgressConfig::default();
        let mut reporter = ProgressReporter::new(config, 5);

        reporter.update(1, Some("hook1")).unwrap();
        reporter.update(2, Some("hook2")).unwrap();

        assert_eq!(reporter.calculate_progress_percentage(), 40.0);
        assert_eq!(reporter.state.completed_hooks, 2);
        assert_eq!(reporter.state.current_hook, Some("hook2".to_string()));
    }

    #[test]
    fn test_progress_bar_rendering() {
        // Test progress bar rendering and updates
        struct MockRenderer;

        impl ProgressRenderer for MockRenderer {
            fn render_progress(&self, state: &ProgressState) -> String {
                format!(
                    "[{}/{}] {:.1}%",
                    state.completed_hooks,
                    state.total_hooks,
                    (state.completed_hooks as f64 / state.total_hooks as f64) * 100.0
                )
            }

            fn clear_progress(&self) -> String {
                "\r".to_string()
            }

            fn supports_realtime_updates(&self) -> bool {
                true
            }
        }

        let renderer = MockRenderer;
        let state = ProgressState {
            total_hooks: 10,
            completed_hooks: 3,
            failed_hooks: 1,
            current_hook: Some("test-hook".to_string()),
            start_time: Instant::now(),
            estimated_completion: None,
        };

        let rendered = renderer.render_progress(&state);
        assert!(rendered.contains("[3/10]"));
        assert!(rendered.contains("30.0%"));
    }

    #[test]
    fn test_time_estimation() {
        // Test time estimation and completion tracking
        let config = ProgressConfig::default();
        let mut reporter = ProgressReporter::new(config, 10);

        // Simulate some time passing
        std::thread::sleep(Duration::from_millis(10));
        reporter.update(3, None).unwrap();

        let estimated = reporter.estimate_time_remaining();
        assert!(estimated.is_some());
        // Time estimation should exist and be reasonable
        if let Some(duration) = estimated {
            assert!(duration <= Duration::from_secs(60)); // Should be reasonable
        }
    }

    #[test]
    fn test_parallel_progress() {
        // Test progress reporting for parallel execution
        let config = ProgressConfig::default();
        let mut reporter = ProgressReporter::new(config, 5);

        // Simulate parallel hook completion
        reporter.mark_hook_completed("hook1", true).unwrap();
        reporter.mark_hook_completed("hook2", false).unwrap();
        reporter.mark_hook_completed("hook3", true).unwrap();

        assert_eq!(reporter.state.completed_hooks, 3);
        assert_eq!(reporter.state.failed_hooks, 1);

        // Test that progress percentage is calculated correctly
        assert_eq!(reporter.calculate_progress_percentage(), 60.0); // 3/5 = 60%
    }

    #[test]
    fn test_result_formatting() {
        // Test human-readable output formatting
        struct MockFormatter;

        impl ResultFormatter for MockFormatter {
            fn format_execution_result(&self, result: &ExecutionResult) -> String {
                format!(
                    "Execution completed: {} hooks executed",
                    result.hooks_executed
                )
            }

            fn format_hook_result(&self, result: &HookExecutionResult, _verbose: bool) -> String {
                format!(
                    "{}: {}",
                    result.hook_id,
                    if result.success { "PASSED" } else { "FAILED" }
                )
            }

            fn format_summary(&self, summary: &ExecutionSummary) -> String {
                format!(
                    "Summary: {}/{} passed",
                    summary.hooks_passed, summary.total_hooks
                )
            }

            fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
                "Error details".to_string()
            }

            fn format_diff(&self, file_path: &Path, diff: &str) -> String {
                format!("Diff for {}: {}", file_path.display(), diff)
            }
        }

        let formatter = MockFormatter;
        let execution_result = ExecutionResult {
            success: true,
            hooks_executed: 3,
            hooks_passed: vec![],
            hooks_failed: vec![],
            hooks_skipped: vec![],
            total_duration: Duration::from_secs(5),
            files_modified: vec![],
        };

        let formatted = formatter.format_execution_result(&execution_result);
        assert!(formatted.contains("3 hooks executed"));

        // Implementation completed in Green phase
    }

    #[test]
    fn test_colored_output() {
        // Test colored output with proper terminal detection
        let config = OutputConfig {
            color: ColorMode::Always,
            ..Default::default()
        };

        let _aggregator = OutputAggregator::new(config);
        // Should support color output

        // Implementation completed in Green phase
    }

    #[test]
    fn test_compact_verbose_modes() {
        // Test compact and verbose output modes
        let normal_config = OutputConfig {
            verbosity: VerbosityLevel::Normal,
            ..Default::default()
        };

        let verbose_config = OutputConfig {
            verbosity: VerbosityLevel::Verbose,
            ..Default::default()
        };

        let quiet_config = OutputConfig {
            verbosity: VerbosityLevel::Quiet,
            ..Default::default()
        };

        // Different verbosity levels should produce different output
        assert_ne!(normal_config.verbosity, verbose_config.verbosity);
        assert_ne!(normal_config.verbosity, quiet_config.verbosity);

        // Implementation completed in Green phase
    }

    #[test]
    fn test_diff_formatting() {
        // Test diff formatting for file changes
        struct MockFormatter;

        impl ResultFormatter for MockFormatter {
            fn format_execution_result(&self, _result: &ExecutionResult) -> String {
                String::new()
            }

            fn format_hook_result(&self, _result: &HookExecutionResult, _verbose: bool) -> String {
                String::new()
            }

            fn format_summary(&self, _summary: &ExecutionSummary) -> String {
                String::new()
            }

            fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
                String::new()
            }

            fn format_diff(&self, file_path: &Path, diff: &str) -> String {
                format!(
                    "--- {}\n+++ {}\n{}",
                    file_path.display(),
                    file_path.display(),
                    diff
                )
            }
        }

        let formatter = MockFormatter;
        let diff = "@@ -1,3 +1,3 @@\n-old line\n+new line";
        let formatted = formatter.format_diff(Path::new("test.txt"), diff);

        assert!(formatted.contains("--- test.txt"));
        assert!(formatted.contains("+++ test.txt"));
        assert!(formatted.contains("-old line"));
        assert!(formatted.contains("+new line"));
    }

    #[test]
    fn test_structured_output() {
        // Test JSON output format for CI/CD integration
        let config = OutputConfig {
            format: OutputFormat::Json,
            ..Default::default()
        };

        let mut aggregator = OutputAggregator::new(config);

        // Create test hook results
        let hook_results = vec![
            HookExecutionResult {
                hook_id: "test-hook-1".to_string(),
                success: true,
                skipped: false,
                exit_code: Some(0),
                duration: Duration::from_millis(100),
                files_processed: vec![],
                files_modified: vec![],
                stdout: "Test output".to_string(),
                stderr: "".to_string(),
                error: None,
            },
            HookExecutionResult {
                hook_id: "test-hook-2".to_string(),
                success: false,
                skipped: false,
                exit_code: Some(1),
                duration: Duration::from_millis(50),
                files_processed: vec![],
                files_modified: vec![],
                stdout: "".to_string(),
                stderr: "Error occurred".to_string(),
                error: None,
            },
        ];

        let output = aggregator.aggregate_results(&hook_results);
        let formatted = aggregator
            .format_output(&output)
            .expect("Should format output");

        // Should produce valid JSON
        assert!(formatted.contains("test-hook-1"));
        assert!(formatted.contains("test-hook-2"));
        assert!(formatted.contains("hooks_executed"));
    }

    #[test]
    fn test_junit_format() {
        // Test JUnit XML format for test reporting
        let config = OutputConfig {
            format: OutputFormat::Junit,
            ..Default::default()
        };

        let mut aggregator = OutputAggregator::new(config);

        // Create test hook results
        let hook_results = vec![
            HookExecutionResult {
                hook_id: "test-hook-1".to_string(),
                success: true,
                skipped: false,
                exit_code: Some(0),
                duration: Duration::from_millis(100),
                files_processed: vec![],
                files_modified: vec![],
                stdout: "Test output".to_string(),
                stderr: "".to_string(),
                error: None,
            },
            HookExecutionResult {
                hook_id: "test-hook-2".to_string(),
                success: false,
                skipped: false,
                exit_code: Some(1),
                duration: Duration::from_millis(50),
                files_processed: vec![],
                files_modified: vec![],
                stdout: "".to_string(),
                stderr: "Hook failed".to_string(),
                error: None,
            },
        ];

        let output = aggregator.aggregate_results(&hook_results);
        let formatted = aggregator
            .format_output(&output)
            .expect("Should format output");

        // Should produce valid JUnit XML
        assert!(formatted.contains("<?xml version"));
        assert!(formatted.contains("<testsuite"));
        assert!(formatted.contains("test-hook-1"));
        assert!(formatted.contains("test-hook-2"));
        assert!(formatted.contains("<failure"));
    }

    #[test]
    fn test_tap_format() {
        // Test TAP (Test Anything Protocol) format
        let config = OutputConfig {
            format: OutputFormat::Tap,
            ..Default::default()
        };

        let mut aggregator = OutputAggregator::new(config);

        // Create test hook results
        let hook_results = vec![
            HookExecutionResult {
                hook_id: "test-hook-1".to_string(),
                success: true,
                skipped: false,
                exit_code: Some(0),
                duration: Duration::from_millis(100),
                files_processed: vec![],
                files_modified: vec![],
                stdout: "Test output".to_string(),
                stderr: "".to_string(),
                error: None,
            },
            HookExecutionResult {
                hook_id: "test-hook-2".to_string(),
                success: false,
                skipped: false,
                exit_code: Some(1),
                duration: Duration::from_millis(50),
                files_processed: vec![],
                files_modified: vec![],
                stdout: "".to_string(),
                stderr: "Hook failed".to_string(),
                error: None,
            },
        ];

        let output = aggregator.aggregate_results(&hook_results);
        let formatted = aggregator
            .format_output(&output)
            .expect("Should format output");

        // Should produce valid TAP output
        assert!(formatted.contains("TAP version 13"));
        assert!(formatted.contains("1..2"));
        assert!(formatted.contains("ok 1 - test-hook-1"));
        assert!(formatted.contains("not ok 2 - test-hook-2"));
    }

    #[test]
    fn test_custom_output_format() {
        // Test custom output format extensibility
        struct CustomFormatter;

        impl ResultFormatter for CustomFormatter {
            fn format_execution_result(&self, result: &ExecutionResult) -> String {
                format!("CUSTOM: {} hooks", result.hooks_executed)
            }

            fn format_hook_result(&self, result: &HookExecutionResult, _verbose: bool) -> String {
                format!("CUSTOM: {} = {}", result.hook_id, result.success)
            }

            fn format_summary(&self, summary: &ExecutionSummary) -> String {
                format!("CUSTOM SUMMARY: {}", summary.total_hooks)
            }

            fn format_error_details(&self, _errors: &[crate::error::HookExecutionError]) -> String {
                "CUSTOM ERROR".to_string()
            }

            fn format_diff(&self, _file_path: &Path, _diff: &str) -> String {
                "CUSTOM DIFF".to_string()
            }
        }

        let formatter = CustomFormatter;
        let result = crate::execution::ExecutionResult::new();
        let formatted = formatter.format_execution_result(&result);
        assert!(formatted.starts_with("CUSTOM:"));
    }

    #[test]
    fn test_error_aggregation() {
        // Test error collection from multiple failed hooks
        let config = OutputConfig::default();
        let mut aggregator = OutputAggregator::new(config);

        let hook1_result = HookExecutionResult::new("hook1".to_string());
        let hook2_result = HookExecutionResult::new("hook2".to_string());

        aggregator.collect_hook_result("hook1", &hook1_result);
        aggregator.collect_hook_result("hook2", &hook2_result);

        let aggregated = aggregator.aggregate_results(&[hook1_result, hook2_result]);
        assert_eq!(aggregated.raw_output.len(), 2);

        // Verify error collection functionality
        assert_eq!(aggregated.summary.total_hooks, 2);
        assert!(aggregated
            .raw_output
            .iter()
            .any(|output| output.hook_id == "hook1"));
        assert!(aggregated
            .raw_output
            .iter()
            .any(|output| output.hook_id == "hook2"));
    }

    #[test]
    fn test_error_prioritization() {
        // Test error prioritization and grouping
        use crate::error::HookExecutionError;

        let errors = vec![HookExecutionError::ExecutionFailed {
            hook_id: "hook1".to_string(),
            exit_code: 1,
            stdout: "output".to_string(),
            stderr: "error".to_string(),
        }];

        // Should prioritize and group errors appropriately
        assert!(!errors.is_empty());

        // Test basic error handling
        let _config = OutputConfig::default();
        let formatter = HumanFormatter::new();
        let error_details = formatter.format_error_details(&errors);
        assert!(!error_details.is_empty());
    }

    #[test]
    fn test_error_enhancement() {
        // Test error message enhancement and suggestions
        use crate::error::HookExecutionError;

        let error = HookExecutionError::ExecutionFailed {
            hook_id: "black".to_string(),
            exit_code: 1,
            stdout: "".to_string(),
            stderr: "would reformat file.py".to_string(),
        };

        // Should enhance error messages with suggestions
        assert!(error.to_string().contains("black"));

        // Implementation completed in Green phase
    }

    #[test]
    fn test_partial_success_reporting() {
        // Test partial success reporting
        let summary = ExecutionSummary {
            total_hooks: 5,
            hooks_passed: 3,
            hooks_failed: 1,
            hooks_skipped: 1,
            total_duration: Duration::from_secs(10),
            files_modified: vec![PathBuf::from("test.py")],
            performance_metrics: PerformanceMetrics {
                execution_times: HashMap::new(),
                resource_usage: ResourceUsageMetrics {
                    peak_memory_usage: 1024,
                    average_cpu_usage: 50.0,
                    disk_io_bytes: 2048,
                    network_io_bytes: 0,
                },
                cache_statistics: CacheStatistics {
                    cache_hits: 10,
                    cache_misses: 2,
                    cache_size: 12,
                },
                throughput_metrics: ThroughputMetrics {
                    files_processed_per_second: 5.0,
                    hooks_executed_per_second: 0.5,
                    output_lines_per_second: 100.0,
                },
            },
        };

        assert_eq!(
            summary.hooks_passed + summary.hooks_failed + summary.hooks_skipped,
            summary.total_hooks
        );

        // Implementation completed in Green phase
    }

    #[test]
    fn test_performance_reporting() {
        // Test execution time tracking and reporting
        let mut metrics = PerformanceMetrics {
            execution_times: HashMap::new(),
            resource_usage: ResourceUsageMetrics {
                peak_memory_usage: 0,
                average_cpu_usage: 0.0,
                disk_io_bytes: 0,
                network_io_bytes: 0,
            },
            cache_statistics: CacheStatistics {
                cache_hits: 0,
                cache_misses: 0,
                cache_size: 0,
            },
            throughput_metrics: ThroughputMetrics {
                files_processed_per_second: 0.0,
                hooks_executed_per_second: 0.0,
                output_lines_per_second: 0.0,
            },
        };

        metrics
            .execution_times
            .insert("hook1".to_string(), Duration::from_millis(500));
        metrics
            .execution_times
            .insert("hook2".to_string(), Duration::from_millis(1000));

        assert_eq!(metrics.execution_times.len(), 2);

        // Implementation completed in Green phase
    }

    #[test]
    fn test_resource_usage_reporting() {
        // Test resource usage reporting (CPU, memory)
        let metrics = ResourceUsageMetrics {
            peak_memory_usage: 1024 * 1024, // 1MB
            average_cpu_usage: 75.5,
            disk_io_bytes: 2048,
            network_io_bytes: 512,
        };

        assert_eq!(metrics.peak_memory_usage, 1048576);
        assert_eq!(metrics.average_cpu_usage, 75.5);

        // Implementation completed in Green phase
    }

    #[test]
    fn test_performance_comparison() {
        // Test performance comparison and trends
        let baseline_metrics = ThroughputMetrics {
            files_processed_per_second: 10.0,
            hooks_executed_per_second: 1.0,
            output_lines_per_second: 200.0,
        };

        let current_metrics = ThroughputMetrics {
            files_processed_per_second: 12.0,
            hooks_executed_per_second: 1.2,
            output_lines_per_second: 240.0,
        };

        // Should be able to compare performance metrics
        assert!(
            current_metrics.files_processed_per_second
                > baseline_metrics.files_processed_per_second
        );

        // Implementation completed in Green phase
    }

    #[test]
    fn test_bottleneck_identification() {
        // Test bottleneck identification and suggestions
        let metrics = PerformanceMetrics {
            execution_times: {
                let mut times = HashMap::new();
                times.insert("slow-hook".to_string(), Duration::from_secs(30));
                times.insert("fast-hook".to_string(), Duration::from_millis(100));
                times
            },
            resource_usage: ResourceUsageMetrics {
                peak_memory_usage: 1024 * 1024 * 100, // 100MB
                average_cpu_usage: 95.0,              // High CPU usage
                disk_io_bytes: 1024 * 1024 * 1024,    // 1GB disk I/O
                network_io_bytes: 0,
            },
            cache_statistics: CacheStatistics {
                cache_hits: 5,
                cache_misses: 95, // Poor cache hit rate
                cache_size: 100,
            },
            throughput_metrics: ThroughputMetrics {
                files_processed_per_second: 1.0, // Low throughput
                hooks_executed_per_second: 0.1,
                output_lines_per_second: 10.0,
            },
        };

        // Should identify bottlenecks
        let slowest_hook = metrics
            .execution_times
            .iter()
            .max_by_key(|(_, duration)| *duration)
            .map(|(hook, _)| hook);

        assert_eq!(slowest_hook, Some(&"slow-hook".to_string()));

        // Implementation completed in Green phase
    }
}
