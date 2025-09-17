// Real-time performance monitoring for Claude authentication integration
// Provides continuous monitoring and alerting for performance metrics

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, Mutex};
use tokio::time::{interval, sleep};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{PerformanceMetrics, PerformanceTargets};
use super::bottleneck_analyzer::{BottleneckAnalyzer, AnalysisReport};

/// Real-time monitoring configuration
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub monitoring_interval_ms: u64,
    pub alert_threshold_breach_count: u32,
    pub metrics_retention_minutes: u32,
    pub real_time_buffer_size: usize,
    pub alert_cooldown_minutes: u32,
    pub health_check_interval_minutes: u32,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            monitoring_interval_ms: 1000,      // Monitor every second
            alert_threshold_breach_count: 3,   // Alert after 3 consecutive breaches
            metrics_retention_minutes: 60,     // Keep 1 hour of detailed metrics
            real_time_buffer_size: 1000,       // Buffer for real-time updates
            alert_cooldown_minutes: 5,         // 5 minute cooldown between alerts
            health_check_interval_minutes: 1,  // Health check every minute
        }
    }
}

/// Performance alert levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,       // Informational alerts
    Warning,    // Performance degradation detected
    Critical,   // Serious performance issues
    Emergency,  // System performance failure
}

/// Performance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub id: String,
    pub level: AlertLevel,
    pub title: String,
    pub description: String,
    pub metric_name: String,
    pub current_value: f64,
    pub threshold_value: f64,
    pub impact_assessment: String,
    pub recommended_actions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

/// Real-time performance event
#[derive(Debug, Clone, Serialize)]
pub enum PerformanceEvent {
    MetricsUpdate(PerformanceMetrics),
    Alert(PerformanceAlert),
    BottleneckDetected(String, f64), // bottleneck_type, severity_score
    HealthStatusChanged(f64, f64),   // old_score, new_score
    SystemRecovered(String),         // recovery_description
}

/// System health status
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub overall_score: f64,
    pub component_scores: HashMap<String, f64>,
    pub status_level: HealthLevel,
    pub active_alerts: u32,
    pub last_updated: DateTime<Utc>,
    pub uptime_hours: f64,
    pub performance_summary: String,
}

/// Health status levels
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum HealthLevel {
    Excellent,   // 90-100%
    Good,        // 80-89%
    Fair,        // 70-79%
    Poor,        // 50-69%
    Critical,    // Below 50%
}

/// Performance monitoring dashboard data
#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    pub current_metrics: PerformanceMetrics,
    pub health_status: HealthStatus,
    pub active_alerts: Vec<PerformanceAlert>,
    pub recent_trends: HashMap<String, Vec<f64>>,
    pub bottleneck_summary: String,
    pub performance_score: f64,
    pub recommendations: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

/// Real-time performance monitor
#[derive(Debug)]
pub struct PerformanceMonitor {
    config: MonitorConfig,
    targets: PerformanceTargets,
    
    // Metrics storage
    current_metrics: Arc<RwLock<Option<PerformanceMetrics>>>,
    metrics_history: Arc<RwLock<VecDeque<(DateTime<Utc>, PerformanceMetrics)>>>,
    
    // Health monitoring
    health_status: Arc<RwLock<HealthStatus>>,
    last_health_check: Arc<RwLock<Instant>>,
    
    // Alert system
    active_alerts: Arc<RwLock<HashMap<String, PerformanceAlert>>>,
    alert_history: Arc<RwLock<VecDeque<PerformanceAlert>>>,
    alert_cooldowns: Arc<RwLock<HashMap<String, Instant>>>,
    
    // Event broadcasting
    event_sender: broadcast::Sender<PerformanceEvent>,
    
    // Analysis integration
    bottleneck_analyzer: Arc<BottleneckAnalyzer>,
    
    // Monitoring state
    monitoring_started: Arc<RwLock<bool>>,
    start_time: Arc<RwLock<Option<Instant>>>,
    breach_counters: Arc<RwLock<HashMap<String, u32>>>,
}

impl PerformanceMonitor {
    /// Create new performance monitor
    pub fn new(targets: PerformanceTargets) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        
        Self {
            config: MonitorConfig::default(),
            targets,
            current_metrics: Arc::new(RwLock::new(None)),
            metrics_history: Arc::new(RwLock::new(VecDeque::new())),
            health_status: Arc::new(RwLock::new(Self::create_initial_health_status())),
            last_health_check: Arc::new(RwLock::new(Instant::now())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(VecDeque::new())),
            alert_cooldowns: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            bottleneck_analyzer: Arc::new(BottleneckAnalyzer::new()),
            monitoring_started: Arc::new(RwLock::new(false)),
            start_time: Arc::new(RwLock::new(None)),
            breach_counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: MonitorConfig, targets: PerformanceTargets) -> Self {
        let mut monitor = Self::new(targets);
        monitor.config = config;
        monitor
    }

    /// Start real-time monitoring
    pub async fn start_monitoring(&self) {
        {
            let mut started_guard = self.monitoring_started.write().await;
            if *started_guard {
                return; // Already started
            }
            *started_guard = true;
            
            let mut start_time_guard = self.start_time.write().await;
            *start_time_guard = Some(Instant::now());
        }

        // Start monitoring task
        let monitor = self.clone();
        tokio::spawn(async move {
            monitor.run_monitoring_loop().await;
        });

        // Start health check task
        let monitor = self.clone();
        tokio::spawn(async move {
            monitor.run_health_check_loop().await;
        });

        // Start alert management task
        let monitor = self.clone();
        tokio::spawn(async move {
            monitor.run_alert_management_loop().await;
        });
    }

    /// Submit performance metrics for monitoring
    pub async fn submit_metrics(&self, metrics: PerformanceMetrics) {
        let timestamp = Utc::now();
        
        // Update current metrics
        {
            let mut current_guard = self.current_metrics.write().await;
            *current_guard = Some(metrics.clone());
        }

        // Add to history with retention cleanup
        {
            let mut history_guard = self.metrics_history.write().await;
            history_guard.push_back((timestamp, metrics.clone()));
            
            // Cleanup old metrics
            let retention_cutoff = timestamp - chrono::Duration::minutes(self.config.metrics_retention_minutes as i64);
            while let Some((ts, _)) = history_guard.front() {
                if *ts < retention_cutoff {
                    history_guard.pop_front();
                } else {
                    break;
                }
            }
        }

        // Analyze for bottlenecks
        self.bottleneck_analyzer.analyze_metrics(&metrics).await;

        // Check for threshold breaches
        self.check_threshold_breaches(&metrics).await;

        // Broadcast event
        let _ = self.event_sender.send(PerformanceEvent::MetricsUpdate(metrics));
    }

    /// Subscribe to real-time performance events
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<PerformanceEvent> {
        self.event_sender.subscribe()
    }

    /// Get current dashboard data
    pub async fn get_dashboard_data(&self) -> DashboardData {
        let current_metrics = {
            let guard = self.current_metrics.read().await;
            guard.clone().unwrap_or_else(|| PerformanceMetrics {
                authentication_time: Duration::from_millis(0),
                token_refresh_time: Duration::from_millis(0),
                cache_hit_rate: 0.0,
                memory_usage: 0,
                concurrent_agents: 0,
                network_requests: 0,
                timestamp: SystemTime::now(),
            })
        };

        let health_status = {
            let guard = self.health_status.read().await;
            guard.clone()
        };

        let active_alerts = {
            let guard = self.active_alerts.read().await;
            guard.values().cloned().collect()
        };

        let recent_trends = self.generate_trend_data().await;
        
        let analysis_report = self.bottleneck_analyzer.analyze_bottlenecks().await;
        let bottleneck_summary = if analysis_report.bottlenecks_detected.is_empty() {
            "No performance bottlenecks detected".to_string()
        } else {
            format!("{} bottlenecks detected", analysis_report.bottlenecks_detected.len())
        };

        let recommendations = self.bottleneck_analyzer.get_recommendations().await;

        DashboardData {
            current_metrics,
            health_status,
            active_alerts,
            recent_trends,
            bottleneck_summary,
            performance_score: analysis_report.overall_health_score,
            recommendations,
            generated_at: Utc::now(),
        }
    }

    /// Get performance alert history
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<PerformanceAlert> {
        let guard = self.alert_history.read().await;
        let limit = limit.unwrap_or(50);
        guard.iter().rev().take(limit).cloned().collect()
    }

    /// Acknowledge an alert
    pub async fn acknowledge_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut active_guard = self.active_alerts.write().await;
        if let Some(alert) = active_guard.get_mut(alert_id) {
            alert.acknowledged_at = Some(Utc::now());
            Ok(())
        } else {
            Err(format!("Alert {} not found", alert_id))
        }
    }

    /// Resolve an alert
    pub async fn resolve_alert(&self, alert_id: &str, resolution_note: &str) -> Result<(), String> {
        let mut active_guard = self.active_alerts.write().await;
        let mut history_guard = self.alert_history.write().await;
        
        if let Some(mut alert) = active_guard.remove(alert_id) {
            alert.resolved_at = Some(Utc::now());
            alert.metadata = serde_json::json!({
                "resolution_note": resolution_note,
                "resolved_by": "user"
            });
            
            history_guard.push_back(alert);
            
            // Keep history size manageable
            while history_guard.len() > 500 {
                history_guard.pop_front();
            }
            
            Ok(())
        } else {
            Err(format!("Alert {} not found", alert_id))
        }
    }

    /// Main monitoring loop
    async fn run_monitoring_loop(&self) {
        let mut interval = interval(Duration::from_millis(self.config.monitoring_interval_ms));
        
        loop {
            interval.tick().await;
            
            // Perform periodic monitoring tasks
            self.update_health_status().await;
            self.cleanup_expired_alerts().await;
        }
    }

    /// Health check loop
    async fn run_health_check_loop(&self) {
        let mut interval = interval(Duration::from_secs((self.config.health_check_interval_minutes * 60) as u64));
        
        loop {
            interval.tick().await;
            
            let old_health_score = {
                let guard = self.health_status.read().await;
                guard.overall_score
            };
            
            self.perform_comprehensive_health_check().await;
            
            let new_health_score = {
                let guard = self.health_status.read().await;
                guard.overall_score
            };
            
            // Broadcast health change if significant
            if (old_health_score - new_health_score).abs() > 5.0 {
                let _ = self.event_sender.send(PerformanceEvent::HealthStatusChanged(old_health_score, new_health_score));
            }
        }
    }

    /// Alert management loop
    async fn run_alert_management_loop(&self) {
        let mut interval = interval(Duration::from_secs(60)); // Check every minute
        
        loop {
            interval.tick().await;
            
            // Auto-resolve alerts that are no longer relevant
            self.auto_resolve_alerts().await;
            
            // Clean up alert cooldowns
            self.cleanup_alert_cooldowns().await;
        }
    }

    /// Check for performance threshold breaches
    async fn check_threshold_breaches(&self, metrics: &PerformanceMetrics) {
        // Authentication time check
        if metrics.authentication_time.as_millis() > self.targets.authentication_cache_ms {
            self.handle_threshold_breach(
                "authentication_time",
                metrics.authentication_time.as_millis() as f64,
                self.targets.authentication_cache_ms as f64,
                AlertLevel::Warning,
                "Authentication taking longer than target",
            ).await;
        }

        // Token refresh time check
        if metrics.token_refresh_time.as_millis() > self.targets.token_refresh_ms {
            self.handle_threshold_breach(
                "token_refresh_time",
                metrics.token_refresh_time.as_millis() as f64,
                self.targets.token_refresh_ms as f64,
                AlertLevel::Warning,
                "Token refresh taking longer than target",
            ).await;
        }

        // Memory usage check
        let memory_mb = metrics.memory_usage / (1024 * 1024);
        if memory_mb > self.targets.memory_usage_mb {
            self.handle_threshold_breach(
                "memory_usage",
                memory_mb as f64,
                self.targets.memory_usage_mb as f64,
                AlertLevel::Critical,
                "Memory usage exceeds target",
            ).await;
        }

        // Cache hit rate check (inverted - low hit rate is bad)
        if metrics.cache_hit_rate < 0.8 {
            self.handle_threshold_breach(
                "cache_hit_rate",
                metrics.cache_hit_rate,
                0.8,
                AlertLevel::Warning,
                "Cache hit rate below optimal threshold",
            ).await;
        }
    }

    /// Handle a threshold breach
    async fn handle_threshold_breach(
        &self,
        metric_name: &str,
        current_value: f64,
        threshold_value: f64,
        level: AlertLevel,
        description: &str,
    ) {
        // Check if we're in cooldown for this metric
        {
            let cooldowns_guard = self.alert_cooldowns.read().await;
            if let Some(last_alert) = cooldowns_guard.get(metric_name) {
                if last_alert.elapsed() < Duration::from_secs((self.config.alert_cooldown_minutes * 60) as u64) {
                    return; // Still in cooldown
                }
            }
        }

        // Increment breach counter
        let breach_count = {
            let mut counters_guard = self.breach_counters.write().await;
            let count = counters_guard.entry(metric_name.to_string()).or_insert(0);
            *count += 1;
            *count
        };

        // Only alert after consecutive breaches
        if breach_count >= self.config.alert_threshold_breach_count {
            self.create_alert(metric_name, current_value, threshold_value, level, description).await;
            
            // Reset counter and set cooldown
            {
                let mut counters_guard = self.breach_counters.write().await;
                counters_guard.insert(metric_name.to_string(), 0);
            }
            
            {
                let mut cooldowns_guard = self.alert_cooldowns.write().await;
                cooldowns_guard.insert(metric_name.to_string(), Instant::now());
            }
        }
    }

    /// Create a performance alert
    async fn create_alert(
        &self,
        metric_name: &str,
        current_value: f64,
        threshold_value: f64,
        level: AlertLevel,
        description: &str,
    ) {
        let alert_id = uuid::Uuid::new_v4().to_string();
        
        let alert = PerformanceAlert {
            id: alert_id.clone(),
            level: level.clone(),
            title: format!("Performance Alert: {}", metric_name),
            description: description.to_string(),
            metric_name: metric_name.to_string(),
            current_value,
            threshold_value,
            impact_assessment: self.assess_impact(metric_name, current_value, threshold_value),
            recommended_actions: self.get_recommended_actions(metric_name),
            created_at: Utc::now(),
            resolved_at: None,
            acknowledged_at: None,
            metadata: serde_json::json!({
                "breach_severity": (current_value - threshold_value).abs(),
                "threshold_exceeded_by": format!("{:.1}%", ((current_value - threshold_value) / threshold_value * 100.0).abs())
            }),
        };

        // Store active alert
        {
            let mut active_guard = self.active_alerts.write().await;
            active_guard.insert(alert_id, alert.clone());
        }

        // Broadcast alert
        let _ = self.event_sender.send(PerformanceEvent::Alert(alert));
    }

    /// Assess impact of performance issue
    fn assess_impact(&self, metric_name: &str, current_value: f64, threshold_value: f64) -> String {
        let deviation_percent = ((current_value - threshold_value) / threshold_value * 100.0).abs();
        
        let impact_level = if deviation_percent > 100.0 {
            "severe"
        } else if deviation_percent > 50.0 {
            "high"
        } else if deviation_percent > 25.0 {
            "moderate"
        } else {
            "minor"
        };

        match metric_name {
            "authentication_time" => format!("Authentication operations experiencing {} delays ({:.1}% above target)", impact_level, deviation_percent),
            "token_refresh_time" => format!("Token refresh operations showing {} performance degradation", impact_level),
            "memory_usage" => format!("Memory pressure at {} level, may affect system stability", impact_level),
            "cache_hit_rate" => format!("Cache efficiency reduced by {:.1}%, causing {} impact on response times", 100.0 - (current_value * 100.0), impact_level),
            _ => format!("Performance metric {} showing {} deviation from target", metric_name, impact_level),
        }
    }

    /// Get recommended actions for metric
    fn get_recommended_actions(&self, metric_name: &str) -> Vec<String> {
        match metric_name {
            "authentication_time" => vec![
                "Check authentication cache hit rate".to_string(),
                "Verify network connectivity to authentication servers".to_string(),
                "Review recent changes to authentication logic".to_string(),
                "Consider scaling authentication infrastructure".to_string(),
            ],
            "token_refresh_time" => vec![
                "Implement token refresh batching".to_string(),
                "Check token refresh API response times".to_string(),
                "Consider background token refresh".to_string(),
            ],
            "memory_usage" => vec![
                "Trigger garbage collection".to_string(),
                "Review agent session retention policies".to_string(),
                "Check for memory leaks in recent changes".to_string(),
                "Consider increasing available memory".to_string(),
            ],
            "cache_hit_rate" => vec![
                "Review cache configuration and TTL settings".to_string(),
                "Check cache eviction patterns".to_string(),
                "Consider increasing cache size".to_string(),
                "Implement cache warming strategies".to_string(),
            ],
            _ => vec![
                "Monitor metric trends".to_string(),
                "Review recent system changes".to_string(),
            ],
        }
    }

    /// Update overall health status
    async fn update_health_status(&self) {
        let current_metrics = {
            let guard = self.current_metrics.read().await;
            guard.clone()
        };

        if let Some(metrics) = current_metrics {
            let component_scores = self.calculate_component_scores(&metrics);
            let overall_score = self.calculate_overall_score(&component_scores);
            let status_level = Self::score_to_health_level(overall_score);
            
            let active_alerts_count = {
                let guard = self.active_alerts.read().await;
                guard.len() as u32
            };

            let uptime_hours = {
                let start_time_guard = self.start_time.read().await;
                if let Some(start_time) = *start_time_guard {
                    start_time.elapsed().as_secs_f64() / 3600.0
                } else {
                    0.0
                }
            };

            let performance_summary = self.generate_performance_summary(&metrics, overall_score);

            let health_status = HealthStatus {
                overall_score,
                component_scores,
                status_level,
                active_alerts: active_alerts_count,
                last_updated: Utc::now(),
                uptime_hours,
                performance_summary,
            };

            let mut health_guard = self.health_status.write().await;
            *health_guard = health_status;
        }
    }

    /// Perform comprehensive health check
    async fn perform_comprehensive_health_check(&self) {
        // Run bottleneck analysis
        let analysis_report = self.bottleneck_analyzer.analyze_bottlenecks().await;
        
        // Check for new bottlenecks
        for bottleneck in &analysis_report.bottlenecks_detected {
            if bottleneck.impact_score > 50.0 {
                let _ = self.event_sender.send(PerformanceEvent::BottleneckDetected(
                    format!("{:?}", bottleneck.bottleneck_type),
                    bottleneck.impact_score,
                ));
            }
        }

        // Update last health check time
        let mut last_check_guard = self.last_health_check.write().await;
        *last_check_guard = Instant::now();
    }

    /// Calculate component health scores
    fn calculate_component_scores(&self, metrics: &PerformanceMetrics) -> HashMap<String, f64> {
        let mut scores = HashMap::new();

        // Authentication performance score
        let auth_score = if metrics.authentication_time.as_millis() <= self.targets.authentication_cache_ms {
            100.0
        } else {
            (self.targets.authentication_cache_ms as f64 / metrics.authentication_time.as_millis() as f64 * 100.0).max(0.0)
        };
        scores.insert("authentication".to_string(), auth_score);

        // Token refresh performance score
        let refresh_score = if metrics.token_refresh_time.as_millis() <= self.targets.token_refresh_ms {
            100.0
        } else {
            (self.targets.token_refresh_ms as f64 / metrics.token_refresh_time.as_millis() as f64 * 100.0).max(0.0)
        };
        scores.insert("token_refresh".to_string(), refresh_score);

        // Memory usage score (inverted - lower usage is better)
        let memory_mb = metrics.memory_usage / (1024 * 1024);
        let memory_score = if memory_mb <= self.targets.memory_usage_mb {
            100.0
        } else {
            (self.targets.memory_usage_mb as f64 / memory_mb as f64 * 100.0).max(0.0)
        };
        scores.insert("memory".to_string(), memory_score);

        // Cache efficiency score
        let cache_score = metrics.cache_hit_rate * 100.0;
        scores.insert("cache".to_string(), cache_score);

        // Concurrency score
        let concurrency_score = if metrics.concurrent_agents <= self.targets.concurrent_agents {
            100.0
        } else {
            (self.targets.concurrent_agents as f64 / metrics.concurrent_agents as f64 * 100.0).max(0.0)
        };
        scores.insert("concurrency".to_string(), concurrency_score);

        scores
    }

    /// Calculate overall health score
    fn calculate_overall_score(&self, component_scores: &HashMap<String, f64>) -> f64 {
        if component_scores.is_empty() {
            return 0.0;
        }

        // Weighted average (authentication and memory are more important)
        let weights = [
            ("authentication", 0.3),
            ("token_refresh", 0.2),
            ("memory", 0.25),
            ("cache", 0.15),
            ("concurrency", 0.1),
        ];

        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;

        for (component, weight) in &weights {
            if let Some(score) = component_scores.get(*component) {
                weighted_sum += score * weight;
                total_weight += weight;
            }
        }

        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        }
    }

    /// Convert score to health level
    fn score_to_health_level(score: f64) -> HealthLevel {
        match score {
            s if s >= 90.0 => HealthLevel::Excellent,
            s if s >= 80.0 => HealthLevel::Good,
            s if s >= 70.0 => HealthLevel::Fair,
            s if s >= 50.0 => HealthLevel::Poor,
            _ => HealthLevel::Critical,
        }
    }

    /// Generate performance summary
    fn generate_performance_summary(&self, metrics: &PerformanceMetrics, score: f64) -> String {
        let level = Self::score_to_health_level(score);
        
        match level {
            HealthLevel::Excellent => "All systems performing optimally".to_string(),
            HealthLevel::Good => format!("Good performance with {}ms avg auth time", metrics.authentication_time.as_millis()),
            HealthLevel::Fair => format!("Fair performance, monitoring {}MB memory usage", metrics.memory_usage / (1024 * 1024)),
            HealthLevel::Poor => format!("Performance issues detected, {} concurrent operations", metrics.concurrent_agents),
            HealthLevel::Critical => "Critical performance issues require immediate attention".to_string(),
        }
    }

    /// Generate trend data for dashboard
    async fn generate_trend_data(&self) -> HashMap<String, Vec<f64>> {
        let history_guard = self.metrics_history.read().await;
        let mut trends = HashMap::new();

        let auth_times: Vec<f64> = history_guard
            .iter()
            .map(|(_, m)| m.authentication_time.as_millis() as f64)
            .collect();
        trends.insert("authentication_time".to_string(), auth_times);

        let memory_usage: Vec<f64> = history_guard
            .iter()
            .map(|(_, m)| m.memory_usage as f64 / (1024.0 * 1024.0))
            .collect();
        trends.insert("memory_usage".to_string(), memory_usage);

        let cache_rates: Vec<f64> = history_guard
            .iter()
            .map(|(_, m)| m.cache_hit_rate * 100.0)
            .collect();
        trends.insert("cache_hit_rate".to_string(), cache_rates);

        trends
    }

    /// Auto-resolve alerts that are no longer relevant
    async fn auto_resolve_alerts(&self) {
        let current_metrics = {
            let guard = self.current_metrics.read().await;
            guard.clone()
        };

        if let Some(metrics) = current_metrics {
            let mut alerts_to_resolve = Vec::new();
            
            {
                let active_guard = self.active_alerts.read().await;
                for (alert_id, alert) in active_guard.iter() {
                    if self.should_auto_resolve_alert(alert, &metrics) {
                        alerts_to_resolve.push(alert_id.clone());
                    }
                }
            }

            // Resolve alerts
            for alert_id in alerts_to_resolve {
                let _ = self.resolve_alert(&alert_id, "Auto-resolved: metrics returned to normal").await;
                let _ = self.event_sender.send(PerformanceEvent::SystemRecovered(
                    format!("Alert {} auto-resolved", alert_id)
                ));
            }
        }
    }

    /// Check if alert should be auto-resolved
    fn should_auto_resolve_alert(&self, alert: &PerformanceAlert, metrics: &PerformanceMetrics) -> bool {
        match alert.metric_name.as_str() {
            "authentication_time" => metrics.authentication_time.as_millis() <= self.targets.authentication_cache_ms,
            "token_refresh_time" => metrics.token_refresh_time.as_millis() <= self.targets.token_refresh_ms,
            "memory_usage" => (metrics.memory_usage / (1024 * 1024)) <= self.targets.memory_usage_mb,
            "cache_hit_rate" => metrics.cache_hit_rate >= 0.8,
            _ => false,
        }
    }

    /// Cleanup expired alerts and cooldowns
    async fn cleanup_expired_alerts(&self) {
        // Move old active alerts to history if they're very old
        let cutoff_time = Utc::now() - chrono::Duration::hours(24);
        let mut alerts_to_archive = Vec::new();

        {
            let active_guard = self.active_alerts.read().await;
            for (alert_id, alert) in active_guard.iter() {
                if alert.created_at < cutoff_time && alert.acknowledged_at.is_none() {
                    alerts_to_archive.push(alert_id.clone());
                }
            }
        }

        for alert_id in alerts_to_archive {
            let _ = self.resolve_alert(&alert_id, "Auto-archived: alert expired").await;
        }
    }

    /// Cleanup alert cooldowns
    async fn cleanup_alert_cooldowns(&self) {
        let mut cooldowns_guard = self.alert_cooldowns.write().await;
        let cooldown_duration = Duration::from_secs((self.config.alert_cooldown_minutes * 60) as u64);
        
        cooldowns_guard.retain(|_, last_alert| {
            last_alert.elapsed() < cooldown_duration
        });
    }

    /// Create initial health status
    fn create_initial_health_status() -> HealthStatus {
        HealthStatus {
            overall_score: 100.0,
            component_scores: HashMap::new(),
            status_level: HealthLevel::Excellent,
            active_alerts: 0,
            last_updated: Utc::now(),
            uptime_hours: 0.0,
            performance_summary: "System starting up".to_string(),
        }
    }
}

impl Clone for PerformanceMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            targets: self.targets.clone(),
            current_metrics: Arc::clone(&self.current_metrics),
            metrics_history: Arc::clone(&self.metrics_history),
            health_status: Arc::clone(&self.health_status),
            last_health_check: Arc::clone(&self.last_health_check),
            active_alerts: Arc::clone(&self.active_alerts),
            alert_history: Arc::clone(&self.alert_history),
            alert_cooldowns: Arc::clone(&self.alert_cooldowns),
            event_sender: self.event_sender.clone(),
            bottleneck_analyzer: Arc::clone(&self.bottleneck_analyzer),
            monitoring_started: Arc::clone(&self.monitoring_started),
            start_time: Arc::clone(&self.start_time),
            breach_counters: Arc::clone(&self.breach_counters),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_performance_monitor_creation() {
        let targets = PerformanceTargets::default();
        let monitor = PerformanceMonitor::new(targets);
        
        let dashboard = monitor.get_dashboard_data().await;
        assert_eq!(dashboard.performance_score, 100.0); // Initial score should be good
    }

    #[tokio::test]
    async fn test_metrics_submission() {
        let targets = PerformanceTargets::default();
        let monitor = PerformanceMonitor::new(targets);
        
        let metrics = PerformanceMetrics {
            authentication_time: Duration::from_millis(50),
            token_refresh_time: Duration::from_millis(200),
            cache_hit_rate: 0.9,
            memory_usage: 30 * 1024 * 1024, // 30MB
            concurrent_agents: 3,
            network_requests: 2,
            timestamp: SystemTime::now(),
        };

        monitor.submit_metrics(metrics).await;
        
        let dashboard = monitor.get_dashboard_data().await;
        assert!(dashboard.current_metrics.authentication_time.as_millis() > 0);
    }

    #[tokio::test]
    async fn test_alert_generation() {
        let targets = PerformanceTargets {
            authentication_cache_ms: 50, // Very low threshold for testing
            ..Default::default()
        };
        
        let config = MonitorConfig {
            alert_threshold_breach_count: 1, // Alert immediately for testing
            ..Default::default()
        };
        
        let monitor = PerformanceMonitor::with_config(config, targets);
        monitor.start_monitoring().await;
        
        // Submit metrics that exceed threshold
        let bad_metrics = PerformanceMetrics {
            authentication_time: Duration::from_millis(200), // Exceeds 50ms threshold
            token_refresh_time: Duration::from_millis(200),
            cache_hit_rate: 0.9,
            memory_usage: 30 * 1024 * 1024,
            concurrent_agents: 3,
            network_requests: 2,
            timestamp: SystemTime::now(),
        };

        monitor.submit_metrics(bad_metrics).await;
        
        // Wait for alert processing
        sleep(Duration::from_millis(100)).await;
        
        let dashboard = monitor.get_dashboard_data().await;
        assert!(!dashboard.active_alerts.is_empty());
    }

    #[tokio::test]
    async fn test_health_status_calculation() {
        let targets = PerformanceTargets::default();
        let monitor = PerformanceMonitor::new(targets);
        
        // Submit good metrics
        let good_metrics = PerformanceMetrics {
            authentication_time: Duration::from_millis(30),
            token_refresh_time: Duration::from_millis(200),
            cache_hit_rate: 0.95,
            memory_usage: 20 * 1024 * 1024, // 20MB
            concurrent_agents: 2,
            network_requests: 1,
            timestamp: SystemTime::now(),
        };

        monitor.submit_metrics(good_metrics).await;
        
        let dashboard = monitor.get_dashboard_data().await;
        assert!(dashboard.health_status.overall_score > 80.0);
        assert!(matches!(dashboard.health_status.status_level, HealthLevel::Good | HealthLevel::Excellent));
    }

    #[tokio::test]
    async fn test_alert_acknowledgment() {
        let targets = PerformanceTargets {
            authentication_cache_ms: 50,
            ..Default::default()
        };
        
        let config = MonitorConfig {
            alert_threshold_breach_count: 1,
            ..Default::default()
        };
        
        let monitor = PerformanceMonitor::with_config(config, targets);
        
        // Generate an alert
        let bad_metrics = PerformanceMetrics {
            authentication_time: Duration::from_millis(200),
            token_refresh_time: Duration::from_millis(200),
            cache_hit_rate: 0.9,
            memory_usage: 30 * 1024 * 1024,
            concurrent_agents: 3,
            network_requests: 2,
            timestamp: SystemTime::now(),
        };

        monitor.submit_metrics(bad_metrics).await;
        
        let dashboard = monitor.get_dashboard_data().await;
        if let Some(alert) = dashboard.active_alerts.first() {
            let result = monitor.acknowledge_alert(&alert.id).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let targets = PerformanceTargets::default();
        let monitor = PerformanceMonitor::new(targets);
        
        let mut event_receiver = monitor.subscribe_to_events();
        
        // Submit metrics in a separate task
        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            let metrics = PerformanceMetrics {
                authentication_time: Duration::from_millis(50),
                token_refresh_time: Duration::from_millis(200),
                cache_hit_rate: 0.9,
                memory_usage: 30 * 1024 * 1024,
                concurrent_agents: 3,
                network_requests: 2,
                timestamp: SystemTime::now(),
            };
            monitor_clone.submit_metrics(metrics).await;
        });
        
        // Wait for event
        tokio::select! {
            event = event_receiver.recv() => {
                assert!(event.is_ok());
                match event.unwrap() {
                    PerformanceEvent::MetricsUpdate(_) => {
                        // Expected event type
                    },
                    _ => panic!("Unexpected event type"),
                }
            },
            _ = sleep(Duration::from_millis(1000)) => {
                panic!("Timeout waiting for event");
            }
        }
    }
}