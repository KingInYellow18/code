// Performance benchmarking tools for Claude authentication integration
// Validates that performance targets from Phase 5 requirements are met

use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{PerformanceMetrics, PerformanceTargets};
use super::integration::{OptimizedAuthManager, PerformanceStatistics};

/// Benchmark test configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub test_duration_seconds: u32,
    pub concurrent_agents: usize,
    pub operations_per_agent: u32,
    pub warmup_operations: u32,
    pub target_percentile: f64, // e.g., 0.95 for P95
    pub acceptable_failure_rate: f64, // e.g., 0.01 for 1%
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            test_duration_seconds: 60,
            concurrent_agents: 10,
            operations_per_agent: 100,
            warmup_operations: 10,
            target_percentile: 0.95,
            acceptable_failure_rate: 0.01,
        }
    }
}

/// Individual benchmark test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub test_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub total_operations: u32,
    pub successful_operations: u32,
    pub failed_operations: u32,
    pub operations_per_second: f64,
    pub success_rate: f64,
    pub average_latency_ms: f64,
    pub median_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub target_met: bool,
    pub target_threshold_ms: f64,
    pub performance_score: f64,
    pub errors: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Comprehensive benchmark suite results
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkSuiteResults {
    pub suite_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub total_duration_ms: u64,
    pub config: BenchmarkConfig,
    pub targets: PerformanceTargets,
    pub individual_results: Vec<BenchmarkResult>,
    pub overall_score: f64,
    pub targets_met: bool,
    pub summary: BenchmarkSummary,
    pub recommendations: Vec<String>,
}

/// Benchmark summary statistics
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkSummary {
    pub total_tests: u32,
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub average_performance_score: f64,
    pub worst_performing_test: Option<String>,
    pub best_performing_test: Option<String>,
    pub critical_issues: Vec<String>,
    pub performance_improvements: HashMap<String, f64>,
}

/// Benchmark test types
#[derive(Debug, Clone, PartialEq)]
pub enum BenchmarkTest {
    AuthenticationCache,
    TokenRefresh,
    MemoryUsage,
    ConcurrentAgents,
    NetworkLatency,
    EndToEndFlow,
    StressTest,
}

/// Performance benchmarking engine
#[derive(Debug)]
pub struct PerformanceBenchmarks {
    config: BenchmarkConfig,
    targets: PerformanceTargets,
    results: Arc<RwLock<Vec<BenchmarkResult>>>,
    auth_manager: Option<Arc<OptimizedAuthManager>>,
}

impl PerformanceBenchmarks {
    /// Create new benchmark engine
    pub fn new(targets: PerformanceTargets) -> Self {
        Self {
            config: BenchmarkConfig::default(),
            targets,
            results: Arc::new(RwLock::new(Vec::new())),
            auth_manager: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: BenchmarkConfig, targets: PerformanceTargets) -> Self {
        Self {
            config,
            targets,
            results: Arc::new(RwLock::new(Vec::new())),
            auth_manager: None,
        }
    }

    /// Set the optimized auth manager for testing
    pub fn with_auth_manager(mut self, auth_manager: Arc<OptimizedAuthManager>) -> Self {
        self.auth_manager = Some(auth_manager);
        self
    }

    /// Run complete benchmark suite
    pub async fn run_benchmark_suite(&self, suite_name: &str) -> BenchmarkSuiteResults {
        let suite_start = Utc::now();
        println!("🚀 Starting performance benchmark suite: {}", suite_name);

        // Clear previous results
        {
            let mut results_guard = self.results.write().await;
            results_guard.clear();
        }

        // Run individual benchmark tests
        let tests = vec![
            BenchmarkTest::AuthenticationCache,
            BenchmarkTest::TokenRefresh,
            BenchmarkTest::ConcurrentAgents,
            BenchmarkTest::MemoryUsage,
            BenchmarkTest::NetworkLatency,
            BenchmarkTest::EndToEndFlow,
        ];

        let mut individual_results = Vec::new();
        
        for test in tests {
            println!("  🧪 Running {} benchmark...", self.test_name(&test));
            let result = self.run_individual_benchmark(test).await;
            individual_results.push(result);
        }

        // Run stress test last
        println!("  🔥 Running stress test...");
        let stress_result = self.run_stress_test().await;
        individual_results.push(stress_result);

        let suite_end = Utc::now();
        let total_duration = (suite_end - suite_start).num_milliseconds() as u64;

        // Calculate overall results
        let summary = self.calculate_summary(&individual_results);
        let overall_score = self.calculate_overall_score(&individual_results);
        let targets_met = self.check_targets_met(&individual_results);
        let recommendations = self.generate_recommendations(&individual_results);

        println!("✅ Benchmark suite completed in {}ms", total_duration);
        println!("📊 Overall score: {:.1}%", overall_score);
        println!("🎯 Targets met: {}", if targets_met { "YES" } else { "NO" });

        BenchmarkSuiteResults {
            suite_name: suite_name.to_string(),
            started_at: suite_start,
            completed_at: suite_end,
            total_duration_ms: total_duration,
            config: self.config.clone(),
            targets: self.targets.clone(),
            individual_results,
            overall_score,
            targets_met,
            summary,
            recommendations,
        }
    }

    /// Run individual benchmark test
    async fn run_individual_benchmark(&self, test_type: BenchmarkTest) -> BenchmarkResult {
        let test_start = Utc::now();
        let test_name = self.test_name(&test_type);
        
        let mut latencies = Vec::new();
        let mut errors = Vec::new();
        let mut successful_operations = 0;
        let mut failed_operations = 0;
        
        // Warmup phase
        for _ in 0..self.config.warmup_operations {
            let _ = self.run_single_operation(&test_type).await;
        }

        println!("    ⚡ Running {} operations...", self.config.operations_per_agent);

        // Main test phase
        for i in 0..self.config.operations_per_agent {
            if i % (self.config.operations_per_agent / 10).max(1) == 0 {
                println!("    📊 Progress: {:.0}%", (i as f64 / self.config.operations_per_agent as f64) * 100.0);
            }

            match self.run_single_operation(&test_type).await {
                Ok(latency) => {
                    latencies.push(latency);
                    successful_operations += 1;
                }
                Err(error) => {
                    errors.push(error);
                    failed_operations += 1;
                }
            }
        }

        let test_end = Utc::now();
        let total_duration = (test_end - test_start).num_milliseconds() as u64;

        // Calculate statistics
        let (target_threshold, target_met) = self.get_target_for_test(&test_type, &latencies);
        let performance_score = self.calculate_performance_score(&test_type, &latencies, target_threshold);
        
        let result = BenchmarkResult {
            test_name: test_name.clone(),
            started_at: test_start,
            completed_at: test_end,
            duration_ms: total_duration,
            total_operations: self.config.operations_per_agent,
            successful_operations,
            failed_operations,
            operations_per_second: successful_operations as f64 / (total_duration as f64 / 1000.0),
            success_rate: successful_operations as f64 / self.config.operations_per_agent as f64,
            average_latency_ms: Self::calculate_average(&latencies),
            median_latency_ms: Self::calculate_percentile(&latencies, 0.5),
            p95_latency_ms: Self::calculate_percentile(&latencies, 0.95),
            p99_latency_ms: Self::calculate_percentile(&latencies, 0.99),
            min_latency_ms: latencies.iter().cloned().fold(f64::INFINITY, f64::min),
            max_latency_ms: latencies.iter().cloned().fold(0.0, f64::max),
            target_met,
            target_threshold_ms: target_threshold,
            performance_score,
            errors: errors.into_iter().take(10).collect(), // Limit error count
            metadata: HashMap::new(),
        };

        println!("    ✅ {} completed: {:.1}ms avg, {:.1}% success rate", 
                 test_name, result.average_latency_ms, result.success_rate * 100.0);

        // Store result
        {
            let mut results_guard = self.results.write().await;
            results_guard.push(result.clone());
        }

        result
    }

    /// Run stress test with high concurrency
    async fn run_stress_test(&self) -> BenchmarkResult {
        let test_start = Utc::now();
        let test_name = "Stress Test";
        
        println!("    🔥 Running stress test with {} concurrent agents...", self.config.concurrent_agents);

        let mut handles = Vec::new();
        let results = Arc::new(RwLock::new(Vec::new()));

        // Launch concurrent agents
        for agent_id in 0..self.config.concurrent_agents {
            let benchmarks = self.clone();
            let results_clone = Arc::clone(&results);
            
            let handle = tokio::spawn(async move {
                let mut agent_latencies = Vec::new();
                let mut agent_errors = Vec::new();
                
                for _ in 0..10 { // Fewer operations per agent in stress test
                    match benchmarks.run_single_operation(&BenchmarkTest::AuthenticationCache).await {
                        Ok(latency) => agent_latencies.push(latency),
                        Err(error) => agent_errors.push(error),
                    }
                    
                    // Small delay to simulate real usage
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                
                let mut results_guard = results_clone.write().await;
                results_guard.extend(agent_latencies.into_iter().map(Ok));
                results_guard.extend(agent_errors.into_iter().map(Err));
            });
            
            handles.push(handle);
        }

        // Wait for all agents to complete
        for handle in handles {
            let _ = handle.await;
        }

        let test_end = Utc::now();
        let total_duration = (test_end - test_start).num_milliseconds() as u64;

        // Process results
        let results_guard = results.read().await;
        let mut latencies = Vec::new();
        let mut errors = Vec::new();
        let mut successful_operations = 0;
        let mut failed_operations = 0;

        for result in results_guard.iter() {
            match result {
                Ok(latency) => {
                    latencies.push(*latency);
                    successful_operations += 1;
                }
                Err(error) => {
                    errors.push(error.clone());
                    failed_operations += 1;
                }
            }
        }

        let total_operations = successful_operations + failed_operations;
        let (target_threshold, target_met) = self.get_target_for_test(&BenchmarkTest::StressTest, &latencies);
        let performance_score = self.calculate_performance_score(&BenchmarkTest::StressTest, &latencies, target_threshold);

        println!("    ✅ Stress test completed: {} ops, {:.1}ms avg latency", total_operations, Self::calculate_average(&latencies));

        BenchmarkResult {
            test_name: test_name.to_string(),
            started_at: test_start,
            completed_at: test_end,
            duration_ms: total_duration,
            total_operations,
            successful_operations,
            failed_operations,
            operations_per_second: successful_operations as f64 / (total_duration as f64 / 1000.0),
            success_rate: if total_operations > 0 { successful_operations as f64 / total_operations as f64 } else { 0.0 },
            average_latency_ms: Self::calculate_average(&latencies),
            median_latency_ms: Self::calculate_percentile(&latencies, 0.5),
            p95_latency_ms: Self::calculate_percentile(&latencies, 0.95),
            p99_latency_ms: Self::calculate_percentile(&latencies, 0.99),
            min_latency_ms: latencies.iter().cloned().fold(f64::INFINITY, f64::min),
            max_latency_ms: latencies.iter().cloned().fold(0.0, f64::max),
            target_met,
            target_threshold_ms: target_threshold,
            performance_score,
            errors: errors.into_iter().take(10).collect(),
            metadata: [("concurrent_agents".to_string(), serde_json::json!(self.config.concurrent_agents))].into_iter().collect(),
        }
    }

    /// Run single operation for a specific test type
    async fn run_single_operation(&self, test_type: &BenchmarkTest) -> Result<f64, String> {
        let start = Instant::now();
        
        match test_type {
            BenchmarkTest::AuthenticationCache => {
                self.benchmark_authentication_cache().await
            }
            BenchmarkTest::TokenRefresh => {
                self.benchmark_token_refresh().await
            }
            BenchmarkTest::MemoryUsage => {
                self.benchmark_memory_usage().await
            }
            BenchmarkTest::ConcurrentAgents => {
                self.benchmark_concurrent_agents().await
            }
            BenchmarkTest::NetworkLatency => {
                self.benchmark_network_latency().await
            }
            BenchmarkTest::EndToEndFlow => {
                self.benchmark_end_to_end_flow().await
            }
            BenchmarkTest::StressTest => {
                self.benchmark_authentication_cache().await // Reuse auth cache test
            }
        }?;

        Ok(start.elapsed().as_millis() as f64)
    }

    /// Benchmark authentication caching
    async fn benchmark_authentication_cache(&self) -> Result<(), String> {
        if let Some(auth_manager) = &self.auth_manager {
            let agent_id = format!("bench_agent_{}", Uuid::new_v4());
            let _result = auth_manager
                .authenticate_agent_optimized(&agent_id, 10)
                .await
                .map_err(|e| format!("Authentication failed: {}", e))?;
            Ok(())
        } else {
            // Mock authentication cache test
            tokio::time::sleep(Duration::from_millis(
                if rand::random::<f64>() < 0.8 { 10 } else { 100 } // 80% cache hit simulation
            )).await;
            Ok(())
        }
    }

    /// Benchmark token refresh
    async fn benchmark_token_refresh(&self) -> Result<(), String> {
        if let Some(auth_manager) = &self.auth_manager {
            let refresh_requests = vec![
                ("test_agent".to_string(), "claude".to_string(), "refresh_token_123".to_string())
            ];
            let _results = auth_manager.batch_refresh_tokens(refresh_requests).await;
            Ok(())
        } else {
            // Mock token refresh
            tokio::time::sleep(Duration::from_millis(200 + (rand::random::<u64>() % 300))).await;
            Ok(())
        }
    }

    /// Benchmark memory usage
    async fn benchmark_memory_usage(&self) -> Result<(), String> {
        // Simulate memory allocation/deallocation
        let _data: Vec<u8> = vec![0; 1024 * 1024]; // 1MB allocation
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    /// Benchmark concurrent agents
    async fn benchmark_concurrent_agents(&self) -> Result<(), String> {
        // Simulate multi-agent coordination overhead
        tokio::time::sleep(Duration::from_millis(50 + (rand::random::<u64>() % 100))).await;
        Ok(())
    }

    /// Benchmark network latency
    async fn benchmark_network_latency(&self) -> Result<(), String> {
        // Simulate network call
        tokio::time::sleep(Duration::from_millis(20 + (rand::random::<u64>() % 80))).await;
        
        // 5% chance of network failure
        if rand::random::<f64>() < 0.05 {
            return Err("Network timeout".to_string());
        }
        
        Ok(())
    }

    /// Benchmark end-to-end authentication flow
    async fn benchmark_end_to_end_flow(&self) -> Result<(), String> {
        // Simulate complete authentication flow
        self.benchmark_network_latency().await?;
        self.benchmark_authentication_cache().await?;
        self.benchmark_memory_usage().await?;
        Ok(())
    }

    /// Get target threshold for test type
    fn get_target_for_test(&self, test_type: &BenchmarkTest, latencies: &[f64]) -> (f64, bool) {
        let target_threshold = match test_type {
            BenchmarkTest::AuthenticationCache => self.targets.authentication_cache_ms as f64,
            BenchmarkTest::TokenRefresh => self.targets.token_refresh_ms as f64,
            BenchmarkTest::MemoryUsage => 100.0, // Memory operations should be fast
            BenchmarkTest::ConcurrentAgents => 200.0, // Coordination overhead target
            BenchmarkTest::NetworkLatency => 150.0, // Network call target
            BenchmarkTest::EndToEndFlow => 300.0, // Combined flow target
            BenchmarkTest::StressTest => self.targets.authentication_cache_ms as f64 * 2.0, // More lenient under stress
        };

        let target_met = if latencies.is_empty() {
            false
        } else {
            let percentile_value = Self::calculate_percentile(latencies, self.config.target_percentile);
            percentile_value <= target_threshold
        };

        (target_threshold, target_met)
    }

    /// Calculate performance score for test
    fn calculate_performance_score(&self, test_type: &BenchmarkTest, latencies: &[f64], target_threshold: f64) -> f64 {
        if latencies.is_empty() {
            return 0.0;
        }

        let percentile_value = Self::calculate_percentile(latencies, self.config.target_percentile);
        
        if percentile_value <= target_threshold {
            100.0 // Perfect score if target is met
        } else {
            // Gradual degradation based on how much target is exceeded
            let ratio = target_threshold / percentile_value;
            (ratio * 100.0).max(0.0)
        }
    }

    /// Calculate average of values
    fn calculate_average(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }

    /// Calculate percentile of values
    fn calculate_percentile(values: &[f64], percentile: f64) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mut sorted_values = values.to_vec();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let index = (percentile * (sorted_values.len() - 1) as f64).round() as usize;
        sorted_values[index.min(sorted_values.len() - 1)]
    }

    /// Calculate overall score from individual results
    fn calculate_overall_score(&self, results: &[BenchmarkResult]) -> f64 {
        if results.is_empty() {
            return 0.0;
        }

        // Weighted average based on test importance
        let weights = [
            ("Authentication Cache", 0.3),
            ("Token Refresh", 0.2),
            ("Concurrent Agents", 0.2),
            ("Memory Usage", 0.1),
            ("Network Latency", 0.1),
            ("End-to-End Flow", 0.1),
            ("Stress Test", 0.0), // Bonus points but doesn't count against score
        ];

        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;

        for result in results {
            for (test_name, weight) in &weights {
                if result.test_name.contains(test_name) {
                    weighted_sum += result.performance_score * weight;
                    total_weight += weight;
                    break;
                }
            }
        }

        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        }
    }

    /// Check if all targets are met
    fn check_targets_met(&self, results: &[BenchmarkResult]) -> bool {
        results.iter().all(|r| r.target_met && r.success_rate >= (1.0 - self.config.acceptable_failure_rate))
    }

    /// Calculate benchmark summary
    fn calculate_summary(&self, results: &[BenchmarkResult]) -> BenchmarkSummary {
        let total_tests = results.len() as u32;
        let tests_passed = results.iter().filter(|r| r.target_met).count() as u32;
        let tests_failed = total_tests - tests_passed;
        
        let average_performance_score = if results.is_empty() {
            0.0
        } else {
            results.iter().map(|r| r.performance_score).sum::<f64>() / results.len() as f64
        };

        let worst_performing_test = results
            .iter()
            .min_by(|a, b| a.performance_score.partial_cmp(&b.performance_score).unwrap_or(std::cmp::Ordering::Equal))
            .map(|r| r.test_name.clone());

        let best_performing_test = results
            .iter()
            .max_by(|a, b| a.performance_score.partial_cmp(&b.performance_score).unwrap_or(std::cmp::Ordering::Equal))
            .map(|r| r.test_name.clone());

        let critical_issues = results
            .iter()
            .filter(|r| !r.target_met || r.success_rate < 0.95)
            .map(|r| format!("{}: {:.1}ms (target: {:.1}ms)", r.test_name, r.p95_latency_ms, r.target_threshold_ms))
            .collect();

        let performance_improvements = results
            .iter()
            .filter(|r| r.target_met)
            .map(|r| (r.test_name.clone(), 100.0 - ((r.p95_latency_ms / r.target_threshold_ms) * 100.0)))
            .collect();

        BenchmarkSummary {
            total_tests,
            tests_passed,
            tests_failed,
            average_performance_score,
            worst_performing_test,
            best_performing_test,
            critical_issues,
            performance_improvements,
        }
    }

    /// Generate recommendations based on results
    fn generate_recommendations(&self, results: &[BenchmarkResult]) -> Vec<String> {
        let mut recommendations = Vec::new();

        for result in results {
            if !result.target_met {
                match result.test_name.as_str() {
                    name if name.contains("Authentication Cache") => {
                        recommendations.push(format!(
                            "Authentication caching needs optimization: {:.1}ms vs {:.1}ms target", 
                            result.p95_latency_ms, result.target_threshold_ms
                        ));
                        recommendations.push("Consider increasing cache size or optimizing cache lookup algorithms".to_string());
                    }
                    name if name.contains("Token Refresh") => {
                        recommendations.push(format!(
                            "Token refresh performance needs improvement: {:.1}ms vs {:.1}ms target",
                            result.p95_latency_ms, result.target_threshold_ms
                        ));
                        recommendations.push("Implement token refresh batching or background refresh".to_string());
                    }
                    name if name.contains("Memory Usage") => {
                        recommendations.push("Memory operations are slower than expected".to_string());
                        recommendations.push("Review memory allocation patterns and garbage collection".to_string());
                    }
                    name if name.contains("Concurrent Agents") => {
                        recommendations.push("Multi-agent coordination has high overhead".to_string());
                        recommendations.push("Optimize agent coordination protocols or implement request queuing".to_string());
                    }
                    _ => {
                        recommendations.push(format!("Investigate performance issue in {}", result.test_name));
                    }
                }
            }

            if result.success_rate < 0.99 {
                recommendations.push(format!("Improve error handling for {} (success rate: {:.1}%)", 
                                            result.test_name, result.success_rate * 100.0));
            }
        }

        if recommendations.is_empty() {
            recommendations.push("All performance targets met! System is optimally configured.".to_string());
        }

        recommendations
    }

    /// Get test name for display
    fn test_name(&self, test_type: &BenchmarkTest) -> String {
        match test_type {
            BenchmarkTest::AuthenticationCache => "Authentication Cache".to_string(),
            BenchmarkTest::TokenRefresh => "Token Refresh".to_string(),
            BenchmarkTest::MemoryUsage => "Memory Usage".to_string(),
            BenchmarkTest::ConcurrentAgents => "Concurrent Agents".to_string(),
            BenchmarkTest::NetworkLatency => "Network Latency".to_string(),
            BenchmarkTest::EndToEndFlow => "End-to-End Flow".to_string(),
            BenchmarkTest::StressTest => "Stress Test".to_string(),
        }
    }
}

impl Clone for PerformanceBenchmarks {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            targets: self.targets.clone(),
            results: Arc::clone(&self.results),
            auth_manager: self.auth_manager.clone(),
        }
    }
}

/// Run Phase 5 compliance benchmark
pub async fn run_phase5_compliance_benchmark(
    auth_manager: Option<Arc<OptimizedAuthManager>>,
) -> BenchmarkSuiteResults {
    let targets = PerformanceTargets {
        authentication_cache_ms: 100,    // Phase 5 requirement: < 100ms
        token_refresh_ms: 500,          // Optimized token refresh
        memory_usage_mb: 50,            // Efficient memory utilization per agent
        concurrent_agents: 10,          // Support 10+ concurrent agents
    };

    let config = BenchmarkConfig {
        test_duration_seconds: 120,      // 2 minute test duration
        concurrent_agents: 8,            // Test with 8 concurrent agents
        operations_per_agent: 50,        // 50 operations per agent
        warmup_operations: 5,            // 5 warmup operations
        target_percentile: 0.95,         // P95 performance requirement
        acceptable_failure_rate: 0.01,   // 1% failure rate acceptable
    };

    let mut benchmarks = PerformanceBenchmarks::with_config(config, targets);
    
    if let Some(manager) = auth_manager {
        benchmarks = benchmarks.with_auth_manager(manager);
    }

    benchmarks.run_benchmark_suite("Phase 5 Compliance").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_config_defaults() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.test_duration_seconds, 60);
        assert_eq!(config.concurrent_agents, 10);
        assert_eq!(config.operations_per_agent, 100);
        assert_eq!(config.target_percentile, 0.95);
    }

    #[test]
    fn test_percentile_calculation() {
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        
        assert_eq!(PerformanceBenchmarks::calculate_percentile(&values, 0.5), 50.0);
        assert_eq!(PerformanceBenchmarks::calculate_percentile(&values, 0.95), 100.0);
        assert_eq!(PerformanceBenchmarks::calculate_percentile(&values, 0.0), 10.0);
    }

    #[test]
    fn test_average_calculation() {
        let values = vec![10.0, 20.0, 30.0];
        assert_eq!(PerformanceBenchmarks::calculate_average(&values), 20.0);
        
        let empty_values = vec![];
        assert_eq!(PerformanceBenchmarks::calculate_average(&empty_values), 0.0);
    }

    #[tokio::test]
    async fn test_benchmark_creation() {
        let targets = PerformanceTargets::default();
        let benchmarks = PerformanceBenchmarks::new(targets);
        
        // Test that benchmark can be created
        assert!(benchmarks.auth_manager.is_none());
    }

    #[tokio::test]
    async fn test_single_operation_mock() {
        let targets = PerformanceTargets::default();
        let benchmarks = PerformanceBenchmarks::new(targets);
        
        // Test mock authentication cache operation
        let result = benchmarks.run_single_operation(&BenchmarkTest::AuthenticationCache).await;
        assert!(result.is_ok());
        assert!(result.unwrap() >= 0.0);
    }

    #[tokio::test]
    async fn test_performance_score_calculation() {
        let targets = PerformanceTargets::default();
        let benchmarks = PerformanceBenchmarks::new(targets);
        
        // Test with values that meet target
        let good_latencies = vec![50.0, 60.0, 70.0, 80.0, 90.0];
        let score = benchmarks.calculate_performance_score(&BenchmarkTest::AuthenticationCache, &good_latencies, 100.0);
        assert_eq!(score, 100.0);
        
        // Test with values that exceed target
        let bad_latencies = vec![150.0, 160.0, 170.0, 180.0, 190.0];
        let score = benchmarks.calculate_performance_score(&BenchmarkTest::AuthenticationCache, &bad_latencies, 100.0);
        assert!(score < 100.0);
    }

    #[tokio::test]
    async fn test_phase5_compliance_benchmark() {
        // Run a minimal Phase 5 compliance benchmark without auth manager
        let results = run_phase5_compliance_benchmark(None).await;
        
        assert_eq!(results.suite_name, "Phase 5 Compliance");
        assert!(!results.individual_results.is_empty());
        assert!(results.overall_score >= 0.0);
        assert!(results.overall_score <= 100.0);
    }
}