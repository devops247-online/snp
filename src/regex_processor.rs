// Comprehensive regex processing system for SNP
use crate::error::SnpError;
use lru::LruCache;
use parking_lot::Mutex;
use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use regex_syntax::ParserBuilder;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Regex compilation configuration
#[derive(Debug, Clone)]
pub struct RegexConfig {
    pub case_insensitive: bool,
    pub multi_line: bool,
    pub dot_matches_new_line: bool,
    pub unicode: bool,
    pub size_limit: Option<usize>,
    pub dfa_size_limit: Option<usize>,
}

impl Default for RegexConfig {
    fn default() -> Self {
        Self {
            case_insensitive: false,
            multi_line: false,
            dot_matches_new_line: false,
            unicode: true,
            size_limit: Some(10 * (1 << 20)),    // 10MB
            dfa_size_limit: Some(2 * (1 << 20)), // 2MB
        }
    }
}

/// Comprehensive regex error types
#[derive(Debug, Error)]
pub enum RegexError {
    #[error("Invalid regex pattern: {pattern}")]
    InvalidPattern {
        pattern: String,
        #[source]
        error: regex::Error,
        suggestion: Option<String>,
    },

    #[error("Pattern too complex: {complexity_score:.2}")]
    ComplexityExceeded {
        pattern: String,
        complexity_score: f64,
        max_allowed: f64,
    },

    #[error("Potential ReDoS vulnerability detected: {pattern}")]
    SecurityViolation {
        pattern: String,
        vulnerability_type: String,
        attack_vector: String,
    },

    #[error("Compilation timeout: {pattern} took {duration:?}")]
    CompilationTimeout {
        pattern: String,
        duration: Duration,
        timeout_limit: Duration,
    },

    #[error("Cache capacity exceeded")]
    CacheOverflow {
        current_size: usize,
        max_size: usize,
        evicted_patterns: Vec<String>,
    },
}

impl RegexError {
    pub fn suggest_fix(&self) -> Option<String> {
        match self {
            RegexError::InvalidPattern { pattern, error, .. } => {
                Self::suggest_pattern_fix(pattern, error)
            }
            RegexError::ComplexityExceeded { pattern, .. } => Self::suggest_simplification(pattern),
            RegexError::SecurityViolation {
                pattern,
                vulnerability_type,
                ..
            } => Self::suggest_security_fix(pattern, vulnerability_type),
            _ => None,
        }
    }

    fn suggest_pattern_fix(pattern: &str, error: &regex::Error) -> Option<String> {
        let error_msg = error.to_string();

        if pattern.contains("[[:") && !pattern.contains("]]") {
            Some("POSIX character classes need double closing brackets: [[:alpha:]]".to_string())
        } else if pattern.contains("(?") && !Self::has_valid_group_syntax(pattern) {
            Some("Invalid group syntax. Use (?:...) for non-capturing groups".to_string())
        } else if error_msg.contains("unclosed") {
            Some("Check for unclosed brackets, parentheses, or character classes".to_string())
        } else if error_msg.contains("quantifier") {
            Some("Quantifiers like *, +, ? must follow a valid pattern".to_string())
        } else {
            None
        }
    }

    fn suggest_simplification(pattern: &str) -> Option<String> {
        if pattern.contains("(.*)*") || pattern.contains("(.+)+") {
            Some(
                "Avoid nested quantifiers like (.*)*. Use possessive quantifiers or atomic groups"
                    .to_string(),
            )
        } else if pattern.matches('(').count() > 10 {
            Some("Consider breaking complex patterns into simpler parts".to_string())
        } else if pattern.len() > 1000 {
            Some(
                "Very long patterns can be slow. Consider using multiple shorter patterns"
                    .to_string(),
            )
        } else {
            Some("Try simplifying the pattern or using more specific matching".to_string())
        }
    }

    fn suggest_security_fix(_pattern: &str, vulnerability_type: &str) -> Option<String> {
        match vulnerability_type {
            "exponential_backtracking" => Some(
                "Avoid nested quantifiers. Use possessive quantifiers (++, *+) or atomic groups"
                    .to_string(),
            ),
            "catastrophic_backtracking" => Some(
                "Replace alternation with character classes where possible: (a|b|c) → [abc]"
                    .to_string(),
            ),
            _ => Some("Review pattern for potential ReDoS vulnerabilities".to_string()),
        }
    }

    fn has_valid_group_syntax(pattern: &str) -> bool {
        // Simple heuristic - more comprehensive validation would parse the AST
        let group_patterns = ["(?:", "(?P<", "(?=", "(?!", "(?<=", "(?<!"];
        if pattern.contains("(?") {
            group_patterns.iter().any(|gp| pattern.contains(gp))
        } else {
            true // No groups, so valid
        }
    }
}

/// Compiled regex with metadata
#[derive(Debug, Clone)]
pub struct CompiledRegex {
    pub pattern: String,
    pub regex: Arc<Regex>,
    pub config: RegexConfig,
    pub complexity_score: f64,
    pub compilation_time: Duration,
    pub usage_count: u64,
}

impl CompiledRegex {
    pub fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    pub fn find_matches<'a>(&self, text: &'a str) -> Vec<regex::Match<'a>> {
        self.regex.find_iter(text).collect()
    }
}

/// Compilation metadata
#[derive(Debug, Clone)]
pub struct CompilationMetadata {
    pub compilation_time: Duration,
    pub pattern_complexity: f64,
    pub memory_usage: usize,
    pub dfa_size: Option<usize>,
}

/// Cache statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub memory_usage: usize,
    pub entry_count: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64
        }
    }

    pub fn memory_usage(&self) -> usize {
        self.memory_usage
    }
}

/// Main regex processor with caching and optimization
pub struct RegexProcessor {
    cache: Arc<Mutex<LruCache<String, CompiledRegex>>>,
    config: RegexConfig,
    stats: Arc<Mutex<CacheStats>>,
    max_complexity: f64,
    compilation_timeout: Duration,
}

impl Default for RegexProcessor {
    fn default() -> Self {
        Self::new(RegexConfig::default())
    }
}

impl RegexProcessor {
    pub fn new(config: RegexConfig) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap()))),
            config,
            stats: Arc::new(Mutex::new(CacheStats::default())),
            max_complexity: 100.0,
            compilation_timeout: Duration::from_millis(500),
        }
    }

    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(size).unwrap())));
        self
    }

    pub fn with_max_complexity(mut self, max_complexity: f64) -> Self {
        self.max_complexity = max_complexity;
        self
    }

    /// Compile a regex pattern with caching
    pub fn compile(&mut self, pattern: &str) -> crate::Result<Arc<Regex>> {
        // Check cache first
        {
            let mut cache = self.cache.lock();
            if let Some(compiled) = cache.get_mut(pattern) {
                compiled.usage_count += 1;
                self.stats.lock().hits += 1;
                return Ok(compiled.regex.clone());
            }
        }

        // Cache miss - compile new pattern
        self.stats.lock().misses += 1;
        let compiled = self
            .compile_pattern(pattern)
            .map_err(|e| SnpError::Regex(Box::new(e)))?;
        let regex = compiled.regex.clone();

        // Update cache
        {
            let mut cache = self.cache.lock();
            let mut stats = self.stats.lock();

            cache.put(pattern.to_string(), compiled);
            stats.entry_count = cache.len();
            // Rough memory estimation
            stats.memory_usage = cache.len() * (pattern.len() + 1000); // Rough estimate
        }

        Ok(regex)
    }

    /// Compile pattern with configuration
    pub fn compile_with_config(
        &mut self,
        pattern: &str,
        config: RegexConfig,
    ) -> crate::Result<Arc<Regex>> {
        let original_config = self.config.clone();
        self.config = config;
        let result = self.compile(pattern);
        self.config = original_config;
        result
    }

    /// Compile multiple patterns into a RegexSet
    pub fn compile_set(&mut self, patterns: &[String]) -> crate::Result<RegexSet> {
        let mut builder = RegexSetBuilder::new(patterns);

        builder
            .case_insensitive(self.config.case_insensitive)
            .multi_line(self.config.multi_line)
            .dot_matches_new_line(self.config.dot_matches_new_line)
            .unicode(self.config.unicode);

        if let Some(limit) = self.config.size_limit {
            builder.size_limit(limit);
        }

        if let Some(limit) = self.config.dfa_size_limit {
            builder.dfa_size_limit(limit);
        }

        builder.build().map_err(|e| {
            SnpError::Regex(Box::new(RegexError::InvalidPattern {
                pattern: format!("RegexSet with {} patterns", patterns.len()),
                error: e,
                suggestion: Some("Check individual patterns for validity".to_string()),
            }))
        })
    }

    /// Validate a pattern without compilation
    pub fn validate_pattern(&self, pattern: &str) -> crate::Result<()> {
        // Check syntax using regex-syntax parser
        let mut parser = ParserBuilder::new().build();

        match parser.parse(pattern) {
            Ok(_ast) => {
                // Additional validation can be added here
                self.check_security_issues_simple(pattern)
                    .map_err(|e| SnpError::Regex(Box::new(e)))?;
                Ok(())
            }
            Err(e) => Err(SnpError::Regex(Box::new(RegexError::InvalidPattern {
                pattern: pattern.to_string(),
                error: regex::Error::Syntax(e.to_string()),
                suggestion: RegexError::suggest_pattern_fix(
                    pattern,
                    &regex::Error::Syntax(e.to_string()),
                ),
            }))),
        }
    }

    /// Analyze pattern complexity and issues
    pub fn analyze_pattern(&self, pattern: &str) -> PatternAnalysis {
        PatternAnalyzer::analyze(pattern)
    }

    /// Check if pattern matches text
    pub fn is_match(&mut self, pattern: &str, text: &str) -> crate::Result<bool> {
        let regex = self.compile(pattern)?;
        Ok(regex.is_match(text))
    }

    /// Find all matches in text
    pub fn find_matches<'a>(
        &mut self,
        pattern: &str,
        text: &'a str,
    ) -> crate::Result<Vec<regex::Match<'a>>> {
        let regex = self.compile(pattern)?;
        Ok(regex.find_iter(text).collect())
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> CacheStats {
        *self.stats.lock()
    }

    /// Clear the regex cache
    pub fn clear_cache(&mut self) {
        let mut cache = self.cache.lock();
        let mut stats = self.stats.lock();

        cache.clear();
        stats.entry_count = 0;
        stats.memory_usage = 0;
    }

    /// Evict unused patterns older than specified age
    pub fn evict_unused(&mut self, _min_age: Duration) {
        // LruCache automatically evicts least recently used items
        // This could be enhanced to track access times
    }

    /// Internal pattern compilation
    fn compile_pattern(&self, pattern: &str) -> std::result::Result<CompiledRegex, RegexError> {
        let start_time = Instant::now();

        // Analyze pattern complexity and security
        let analysis = PatternAnalyzer::analyze(pattern);

        if analysis.complexity_score > self.max_complexity {
            return Err(RegexError::ComplexityExceeded {
                pattern: pattern.to_string(),
                complexity_score: analysis.complexity_score,
                max_allowed: self.max_complexity,
            });
        }

        // Check for security issues
        if let Some(warning) = analysis.security_warnings.first() {
            match warning {
                SecurityWarning::ReDoSVulnerability { attack_vector, .. } => {
                    return Err(RegexError::SecurityViolation {
                        pattern: pattern.to_string(),
                        vulnerability_type: "redos".to_string(),
                        attack_vector: attack_vector.clone(),
                    });
                }
                SecurityWarning::ResourceExhaustion { issue_type, .. } => {
                    return Err(RegexError::SecurityViolation {
                        pattern: pattern.to_string(),
                        vulnerability_type: issue_type.clone(),
                        attack_vector: "resource exhaustion".to_string(),
                    });
                }
            }
        }

        // Build regex with configuration
        let mut builder = RegexBuilder::new(pattern);

        builder
            .case_insensitive(self.config.case_insensitive)
            .multi_line(self.config.multi_line)
            .dot_matches_new_line(self.config.dot_matches_new_line)
            .unicode(self.config.unicode);

        if let Some(limit) = self.config.size_limit {
            builder.size_limit(limit);
        }

        if let Some(limit) = self.config.dfa_size_limit {
            builder.dfa_size_limit(limit);
        }

        let regex = builder.build().map_err(|e| RegexError::InvalidPattern {
            pattern: pattern.to_string(),
            error: e.clone(),
            suggestion: RegexError::suggest_pattern_fix(pattern, &e),
        })?;

        let compilation_time = start_time.elapsed();

        if compilation_time > self.compilation_timeout {
            return Err(RegexError::CompilationTimeout {
                pattern: pattern.to_string(),
                duration: compilation_time,
                timeout_limit: self.compilation_timeout,
            });
        }

        Ok(CompiledRegex {
            pattern: pattern.to_string(),
            regex: Arc::new(regex),
            config: self.config.clone(),
            complexity_score: analysis.complexity_score,
            compilation_time,
            usage_count: 1,
        })
    }

    fn check_security_issues_simple(&self, pattern: &str) -> std::result::Result<(), RegexError> {
        // Basic ReDoS pattern detection
        let dangerous_patterns = [
            r"\([^)]*\)\*\*",      // (...)** patterns
            r"\([^)]*\)\+\+",      // (...)++ patterns
            r"\([^)]*\*[^)]*\)\*", // (.*.*)* patterns
        ];

        for dangerous in &dangerous_patterns {
            if let Ok(check_regex) = Regex::new(dangerous) {
                if check_regex.is_match(pattern) {
                    return Err(RegexError::SecurityViolation {
                        pattern: pattern.to_string(),
                        vulnerability_type: "exponential_backtracking".to_string(),
                        attack_vector: "nested quantifiers".to_string(),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Pattern complexity and performance analysis
#[derive(Debug)]
pub struct PatternAnalysis {
    pub complexity_score: f64,
    pub estimated_performance: PerformanceClass,
    pub potential_issues: Vec<PatternIssue>,
    pub optimization_suggestions: Vec<String>,
    pub security_warnings: Vec<SecurityWarning>,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum PerformanceClass {
    Fast,
    Moderate,
    Slow,
    PotentiallyDangerous,
}

#[derive(Debug)]
pub enum PatternIssue {
    ExcessiveBacktracking {
        problematic_part: String,
        suggestion: String,
    },
    UnboundedRepetition {
        quantifier: String,
        position: usize,
    },
    ComplexLookaround {
        assertion: String,
        complexity: f64,
    },
    LargeCharacterClass {
        size: usize,
        suggestion: String,
    },
}

#[derive(Debug, Clone)]
pub enum SecurityWarning {
    ReDoSVulnerability {
        pattern_part: String,
        attack_vector: String,
        mitigation: String,
    },
    ResourceExhaustion {
        issue_type: String,
        estimated_impact: String,
    },
}

/// Pattern analyzer for complexity and security analysis
pub struct PatternAnalyzer;

impl PatternAnalyzer {
    pub fn analyze(pattern: &str) -> PatternAnalysis {
        let complexity_score = Self::calculate_complexity(pattern);
        let estimated_performance = Self::estimate_performance(complexity_score);
        let potential_issues = Self::detect_issues(pattern);
        let optimization_suggestions = Self::suggest_optimizations_internal(pattern);
        let security_warnings = Self::detect_security_issues(pattern);

        PatternAnalysis {
            complexity_score,
            estimated_performance,
            potential_issues,
            optimization_suggestions,
            security_warnings,
        }
    }

    pub fn calculate_complexity(pattern: &str) -> f64 {
        let mut score = 0.0;

        // Base complexity
        score += pattern.len() as f64 * 0.1;

        // Quantifiers add complexity
        score += pattern.matches('*').count() as f64 * 2.0;
        score += pattern.matches('+').count() as f64 * 2.0;
        score += pattern.matches('?').count() as f64 * 1.0;
        score += pattern.matches("{").count() as f64 * 1.5;

        // Groups add complexity
        score += pattern.matches('(').count() as f64 * 3.0;

        // Character classes
        score += pattern.matches('[').count() as f64 * 2.0;

        // Alternation
        score += pattern.matches('|').count() as f64 * 2.5;

        // Lookarounds are expensive
        score += pattern.matches("(?=").count() as f64 * 10.0;
        score += pattern.matches("(?!").count() as f64 * 10.0;
        score += pattern.matches("(?<=").count() as f64 * 15.0;
        score += pattern.matches("(?<!").count() as f64 * 15.0;

        // Nested quantifiers are very expensive
        if pattern.contains(")*") || pattern.contains(")+") || pattern.contains(")*+") {
            score += 50.0;
        }

        score
    }

    fn estimate_performance(complexity_score: f64) -> PerformanceClass {
        match complexity_score {
            s if s < 5.0 => PerformanceClass::Fast,
            s if s < 20.0 => PerformanceClass::Moderate,
            s if s < 50.0 => PerformanceClass::Slow,
            _ => PerformanceClass::PotentiallyDangerous,
        }
    }

    fn detect_issues(pattern: &str) -> Vec<PatternIssue> {
        let mut issues = Vec::new();

        // Check for nested quantifiers
        if pattern.contains(")*") || pattern.contains(")+") {
            issues.push(PatternIssue::ExcessiveBacktracking {
                problematic_part: "nested quantifiers".to_string(),
                suggestion: "Use possessive quantifiers or atomic groups".to_string(),
            });
        }

        // Check for unbounded repetition
        if pattern.contains(".*") || pattern.contains(".+") {
            issues.push(PatternIssue::UnboundedRepetition {
                quantifier: ".*/.+".to_string(),
                position: pattern
                    .find(".*")
                    .or_else(|| pattern.find(".+"))
                    .unwrap_or(0),
            });
        }

        issues
    }

    fn suggest_optimizations_internal(pattern: &str) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Suggest character classes for alternation
        if pattern.contains("(a|b|c|d|e)") || pattern.matches('|').count() >= 3 {
            suggestions.push(
                "Consider using character classes [abc] instead of alternation (a|b|c)".to_string(),
            );
        }

        // Suggest anchoring
        if !pattern.starts_with('^') && !pattern.ends_with('$') && pattern.len() > 20 {
            suggestions.push(
                "Consider anchoring the pattern with ^ or $ to improve performance".to_string(),
            );
        }

        // Suggest avoiding .* at the beginning
        if pattern.starts_with(".*") {
            suggestions.push("Avoid starting patterns with .* as it can be slow".to_string());
        }

        suggestions
    }

    fn detect_security_issues(pattern: &str) -> Vec<SecurityWarning> {
        let mut warnings = Vec::new();

        // ReDoS patterns
        let redos_patterns = [
            r"\([^)]*\*[^)]*\)\*", // (.*.*)* type patterns
            r"\([^)]*\+[^)]*\)\+", // (.+.+)+ type patterns
            r"\([^)]*\|[^)]*\)\*", // (a|a)* type patterns
        ];

        for redos_pattern in &redos_patterns {
            if let Ok(check_regex) = Regex::new(redos_pattern) {
                if check_regex.is_match(pattern) {
                    warnings.push(SecurityWarning::ReDoSVulnerability {
                        pattern_part: pattern.to_string(),
                        attack_vector: "exponential backtracking".to_string(),
                        mitigation: "Use possessive quantifiers or atomic groups".to_string(),
                    });
                    break;
                }
            }
        }

        // Resource exhaustion patterns
        if pattern.len() > 10000 {
            warnings.push(SecurityWarning::ResourceExhaustion {
                issue_type: "large_pattern".to_string(),
                estimated_impact: "high memory usage".to_string(),
            });
        }

        warnings
    }

    pub fn detect_redos_vulnerability(pattern: &str) -> Option<SecurityWarning> {
        Self::detect_security_issues(pattern)
            .into_iter()
            .find(|w| matches!(w, SecurityWarning::ReDoSVulnerability { .. }))
    }

    pub fn suggest_optimizations(pattern: &str) -> Vec<String> {
        Self::suggest_optimizations_internal(pattern)
    }

    pub fn validate_security(pattern: &str) -> std::result::Result<(), SecurityWarning> {
        let warnings = Self::detect_security_issues(pattern);
        if let Some(warning) = warnings.first() {
            Err(warning.clone())
        } else {
            Ok(())
        }
    }
}

/// Batch processing results
#[derive(Debug)]
pub struct MatchMatrix {
    pub patterns: Vec<String>,
    pub texts: Vec<String>,
    pub matches: Vec<Vec<bool>>, // [pattern_index][text_index]
    pub processing_time: Duration,
}

impl MatchMatrix {
    pub fn get_match(&self, pattern_index: usize, text_index: usize) -> Option<bool> {
        self.matches.get(pattern_index)?.get(text_index).copied()
    }

    pub fn get_matches_for_pattern(&self, pattern_index: usize) -> Option<&[bool]> {
        self.matches.get(pattern_index).map(|v| v.as_slice())
    }

    pub fn get_matches_for_text(&self, text_index: usize) -> Vec<bool> {
        self.matches
            .iter()
            .map(|pattern_matches| pattern_matches.get(text_index).copied().unwrap_or(false))
            .collect()
    }

    pub fn count_total_matches(&self) -> usize {
        self.matches.iter().flatten().filter(|&&m| m).count()
    }
}

/// High-performance batch processor
#[derive(Default)]
pub struct BatchRegexProcessor {
    processor: RegexProcessor,
}

impl BatchRegexProcessor {
    pub fn new() -> Self {
        Self {
            processor: RegexProcessor::new(RegexConfig::default()),
        }
    }

    pub fn process_batch(
        &mut self,
        patterns: &[String],
        texts: &[String],
    ) -> crate::Result<MatchMatrix> {
        let start_time = Instant::now();
        let mut matches = Vec::with_capacity(patterns.len());

        for pattern in patterns {
            let mut pattern_matches = Vec::with_capacity(texts.len());
            let compiled_regex = self.processor.compile(pattern)?;

            for text in texts {
                pattern_matches.push(compiled_regex.is_match(text));
            }
            matches.push(pattern_matches);
        }

        Ok(MatchMatrix {
            patterns: patterns.to_vec(),
            texts: texts.to_vec(),
            matches,
            processing_time: start_time.elapsed(),
        })
    }
}

// Main exports are done via the struct/enum definitions above
