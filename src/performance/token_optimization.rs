// Token refresh optimization for Claude authentication
// Implements batching and intelligent refresh strategies

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex, Semaphore};
use tokio::time::{sleep, interval};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Token refresh request
#[derive(Debug, Clone)]
pub struct TokenRefreshRequest {
    pub request_id: String,
    pub provider: String,
    pub user_id: String,
    pub refresh_token: String,
    pub priority: RefreshPriority,
    pub requested_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
}

/// Priority levels for token refresh
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefreshPriority {
    Low,        // Background refresh
    Normal,     // Standard refresh
    High,       // User-initiated action
    Critical,   // Token about to expire
}

/// Token refresh result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRefreshResult {
    pub request_id: String,
    pub success: bool,
    pub new_token: Option<String>,
    pub new_refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub refresh_time_ms: u64,
}

/// Batch refresh configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub batch_timeout_ms: u64,
    pub max_concurrent_batches: usize,
    pub retry_attempts: u32,
    pub backoff_base_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,         // Max 10 refreshes per batch
            batch_timeout_ms: 500,      // 500ms batch timeout
            max_concurrent_batches: 3,  // Max 3 concurrent batches
            retry_attempts: 3,          // Retry up to 3 times
            backoff_base_ms: 1000,     // 1 second base backoff
        }
    }
}

/// Token refresh statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshStats {
    pub total_requests: u64,
    pub successful_refreshes: u64,
    pub failed_refreshes: u64,
    pub average_refresh_time_ms: f64,
    pub batch_efficiency: f64,
    pub cache_saves: u64,
    pub concurrent_batches: usize,
}

/// Token refresh optimizer with batching and intelligent scheduling
#[derive(Debug)]
pub struct TokenOptimizer {
    config: BatchConfig,
    pending_refreshes: Arc<RwLock<VecDeque<TokenRefreshRequest>>>,
    in_progress: Arc<RwLock<HashMap<String, Instant>>>,
    results: Arc<RwLock<HashMap<String, TokenRefreshResult>>>,
    stats: Arc<RwLock<RefreshStats>>,
    batch_semaphore: Arc<Semaphore>,
    client: reqwest::Client,
}

impl TokenOptimizer {
    /// Create new token optimizer
    pub fn new() -> Self {
        Self::with_config(BatchConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: BatchConfig) -> Self {
        let batch_semaphore = Arc::new(Semaphore::new(config.max_concurrent_batches));
        
        Self {
            config,
            pending_refreshes: Arc::new(RwLock::new(VecDeque::new())),
            in_progress: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RefreshStats {
                total_requests: 0,
                successful_refreshes: 0,
                failed_refreshes: 0,
                average_refresh_time_ms: 0.0,
                batch_efficiency: 0.0,
                cache_saves: 0,
                concurrent_batches: 0,
            })),
            batch_semaphore,
            client: reqwest::Client::new(),
        }
    }

    /// Start the token optimizer background processing
    pub async fn start(&self) {
        let optimizer = self.clone();
        tokio::spawn(async move {
            optimizer.run_batch_processor().await;
        });
    }

    /// Request token refresh with intelligent batching
    pub async fn request_refresh(
        &self,
        provider: &str,
        user_id: &str,
        refresh_token: &str,
        priority: RefreshPriority,
        deadline: Option<DateTime<Utc>>,
    ) -> String {
        let request_id = uuid::Uuid::new_v4().to_string();
        
        let request = TokenRefreshRequest {
            request_id: request_id.clone(),
            provider: provider.to_string(),
            user_id: user_id.to_string(),
            refresh_token: refresh_token.to_string(),
            priority,
            requested_at: Utc::now(),
            deadline,
        };

        // Add to pending queue with priority ordering
        {
            let mut pending_guard = self.pending_refreshes.write().await;
            
            // Insert based on priority (higher priority goes first)
            let insert_position = pending_guard
                .iter()
                .position(|r| r.priority < request.priority)
                .unwrap_or(pending_guard.len());
            
            pending_guard.insert(insert_position, request);
            
            let mut stats_guard = self.stats.write().await;
            stats_guard.total_requests += 1;
        }

        request_id
    }

    /// Get refresh result (non-blocking)
    pub async fn get_result(&self, request_id: &str) -> Option<TokenRefreshResult> {
        let results_guard = self.results.read().await;
        results_guard.get(request_id).cloned()
    }

    /// Wait for refresh result with timeout
    pub async fn wait_for_result(&self, request_id: &str, timeout: Duration) -> Option<TokenRefreshResult> {
        let start = Instant::now();
        
        while start.elapsed() < timeout {
            if let Some(result) = self.get_result(request_id).await {
                return Some(result);
            }
            
            // Check if still in progress
            {
                let in_progress_guard = self.in_progress.read().await;
                if !in_progress_guard.contains_key(request_id) {
                    // Not in progress and no result = failed or not started
                    break;
                }
            }
            
            sleep(Duration::from_millis(10)).await;
        }
        
        None
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> RefreshStats {
        let stats_guard = self.stats.read().await;
        let mut stats = stats_guard.clone();
        
        // Update concurrent batches count
        let in_progress_guard = self.in_progress.read().await;
        stats.concurrent_batches = in_progress_guard.len();
        
        stats
    }

    /// Main batch processing loop
    async fn run_batch_processor(&self) {
        let mut interval = interval(Duration::from_millis(self.config.batch_timeout_ms));
        
        loop {
            interval.tick().await;
            
            // Process pending refreshes in batches
            if let Some(batch) = self.create_batch().await {
                let optimizer = self.clone();
                tokio::spawn(async move {
                    optimizer.process_batch(batch).await;
                });
            }
        }
    }

    /// Create a batch from pending refreshes
    async fn create_batch(&self) -> Option<Vec<TokenRefreshRequest>> {
        let mut pending_guard = self.pending_refreshes.write().await;
        
        if pending_guard.is_empty() {
            return None;
        }

        let batch_size = std::cmp::min(self.config.max_batch_size, pending_guard.len());
        let mut batch = Vec::with_capacity(batch_size);
        
        // Take highest priority requests first
        for _ in 0..batch_size {
            if let Some(request) = pending_guard.pop_front() {
                batch.push(request);
            }
        }

        if batch.is_empty() {
            None
        } else {
            Some(batch)
        }
    }

    /// Process a batch of token refresh requests
    async fn process_batch(&self, batch: Vec<TokenRefreshRequest>) {
        // Acquire batch processing semaphore
        let _permit = self.batch_semaphore.acquire().await.unwrap();
        
        let batch_start = Instant::now();
        let batch_id = uuid::Uuid::new_v4().to_string();
        
        // Mark all requests as in progress
        {
            let mut in_progress_guard = self.in_progress.write().await;
            for request in &batch {
                in_progress_guard.insert(request.request_id.clone(), batch_start);
            }
        }

        // Group requests by provider for optimal API usage
        let mut provider_groups: HashMap<String, Vec<&TokenRefreshRequest>> = HashMap::new();
        for request in &batch {
            provider_groups
                .entry(request.provider.clone())
                .or_insert_with(Vec::new)
                .push(request);
        }

        // Process each provider group
        let mut batch_results = Vec::new();
        for (provider, requests) in provider_groups {
            let group_results = self.process_provider_group(&provider, requests).await;
            batch_results.extend(group_results);
        }

        // Store results and clean up progress tracking
        {
            let mut results_guard = self.results.write().await;
            let mut in_progress_guard = self.in_progress.write().await;
            
            for result in &batch_results {
                results_guard.insert(result.request_id.clone(), result.clone());
                in_progress_guard.remove(&result.request_id);
            }
        }

        // Update statistics
        self.update_batch_stats(&batch_results, batch_start.elapsed()).await;
    }

    /// Process a group of requests for the same provider
    async fn process_provider_group(&self, provider: &str, requests: Vec<&TokenRefreshRequest>) -> Vec<TokenRefreshResult> {
        let mut results = Vec::new();
        
        // Process requests with retry logic
        for request in requests {
            let result = self.refresh_token_with_retry(request).await;
            results.push(result);
        }
        
        results
    }

    /// Refresh a single token with retry logic
    async fn refresh_token_with_retry(&self, request: &TokenRefreshRequest) -> TokenRefreshResult {
        let mut attempts = 0;
        let mut last_error = None;
        
        while attempts < self.config.retry_attempts {
            let start = Instant::now();
            
            match self.refresh_token(request).await {
                Ok(result) => {
                    return TokenRefreshResult {
                        request_id: request.request_id.clone(),
                        success: true,
                        new_token: Some(result.access_token),
                        new_refresh_token: Some(result.refresh_token),
                        expires_at: Some(result.expires_at),
                        error: None,
                        refresh_time_ms: start.elapsed().as_millis() as u64,
                    };
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    attempts += 1;
                    
                    if attempts < self.config.retry_attempts {
                        // Exponential backoff
                        let delay = Duration::from_millis(
                            self.config.backoff_base_ms * (2_u64.pow(attempts - 1))
                        );
                        sleep(delay).await;
                    }
                }
            }
        }
        
        // All attempts failed
        TokenRefreshResult {
            request_id: request.request_id.clone(),
            success: false,
            new_token: None,
            new_refresh_token: None,
            expires_at: None,
            error: last_error,
            refresh_time_ms: 0,
        }
    }

    /// Perform actual token refresh API call
    async fn refresh_token(&self, request: &TokenRefreshRequest) -> Result<RefreshTokenResponse, Box<dyn std::error::Error + Send + Sync>> {
        match request.provider.as_str() {
            "claude" => self.refresh_claude_token(request).await,
            "openai" => self.refresh_openai_token(request).await,
            _ => Err(format!("Unsupported provider: {}", request.provider).into()),
        }
    }

    /// Refresh Claude token
    async fn refresh_claude_token(&self, request: &TokenRefreshRequest) -> Result<RefreshTokenResponse, Box<dyn std::error::Error + Send + Sync>> {
        let refresh_request = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": request.refresh_token,
        });

        let response = self.client
            .post("https://api.anthropic.com/v1/oauth/token") // Placeholder URL
            .json(&refresh_request)
            .send()
            .await?;

        if response.status().is_success() {
            let token_response: ClaudeTokenResponse = response.json().await?;
            Ok(RefreshTokenResponse {
                access_token: token_response.access_token,
                refresh_token: token_response.refresh_token,
                expires_at: Utc::now() + chrono::Duration::seconds(token_response.expires_in as i64),
            })
        } else {
            Err(format!("Claude token refresh failed: {}", response.status()).into())
        }
    }

    /// Refresh OpenAI token
    async fn refresh_openai_token(&self, request: &TokenRefreshRequest) -> Result<RefreshTokenResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Implementation for OpenAI token refresh
        // This would integrate with existing OpenAI refresh logic
        Err("OpenAI token refresh not implemented in optimizer".into())
    }

    /// Update batch processing statistics
    async fn update_batch_stats(&self, results: &[TokenRefreshResult], batch_duration: Duration) {
        let mut stats_guard = self.stats.write().await;
        
        let successful_count = results.iter().filter(|r| r.success).count() as u64;
        let failed_count = results.len() as u64 - successful_count;
        
        stats_guard.successful_refreshes += successful_count;
        stats_guard.failed_refreshes += failed_count;
        
        // Update average refresh time
        let total_refresh_time: u64 = results.iter().map(|r| r.refresh_time_ms).sum();
        let current_avg = stats_guard.average_refresh_time_ms;
        let total_requests = stats_guard.total_requests as f64;
        
        stats_guard.average_refresh_time_ms = 
            (current_avg * (total_requests - results.len() as f64) + total_refresh_time as f64) / total_requests;
        
        // Update batch efficiency (requests per second)
        let batch_requests_per_second = results.len() as f64 / batch_duration.as_secs_f64();
        stats_guard.batch_efficiency = 
            (stats_guard.batch_efficiency + batch_requests_per_second) / 2.0; // Simple moving average
    }
}

impl Clone for TokenOptimizer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            pending_refreshes: Arc::clone(&self.pending_refreshes),
            in_progress: Arc::clone(&self.in_progress),
            results: Arc::clone(&self.results),
            stats: Arc::clone(&self.stats),
            batch_semaphore: Arc::clone(&self.batch_semaphore),
            client: self.client.clone(),
        }
    }
}

/// Internal refresh token response
#[derive(Debug)]
struct RefreshTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

/// Claude token response format
#[derive(Debug, Deserialize)]
struct ClaudeTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[tokio::test]
    async fn test_token_optimizer_creation() {
        let optimizer = TokenOptimizer::new();
        let stats = optimizer.get_stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_request_queuing() {
        let optimizer = TokenOptimizer::new();
        
        let request_id = optimizer.request_refresh(
            "claude",
            "test_user",
            "refresh_token_123",
            RefreshPriority::Normal,
            None,
        ).await;
        
        assert!(!request_id.is_empty());
        
        let stats = optimizer.get_stats().await;
        assert_eq!(stats.total_requests, 1);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let optimizer = TokenOptimizer::new();
        
        // Add requests with different priorities
        let _low = optimizer.request_refresh("claude", "user1", "token1", RefreshPriority::Low, None).await;
        let _high = optimizer.request_refresh("claude", "user2", "token2", RefreshPriority::High, None).await;
        let _critical = optimizer.request_refresh("claude", "user3", "token3", RefreshPriority::Critical, None).await;
        
        // Create a batch - should have critical first, then high, then low
        let batch = optimizer.create_batch().await.unwrap();
        assert_eq!(batch[0].priority, RefreshPriority::Critical);
        assert_eq!(batch[1].priority, RefreshPriority::High);
        assert_eq!(batch[2].priority, RefreshPriority::Low);
    }

    #[tokio::test]
    async fn test_batch_size_limit() {
        let mut config = BatchConfig::default();
        config.max_batch_size = 2;
        
        let optimizer = TokenOptimizer::with_config(config);
        
        // Add 5 requests
        for i in 0..5 {
            optimizer.request_refresh(
                "claude",
                &format!("user{}", i),
                &format!("token{}", i),
                RefreshPriority::Normal,
                None,
            ).await;
        }
        
        // Batch should be limited to 2
        let batch = optimizer.create_batch().await.unwrap();
        assert_eq!(batch.len(), 2);
        
        // Remaining requests should still be pending
        let remaining_batch = optimizer.create_batch().await.unwrap();
        assert_eq!(remaining_batch.len(), 2);
    }
}