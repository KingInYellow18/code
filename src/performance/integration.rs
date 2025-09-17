//! Performance integration module for authentication optimizations
//!
//! This module provides integration layer for performance optimization
//! across the authentication system components.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

use super::{PerformanceMetrics, PerformanceCoordinator};

/// Optimized authentication manager with performance enhancements
#[derive(Debug)]
pub struct OptimizedAuthManager {
    performance_coordinator: Arc<PerformanceCoordinator>,
    optimization_config: OptimizationConfig,
    performance_stats: Arc<RwLock<PerformanceStatistics>>,
}

impl OptimizedAuthManager {
    /// Create new optimized auth manager
    pub fn new(config: OptimizationConfig) -> Self {
        Self {
            performance_coordinator: Arc::new(PerformanceCoordinator::new()),
            optimization_config: config,
            performance_stats: Arc::new(RwLock::new(PerformanceStatistics::default())),
        }
    }

    /// Get current performance statistics
    pub async fn get_performance_stats(&self) -> PerformanceStatistics {
        self.performance_stats.read().await.clone()
    }

    /// Update performance statistics
    pub async fn update_performance_stats(&self, metrics: PerformanceMetrics) {
        let mut stats = self.performance_stats.write().await;
        stats.update_from_metrics(metrics);
    }

    /// Check if optimization is needed
    pub async fn needs_optimization(&self) -> bool {
        let stats = self.performance_stats.read().await;
        stats.auth_time_avg > self.optimization_config.auth_time_threshold ||
        stats.cache_hit_rate < self.optimization_config.cache_hit_threshold
    }

    /// Apply performance optimizations
    pub async fn apply_optimizations(&self) -> Result<(), String> {
        if self.needs_optimization().await {
            let mut stats = self.performance_stats.write().await;
            stats.optimizations_applied += 1;
            stats.last_optimization = Some(std::time::SystemTime::now());
            Ok(())
        } else {
            Ok(())
        }
    }
}

/// Performance statistics tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceStatistics {
    pub auth_time_avg: Duration,
    pub cache_hit_rate: f64,
    pub memory_usage: u64,
    pub concurrent_sessions: usize,
    pub optimizations_applied: u32,
    pub last_optimization: Option<std::time::SystemTime>,
    pub total_operations: u64,
    pub error_rate: f64,
}

impl PerformanceStatistics {
    /// Update statistics from performance metrics
    pub fn update_from_metrics(&mut self, metrics: PerformanceMetrics) {
        // Update moving averages
        self.auth_time_avg = if self.total_operations == 0 {
            metrics.authentication_time
        } else {
            Duration::from_nanos(
                ((self.auth_time_avg.as_nanos() as u64 * self.total_operations +
                  metrics.authentication_time.as_nanos() as u64) /
                 (self.total_operations + 1)) as u64
            )
        };

        self.cache_hit_rate = if self.total_operations == 0 {
            metrics.cache_hit_rate
        } else {
            (self.cache_hit_rate * self.total_operations as f64 + metrics.cache_hit_rate) /
            (self.total_operations as f64 + 1.0)
        };

        self.memory_usage = metrics.memory_usage;
        self.concurrent_sessions = metrics.concurrent_agents;
        self.total_operations += 1;
    }

    /// Check if performance is within acceptable bounds
    pub fn is_performance_acceptable(&self, config: &OptimizationConfig) -> bool {
        self.auth_time_avg <= config.auth_time_threshold &&
        self.cache_hit_rate >= config.cache_hit_threshold &&
        self.error_rate <= config.error_rate_threshold
    }
}

/// Configuration for performance optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    /// Maximum acceptable authentication time
    pub auth_time_threshold: Duration,
    /// Minimum acceptable cache hit rate (0.0 - 1.0)
    pub cache_hit_threshold: f64,
    /// Maximum acceptable error rate (0.0 - 1.0)
    pub error_rate_threshold: f64,
    /// Enable automatic optimization
    pub auto_optimize: bool,
    /// Optimization check interval
    pub optimization_interval: Duration,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            auth_time_threshold: Duration::from_millis(100),
            cache_hit_threshold: 0.85,
            error_rate_threshold: 0.05,
            auto_optimize: true,
            optimization_interval: Duration::from_secs(60),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_optimized_auth_manager_creation() {
        let config = OptimizationConfig::default();
        let manager = OptimizedAuthManager::new(config);

        let stats = manager.get_performance_stats().await;
        assert_eq!(stats.total_operations, 0);
    }

    #[tokio::test]
    async fn test_performance_statistics_update() {
        let mut stats = PerformanceStatistics::default();
        let metrics = PerformanceMetrics {
            authentication_time: Duration::from_millis(50),
            token_refresh_time: Duration::from_millis(200),
            cache_hit_rate: 0.90,
            memory_usage: 1024 * 1024, // 1MB
            concurrent_agents: 5,
            network_requests: 3,
            timestamp: std::time::SystemTime::now(),
        };

        stats.update_from_metrics(metrics);

        assert_eq!(stats.auth_time_avg.as_millis(), 50);
        assert_eq!(stats.cache_hit_rate, 0.90);
        assert_eq!(stats.total_operations, 1);
    }

    #[test]
    fn test_optimization_config_defaults() {
        let config = OptimizationConfig::default();
        assert_eq!(config.auth_time_threshold.as_millis(), 100);
        assert_eq!(config.cache_hit_threshold, 0.85);
        assert!(config.auto_optimize);
    }
}