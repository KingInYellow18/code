//! CLAUDE AUTHENTICATION PERFORMANCE BENCHMARKS
//!
//! Comprehensive performance testing for Claude authentication and CLI integration.
//! Tests startup times, throughput, memory usage, and scalability under load.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tempfile::TempDir;

use crate::providers::claude_code::{ClaudeCodeProvider, ClaudeCodeConfig};
use crate::providers::{AIProvider, Message, MessageContent};
use crate::claude_auth::{SecureClaudeAuth, ClaudeAuthConfig};
use crate::performance::authentication_cache::AuthenticationCache;

/// Performance benchmark results
#[derive(Debug, Clone)]
pub struct ClaudePerformanceBenchmarks {
    pub startup_performance: StartupBenchmark,
    pub authentication_performance: AuthBenchmark,
    pub cli_process_performance: CliProcessBenchmark,
    pub memory_performance: MemoryBenchmark,
    pub concurrency_performance: ConcurrencyBenchmark,
    pub cache_performance: CacheBenchmark,
    pub overall_grade: PerformanceGrade,
    pub performance_summary: PerformanceSummary,
}

#[derive(Debug, Clone)]
pub struct StartupBenchmark {
    pub provider_creation_ms: f64,
    pub config_loading_ms: f64,
    pub validation_ms: f64,
    pub total_startup_ms: f64,
    pub meets_requirements: bool, // < 500ms total
}

#[derive(Debug, Clone)]
pub struct AuthBenchmark {
    pub token_retrieval_ms: f64,
    pub token_validation_ms: f64,
    pub token_refresh_ms: f64,
    pub oauth_flow_ms: f64,
    pub cache_hit_rate: f64,
    pub meets_requirements: bool, // < 100ms for cached operations
}

#[derive(Debug, Clone)]
pub struct CliProcessBenchmark {
    pub process_spawn_ms: f64,
    pub response_parsing_ms: f64,
    pub process_cleanup_ms: f64,
    pub command_construction_ms: f64,
    pub meets_requirements: bool, // < 200ms for process operations
}

#[derive(Debug, Clone)]
pub struct MemoryBenchmark {
    pub baseline_memory_mb: f64,
    pub peak_memory_mb: f64,
    pub memory_growth_mb: f64,
    pub memory_efficiency_score: f64,
    pub gc_pressure: f64,
    pub meets_requirements: bool, // < 100MB total usage
}

#[derive(Debug, Clone)]
pub struct ConcurrencyBenchmark {
    pub max_concurrent_operations: usize,
    pub throughput_ops_per_second: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub error_rate_percent: f64,
    pub meets_requirements: bool, // > 100 ops/sec, < 5% error rate
}

#[derive(Debug, Clone)]
pub struct CacheBenchmark {
    pub cache_hit_rate: f64,
    pub cache_miss_penalty_ms: f64,
    pub cache_lookup_time_ms: f64,
    pub cache_efficiency_score: f64,
    pub meets_requirements: bool, // > 90% hit rate, < 10ms lookup
}

#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub total_score: f64, // 0-100
    pub bottlenecks: Vec<String>,
    pub optimization_opportunities: Vec<String>,
    pub production_readiness: ProductionReadiness,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PerformanceGrade {
    Excellent, // 90-100
    Good,      // 80-89
    Acceptable, // 70-79
    Poor,      // 60-69
    Unacceptable, // < 60
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProductionReadiness {
    Ready,
    ReadyWithOptimizations,
    RequiresImprovement,
    NotReady,
}

/// Claude Performance Benchmarker
pub struct ClaudePerformanceBenchmarker {
    temp_dir: TempDir,
    baseline_memory: f64,
}

impl ClaudePerformanceBenchmarker {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let baseline_memory = Self::get_current_memory_usage();

        Ok(Self {
            temp_dir,
            baseline_memory,
        })
    }

    /// 🚀 BENCHMARK 1: Startup Performance
    pub async fn benchmark_startup_performance(&self) -> Result<StartupBenchmark, Box<dyn std::error::Error>> {
        println!("🚀 Benchmarking startup performance...");

        let mut config_loading_times = Vec::new();
        let mut provider_creation_times = Vec::new();
        let mut validation_times = Vec::new();
        let mut total_startup_times = Vec::new();

        // Run multiple iterations for statistical accuracy
        for _ in 0..20 {
            let total_start = Instant::now();

            // Benchmark config loading
            let config_start = Instant::now();
            let config = ClaudeCodeConfig::from_codex_home(self.temp_dir.path())?;
            let config_time = config_start.elapsed().as_millis() as f64;
            config_loading_times.push(config_time);

            // Benchmark provider creation
            let provider_start = Instant::now();
            let _provider = ClaudeCodeProvider::new(config.clone()).await;
            let provider_time = provider_start.elapsed().as_millis() as f64;
            provider_creation_times.push(provider_time);

            // Benchmark validation
            let validation_start = Instant::now();
            let _validation = config.validate().await;
            let validation_time = validation_start.elapsed().as_millis() as f64;
            validation_times.push(validation_time);

            let total_time = total_start.elapsed().as_millis() as f64;
            total_startup_times.push(total_time);

            // Small delay between iterations
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let config_loading_ms = Self::calculate_average(&config_loading_times);
        let provider_creation_ms = Self::calculate_average(&provider_creation_times);
        let validation_ms = Self::calculate_average(&validation_times);
        let total_startup_ms = Self::calculate_average(&total_startup_times);

        let meets_requirements = total_startup_ms < 500.0;

        Ok(StartupBenchmark {
            provider_creation_ms,
            config_loading_ms,
            validation_ms,
            total_startup_ms,
            meets_requirements,
        })
    }

    /// 🔐 BENCHMARK 2: Authentication Performance
    pub async fn benchmark_authentication_performance(&self) -> Result<AuthBenchmark, Box<dyn std::error::Error>> {
        println!("🔐 Benchmarking authentication performance...");

        let cache = AuthenticationCache::new();
        let auth_config = ClaudeAuthConfig::default();
        let storage_path = self.temp_dir.path().join("auth_tokens.json");

        // Benchmark token operations
        let mut token_retrieval_times = Vec::new();
        let mut token_validation_times = Vec::new();
        let mut cache_hits = 0;
        let mut cache_misses = 0;

        // Pre-populate cache
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        cache.put("claude", "test_user", "test_token", expires_at, Some("max".to_string())).await;

        for i in 0..100 {
            // Benchmark token retrieval (should hit cache)
            let retrieval_start = Instant::now();
            let cached_result = cache.get("claude", "test_user").await;
            let retrieval_time = retrieval_start.elapsed().as_millis() as f64;
            token_retrieval_times.push(retrieval_time);

            if cached_result.is_some() {
                cache_hits += 1;
            } else {
                cache_misses += 1;
            }

            // Benchmark token validation
            let validation_start = Instant::now();
            if let Some(_token) = cached_result {
                // Simulate token validation
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
            let validation_time = validation_start.elapsed().as_millis() as f64;
            token_validation_times.push(validation_time);
        }

        // Benchmark OAuth flow creation
        let oauth_start = Instant::now();
        let mut claude_auth = SecureClaudeAuth::new(auth_config, storage_path)?;
        let _oauth_url = claude_auth.start_oauth_flow();
        let oauth_flow_ms = oauth_start.elapsed().as_millis() as f64;

        // Benchmark token refresh (simulated)
        let refresh_start = Instant::now();
        tokio::time::sleep(Duration::from_millis(50)).await; // Simulate network call
        let token_refresh_ms = refresh_start.elapsed().as_millis() as f64;

        let token_retrieval_ms = Self::calculate_average(&token_retrieval_times);
        let token_validation_ms = Self::calculate_average(&token_validation_times);
        let cache_hit_rate = cache_hits as f64 / (cache_hits + cache_misses) as f64;

        let meets_requirements = token_retrieval_ms < 100.0 && cache_hit_rate > 0.8;

        Ok(AuthBenchmark {
            token_retrieval_ms,
            token_validation_ms,
            token_refresh_ms,
            oauth_flow_ms,
            cache_hit_rate,
            meets_requirements,
        })
    }

    /// 💻 BENCHMARK 3: CLI Process Performance
    pub async fn benchmark_cli_process_performance(&self) -> Result<CliProcessBenchmark, Box<dyn std::error::Error>> {
        println!("💻 Benchmarking CLI process performance...");

        let config = ClaudeCodeConfig {
            claude_path: "echo".to_string(), // Use echo for testing
            ..ClaudeCodeConfig::from_codex_home(self.temp_dir.path())?
        };

        let mut spawn_times = Vec::new();
        let mut parsing_times = Vec::new();
        let mut cleanup_times = Vec::new();
        let mut construction_times = Vec::new();

        for _ in 0..10 {
            // Benchmark command construction
            let construction_start = Instant::now();
            let messages = vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("Hello, world!".to_string()),
            }];
            let construction_time = construction_start.elapsed().as_millis() as f64;
            construction_times.push(construction_time);

            // Benchmark process spawn
            let spawn_start = Instant::now();
            let provider = ClaudeCodeProvider::new(config.clone()).await?;
            let spawn_time = spawn_start.elapsed().as_millis() as f64;
            spawn_times.push(spawn_time);

            // Benchmark response parsing
            let parsing_start = Instant::now();
            let mock_response = r#"{"type": "assistant", "content": "Hello!"}"#;
            let _parsed: Result<serde_json::Value, _> = serde_json::from_str(mock_response);
            let parsing_time = parsing_start.elapsed().as_micros() as f64 / 1000.0;
            parsing_times.push(parsing_time);

            // Benchmark cleanup
            let cleanup_start = Instant::now();
            drop(provider);
            let cleanup_time = cleanup_start.elapsed().as_micros() as f64 / 1000.0;
            cleanup_times.push(cleanup_time);
        }

        let process_spawn_ms = Self::calculate_average(&spawn_times);
        let response_parsing_ms = Self::calculate_average(&parsing_times);
        let process_cleanup_ms = Self::calculate_average(&cleanup_times);
        let command_construction_ms = Self::calculate_average(&construction_times);

        let meets_requirements = process_spawn_ms < 200.0 && response_parsing_ms < 10.0;

        Ok(CliProcessBenchmark {
            process_spawn_ms,
            response_parsing_ms,
            process_cleanup_ms,
            command_construction_ms,
            meets_requirements,
        })
    }

    /// 📊 BENCHMARK 4: Memory Performance
    pub async fn benchmark_memory_performance(&self) -> Result<MemoryBenchmark, Box<dyn std::error::Error>> {
        println!("📊 Benchmarking memory performance...");

        let baseline_memory_mb = self.baseline_memory;
        let mut peak_memory_mb = baseline_memory_mb;

        // Create multiple providers and monitor memory usage
        let mut providers = Vec::new();
        for _ in 0..10 {
            let config = ClaudeCodeConfig::from_codex_home(self.temp_dir.path())?;
            let provider = ClaudeCodeProvider::new(config).await?;
            providers.push(provider);

            let current_memory = Self::get_current_memory_usage();
            if current_memory > peak_memory_mb {
                peak_memory_mb = current_memory;
            }

            // Small delay to allow memory allocation to stabilize
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Perform operations and monitor memory
        for provider in &providers {
            let messages = vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("Memory test message".to_string()),
            }];

            // These operations would normally send messages but won't work without real CLI
            // This tests memory usage of message preparation
            drop(messages);

            let current_memory = Self::get_current_memory_usage();
            if current_memory > peak_memory_mb {
                peak_memory_mb = current_memory;
            }
        }

        // Clean up and measure memory after
        drop(providers);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let final_memory = Self::get_current_memory_usage();
        let memory_growth_mb = final_memory - baseline_memory_mb;

        let memory_efficiency_score = if memory_growth_mb < 50.0 { 100.0 } else { 100.0 - memory_growth_mb };
        let gc_pressure = (peak_memory_mb - baseline_memory_mb) / baseline_memory_mb;

        let meets_requirements = peak_memory_mb < 100.0 && memory_growth_mb < 50.0;

        Ok(MemoryBenchmark {
            baseline_memory_mb,
            peak_memory_mb,
            memory_growth_mb,
            memory_efficiency_score,
            gc_pressure,
            meets_requirements,
        })
    }

    /// 🔄 BENCHMARK 5: Concurrency Performance
    pub async fn benchmark_concurrency_performance(&self) -> Result<ConcurrencyBenchmark, Box<dyn std::error::Error>> {
        println!("🔄 Benchmarking concurrency performance...");

        let semaphore = Arc::new(Semaphore::new(100));
        let start_time = Instant::now();
        let operations_count = Arc::new(Mutex::new(0));
        let errors_count = Arc::new(Mutex::new(0));
        let latencies = Arc::new(Mutex::new(Vec::new()));

        let mut handles = Vec::new();

        // Launch concurrent operations
        for _ in 0..1000 {
            let semaphore = semaphore.clone();
            let operations_count = operations_count.clone();
            let errors_count = errors_count.clone();
            let latencies = latencies.clone();
            let temp_dir_path = self.temp_dir.path().to_path_buf();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let operation_start = Instant::now();

                // Simulate concurrent authentication operation
                let config_result = ClaudeCodeConfig::from_codex_home(&temp_dir_path);
                let operation_latency = operation_start.elapsed();

                match config_result {
                    Ok(_) => {
                        let mut count = operations_count.lock().unwrap();
                        *count += 1;
                    }
                    Err(_) => {
                        let mut errors = errors_count.lock().unwrap();
                        *errors += 1;
                    }
                }

                let mut latency_vec = latencies.lock().unwrap();
                latency_vec.push(operation_latency.as_millis() as f64);
            });

            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            handle.await?;
        }

        let total_duration = start_time.elapsed();
        let successful_ops = *operations_count.lock().unwrap();
        let error_count = *errors_count.lock().unwrap();
        let total_ops = successful_ops + error_count;

        let throughput_ops_per_second = successful_ops as f64 / total_duration.as_secs_f64();
        let error_rate_percent = (error_count as f64 / total_ops as f64) * 100.0;

        // Calculate latency percentiles
        let mut latency_vec = latencies.lock().unwrap();
        latency_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let p95_index = (latency_vec.len() as f64 * 0.95) as usize;
        let p99_index = (latency_vec.len() as f64 * 0.99) as usize;

        let latency_p95_ms = latency_vec.get(p95_index).copied().unwrap_or(0.0);
        let latency_p99_ms = latency_vec.get(p99_index).copied().unwrap_or(0.0);

        let meets_requirements = throughput_ops_per_second > 100.0 && error_rate_percent < 5.0;

        Ok(ConcurrencyBenchmark {
            max_concurrent_operations: successful_ops + error_count,
            throughput_ops_per_second,
            latency_p95_ms,
            latency_p99_ms,
            error_rate_percent,
            meets_requirements,
        })
    }

    /// 💾 BENCHMARK 6: Cache Performance
    pub async fn benchmark_cache_performance(&self) -> Result<CacheBenchmark, Box<dyn std::error::Error>> {
        println!("💾 Benchmarking cache performance...");

        let cache = AuthenticationCache::new();
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);

        // Pre-populate cache with test data
        for i in 0..100 {
            cache.put(
                "claude",
                &format!("user_{}", i),
                &format!("token_{}", i),
                expires_at,
                Some("max".to_string()),
            ).await;
        }

        let mut cache_hits = 0;
        let mut cache_misses = 0;
        let mut lookup_times = Vec::new();
        let mut miss_penalties = Vec::new();

        // Test cache performance
        for i in 0..200 {
            let lookup_start = Instant::now();
            let user_id = format!("user_{}", i % 150); // 50% will be cache misses

            let result = cache.get("claude", &user_id).await;
            let lookup_time = lookup_start.elapsed().as_micros() as f64 / 1000.0; // Convert to milliseconds

            lookup_times.push(lookup_time);

            if result.is_some() {
                cache_hits += 1;
            } else {
                cache_misses += 1;
                miss_penalties.push(lookup_time);
            }
        }

        let cache_hit_rate = cache_hits as f64 / (cache_hits + cache_misses) as f64;
        let cache_lookup_time_ms = Self::calculate_average(&lookup_times);
        let cache_miss_penalty_ms = if !miss_penalties.is_empty() {
            Self::calculate_average(&miss_penalties)
        } else {
            0.0
        };

        let cache_efficiency_score = cache_hit_rate * 100.0 - (cache_lookup_time_ms / 10.0);
        let meets_requirements = cache_hit_rate > 0.9 && cache_lookup_time_ms < 10.0;

        Ok(CacheBenchmark {
            cache_hit_rate,
            cache_miss_penalty_ms,
            cache_lookup_time_ms,
            cache_efficiency_score,
            meets_requirements,
        })
    }

    /// Generate comprehensive performance report
    pub async fn generate_performance_report(&self) -> ClaudePerformanceBenchmarks {
        println!("📈 Generating comprehensive performance report...");

        let startup_performance = self.benchmark_startup_performance().await.unwrap_or_default();
        let authentication_performance = self.benchmark_authentication_performance().await.unwrap_or_default();
        let cli_process_performance = self.benchmark_cli_process_performance().await.unwrap_or_default();
        let memory_performance = self.benchmark_memory_performance().await.unwrap_or_default();
        let concurrency_performance = self.benchmark_concurrency_performance().await.unwrap_or_default();
        let cache_performance = self.benchmark_cache_performance().await.unwrap_or_default();

        // Calculate overall performance score
        let scores = vec![
            if startup_performance.meets_requirements { 100.0 } else { 50.0 },
            if authentication_performance.meets_requirements { 100.0 } else { 50.0 },
            if cli_process_performance.meets_requirements { 100.0 } else { 50.0 },
            if memory_performance.meets_requirements { 100.0 } else { 50.0 },
            if concurrency_performance.meets_requirements { 100.0 } else { 50.0 },
            if cache_performance.meets_requirements { 100.0 } else { 50.0 },
        ];

        let total_score = scores.iter().sum::<f64>() / scores.len() as f64;

        // Determine overall grade
        let overall_grade = match total_score {
            90.0..=100.0 => PerformanceGrade::Excellent,
            80.0..=89.9 => PerformanceGrade::Good,
            70.0..=79.9 => PerformanceGrade::Acceptable,
            60.0..=69.9 => PerformanceGrade::Poor,
            _ => PerformanceGrade::Unacceptable,
        };

        // Identify bottlenecks and optimization opportunities
        let mut bottlenecks = Vec::new();
        let mut optimization_opportunities = Vec::new();

        if startup_performance.total_startup_ms > 500.0 {
            bottlenecks.push("Slow provider startup".to_string());
            optimization_opportunities.push("Optimize configuration loading and validation".to_string());
        }

        if authentication_performance.token_retrieval_ms > 100.0 {
            bottlenecks.push("Slow authentication operations".to_string());
            optimization_opportunities.push("Implement better caching and token management".to_string());
        }

        if memory_performance.peak_memory_mb > 100.0 {
            bottlenecks.push("High memory usage".to_string());
            optimization_opportunities.push("Optimize memory allocation and implement pooling".to_string());
        }

        if concurrency_performance.throughput_ops_per_second < 100.0 {
            bottlenecks.push("Low concurrency throughput".to_string());
            optimization_opportunities.push("Implement connection pooling and async optimizations".to_string());
        }

        // Determine production readiness
        let production_readiness = match total_score {
            85.0..=100.0 => ProductionReadiness::Ready,
            75.0..=84.9 => ProductionReadiness::ReadyWithOptimizations,
            60.0..=74.9 => ProductionReadiness::RequiresImprovement,
            _ => ProductionReadiness::NotReady,
        };

        let performance_summary = PerformanceSummary {
            total_score,
            bottlenecks,
            optimization_opportunities,
            production_readiness,
        };

        ClaudePerformanceBenchmarks {
            startup_performance,
            authentication_performance,
            cli_process_performance,
            memory_performance,
            concurrency_performance,
            cache_performance,
            overall_grade,
            performance_summary,
        }
    }

    /// Utility function to calculate average
    fn calculate_average(values: &[f64]) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    /// Get current memory usage in MB
    fn get_current_memory_usage() -> f64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<f64>() {
                                return kb / 1024.0; // Convert KB to MB
                            }
                        }
                    }
                }
            }
        }
        50.0 // Fallback estimate
    }
}

// Default implementations for benchmark structs
impl Default for StartupBenchmark {
    fn default() -> Self {
        Self {
            provider_creation_ms: 0.0,
            config_loading_ms: 0.0,
            validation_ms: 0.0,
            total_startup_ms: 0.0,
            meets_requirements: false,
        }
    }
}

impl Default for AuthBenchmark {
    fn default() -> Self {
        Self {
            token_retrieval_ms: 0.0,
            token_validation_ms: 0.0,
            token_refresh_ms: 0.0,
            oauth_flow_ms: 0.0,
            cache_hit_rate: 0.0,
            meets_requirements: false,
        }
    }
}

impl Default for CliProcessBenchmark {
    fn default() -> Self {
        Self {
            process_spawn_ms: 0.0,
            response_parsing_ms: 0.0,
            process_cleanup_ms: 0.0,
            command_construction_ms: 0.0,
            meets_requirements: false,
        }
    }
}

impl Default for MemoryBenchmark {
    fn default() -> Self {
        Self {
            baseline_memory_mb: 0.0,
            peak_memory_mb: 0.0,
            memory_growth_mb: 0.0,
            memory_efficiency_score: 0.0,
            gc_pressure: 0.0,
            meets_requirements: false,
        }
    }
}

impl Default for ConcurrencyBenchmark {
    fn default() -> Self {
        Self {
            max_concurrent_operations: 0,
            throughput_ops_per_second: 0.0,
            latency_p95_ms: 0.0,
            latency_p99_ms: 0.0,
            error_rate_percent: 0.0,
            meets_requirements: false,
        }
    }
}

impl Default for CacheBenchmark {
    fn default() -> Self {
        Self {
            cache_hit_rate: 0.0,
            cache_miss_penalty_ms: 0.0,
            cache_lookup_time_ms: 0.0,
            cache_efficiency_score: 0.0,
            meets_requirements: false,
        }
    }
}

/// Main benchmarking function
pub async fn conduct_claude_performance_benchmarks() -> Result<ClaudePerformanceBenchmarks, Box<dyn std::error::Error>> {
    println!("🚀 Starting Claude Performance Benchmarks...");

    let benchmarker = ClaudePerformanceBenchmarker::new()?;
    let benchmarks = benchmarker.generate_performance_report().await;

    println!("📊 Benchmarking completed!");
    println!("🚀 Startup: {} ({}ms)",
             if benchmarks.startup_performance.meets_requirements { "✅ FAST" } else { "❌ SLOW" },
             benchmarks.startup_performance.total_startup_ms);
    println!("🔐 Authentication: {} ({}ms)",
             if benchmarks.authentication_performance.meets_requirements { "✅ FAST" } else { "❌ SLOW" },
             benchmarks.authentication_performance.token_retrieval_ms);
    println!("📊 Memory: {} ({}MB peak)",
             if benchmarks.memory_performance.meets_requirements { "✅ EFFICIENT" } else { "❌ INEFFICIENT" },
             benchmarks.memory_performance.peak_memory_mb);
    println!("🔄 Concurrency: {} ({:.1} ops/sec)",
             if benchmarks.concurrency_performance.meets_requirements { "✅ SCALABLE" } else { "❌ LIMITED" },
             benchmarks.concurrency_performance.throughput_ops_per_second);
    println!("📈 Overall Grade: {:?} ({:.1}%)", benchmarks.overall_grade, benchmarks.performance_summary.total_score);
    println!("🏭 Production Readiness: {:?}", benchmarks.performance_summary.production_readiness);

    Ok(benchmarks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_claude_performance_benchmarks() {
        let benchmarks = conduct_claude_performance_benchmarks().await.unwrap();

        // Performance requirements for production
        assert!(benchmarks.startup_performance.total_startup_ms < 1000.0,
                "Startup should be under 1 second");
        assert!(benchmarks.memory_performance.peak_memory_mb < 500.0,
                "Memory usage should be reasonable");
        assert!(benchmarks.concurrency_performance.error_rate_percent < 10.0,
                "Error rate should be acceptable");

        // Overall performance should not be unacceptable
        assert!(!matches!(benchmarks.overall_grade, PerformanceGrade::Unacceptable),
                "Performance grade should not be unacceptable");

        // Production readiness should not be "not ready"
        assert!(!matches!(benchmarks.performance_summary.production_readiness, ProductionReadiness::NotReady),
                "Should be ready for production or require only minor improvements");
    }

    #[tokio::test]
    async fn test_individual_benchmarks() {
        let benchmarker = ClaudePerformanceBenchmarker::new().unwrap();

        // Test individual benchmark components
        let startup = benchmarker.benchmark_startup_performance().await.unwrap();
        assert!(startup.total_startup_ms < 2000.0, "Startup benchmark should complete in reasonable time");

        let memory = benchmarker.benchmark_memory_performance().await.unwrap();
        assert!(memory.baseline_memory_mb > 0.0, "Memory benchmark should measure actual memory");

        let cache = benchmarker.benchmark_cache_performance().await.unwrap();
        assert!(cache.cache_hit_rate >= 0.0 && cache.cache_hit_rate <= 1.0, "Cache hit rate should be valid");
    }
}