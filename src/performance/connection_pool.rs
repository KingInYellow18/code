// Connection pooling for Claude API to optimize network performance
// Reduces connection overhead and improves response times

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use reqwest::Client;
use serde::{Serialize, Deserialize};

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_connections_per_host: usize,
    pub connection_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub idle_timeout_ms: u64,
    pub max_idle_connections: usize,
    pub keep_alive_enabled: bool,
    pub http2_enabled: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_host: 20,   // Max 20 connections per host
            connection_timeout_ms: 5000,    // 5 second connection timeout
            request_timeout_ms: 30000,      // 30 second request timeout  
            idle_timeout_ms: 60000,         // 1 minute idle timeout
            max_idle_connections: 10,       // Max 10 idle connections
            keep_alive_enabled: true,       // Enable HTTP keep-alive
            http2_enabled: true,            // Enable HTTP/2
        }
    }
}

/// Connection pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub connection_reuse_rate: f64,
    pub average_connection_time_ms: f64,
    pub failed_connections: u64,
    pub total_requests: u64,
    pub cache_hits: u64,
}

/// Request statistics
#[derive(Debug, Clone)]
pub struct RequestStats {
    pub start_time: Instant,
    pub connection_time: Option<Duration>,
    pub response_time: Duration,
    pub reused_connection: bool,
}

/// Host-specific connection pool
#[derive(Debug)]
struct HostPool {
    client: Client,
    active_requests: Arc<Semaphore>,
    stats: PoolStats,
    last_used: Instant,
}

/// High-performance connection pool for Claude API
#[derive(Debug)]
pub struct ClaudeConnectionPool {
    config: PoolConfig,
    pools: Arc<RwLock<HashMap<String, HostPool>>>,
    global_stats: Arc<RwLock<PoolStats>>,
}

impl ClaudeConnectionPool {
    /// Create new connection pool with default configuration
    pub fn new() -> Self {
        Self::with_config(PoolConfig::default())
    }

    /// Create connection pool with custom configuration
    pub fn with_config(config: PoolConfig) -> Self {
        Self {
            config,
            pools: Arc::new(RwLock::new(HashMap::new())),
            global_stats: Arc::new(RwLock::new(PoolStats {
                total_connections: 0,
                active_connections: 0,
                idle_connections: 0,
                connection_reuse_rate: 0.0,
                average_connection_time_ms: 0.0,
                failed_connections: 0,
                total_requests: 0,
                cache_hits: 0,
            })),
        }
    }

    /// Get optimized HTTP client for a specific host
    pub async fn get_client(&self, host: &str) -> Client {
        // Check if we already have a pool for this host
        {
            let pools_guard = self.pools.read().await;
            if let Some(host_pool) = pools_guard.get(host) {
                return host_pool.client.clone();
            }
        }

        // Create new client pool for this host
        self.create_host_pool(host).await
    }

    /// Create optimized HTTP client pool for a host
    async fn create_host_pool(&self, host: &str) -> Client {
        let client = Client::builder()
            .timeout(Duration::from_millis(self.config.request_timeout_ms))
            .connect_timeout(Duration::from_millis(self.config.connection_timeout_ms))
            .pool_idle_timeout(Duration::from_millis(self.config.idle_timeout_ms))
            .pool_max_idle_per_host(self.config.max_idle_connections)
            .http2_prior_knowledge()
            .http2_keep_alive_interval(Duration::from_secs(30))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(60))
            .user_agent("Claude-Code-Integration/1.0")
            .build()
            .unwrap_or_else(|_| Client::new());

        let host_pool = HostPool {
            client: client.clone(),
            active_requests: Arc::new(Semaphore::new(self.config.max_connections_per_host)),
            stats: PoolStats {
                total_connections: 1,
                active_connections: 0,
                idle_connections: 1,
                connection_reuse_rate: 0.0,
                average_connection_time_ms: 0.0,
                failed_connections: 0,
                total_requests: 0,
                cache_hits: 0,
            },
            last_used: Instant::now(),
        };

        // Store the pool
        {
            let mut pools_guard = self.pools.write().await;
            pools_guard.insert(host.to_string(), host_pool);
        }

        // Update global stats
        {
            let mut global_stats_guard = self.global_stats.write().await;
            global_stats_guard.total_connections += 1;
            global_stats_guard.idle_connections += 1;
        }

        client
    }

    /// Execute HTTP request with connection pooling and performance tracking
    pub async fn execute_request(
        &self,
        host: &str,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let start_time = Instant::now();

        // Get client and acquire semaphore for rate limiting
        let client = self.get_client(host).await;
        let _permit = {
            let pools_guard = self.pools.read().await;
            if let Some(host_pool) = pools_guard.get(host) {
                Some(host_pool.active_requests.acquire().await.unwrap())
            } else {
                None
            }
        };

        // Update active connections
        self.increment_active_connections().await;

        // Execute request
        let result = request_builder.send().await;
        let request_time = start_time.elapsed();

        // Update statistics
        self.update_request_stats(host, request_time, result.is_ok()).await;

        // Decrement active connections
        self.decrement_active_connections().await;

        result
    }

    /// Make GET request with connection pooling
    pub async fn get(&self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        let host = self.extract_host(url);
        let client = self.get_client(&host).await;
        self.execute_request(&host, client.get(url)).await
    }

    /// Make POST request with connection pooling
    pub async fn post(&self, url: &str, body: impl Into<reqwest::Body>) -> Result<reqwest::Response, reqwest::Error> {
        let host = self.extract_host(url);
        let client = self.get_client(&host).await;
        self.execute_request(&host, client.post(url).body(body)).await
    }

    /// Make authenticated request with token
    pub async fn authenticated_request(
        &self,
        method: reqwest::Method,
        url: &str,
        token: &str,
        body: Option<impl Into<reqwest::Body>>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let host = self.extract_host(url);
        let client = self.get_client(&host).await;
        
        let mut request_builder = client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json");

        if let Some(body) = body {
            request_builder = request_builder.body(body);
        }

        self.execute_request(&host, request_builder).await
    }

    /// Get connection pool statistics
    pub async fn get_stats(&self) -> PoolStats {
        self.global_stats.read().await.clone()
    }

    /// Get host-specific statistics
    pub async fn get_host_stats(&self, host: &str) -> Option<PoolStats> {
        let pools_guard = self.pools.read().await;
        pools_guard.get(host).map(|pool| pool.stats.clone())
    }

    /// Clean up idle connections
    pub async fn cleanup_idle_connections(&self) {
        let idle_threshold = Duration::from_millis(self.config.idle_timeout_ms);
        let now = Instant::now();

        let mut pools_guard = self.pools.write().await;
        let hosts_to_remove: Vec<String> = pools_guard
            .iter()
            .filter(|(_, pool)| now.duration_since(pool.last_used) > idle_threshold)
            .map(|(host, _)| host.clone())
            .collect();

        for host in hosts_to_remove {
            if let Some(removed_pool) = pools_guard.remove(&host) {
                // Update global stats
                let mut global_stats_guard = self.global_stats.write().await;
                global_stats_guard.total_connections -= 1;
                global_stats_guard.idle_connections -= 1;
            }
        }
    }

    /// Get connection pool health report
    pub async fn get_health_report(&self) -> ConnectionPoolHealth {
        let stats = self.get_stats().await;
        let pools_count = self.pools.read().await.len();

        ConnectionPoolHealth {
            is_healthy: stats.connection_reuse_rate > 0.7 && stats.failed_connections < stats.total_requests / 10,
            connection_reuse_rate: stats.connection_reuse_rate,
            average_connection_time_ms: stats.average_connection_time_ms,
            active_pools: pools_count,
            recommendations: Self::generate_pool_recommendations(&stats),
        }
    }

    /// Extract host from URL
    fn extract_host(&self, url: &str) -> String {
        url::Url::parse(url)
            .ok()
            .and_then(|parsed_url| parsed_url.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Update request statistics
    async fn update_request_stats(&self, host: &str, request_time: Duration, success: bool) {
        // Update global stats
        {
            let mut global_stats_guard = self.global_stats.write().await;
            global_stats_guard.total_requests += 1;
            
            if !success {
                global_stats_guard.failed_connections += 1;
            }

            // Update average connection time
            let current_avg = global_stats_guard.average_connection_time_ms;
            let new_time = request_time.as_millis() as f64;
            let total_requests = global_stats_guard.total_requests as f64;
            
            global_stats_guard.average_connection_time_ms = 
                (current_avg * (total_requests - 1.0) + new_time) / total_requests;
        }

        // Update host-specific stats
        {
            let mut pools_guard = self.pools.write().await;
            if let Some(host_pool) = pools_guard.get_mut(host) {
                host_pool.stats.total_requests += 1;
                host_pool.last_used = Instant::now();
                
                if !success {
                    host_pool.stats.failed_connections += 1;
                }
            }
        }
    }

    /// Increment active connections counter
    async fn increment_active_connections(&self) {
        let mut global_stats_guard = self.global_stats.write().await;
        global_stats_guard.active_connections += 1;
    }

    /// Decrement active connections counter
    async fn decrement_active_connections(&self) {
        let mut global_stats_guard = self.global_stats.write().await;
        if global_stats_guard.active_connections > 0 {
            global_stats_guard.active_connections -= 1;
        }
    }

    /// Generate performance recommendations
    fn generate_pool_recommendations(stats: &PoolStats) -> Vec<String> {
        let mut recommendations = Vec::new();

        if stats.connection_reuse_rate < 0.5 {
            recommendations.push("Low connection reuse rate - consider enabling keep-alive".to_string());
        }

        if stats.average_connection_time_ms > 1000.0 {
            recommendations.push("High connection time - check network latency or increase timeout".to_string());
        }

        if stats.failed_connections > stats.total_requests / 20 {
            recommendations.push("High failure rate - check network stability and error handling".to_string());
        }

        if stats.active_connections > 15 {
            recommendations.push("High concurrent connections - consider request queuing".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("Connection pool performance is optimal".to_string());
        }

        recommendations
    }

    /// Start background cleanup task
    pub async fn start_cleanup_task(&self) {
        let pool = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                pool.cleanup_idle_connections().await;
            }
        });
    }
}

impl Clone for ClaudeConnectionPool {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            pools: Arc::clone(&self.pools),
            global_stats: Arc::clone(&self.global_stats),
        }
    }
}

/// Connection pool health report
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionPoolHealth {
    pub is_healthy: bool,
    pub connection_reuse_rate: f64,
    pub average_connection_time_ms: f64,
    pub active_pools: usize,
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_connection_pool_creation() {
        let pool = ClaudeConnectionPool::new();
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_connections, 0);
    }

    #[tokio::test]
    async fn test_client_creation_and_reuse() {
        let pool = ClaudeConnectionPool::new();
        
        let client1 = pool.get_client("api.anthropic.com").await;
        let client2 = pool.get_client("api.anthropic.com").await;
        
        // Should reuse the same client for the same host
        // Note: We can't directly compare Client instances, but we can check pool stats
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_connections, 1); // Only one pool created
    }

    #[tokio::test]
    async fn test_multiple_hosts() {
        let pool = ClaudeConnectionPool::new();
        
        let _client1 = pool.get_client("api.anthropic.com").await;
        let _client2 = pool.get_client("api.openai.com").await;
        
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_connections, 2); // Two pools for different hosts
    }

    #[tokio::test]
    async fn test_url_host_extraction() {
        let pool = ClaudeConnectionPool::new();
        
        let host = pool.extract_host("https://api.anthropic.com/v1/messages");
        assert_eq!(host, "api.anthropic.com");
        
        let host = pool.extract_host("http://localhost:8080/test");
        assert_eq!(host, "localhost");
    }

    #[tokio::test]
    async fn test_pool_configuration() {
        let config = PoolConfig {
            max_connections_per_host: 5,
            connection_timeout_ms: 1000,
            ..Default::default()
        };
        
        let pool = ClaudeConnectionPool::with_config(config.clone());
        assert_eq!(pool.config.max_connections_per_host, 5);
        assert_eq!(pool.config.connection_timeout_ms, 1000);
    }

    #[tokio::test]
    async fn test_cleanup_idle_connections() {
        let mut config = PoolConfig::default();
        config.idle_timeout_ms = 100; // Very short timeout for testing
        
        let pool = ClaudeConnectionPool::with_config(config);
        
        // Create a client
        let _client = pool.get_client("test.example.com").await;
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Cleanup should remove the idle connection
        pool.cleanup_idle_connections().await;
        
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_connections, 0);
    }
}