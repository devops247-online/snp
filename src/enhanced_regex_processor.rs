// Enhanced regex processor with multi-tier caching
use crate::cache::{CacheConfig, MultiTierCache};
use crate::error::SnpError;
use crate::regex_processor::{CompiledRegex, PatternAnalyzer, RegexConfig, RegexError};
use regex::{Regex, RegexBuilder};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Enhanced regex processor with multi-tier caching
pub struct EnhancedRegexProcessor {
    cache: MultiTierCache<String, Arc<Regex>>,
    compilation_cache: MultiTierCache<String, CompiledRegex>,
    config: RegexConfig,
    max_complexity: f64,
    compilation_timeout: Duration,
}

impl EnhancedRegexProcessor {
    /// Create a new enhanced regex processor with multi-tier caching
    pub async fn new(config: RegexConfig) -> crate::Result<Self> {
        let cache_config = CacheConfig {
            l1_max_entries: 500,         // Hot regex patterns
            l2_max_entries: 2000,        // Warm regex patterns
            promotion_threshold: 2,      // Promote after 2 accesses
            enable_l3_persistence: true, // Store in SQLite
            cache_warming: true,         // Enable cache warming
            metrics_enabled: true,       // Enable metrics
        };

        let cache = MultiTierCache::with_config(cache_config.clone()).await?;
        let compilation_cache = MultiTierCache::with_config(cache_config).await?;

        Ok(Self {
            cache,
            compilation_cache,
            config,
            max_complexity: 100.0,
            compilation_timeout: Duration::from_millis(500),
        })
    }

    /// Create with custom cache configuration
    pub async fn with_cache_config(
        regex_config: RegexConfig,
        cache_config: CacheConfig,
    ) -> crate::Result<Self> {
        let cache = MultiTierCache::with_config(cache_config.clone()).await?;
        let compilation_cache = MultiTierCache::with_config(cache_config).await?;

        Ok(Self {
            cache,
            compilation_cache,
            config: regex_config,
            max_complexity: 100.0,
            compilation_timeout: Duration::from_millis(500),
        })
    }

    /// Compile a regex pattern with multi-tier caching
    pub async fn compile(&mut self, pattern: &str) -> crate::Result<Arc<Regex>> {
        // Check multi-tier cache first
        if let Some(cached_regex) = self.cache.get(&pattern.to_string()).await? {
            return Ok(cached_regex);
        }

        // Cache miss - compile new pattern
        let start_time = Instant::now();

        // Analyze pattern complexity and security
        let analysis = PatternAnalyzer::analyze(pattern);

        if analysis.complexity_score > self.max_complexity {
            return Err(SnpError::Regex(Box::new(RegexError::ComplexityExceeded {
                pattern: pattern.to_string(),
                complexity_score: analysis.complexity_score,
                max_allowed: self.max_complexity,
            })));
        }

        // Check for security issues
        if let Some(_warning) = analysis.security_warnings.first() {
            return Err(SnpError::Regex(Box::new(RegexError::SecurityViolation {
                pattern: pattern.to_string(),
                vulnerability_type: "security_warning".to_string(),
                attack_vector: "pattern analysis".to_string(),
            })));
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

        let regex = builder.build().map_err(|e| {
            SnpError::Regex(Box::new(RegexError::InvalidPattern {
                pattern: pattern.to_string(),
                error: e.clone(),
                suggestion: None, // Remove private method call
            }))
        })?;

        let compilation_time = start_time.elapsed();

        if compilation_time > self.compilation_timeout {
            return Err(SnpError::Regex(Box::new(RegexError::CompilationTimeout {
                pattern: pattern.to_string(),
                duration: compilation_time,
                timeout_limit: self.compilation_timeout,
            })));
        }

        let compiled_regex = CompiledRegex {
            pattern: pattern.to_string(),
            regex: Arc::new(regex.clone()),
            config: self.config.clone(),
            complexity_score: analysis.complexity_score,
            compilation_time,
            usage_count: 1,
        };

        let arc_regex = Arc::new(regex);

        // Store in multi-tier cache
        self.cache
            .put(pattern.to_string(), arc_regex.clone())
            .await?;

        // Store compilation metadata
        self.compilation_cache
            .put(pattern.to_string(), compiled_regex)
            .await?;

        Ok(arc_regex)
    }

    /// Check if pattern matches text using multi-tier cached regex
    pub async fn is_match(&mut self, pattern: &str, text: &str) -> crate::Result<bool> {
        let regex = self.compile(pattern).await?;
        Ok(regex.is_match(text))
    }

    /// Find all matches in text using multi-tier cached regex
    pub async fn find_matches<'a>(
        &mut self,
        pattern: &str,
        text: &'a str,
    ) -> crate::Result<Vec<regex::Match<'a>>> {
        let regex = self.compile(pattern).await?;
        Ok(regex.find_iter(text).collect())
    }

    /// Get compilation metadata for a pattern
    pub async fn get_compilation_info(&self, pattern: &str) -> Option<CompiledRegex> {
        self.compilation_cache
            .get(&pattern.to_string())
            .await
            .ok()
            .flatten()
    }

    /// Warm the cache with commonly used patterns
    pub async fn warm_cache(&mut self, patterns: &[String]) -> crate::Result<()> {
        self.cache.warm_cache(patterns).await?;
        self.compilation_cache.warm_cache(patterns).await?;
        Ok(())
    }

    /// Get cache metrics for performance monitoring
    pub async fn get_cache_metrics(
        &self,
    ) -> (crate::cache::CacheMetrics, crate::cache::CacheMetrics) {
        let regex_metrics = self.cache.get_metrics().await;
        let compilation_metrics = self.compilation_cache.get_metrics().await;
        (regex_metrics, compilation_metrics)
    }

    /// Clear all caches
    pub async fn clear_cache(&mut self) -> crate::Result<()> {
        self.cache.clear().await?;
        self.compilation_cache.clear().await?;
        Ok(())
    }

    /// Evict old entries from cache
    pub async fn evict_old_entries(&mut self, max_age: Duration) -> crate::Result<()> {
        self.cache.evict_older_than(max_age).await?;
        self.compilation_cache.evict_older_than(max_age).await?;
        Ok(())
    }

    /// Batch compile multiple patterns efficiently
    pub async fn batch_compile(&mut self, patterns: &[String]) -> crate::Result<Vec<Arc<Regex>>> {
        let mut results = Vec::with_capacity(patterns.len());

        for pattern in patterns {
            let regex = self.compile(pattern).await?;
            results.push(regex);
        }

        Ok(results)
    }

    /// Analyze cache performance and provide optimization suggestions
    pub async fn analyze_cache_performance(&self) -> CachePerformanceReport {
        let (regex_metrics, compilation_metrics) = self.get_cache_metrics().await;

        let mut suggestions = Vec::new();

        // Analyze hit rates
        if regex_metrics.hit_rate() < 0.7 {
            suggestions
                .push("Consider increasing L1 or L2 cache sizes for better hit rates".to_string());
        }

        if regex_metrics.l1_hit_rate() < 0.3 {
            suggestions.push(
                "L1 cache hit rate is low - consider adjusting promotion threshold".to_string(),
            );
        }

        // Analyze memory usage
        if regex_metrics.memory_usage_bytes > 100 * 1024 * 1024 {
            // 100MB
            suggestions
                .push("High memory usage detected - consider enabling cache eviction".to_string());
        }

        // Analyze promotion patterns
        if regex_metrics.l2_to_l1_promotions > regex_metrics.l1_hits / 2 {
            suggestions
                .push("High promotion rate - consider lowering promotion threshold".to_string());
        }

        CachePerformanceReport {
            regex_cache_metrics: regex_metrics.clone(),
            compilation_cache_metrics: compilation_metrics.clone(),
            overall_hit_rate: (regex_metrics.total_hits + compilation_metrics.total_hits) as f64
                / (regex_metrics.total_hits
                    + regex_metrics.total_misses
                    + compilation_metrics.total_hits
                    + compilation_metrics.total_misses) as f64,
            optimization_suggestions: suggestions,
        }
    }
}

/// Cache performance analysis report
#[derive(Debug)]
pub struct CachePerformanceReport {
    pub regex_cache_metrics: crate::cache::CacheMetrics,
    pub compilation_cache_metrics: crate::cache::CacheMetrics,
    pub overall_hit_rate: f64,
    pub optimization_suggestions: Vec<String>,
}

impl CachePerformanceReport {
    pub fn print_summary(&self) {
        println!("=== Multi-Tier Cache Performance Report ===");
        println!("Overall Hit Rate: {:.2}%", self.overall_hit_rate * 100.0);
        println!();

        println!("Regex Cache:");
        println!(
            "  L1 Hits: {} ({:.1}%)",
            self.regex_cache_metrics.l1_hits,
            self.regex_cache_metrics.l1_hit_rate() * 100.0
        );
        println!(
            "  L2 Hits: {} ({:.1}%)",
            self.regex_cache_metrics.l2_hits,
            self.regex_cache_metrics.l2_hit_rate() * 100.0
        );
        println!(
            "  L3 Hits: {} ({:.1}%)",
            self.regex_cache_metrics.l3_hits,
            self.regex_cache_metrics.l3_hit_rate() * 100.0
        );
        println!(
            "  Memory Usage: {} KB",
            self.regex_cache_metrics.memory_usage_bytes / 1024
        );
        println!();

        if !self.optimization_suggestions.is_empty() {
            println!("Optimization Suggestions:");
            for suggestion in &self.optimization_suggestions {
                println!("  • {suggestion}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enhanced_regex_processor_creation() {
        let processor = EnhancedRegexProcessor::new(RegexConfig::default()).await;
        assert!(processor.is_ok());
    }

    #[tokio::test]
    async fn test_pattern_compilation_and_caching() {
        let mut processor = EnhancedRegexProcessor::new(RegexConfig::default())
            .await
            .unwrap();

        // First compilation - should be a cache miss
        let regex1 = processor.compile(r"\d+").await.unwrap();

        // Second compilation - should be a cache hit
        let regex2 = processor.compile(r"\d+").await.unwrap();

        // Should be the same Arc (cached)
        assert!(Arc::ptr_eq(&regex1, &regex2));

        let (regex_metrics, _) = processor.get_cache_metrics().await;
        assert!(regex_metrics.total_hits > 0);
    }

    #[tokio::test]
    async fn test_is_match_with_caching() {
        let mut processor = EnhancedRegexProcessor::new(RegexConfig::default())
            .await
            .unwrap();

        let result1 = processor.is_match(r"\d+", "123").await.unwrap();
        let result2 = processor.is_match(r"\d+", "abc").await.unwrap();

        assert!(result1);
        assert!(!result2);

        // Should have cached the regex
        let (regex_metrics, _) = processor.get_cache_metrics().await;
        assert!(regex_metrics.total_hits > 0);
    }

    #[tokio::test]
    async fn test_batch_compilation() {
        let mut processor = EnhancedRegexProcessor::new(RegexConfig::default())
            .await
            .unwrap();

        let patterns = vec![
            r"\d+".to_string(),
            r"[a-zA-Z]+".to_string(),
            r"\w+@\w+\.\w+".to_string(),
        ];

        let results = processor.batch_compile(&patterns).await.unwrap();
        assert_eq!(results.len(), 3);

        // All patterns should be compiled successfully
        for regex in results {
            assert!(regex.is_match("test123"));
        }
    }

    #[tokio::test]
    async fn test_cache_performance_analysis() {
        let mut processor = EnhancedRegexProcessor::new(RegexConfig::default())
            .await
            .unwrap();

        // Use the processor to generate some metrics
        processor.is_match(r"\d+", "123").await.unwrap();
        processor.is_match(r"[a-z]+", "abc").await.unwrap();
        processor.is_match(r"\d+", "456").await.unwrap(); // Cache hit

        let report = processor.analyze_cache_performance().await;
        assert!(report.overall_hit_rate >= 0.0);
        assert!(report.overall_hit_rate <= 1.0);
    }
}
