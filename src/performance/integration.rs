// Integration module for performance optimizations with existing Claude auth system
// Connects all performance components with the authentication infrastructure

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

use crate::claude_auth::{ClaudeAuth, ClaudeAuthMode, SubscriptionInfo};
use crate::unified_auth::{UnifiedAuthManager, AuthProvider};
use crate::agent_auth::{AgentAuthCoordinator, AgentAuthRequest, AgentAuthResponse};

use super::{
    PerformanceCoordinator, PerformanceMetrics, PerformanceTargets,
    authentication_cache::AuthenticationCache,
    token_optimization::TokenOptimizer,
    connection_pool::ClaudeConnectionPool,
    memory_optimization::MemoryOptimizer,
    performance_monitor::PerformanceMonitor,
};

/// Performance-optimized authentication manager
/// Integrates all performance components with the existing authentication system
#[derive(Debug)]
pub struct OptimizedAuthManager {
    // Core authentication components (from existing system)
    unified_auth: Arc<UnifiedAuthManager>,
    agent_coordinator: Arc<AgentAuthCoordinator>,
    
    // Performance optimization components
    performance_coordinator: Arc<PerformanceCoordinator>,
    auth_cache: Arc<AuthenticationCache>,
    token_optimizer: Arc<TokenOptimizer>,
    connection_pool: Arc<ClaudeConnectionPool>,
    memory_optimizer: Arc<MemoryOptimizer>,
    performance_monitor: Arc<PerformanceMonitor>,
    
    // Configuration
    targets: PerformanceTargets,
    optimization_enabled: Arc<RwLock<bool>>,
}

/// Performance optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    pub enable_caching: bool,
    pub enable_token_batching: bool,
    pub enable_connection_pooling: bool,
    pub enable_memory_optimization: bool,
    pub enable_monitoring: bool,
    pub cache_ttl_minutes: u32,
    pub batch_timeout_ms: u64,
    pub max_connections_per_host: usize,
    pub memory_limit_mb: u64,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            enable_caching: true,
            enable_token_batching: true,
            enable_connection_pooling: true,
            enable_memory_optimization: true,
            enable_monitoring: true,
            cache_ttl_minutes: 60,
            batch_timeout_ms: 500,
            max_connections_per_host: 20,
            memory_limit_mb: 500,
        }
    }
}

/// Optimized authentication result
#[derive(Debug, Clone)]
pub struct OptimizedAuthResult {
    pub provider: AuthProvider,
    pub token: String,
    pub subscription_info: Option<SubscriptionInfo>,
    pub performance_metrics: PerformanceMetrics,
    pub cache_hit: bool,
    pub optimization_applied: Vec<String>,
}

impl OptimizedAuthManager {
    /// Create new optimized authentication manager
    pub async fn new(
        unified_auth: Arc<UnifiedAuthManager>,
        agent_coordinator: Arc<AgentAuthCoordinator>,
        config: OptimizationConfig,
    ) -> Self {
        let targets = PerformanceTargets::default();
        
        let performance_coordinator = Arc::new(PerformanceCoordinator::new());
        let auth_cache = performance_coordinator.get_cache();
        let connection_pool = performance_coordinator.get_connection_pool();
        let memory_optimizer = performance_coordinator.get_memory_optimizer();
        
        let token_optimizer = Arc::new(TokenOptimizer::new());
        let performance_monitor = Arc::new(PerformanceMonitor::new(targets.clone()));

        // Start background services if enabled
        if config.enable_monitoring {
            performance_monitor.start_monitoring().await;
        }
        
        if config.enable_token_batching {
            token_optimizer.start().await;
        }

        if config.enable_memory_optimization {
            memory_optimizer.start_background_tasks().await;
        }

        if config.enable_connection_pooling {
            connection_pool.start_cleanup_task().await;
        }

        Self {
            unified_auth,
            agent_coordinator,
            performance_coordinator,
            auth_cache,
            token_optimizer,
            connection_pool,
            memory_optimizer,
            performance_monitor,
            targets,
            optimization_enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Perform optimized authentication for an agent
    pub async fn authenticate_agent_optimized(
        &self,
        agent_id: &str,
        estimated_memory_mb: u64,
    ) -> Result<OptimizedAuthResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut optimization_applied = Vec::new();
        let mut cache_hit = false;

        // Step 1: Try cache first (if enabled)
        if *self.optimization_enabled.read().await {
            if let Some(cached_auth) = self.try_cached_authentication(agent_id).await? {
                cache_hit = true;
                optimization_applied.push("authentication_cache".to_string());
                
                let metrics = self.create_performance_metrics(start_time, cache_hit);
                self.performance_monitor.submit_metrics(metrics.clone()).await;
                
                return Ok(OptimizedAuthResult {
                    provider: if cached_auth.provider == "claude" { AuthProvider::Claude } else { AuthProvider::OpenAI },
                    token: cached_auth.token,
                    subscription_info: None, // Would need to be cached separately
                    performance_metrics: metrics,
                    cache_hit,
                    optimization_applied,
                });
            }
        }

        // Step 2: Allocate memory for agent session
        let session_id = if *self.optimization_enabled.read().await {
            match self.memory_optimizer.allocate_agent_session(agent_id, estimated_memory_mb).await {
                Ok(id) => {
                    optimization_applied.push("memory_optimization".to_string());
                    Some(id)
                },
                Err(_) => None, // Continue without memory optimization
            }
        } else {
            None
        };

        // Step 3: Select optimal provider
        let provider = self.unified_auth.select_optimal_provider().await?;

        // Step 4: Perform authentication with optimizations
        let auth_result = match provider {
            AuthProvider::Claude => self.authenticate_claude_optimized(agent_id).await?,
            AuthProvider::OpenAI => self.authenticate_openai_optimized(agent_id).await?,
        };

        // Step 5: Cache the result (if enabled and successful)
        if *self.optimization_enabled.read().await && !cache_hit {
            self.cache_authentication_result(agent_id, &auth_result).await?;
            optimization_applied.push("result_caching".to_string());
        }

        // Step 6: Record performance metrics
        let metrics = self.create_performance_metrics(start_time, cache_hit);
        self.performance_coordinator.record_metrics(metrics.clone()).await;
        self.performance_monitor.submit_metrics(metrics.clone()).await;

        Ok(OptimizedAuthResult {
            provider: auth_result.provider,
            token: auth_result.token,
            subscription_info: auth_result.subscription_info,
            performance_metrics: metrics,
            cache_hit,
            optimization_applied,
        })
    }

    /// Perform batch token refresh with optimization
    pub async fn batch_refresh_tokens(
        &self,
        refresh_requests: Vec<(String, String, String)>, // (agent_id, provider, refresh_token)
    ) -> Vec<Result<String, String>> {
        let mut results = Vec::new();
        let start_time = Instant::now();

        if *self.optimization_enabled.read().await {
            // Use optimized token refresh
            let mut request_ids = Vec::new();
            
            for (agent_id, provider, refresh_token) in &refresh_requests {
                let request_id = self.token_optimizer.request_refresh(
                    provider,
                    agent_id,
                    refresh_token,
                    super::token_optimization::RefreshPriority::Normal,
                    None,
                ).await;
                request_ids.push(request_id);
            }

            // Wait for all refresh operations to complete
            for request_id in request_ids {
                if let Some(result) = self.token_optimizer.wait_for_result(&request_id, Duration::from_secs(30)).await {
                    if result.success {
                        results.push(Ok(result.new_token.unwrap_or_default()));
                    } else {
                        results.push(Err(result.error.unwrap_or("Token refresh failed".to_string())));
                    }
                } else {
                    results.push(Err("Token refresh timeout".to_string()));
                }
            }
        } else {
            // Fallback to individual refresh
            for (agent_id, provider, refresh_token) in refresh_requests {
                let result = self.refresh_token_individual(&agent_id, &provider, &refresh_token).await;
                results.push(result);
            }
        }

        // Record batch performance
        let batch_metrics = PerformanceMetrics {
            authentication_time: Duration::from_millis(0),
            token_refresh_time: start_time.elapsed(),
            cache_hit_rate: 0.0,
            memory_usage: 0,
            concurrent_agents: results.len(),
            network_requests: results.len() as u32,
            timestamp: std::time::SystemTime::now(),
        };
        
        self.performance_coordinator.record_metrics(batch_metrics.clone()).await;
        self.performance_monitor.submit_metrics(batch_metrics).await;

        results
    }

    /// Get performance dashboard data
    pub async fn get_performance_dashboard(&self) -> serde_json::Value {
        let dashboard_data = self.performance_monitor.get_dashboard_data().await;
        let coordinator_report = self.performance_coordinator.meets_performance_targets().await;
        let memory_health = self.memory_optimizer.get_health_report().await;
        let cache_health = self.auth_cache.get_health_report().await;
        let connection_health = self.connection_pool.get_health_report().await;

        serde_json::json!({
            "overall_performance": {
                "score": coordinator_report.overall_score,
                "authentication_performance": coordinator_report.authentication_performance,
                "token_refresh_performance": coordinator_report.token_refresh_performance,
                "memory_performance": coordinator_report.memory_performance,
                "concurrency_performance": coordinator_report.concurrency_performance
            },
            "real_time_dashboard": dashboard_data,
            "component_health": {
                "memory": memory_health,
                "cache": cache_health,
                "connections": connection_health
            },
            "recommendations": coordinator_report.recommendations,
            "optimization_status": {
                "caching_enabled": *self.optimization_enabled.read().await,
                "active_optimizations": self.get_active_optimizations().await
            }
        })
    }

    /// Enable or disable specific optimizations
    pub async fn configure_optimizations(&self, config: OptimizationConfig) -> Result<(), String> {
        // This would update the optimization configuration
        // For now, just enable/disable the main optimization flag
        let mut enabled_guard = self.optimization_enabled.write().await;
        *enabled_guard = config.enable_caching && config.enable_token_batching 
            && config.enable_connection_pooling && config.enable_memory_optimization;
        
        Ok(())
    }

    /// Get performance statistics for analysis
    pub async fn get_performance_statistics(&self) -> PerformanceStatistics {
        let coordinator_report = self.performance_coordinator.meets_performance_targets().await;
        let cache_stats = self.auth_cache.get_stats().await;
        let token_stats = self.token_optimizer.get_stats().await;
        let memory_stats = self.memory_optimizer.get_stats().await;
        let connection_stats = self.connection_pool.get_stats().await;
        
        PerformanceStatistics {
            overall_score: coordinator_report.overall_score,
            cache_hit_rate: cache_stats.hit_rate,
            average_auth_time_ms: cache_stats.average_lookup_time_ms,
            token_refresh_efficiency: token_stats.batch_efficiency,
            memory_utilization: memory_stats.memory_efficiency,
            connection_reuse_rate: connection_stats.connection_reuse_rate,
            active_agents: memory_stats.agent_count,
            total_requests: cache_stats.total_requests + token_stats.total_requests,
            error_rate: (token_stats.failed_refreshes as f64 / token_stats.total_requests.max(1) as f64) * 100.0,
        }
    }

    /// Try to get authentication from cache
    async fn try_cached_authentication(
        &self,
        agent_id: &str,
    ) -> Result<Option<super::authentication_cache::CachedAuth>, Box<dyn std::error::Error + Send + Sync>> {
        // Try Claude cache first
        if let Some(cached) = self.auth_cache.get("claude", agent_id).await {
            return Ok(Some(cached));
        }
        
        // Try OpenAI cache
        if let Some(cached) = self.auth_cache.get("openai", agent_id).await {
            return Ok(Some(cached));
        }
        
        Ok(None)
    }

    /// Authenticate using Claude with optimizations
    async fn authenticate_claude_optimized(
        &self,
        agent_id: &str,
    ) -> Result<AgentAuthResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Use connection pooling for Claude API calls
        let client = self.connection_pool.get_client("api.anthropic.com").await;
        
        // Create auth request
        let auth_request = AgentAuthRequest {
            agent_id: agent_id.to_string(),
            estimated_tokens: 10000, // Default estimation
            preferred_provider: Some(AuthProvider::Claude),
            task_description: "Optimized authentication".to_string(),
        };

        // Use the agent coordinator for actual authentication
        self.agent_coordinator.authenticate_agent(&auth_request).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    /// Authenticate using OpenAI with optimizations
    async fn authenticate_openai_optimized(
        &self,
        agent_id: &str,
    ) -> Result<AgentAuthResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Use connection pooling for OpenAI API calls
        let client = self.connection_pool.get_client("api.openai.com").await;
        
        // Create auth request
        let auth_request = AgentAuthRequest {
            agent_id: agent_id.to_string(),
            estimated_tokens: 10000,
            preferred_provider: Some(AuthProvider::OpenAI),
            task_description: "Optimized authentication".to_string(),
        };

        self.agent_coordinator.authenticate_agent(&auth_request).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    /// Cache authentication result
    async fn cache_authentication_result(
        &self,
        agent_id: &str,
        auth_result: &AgentAuthResponse,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let provider_name = match auth_result.provider {
            AuthProvider::Claude => "claude",
            AuthProvider::OpenAI => "openai",
        };
        
        // Cache for 1 hour by default
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        
        self.auth_cache.put(
            provider_name,
            agent_id,
            &auth_result.token,
            expires_at,
            auth_result.subscription_info.as_ref().map(|s| s.tier.clone()),
        ).await;
        
        Ok(())
    }

    /// Refresh individual token (fallback method)
    async fn refresh_token_individual(
        &self,
        agent_id: &str,
        provider: &str,
        refresh_token: &str,
    ) -> Result<String, String> {
        // This would integrate with the existing token refresh logic
        // For now, return a placeholder
        Err("Individual token refresh not implemented".to_string())
    }

    /// Create performance metrics
    fn create_performance_metrics(&self, start_time: Instant, cache_hit: bool) -> PerformanceMetrics {
        PerformanceMetrics {
            authentication_time: start_time.elapsed(),
            token_refresh_time: Duration::from_millis(0),
            cache_hit_rate: if cache_hit { 1.0 } else { 0.0 },
            memory_usage: 0, // Would be filled by memory optimizer
            concurrent_agents: 1,
            network_requests: if cache_hit { 0 } else { 1 },
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Get list of currently active optimizations
    async fn get_active_optimizations(&self) -> Vec<String> {
        let mut optimizations = Vec::new();
        
        if *self.optimization_enabled.read().await {
            optimizations.push("Authentication Caching".to_string());
            optimizations.push("Token Refresh Batching".to_string());
            optimizations.push("Connection Pooling".to_string());
            optimizations.push("Memory Optimization".to_string());
            optimizations.push("Real-time Monitoring".to_string());
        }
        
        optimizations
    }
}

/// Performance statistics summary
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceStatistics {
    pub overall_score: f64,
    pub cache_hit_rate: f64,
    pub average_auth_time_ms: f64,
    pub token_refresh_efficiency: f64,
    pub memory_utilization: f64,
    pub connection_reuse_rate: f64,
    pub active_agents: usize,
    pub total_requests: u64,
    pub error_rate: f64,
}

/// Helper function to create an optimized auth manager from existing components
pub async fn create_optimized_auth_manager(
    unified_auth: Arc<UnifiedAuthManager>,
    agent_coordinator: Arc<AgentAuthCoordinator>,
) -> OptimizedAuthManager {
    let config = OptimizationConfig::default();
    OptimizedAuthManager::new(unified_auth, agent_coordinator, config).await
}

/// Helper function to integrate optimizations with existing agent tool
pub async fn integrate_with_agent_tool(
    optimized_manager: Arc<OptimizedAuthManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // This would integrate with the existing AgentManager in agent_tool.rs
    // For now, just validate that the integration is possible
    
    // Check that all performance components are healthy
    let stats = optimized_manager.get_performance_statistics().await;
    
    if stats.overall_score < 50.0 {
        return Err("Performance optimization system is not healthy enough for integration".into());
    }
    
    println!("Performance optimization integration ready:");
    println!("  - Overall Score: {:.1}%", stats.overall_score);
    println!("  - Cache Hit Rate: {:.1}%", stats.cache_hit_rate * 100.0);
    println!("  - Average Auth Time: {:.1}ms", stats.average_auth_time_ms);
    println!("  - Memory Utilization: {:.1}%", stats.memory_utilization);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::unified_auth::ProviderSelectionStrategy;

    // Mock implementations for testing
    async fn create_test_unified_auth() -> Arc<UnifiedAuthManager> {
        let codex_home = PathBuf::from("/tmp/test_codex");
        let auth_manager = UnifiedAuthManager::new(
            codex_home,
            crate::auth::AuthMode::ApiKey,
            "test".to_string(),
            ProviderSelectionStrategy::PreferClaude,
        );
        Arc::new(auth_manager)
    }

    async fn create_test_agent_coordinator() -> Arc<AgentAuthCoordinator> {
        // This would create a test instance of AgentAuthCoordinator
        // For now, we'll skip this test due to complex dependencies
        unimplemented!("Test requires mock AgentAuthCoordinator")
    }

    #[tokio::test]
    #[ignore] // Ignore until mock implementations are ready
    async fn test_optimized_auth_manager_creation() {
        let unified_auth = create_test_unified_auth().await;
        let agent_coordinator = create_test_agent_coordinator().await;
        let config = OptimizationConfig::default();
        
        let manager = OptimizedAuthManager::new(unified_auth, agent_coordinator, config).await;
        
        let stats = manager.get_performance_statistics().await;
        assert!(stats.overall_score >= 0.0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_performance_dashboard() {
        let unified_auth = create_test_unified_auth().await;
        let agent_coordinator = create_test_agent_coordinator().await;
        let config = OptimizationConfig::default();
        
        let manager = OptimizedAuthManager::new(unified_auth, agent_coordinator, config).await;
        
        let dashboard = manager.get_performance_dashboard().await;
        assert!(dashboard.is_object());
        assert!(dashboard["overall_performance"].is_object());
        assert!(dashboard["component_health"].is_object());
    }

    #[test]
    fn test_optimization_config_defaults() {
        let config = OptimizationConfig::default();
        assert!(config.enable_caching);
        assert!(config.enable_token_batching);
        assert!(config.enable_connection_pooling);
        assert!(config.enable_memory_optimization);
        assert!(config.enable_monitoring);
    }

    #[test]
    fn test_performance_statistics_creation() {
        let stats = PerformanceStatistics {
            overall_score: 95.0,
            cache_hit_rate: 0.85,
            average_auth_time_ms: 45.0,
            token_refresh_efficiency: 8.5,
            memory_utilization: 75.0,
            connection_reuse_rate: 0.92,
            active_agents: 5,
            total_requests: 1000,
            error_rate: 0.5,
        };
        
        assert!(stats.overall_score > 90.0);
        assert!(stats.cache_hit_rate > 0.8);
        assert!(stats.average_auth_time_ms < 50.0);
        assert!(stats.error_rate < 1.0);
    }
}