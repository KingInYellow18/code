// Authentication caching system for Claude integration
// Target: < 100ms cached authentication as per Phase 5 requirements

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

/// Cached authentication result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAuth {
    pub provider: String,
    pub user_id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub subscription_tier: Option<String>,
    pub cached_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
}

/// Cache performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hit_rate: f64,
    pub miss_rate: f64,
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub average_lookup_time_ms: f64,
    pub cache_size: usize,
    pub max_cache_size: usize,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_size: usize,
    pub ttl_minutes: u64,
    pub cleanup_interval_minutes: u64,
    pub preemptive_refresh_threshold_minutes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,                    // Max 1000 cached auth entries
            ttl_minutes: 60,                   // 1 hour TTL
            cleanup_interval_minutes: 10,      // Cleanup every 10 minutes
            preemptive_refresh_threshold_minutes: 5, // Refresh 5 minutes before expiry
        }
    }
}

/// High-performance authentication cache with sub-100ms lookup target
#[derive(Debug)]
pub struct AuthenticationCache {
    cache: Arc<RwLock<HashMap<String, CachedAuth>>>,
    config: CacheConfig,
    stats: Arc<RwLock<CacheStats>>,
    last_cleanup: Arc<RwLock<Instant>>,
}

impl AuthenticationCache {
    /// Create new authentication cache
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create cache with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats {
                hit_rate: 0.0,
                miss_rate: 0.0,
                total_requests: 0,
                cache_hits: 0,
                cache_misses: 0,
                evictions: 0,
                average_lookup_time_ms: 0.0,
                cache_size: 0,
                max_cache_size: 1000,
            })),
            last_cleanup: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Generate cache key for authentication request
    fn generate_cache_key(provider: &str, user_identifier: &str) -> String {
        format!("{}:{}", provider, user_identifier)
    }

    /// Get cached authentication (target: < 100ms)
    pub async fn get(&self, provider: &str, user_identifier: &str) -> Option<CachedAuth> {
        let start = Instant::now();
        
        // Check if cleanup is needed (non-blocking)
        self.maybe_cleanup().await;

        let cache_key = Self::generate_cache_key(provider, user_identifier);
        
        let result = {
            let cache_guard = self.cache.read().await;
            cache_guard.get(&cache_key).cloned()
        };

        // Update statistics
        let lookup_time = start.elapsed();
        self.update_stats(result.is_some(), lookup_time).await;

        // Check if cached auth is still valid
        match result {
            Some(mut cached_auth) => {
                let now = Utc::now();
                
                // Check if token is expired
                if cached_auth.expires_at <= now {
                    // Remove expired entry
                    self.remove(provider, user_identifier).await;
                    return None;
                }

                // Update last accessed time
                cached_auth.last_accessed = now;
                cached_auth.access_count += 1;
                
                // Update in cache
                {
                    let mut cache_guard = self.cache.write().await;
                    cache_guard.insert(cache_key, cached_auth.clone());
                }

                Some(cached_auth)
            }
            None => None,
        }
    }

    /// Cache authentication result
    pub async fn put(
        &self,
        provider: &str,
        user_identifier: &str,
        token: &str,
        expires_at: DateTime<Utc>,
        subscription_tier: Option<String>,
    ) {
        let cache_key = Self::generate_cache_key(provider, user_identifier);
        let now = Utc::now();

        let cached_auth = CachedAuth {
            provider: provider.to_string(),
            user_id: user_identifier.to_string(),
            token: token.to_string(),
            expires_at,
            subscription_tier,
            cached_at: now,
            last_accessed: now,
            access_count: 0,
        };

        // Check if we need to evict entries to make space
        let should_evict = {
            let cache_guard = self.cache.read().await;
            cache_guard.len() >= self.config.max_size
        };

        if should_evict {
            self.evict_lru().await;
        }

        // Insert new entry
        {
            let mut cache_guard = self.cache.write().await;
            cache_guard.insert(cache_key, cached_auth);
        }

        // Update cache size in stats
        {
            let mut stats_guard = self.stats.write().await;
            let cache_guard = self.cache.read().await;
            stats_guard.cache_size = cache_guard.len();
        }
    }

    /// Remove cached authentication
    pub async fn remove(&self, provider: &str, user_identifier: &str) {
        let cache_key = Self::generate_cache_key(provider, user_identifier);
        
        let mut cache_guard = self.cache.write().await;
        cache_guard.remove(&cache_key);
        
        let mut stats_guard = self.stats.write().await;
        stats_guard.cache_size = cache_guard.len();
    }

    /// Clear all cached authentications
    pub async fn clear(&self) {
        let mut cache_guard = self.cache.write().await;
        cache_guard.clear();
        
        let mut stats_guard = self.stats.write().await;
        stats_guard.cache_size = 0;
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    /// Check if authentication should be preemptively refreshed
    pub async fn should_refresh(&self, provider: &str, user_identifier: &str) -> bool {
        if let Some(cached_auth) = self.get(provider, user_identifier).await {
            let now = Utc::now();
            let refresh_threshold = chrono::Duration::minutes(self.config.preemptive_refresh_threshold_minutes as i64);
            
            cached_auth.expires_at <= now + refresh_threshold
        } else {
            false
        }
    }

    /// Get all cached authentications that need refresh
    pub async fn get_refresh_candidates(&self) -> Vec<(String, String)> {
        let cache_guard = self.cache.read().await;
        let now = Utc::now();
        let refresh_threshold = chrono::Duration::minutes(self.config.preemptive_refresh_threshold_minutes as i64);

        cache_guard
            .values()
            .filter(|auth| auth.expires_at <= now + refresh_threshold)
            .map(|auth| (auth.provider.clone(), auth.user_id.clone()))
            .collect()
    }

    /// Update cache statistics
    async fn update_stats(&self, was_hit: bool, lookup_time: Duration) {
        let mut stats_guard = self.stats.write().await;
        
        stats_guard.total_requests += 1;
        
        if was_hit {
            stats_guard.cache_hits += 1;
        } else {
            stats_guard.cache_misses += 1;
        }

        // Update hit/miss rates
        stats_guard.hit_rate = stats_guard.cache_hits as f64 / stats_guard.total_requests as f64;
        stats_guard.miss_rate = stats_guard.cache_misses as f64 / stats_guard.total_requests as f64;

        // Update average lookup time
        let current_avg = stats_guard.average_lookup_time_ms;
        let new_time_ms = lookup_time.as_millis() as f64;
        stats_guard.average_lookup_time_ms = 
            (current_avg * (stats_guard.total_requests - 1) as f64 + new_time_ms) / stats_guard.total_requests as f64;
    }

    /// Evict least recently used entry
    async fn evict_lru(&self) {
        let mut cache_guard = self.cache.write().await;
        
        if let Some((key_to_remove, _)) = cache_guard
            .iter()
            .min_by_key(|(_, auth)| auth.last_accessed)
            .map(|(k, v)| (k.clone(), v.clone()))
        {
            cache_guard.remove(&key_to_remove);
            
            let mut stats_guard = self.stats.write().await;
            stats_guard.evictions += 1;
        }
    }

    /// Cleanup expired entries if needed
    async fn maybe_cleanup(&self) {
        let should_cleanup = {
            let last_cleanup_guard = self.last_cleanup.read().await;
            last_cleanup_guard.elapsed() > Duration::from_secs(self.config.cleanup_interval_minutes * 60)
        };

        if should_cleanup {
            self.cleanup_expired().await;
            
            let mut last_cleanup_guard = self.last_cleanup.write().await;
            *last_cleanup_guard = Instant::now();
        }
    }

    /// Remove all expired entries
    async fn cleanup_expired(&self) {
        let now = Utc::now();
        let mut cache_guard = self.cache.write().await;
        
        let expired_keys: Vec<String> = cache_guard
            .iter()
            .filter(|(_, auth)| auth.expires_at <= now)
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            cache_guard.remove(&key);
        }

        let mut stats_guard = self.stats.write().await;
        stats_guard.cache_size = cache_guard.len();
    }

    /// Get cache health report
    pub async fn get_health_report(&self) -> CacheHealthReport {
        let stats = self.get_stats().await;
        let now = Instant::now();
        
        CacheHealthReport {
            is_healthy: stats.hit_rate > 0.8 && stats.average_lookup_time_ms < 100.0,
            hit_rate: stats.hit_rate,
            average_lookup_time_ms: stats.average_lookup_time_ms,
            cache_utilization: stats.cache_size as f64 / stats.max_cache_size as f64,
            recommendations: Self::generate_recommendations(&stats),
        }
    }

    /// Generate performance recommendations
    fn generate_recommendations(stats: &CacheStats) -> Vec<String> {
        let mut recommendations = Vec::new();

        if stats.hit_rate < 0.7 {
            recommendations.push("Low cache hit rate - consider increasing TTL or cache size".to_string());
        }

        if stats.average_lookup_time_ms > 50.0 {
            recommendations.push("Cache lookup time is high - consider optimizing cache structure".to_string());
        }

        if stats.cache_size as f64 / stats.max_cache_size as f64 > 0.9 {
            recommendations.push("Cache is near capacity - consider increasing max_size".to_string());
        }

        if stats.evictions > stats.total_requests / 10 {
            recommendations.push("High eviction rate - increase cache size or reduce TTL".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("Cache performance is optimal".to_string());
        }

        recommendations
    }
}

/// Cache health report
#[derive(Debug, Clone, Serialize)]
pub struct CacheHealthReport {
    pub is_healthy: bool,
    pub hit_rate: f64,
    pub average_lookup_time_ms: f64,
    pub cache_utilization: f64,
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = AuthenticationCache::new();
        let stats = cache.get_stats().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.cache_size, 0);
    }

    #[tokio::test]
    async fn test_cache_put_get() {
        let cache = AuthenticationCache::new();
        let expires_at = Utc::now() + chrono::Duration::hours(1);

        cache.put("claude", "test_user", "test_token", expires_at, Some("max".to_string())).await;

        let result = cache.get("claude", "test_user").await;
        assert!(result.is_some());
        
        let cached_auth = result.unwrap();
        assert_eq!(cached_auth.provider, "claude");
        assert_eq!(cached_auth.user_id, "test_user");
        assert_eq!(cached_auth.token, "test_token");
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = AuthenticationCache::new();
        let expires_at = Utc::now() - chrono::Duration::minutes(1); // Expired

        cache.put("claude", "test_user", "test_token", expires_at, None).await;

        let result = cache.get("claude", "test_user").await;
        assert!(result.is_none()); // Should be None due to expiration
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = AuthenticationCache::new();
        let expires_at = Utc::now() + chrono::Duration::hours(1);

        // Cache miss
        let result = cache.get("claude", "test_user").await;
        assert!(result.is_none());

        // Cache put
        cache.put("claude", "test_user", "test_token", expires_at, None).await;

        // Cache hit
        let result = cache.get("claude", "test_user").await;
        assert!(result.is_some());

        let stats = cache.get_stats().await;
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }

    #[tokio::test]
    async fn test_cache_performance_target() {
        let cache = AuthenticationCache::new();
        let expires_at = Utc::now() + chrono::Duration::hours(1);

        // Put multiple entries
        for i in 0..100 {
            cache.put("claude", &format!("user_{}", i), &format!("token_{}", i), expires_at, None).await;
        }

        // Test lookup performance
        let start = Instant::now();
        for i in 0..100 {
            let result = cache.get("claude", &format!("user_{}", i)).await;
            assert!(result.is_some());
        }
        let total_time = start.elapsed();
        let avg_time_per_lookup = total_time.as_millis() / 100;

        // Should be well under 100ms target per lookup
        assert!(avg_time_per_lookup < 10, "Average lookup time {} ms exceeds performance expectations", avg_time_per_lookup);
    }

    #[tokio::test]
    async fn test_preemptive_refresh() {
        let mut config = CacheConfig::default();
        config.preemptive_refresh_threshold_minutes = 10; // 10 minutes threshold
        
        let cache = AuthenticationCache::with_config(config);
        let expires_at = Utc::now() + chrono::Duration::minutes(5); // Expires in 5 minutes

        cache.put("claude", "test_user", "test_token", expires_at, None).await;

        let should_refresh = cache.should_refresh("claude", "test_user").await;
        assert!(should_refresh); // Should recommend refresh since it expires in 5 minutes
    }
}