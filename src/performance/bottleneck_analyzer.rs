// Performance bottleneck analyzer for Claude authentication integration
// Identifies and analyzes performance issues in authentication workflows

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

use super::PerformanceMetrics;

/// Types of performance bottlenecks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BottleneckType {
    /// Authentication operations taking too long
    SlowAuthentication,
    /// Token refresh operations are inefficient
    SlowTokenRefresh,
    /// Network connectivity issues
    NetworkLatency,
    /// Memory usage is excessive
    MemoryPressure,
    /// Too many concurrent operations
    ConcurrencyOverload,
    /// Cache performance is poor
    CacheInefficiency,
    /// Sequential operations that could be parallel
    SequentialBottleneck,
    /// Agent coordination overhead
    CoordinationOverhead,
    /// Database/storage performance issues
    StorageLatency,
}

/// Severity levels for bottlenecks
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Low,      // Minor impact, optimization recommended
    Medium,   // Moderate impact, should be addressed
    High,     // Significant impact, needs attention
    Critical, // Severe impact, immediate action required
}

/// Detected performance bottleneck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    pub id: String,
    pub bottleneck_type: BottleneckType,
    pub severity: Severity,
    pub description: String,
    pub impact_score: f64,        // 0-100 scale
    pub frequency: u32,           // How often this bottleneck occurs
    pub average_delay_ms: f64,    // Average delay caused
    pub affected_operations: Vec<String>,
    pub first_detected: DateTime<Utc>,
    pub last_detected: DateTime<Utc>,
    pub recommendations: Vec<String>,
    pub metrics_evidence: serde_json::Value,
}

/// Bottleneck analysis configuration
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub analysis_window_minutes: u32,
    pub min_samples_for_analysis: u32,
    pub slow_auth_threshold_ms: u128,
    pub slow_refresh_threshold_ms: u128,
    pub high_memory_threshold_mb: u64,
    pub low_cache_hit_threshold: f64,
    pub high_concurrency_threshold: usize,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            analysis_window_minutes: 15,    // Analyze last 15 minutes
            min_samples_for_analysis: 10,   // Need at least 10 samples
            slow_auth_threshold_ms: 100,    // > 100ms auth is slow
            slow_refresh_threshold_ms: 500, // > 500ms refresh is slow
            high_memory_threshold_mb: 100,  // > 100MB is high memory
            low_cache_hit_threshold: 0.7,   // < 70% cache hit is low
            high_concurrency_threshold: 8,  // > 8 concurrent is high
        }
    }
}

/// Historical performance data point
#[derive(Debug, Clone)]
struct PerformanceDataPoint {
    timestamp: DateTime<Utc>,
    metrics: PerformanceMetrics,
    context: OperationContext,
}

/// Context about the operation being performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationContext {
    pub operation_type: String,
    pub agent_count: usize,
    pub provider: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
}

/// Analysis results and trends
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisReport {
    pub analysis_period_minutes: u32,
    pub total_samples: usize,
    pub bottlenecks_detected: Vec<Bottleneck>,
    pub overall_health_score: f64,
    pub trend_analysis: TrendAnalysis,
    pub optimization_priority: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

/// Performance trend analysis
#[derive(Debug, Clone, Serialize)]
pub struct TrendAnalysis {
    pub auth_performance_trend: Trend,
    pub memory_usage_trend: Trend,
    pub cache_efficiency_trend: Trend,
    pub concurrency_trend: Trend,
    pub predictions: Vec<PerformancePrediction>,
}

/// Trend direction and magnitude
#[derive(Debug, Clone, Serialize)]
pub enum Trend {
    Improving(f64),   // Performance getting better (improvement percentage)
    Degrading(f64),   // Performance getting worse (degradation percentage)
    Stable,           // Performance is stable
    Insufficient,     // Not enough data for trend analysis
}

/// Performance prediction
#[derive(Debug, Clone, Serialize)]
pub struct PerformancePrediction {
    pub metric: String,
    pub predicted_value: f64,
    pub confidence: f64,
    pub time_horizon_hours: u32,
    pub potential_issues: Vec<String>,
}

/// Bottleneck analyzer engine
#[derive(Debug)]
pub struct BottleneckAnalyzer {
    config: AnalysisConfig,
    performance_history: Arc<RwLock<VecDeque<PerformanceDataPoint>>>,
    detected_bottlenecks: Arc<RwLock<HashMap<String, Bottleneck>>>,
    pattern_cache: Arc<RwLock<HashMap<String, PatternAnalysis>>>,
}

/// Pattern analysis cache entry
#[derive(Debug, Clone)]
struct PatternAnalysis {
    pattern_type: String,
    frequency: f64,
    impact: f64,
    last_updated: DateTime<Utc>,
}

impl BottleneckAnalyzer {
    /// Create new bottleneck analyzer
    pub fn new() -> Self {
        Self::with_config(AnalysisConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: AnalysisConfig) -> Self {
        Self {
            config,
            performance_history: Arc::new(RwLock::new(VecDeque::new())),
            detected_bottlenecks: Arc::new(RwLock::new(HashMap::new())),
            pattern_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Analyze performance metrics for bottlenecks
    pub async fn analyze_metrics(&self, metrics: &PerformanceMetrics) {
        let data_point = PerformanceDataPoint {
            timestamp: Utc::now(),
            metrics: metrics.clone(),
            context: OperationContext {
                operation_type: "authentication".to_string(),
                agent_count: metrics.concurrent_agents,
                provider: "claude".to_string(), // Could be dynamic
                user_id: None,
                session_id: None,
            },
        };

        // Add to history
        {
            let mut history_guard = self.performance_history.write().await;
            history_guard.push_back(data_point.clone());
            
            // Keep only data within analysis window
            let cutoff_time = Utc::now() - chrono::Duration::minutes(
                self.config.analysis_window_minutes as i64
            );
            
            while let Some(oldest) = history_guard.front() {
                if oldest.timestamp < cutoff_time {
                    history_guard.pop_front();
                } else {
                    break;
                }
            }
        }

        // Perform real-time analysis
        self.perform_real_time_analysis(&data_point).await;
    }

    /// Perform comprehensive bottleneck analysis
    pub async fn analyze_bottlenecks(&self) -> AnalysisReport {
        let history = {
            let history_guard = self.performance_history.read().await;
            history_guard.clone()
        };

        if history.len() < self.config.min_samples_for_analysis as usize {
            return AnalysisReport {
                analysis_period_minutes: self.config.analysis_window_minutes,
                total_samples: history.len(),
                bottlenecks_detected: vec![],
                overall_health_score: 100.0,
                trend_analysis: TrendAnalysis {
                    auth_performance_trend: Trend::Insufficient,
                    memory_usage_trend: Trend::Insufficient,
                    cache_efficiency_trend: Trend::Insufficient,
                    concurrency_trend: Trend::Insufficient,
                    predictions: vec![],
                },
                optimization_priority: vec!["Collect more performance data to enable analysis".to_string()],
                generated_at: Utc::now(),
            };
        }

        // Analyze different types of bottlenecks
        let mut bottlenecks = Vec::new();

        // Authentication performance analysis
        bottlenecks.extend(self.analyze_authentication_performance(&history).await);
        
        // Memory usage analysis
        bottlenecks.extend(self.analyze_memory_bottlenecks(&history).await);
        
        // Cache efficiency analysis
        bottlenecks.extend(self.analyze_cache_performance(&history).await);
        
        // Concurrency analysis
        bottlenecks.extend(self.analyze_concurrency_bottlenecks(&history).await);
        
        // Network latency analysis
        bottlenecks.extend(self.analyze_network_performance(&history).await);

        // Calculate overall health score
        let health_score = self.calculate_health_score(&bottlenecks);

        // Perform trend analysis
        let trend_analysis = self.analyze_trends(&history).await;

        // Generate optimization priorities
        let optimization_priority = self.generate_optimization_priorities(&bottlenecks);

        // Update detected bottlenecks cache
        {
            let mut bottlenecks_guard = self.detected_bottlenecks.write().await;
            bottlenecks_guard.clear();
            for bottleneck in &bottlenecks {
                bottlenecks_guard.insert(bottleneck.id.clone(), bottleneck.clone());
            }
        }

        AnalysisReport {
            analysis_period_minutes: self.config.analysis_window_minutes,
            total_samples: history.len(),
            bottlenecks_detected: bottlenecks,
            overall_health_score: health_score,
            trend_analysis,
            optimization_priority,
            generated_at: Utc::now(),
        }
    }

    /// Get current recommendations
    pub async fn get_recommendations(&self) -> Vec<String> {
        let bottlenecks_guard = self.detected_bottlenecks.read().await;
        let mut recommendations = Vec::new();

        // Collect recommendations from all detected bottlenecks
        for bottleneck in bottlenecks_guard.values() {
            recommendations.extend(bottleneck.recommendations.clone());
        }

        // Add general recommendations if no bottlenecks
        if recommendations.is_empty() {
            recommendations.push("Performance is optimal - continue monitoring".to_string());
        } else {
            // Sort by severity
            recommendations.sort();
            recommendations.dedup();
        }

        recommendations
    }

    /// Perform real-time analysis on new data point
    async fn perform_real_time_analysis(&self, data_point: &PerformanceDataPoint) {
        // Check for immediate issues
        if data_point.metrics.authentication_time.as_millis() > self.config.slow_auth_threshold_ms {
            self.record_bottleneck(
                BottleneckType::SlowAuthentication,
                Severity::High,
                &format!("Authentication took {}ms", data_point.metrics.authentication_time.as_millis()),
                data_point,
            ).await;
        }

        if data_point.metrics.token_refresh_time.as_millis() > self.config.slow_refresh_threshold_ms {
            self.record_bottleneck(
                BottleneckType::SlowTokenRefresh,
                Severity::Medium,
                &format!("Token refresh took {}ms", data_point.metrics.token_refresh_time.as_millis()),
                data_point,
            ).await;
        }

        if data_point.metrics.memory_usage > self.config.high_memory_threshold_mb * 1024 * 1024 {
            self.record_bottleneck(
                BottleneckType::MemoryPressure,
                Severity::High,
                &format!("High memory usage: {}MB", data_point.metrics.memory_usage / (1024 * 1024)),
                data_point,
            ).await;
        }

        if data_point.metrics.cache_hit_rate < self.config.low_cache_hit_threshold {
            self.record_bottleneck(
                BottleneckType::CacheInefficiency,
                Severity::Medium,
                &format!("Low cache hit rate: {:.1}%", data_point.metrics.cache_hit_rate * 100.0),
                data_point,
            ).await;
        }

        if data_point.metrics.concurrent_agents > self.config.high_concurrency_threshold {
            self.record_bottleneck(
                BottleneckType::ConcurrencyOverload,
                Severity::Medium,
                &format!("High concurrency: {} agents", data_point.metrics.concurrent_agents),
                data_point,
            ).await;
        }
    }

    /// Record a detected bottleneck
    async fn record_bottleneck(
        &self,
        bottleneck_type: BottleneckType,
        severity: Severity,
        description: &str,
        data_point: &PerformanceDataPoint,
    ) {
        let bottleneck_id = format!("{:?}_{}", bottleneck_type, Utc::now().timestamp());
        
        let bottleneck = Bottleneck {
            id: bottleneck_id.clone(),
            bottleneck_type: bottleneck_type.clone(),
            severity,
            description: description.to_string(),
            impact_score: self.calculate_impact_score(&bottleneck_type, &data_point.metrics),
            frequency: 1,
            average_delay_ms: self.calculate_delay(&bottleneck_type, &data_point.metrics),
            affected_operations: vec!["authentication".to_string()],
            first_detected: Utc::now(),
            last_detected: Utc::now(),
            recommendations: self.generate_recommendations_for_type(&bottleneck_type),
            metrics_evidence: serde_json::to_value(&data_point.metrics).unwrap_or_default(),
        };

        let mut bottlenecks_guard = self.detected_bottlenecks.write().await;
        bottlenecks_guard.insert(bottleneck_id, bottleneck);
    }

    /// Analyze authentication performance bottlenecks
    async fn analyze_authentication_performance(&self, history: &VecDeque<PerformanceDataPoint>) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();
        
        let auth_times: Vec<u128> = history
            .iter()
            .map(|dp| dp.metrics.authentication_time.as_millis())
            .collect();

        if auth_times.is_empty() {
            return bottlenecks;
        }

        let average_auth_time = auth_times.iter().sum::<u128>() as f64 / auth_times.len() as f64;
        let slow_auth_count = auth_times
            .iter()
            .filter(|&&time| time > self.config.slow_auth_threshold_ms)
            .count();

        if average_auth_time > self.config.slow_auth_threshold_ms as f64 {
            let severity = if average_auth_time > 500.0 { Severity::Critical } else { Severity::High };
            
            bottlenecks.push(Bottleneck {
                id: "auth_performance_bottleneck".to_string(),
                bottleneck_type: BottleneckType::SlowAuthentication,
                severity,
                description: format!("Average authentication time is {:.1}ms", average_auth_time),
                impact_score: ((average_auth_time / 100.0) * 10.0).min(100.0),
                frequency: slow_auth_count as u32,
                average_delay_ms: average_auth_time,
                affected_operations: vec!["authentication".to_string()],
                first_detected: history.front().unwrap().timestamp,
                last_detected: history.back().unwrap().timestamp,
                recommendations: vec![
                    "Enable authentication caching".to_string(),
                    "Optimize token validation logic".to_string(),
                    "Consider connection pooling".to_string(),
                ],
                metrics_evidence: serde_json::json!({
                    "average_auth_time_ms": average_auth_time,
                    "slow_auth_percentage": (slow_auth_count as f64 / auth_times.len() as f64) * 100.0,
                    "sample_count": auth_times.len()
                }),
            });
        }

        bottlenecks
    }

    /// Analyze memory usage bottlenecks
    async fn analyze_memory_bottlenecks(&self, history: &VecDeque<PerformanceDataPoint>) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();
        
        let memory_usages: Vec<u64> = history
            .iter()
            .map(|dp| dp.metrics.memory_usage)
            .collect();

        if memory_usages.is_empty() {
            return bottlenecks;
        }

        let average_memory = memory_usages.iter().sum::<u64>() / memory_usages.len() as u64;
        let max_memory = *memory_usages.iter().max().unwrap();
        let threshold_bytes = self.config.high_memory_threshold_mb * 1024 * 1024;

        if average_memory > threshold_bytes {
            let severity = if max_memory > threshold_bytes * 2 { Severity::Critical } else { Severity::High };
            
            bottlenecks.push(Bottleneck {
                id: "memory_pressure_bottleneck".to_string(),
                bottleneck_type: BottleneckType::MemoryPressure,
                severity,
                description: format!("High memory usage: average {}MB, peak {}MB", 
                    average_memory / (1024 * 1024), max_memory / (1024 * 1024)),
                impact_score: ((average_memory as f64 / threshold_bytes as f64) * 25.0).min(100.0),
                frequency: memory_usages.iter().filter(|&&usage| usage > threshold_bytes).count() as u32,
                average_delay_ms: 0.0, // Memory pressure doesn't directly cause delays
                affected_operations: vec!["authentication", "token_refresh", "agent_coordination"].iter().map(|s| s.to_string()).collect(),
                first_detected: history.front().unwrap().timestamp,
                last_detected: history.back().unwrap().timestamp,
                recommendations: vec![
                    "Implement memory pooling for agent sessions".to_string(),
                    "Increase garbage collection frequency".to_string(),
                    "Optimize token storage size".to_string(),
                    "Consider reducing session timeout".to_string(),
                ],
                metrics_evidence: serde_json::json!({
                    "average_memory_mb": average_memory / (1024 * 1024),
                    "peak_memory_mb": max_memory / (1024 * 1024),
                    "threshold_mb": self.config.high_memory_threshold_mb
                }),
            });
        }

        bottlenecks
    }

    /// Analyze cache performance bottlenecks
    async fn analyze_cache_performance(&self, history: &VecDeque<PerformanceDataPoint>) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();
        
        let hit_rates: Vec<f64> = history
            .iter()
            .map(|dp| dp.metrics.cache_hit_rate)
            .collect();

        if hit_rates.is_empty() {
            return bottlenecks;
        }

        let average_hit_rate = hit_rates.iter().sum::<f64>() / hit_rates.len() as f64;

        if average_hit_rate < self.config.low_cache_hit_threshold {
            let severity = if average_hit_rate < 0.5 { Severity::High } else { Severity::Medium };
            
            bottlenecks.push(Bottleneck {
                id: "cache_inefficiency_bottleneck".to_string(),
                bottleneck_type: BottleneckType::CacheInefficiency,
                severity,
                description: format!("Low cache hit rate: {:.1}%", average_hit_rate * 100.0),
                impact_score: ((1.0 - average_hit_rate) * 40.0).min(100.0),
                frequency: hit_rates.iter().filter(|&&rate| rate < self.config.low_cache_hit_threshold).count() as u32,
                average_delay_ms: (1.0 - average_hit_rate) * 100.0, // Estimated delay from cache misses
                affected_operations: vec!["authentication".to_string()],
                first_detected: history.front().unwrap().timestamp,
                last_detected: history.back().unwrap().timestamp,
                recommendations: vec![
                    "Increase cache TTL if appropriate".to_string(),
                    "Optimize cache key generation".to_string(),
                    "Implement preemptive cache warming".to_string(),
                    "Review cache size limits".to_string(),
                ],
                metrics_evidence: serde_json::json!({
                    "average_hit_rate": average_hit_rate,
                    "hit_rate_threshold": self.config.low_cache_hit_threshold
                }),
            });
        }

        bottlenecks
    }

    /// Analyze concurrency bottlenecks
    async fn analyze_concurrency_bottlenecks(&self, history: &VecDeque<PerformanceDataPoint>) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();
        
        let agent_counts: Vec<usize> = history
            .iter()
            .map(|dp| dp.metrics.concurrent_agents)
            .collect();

        if agent_counts.is_empty() {
            return bottlenecks;
        }

        let max_agents = *agent_counts.iter().max().unwrap();
        let average_agents = agent_counts.iter().sum::<usize>() / agent_counts.len();

        if max_agents > self.config.high_concurrency_threshold {
            let severity = if max_agents > self.config.high_concurrency_threshold * 2 { 
                Severity::Critical 
            } else { 
                Severity::Medium 
            };
            
            bottlenecks.push(Bottleneck {
                id: "concurrency_overload_bottleneck".to_string(),
                bottleneck_type: BottleneckType::ConcurrencyOverload,
                severity,
                description: format!("High concurrency detected: peak {} agents, average {}", max_agents, average_agents),
                impact_score: ((max_agents as f64 / self.config.high_concurrency_threshold as f64) * 20.0).min(100.0),
                frequency: agent_counts.iter().filter(|&&count| count > self.config.high_concurrency_threshold).count() as u32,
                average_delay_ms: (max_agents.saturating_sub(self.config.high_concurrency_threshold) as f64) * 10.0,
                affected_operations: vec!["authentication", "agent_coordination"].iter().map(|s| s.to_string()).collect(),
                first_detected: history.front().unwrap().timestamp,
                last_detected: history.back().unwrap().timestamp,
                recommendations: vec![
                    "Implement request queuing for high load".to_string(),
                    "Consider horizontal scaling".to_string(),
                    "Optimize agent coordination overhead".to_string(),
                    "Implement backpressure mechanisms".to_string(),
                ],
                metrics_evidence: serde_json::json!({
                    "peak_agents": max_agents,
                    "average_agents": average_agents,
                    "threshold": self.config.high_concurrency_threshold
                }),
            });
        }

        bottlenecks
    }

    /// Analyze network performance bottlenecks
    async fn analyze_network_performance(&self, history: &VecDeque<PerformanceDataPoint>) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();
        
        // Analyze network requests vs performance
        let high_request_count = history
            .iter()
            .filter(|dp| dp.metrics.network_requests > 5)
            .count();

        if high_request_count > history.len() / 2 {
            bottlenecks.push(Bottleneck {
                id: "network_latency_bottleneck".to_string(),
                bottleneck_type: BottleneckType::NetworkLatency,
                severity: Severity::Medium,
                description: "High number of network requests detected".to_string(),
                impact_score: (high_request_count as f64 / history.len() as f64) * 30.0,
                frequency: high_request_count as u32,
                average_delay_ms: 50.0, // Estimated network overhead
                affected_operations: vec!["authentication", "token_refresh"].iter().map(|s| s.to_string()).collect(),
                first_detected: history.front().unwrap().timestamp,
                last_detected: history.back().unwrap().timestamp,
                recommendations: vec![
                    "Implement request batching".to_string(),
                    "Use connection pooling".to_string(),
                    "Cache frequently accessed data".to_string(),
                    "Consider CDN for static resources".to_string(),
                ],
                metrics_evidence: serde_json::json!({
                    "high_request_samples": high_request_count,
                    "total_samples": history.len(),
                    "percentage": (high_request_count as f64 / history.len() as f64) * 100.0
                }),
            });
        }

        bottlenecks
    }

    /// Analyze performance trends
    async fn analyze_trends(&self, history: &VecDeque<PerformanceDataPoint>) -> TrendAnalysis {
        if history.len() < 20 {
            return TrendAnalysis {
                auth_performance_trend: Trend::Insufficient,
                memory_usage_trend: Trend::Insufficient,
                cache_efficiency_trend: Trend::Insufficient,
                concurrency_trend: Trend::Insufficient,
                predictions: vec![],
            };
        }

        // Split data into two halves for trend comparison
        let mid_point = history.len() / 2;
        let first_half: Vec<_> = history.iter().take(mid_point).collect();
        let second_half: Vec<_> = history.iter().skip(mid_point).collect();

        // Analyze authentication performance trend
        let auth_trend = self.calculate_trend(
            &first_half.iter().map(|dp| dp.metrics.authentication_time.as_millis() as f64).collect::<Vec<_>>(),
            &second_half.iter().map(|dp| dp.metrics.authentication_time.as_millis() as f64).collect::<Vec<_>>(),
        );

        // Analyze memory usage trend
        let memory_trend = self.calculate_trend(
            &first_half.iter().map(|dp| dp.metrics.memory_usage as f64).collect::<Vec<_>>(),
            &second_half.iter().map(|dp| dp.metrics.memory_usage as f64).collect::<Vec<_>>(),
        );

        // Analyze cache efficiency trend
        let cache_trend = self.calculate_trend(
            &first_half.iter().map(|dp| dp.metrics.cache_hit_rate).collect::<Vec<_>>(),
            &second_half.iter().map(|dp| dp.metrics.cache_hit_rate).collect::<Vec<_>>(),
        );

        // Analyze concurrency trend
        let concurrency_trend = self.calculate_trend(
            &first_half.iter().map(|dp| dp.metrics.concurrent_agents as f64).collect::<Vec<_>>(),
            &second_half.iter().map(|dp| dp.metrics.concurrent_agents as f64).collect::<Vec<_>>(),
        );

        // Generate predictions
        let predictions = self.generate_predictions(history);

        TrendAnalysis {
            auth_performance_trend: auth_trend,
            memory_usage_trend: memory_trend,
            cache_efficiency_trend: cache_trend,
            concurrency_trend: concurrency_trend,
            predictions,
        }
    }

    /// Calculate trend between two data sets
    fn calculate_trend(&self, first_half: &[f64], second_half: &[f64]) -> Trend {
        if first_half.is_empty() || second_half.is_empty() {
            return Trend::Insufficient;
        }

        let first_avg = first_half.iter().sum::<f64>() / first_half.len() as f64;
        let second_avg = second_half.iter().sum::<f64>() / second_half.len() as f64;

        if first_avg == 0.0 {
            return Trend::Stable;
        }

        let change_percentage = ((second_avg - first_avg) / first_avg) * 100.0;
        
        if change_percentage.abs() < 5.0 {
            Trend::Stable
        } else if change_percentage > 0.0 {
            Trend::Degrading(change_percentage)
        } else {
            Trend::Improving(-change_percentage)
        }
    }

    /// Generate performance predictions
    fn generate_predictions(&self, history: &VecDeque<PerformanceDataPoint>) -> Vec<PerformancePrediction> {
        let mut predictions = Vec::new();

        // Simple linear trend prediction for authentication time
        if history.len() >= 10 {
            let auth_times: Vec<f64> = history
                .iter()
                .map(|dp| dp.metrics.authentication_time.as_millis() as f64)
                .collect();
            
            let trend = self.calculate_linear_trend(&auth_times);
            let current_avg = auth_times.iter().sum::<f64>() / auth_times.len() as f64;
            let predicted_value = current_avg + (trend * 24.0); // 24 hours ahead
            
            predictions.push(PerformancePrediction {
                metric: "authentication_time_ms".to_string(),
                predicted_value,
                confidence: if auth_times.len() > 50 { 0.8 } else { 0.6 },
                time_horizon_hours: 24,
                potential_issues: if predicted_value > 200.0 {
                    vec!["Authentication performance may degrade significantly".to_string()]
                } else {
                    vec![]
                },
            });
        }

        predictions
    }

    /// Calculate simple linear trend
    fn calculate_linear_trend(&self, values: &[f64]) -> f64 {
        if values.len() < 2 {
            return 0.0;
        }

        let n = values.len() as f64;
        let sum_x = (0..values.len()).sum::<usize>() as f64;
        let sum_y = values.iter().sum::<f64>();
        let sum_xy = values
            .iter()
            .enumerate()
            .map(|(i, &y)| i as f64 * y)
            .sum::<f64>();
        let sum_x2 = (0..values.len())
            .map(|i| (i * i) as f64)
            .sum::<f64>();

        (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x)
    }

    /// Calculate overall health score
    fn calculate_health_score(&self, bottlenecks: &[Bottleneck]) -> f64 {
        if bottlenecks.is_empty() {
            return 100.0;
        }

        let total_impact: f64 = bottlenecks.iter().map(|b| b.impact_score).sum();
        let max_possible_impact = bottlenecks.len() as f64 * 100.0;
        
        ((max_possible_impact - total_impact) / max_possible_impact * 100.0).max(0.0)
    }

    /// Generate optimization priorities
    fn generate_optimization_priorities(&self, bottlenecks: &[Bottleneck]) -> Vec<String> {
        let mut priorities = Vec::new();

        // Sort bottlenecks by severity and impact
        let mut sorted_bottlenecks = bottlenecks.to_vec();
        sorted_bottlenecks.sort_by(|a, b| {
            b.severity.cmp(&a.severity)
                .then_with(|| b.impact_score.partial_cmp(&a.impact_score).unwrap_or(std::cmp::Ordering::Equal))
        });

        for bottleneck in sorted_bottlenecks.iter().take(5) {
            priorities.push(format!("{:?}: {}", bottleneck.severity, bottleneck.description));
        }

        if priorities.is_empty() {
            priorities.push("No critical performance issues detected".to_string());
        }

        priorities
    }

    /// Calculate impact score for bottleneck type
    fn calculate_impact_score(&self, bottleneck_type: &BottleneckType, metrics: &PerformanceMetrics) -> f64 {
        match bottleneck_type {
            BottleneckType::SlowAuthentication => (metrics.authentication_time.as_millis() as f64 / 100.0) * 20.0,
            BottleneckType::SlowTokenRefresh => (metrics.token_refresh_time.as_millis() as f64 / 500.0) * 15.0,
            BottleneckType::MemoryPressure => (metrics.memory_usage as f64 / (100.0 * 1024.0 * 1024.0)) * 25.0,
            BottleneckType::CacheInefficiency => (1.0 - metrics.cache_hit_rate) * 30.0,
            BottleneckType::ConcurrencyOverload => (metrics.concurrent_agents as f64 / 10.0) * 15.0,
            BottleneckType::NetworkLatency => (metrics.network_requests as f64 / 5.0) * 10.0,
            _ => 10.0, // Default impact
        }
    }

    /// Calculate delay caused by bottleneck
    fn calculate_delay(&self, bottleneck_type: &BottleneckType, metrics: &PerformanceMetrics) -> f64 {
        match bottleneck_type {
            BottleneckType::SlowAuthentication => metrics.authentication_time.as_millis() as f64,
            BottleneckType::SlowTokenRefresh => metrics.token_refresh_time.as_millis() as f64,
            BottleneckType::CacheInefficiency => (1.0 - metrics.cache_hit_rate) * 100.0,
            BottleneckType::NetworkLatency => metrics.network_requests as f64 * 20.0,
            _ => 0.0,
        }
    }

    /// Generate recommendations for specific bottleneck type
    fn generate_recommendations_for_type(&self, bottleneck_type: &BottleneckType) -> Vec<String> {
        match bottleneck_type {
            BottleneckType::SlowAuthentication => vec![
                "Enable authentication caching".to_string(),
                "Optimize token validation".to_string(),
                "Use connection pooling".to_string(),
            ],
            BottleneckType::SlowTokenRefresh => vec![
                "Implement token refresh batching".to_string(),
                "Use background token refresh".to_string(),
                "Optimize refresh API calls".to_string(),
            ],
            BottleneckType::MemoryPressure => vec![
                "Increase garbage collection frequency".to_string(),
                "Optimize memory allocation".to_string(),
                "Reduce session timeout".to_string(),
            ],
            BottleneckType::CacheInefficiency => vec![
                "Increase cache size".to_string(),
                "Optimize cache TTL".to_string(),
                "Implement cache warming".to_string(),
            ],
            BottleneckType::ConcurrencyOverload => vec![
                "Implement request queuing".to_string(),
                "Add backpressure handling".to_string(),
                "Consider horizontal scaling".to_string(),
            ],
            BottleneckType::NetworkLatency => vec![
                "Use connection pooling".to_string(),
                "Implement request batching".to_string(),
                "Add CDN for static content".to_string(),
            ],
            _ => vec!["Monitor and analyze further".to_string()],
        }
    }
}

impl Clone for BottleneckAnalyzer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            performance_history: Arc::clone(&self.performance_history),
            detected_bottlenecks: Arc::clone(&self.detected_bottlenecks),
            pattern_cache: Arc::clone(&self.pattern_cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_metrics(auth_time_ms: u64, memory_mb: u64, cache_hit_rate: f64, agents: usize) -> PerformanceMetrics {
        PerformanceMetrics {
            authentication_time: Duration::from_millis(auth_time_ms),
            token_refresh_time: Duration::from_millis(200),
            cache_hit_rate,
            memory_usage: memory_mb * 1024 * 1024,
            concurrent_agents: agents,
            network_requests: 2,
            timestamp: std::time::SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn test_bottleneck_analyzer_creation() {
        let analyzer = BottleneckAnalyzer::new();
        let report = analyzer.analyze_bottlenecks().await;
        assert_eq!(report.bottlenecks_detected.len(), 0);
        assert_eq!(report.overall_health_score, 100.0);
    }

    #[tokio::test]
    async fn test_slow_authentication_detection() {
        let analyzer = BottleneckAnalyzer::new();
        
        // Add metrics with slow authentication
        let metrics = create_test_metrics(500, 50, 0.8, 3); // 500ms auth time
        analyzer.analyze_metrics(&metrics).await;
        
        let report = analyzer.analyze_bottlenecks().await;
        assert!(report.bottlenecks_detected.iter().any(|b| matches!(b.bottleneck_type, BottleneckType::SlowAuthentication)));
    }

    #[tokio::test]
    async fn test_memory_pressure_detection() {
        let analyzer = BottleneckAnalyzer::new();
        
        // Add metrics with high memory usage
        let metrics = create_test_metrics(50, 150, 0.8, 3); // 150MB memory
        analyzer.analyze_metrics(&metrics).await;
        
        let report = analyzer.analyze_bottlenecks().await;
        assert!(report.bottlenecks_detected.iter().any(|b| matches!(b.bottleneck_type, BottleneckType::MemoryPressure)));
    }

    #[tokio::test]
    async fn test_cache_inefficiency_detection() {
        let analyzer = BottleneckAnalyzer::new();
        
        // Add metrics with low cache hit rate
        let metrics = create_test_metrics(50, 50, 0.5, 3); // 50% hit rate
        analyzer.analyze_metrics(&metrics).await;
        
        let report = analyzer.analyze_bottlenecks().await;
        assert!(report.bottlenecks_detected.iter().any(|b| matches!(b.bottleneck_type, BottleneckType::CacheInefficiency)));
    }

    #[tokio::test]
    async fn test_concurrency_overload_detection() {
        let analyzer = BottleneckAnalyzer::new();
        
        // Add metrics with high concurrency
        let metrics = create_test_metrics(50, 50, 0.8, 15); // 15 concurrent agents
        analyzer.analyze_metrics(&metrics).await;
        
        let report = analyzer.analyze_bottlenecks().await;
        assert!(report.bottlenecks_detected.iter().any(|b| matches!(b.bottleneck_type, BottleneckType::ConcurrencyOverload)));
    }

    #[tokio::test]
    async fn test_health_score_calculation() {
        let analyzer = BottleneckAnalyzer::new();
        
        // Add good metrics
        let good_metrics = create_test_metrics(50, 30, 0.9, 3);
        analyzer.analyze_metrics(&good_metrics).await;
        
        let report = analyzer.analyze_bottlenecks().await;
        assert!(report.overall_health_score > 80.0);
        
        // Add bad metrics
        let bad_metrics = create_test_metrics(500, 200, 0.3, 20);
        analyzer.analyze_metrics(&bad_metrics).await;
        
        let report = analyzer.analyze_bottlenecks().await;
        assert!(report.overall_health_score < 60.0);
    }

    #[tokio::test]
    async fn test_recommendations_generation() {
        let analyzer = BottleneckAnalyzer::new();
        
        // Add problematic metrics
        let metrics = create_test_metrics(200, 120, 0.4, 12);
        analyzer.analyze_metrics(&metrics).await;
        
        let recommendations = analyzer.get_recommendations().await;
        assert!(!recommendations.is_empty());
        assert!(recommendations.len() > 3); // Should have multiple recommendations
    }
}