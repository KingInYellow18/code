// Memory optimization for multi-agent Claude authentication scenarios
// Efficient memory utilization and garbage collection for agent sessions

use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_allocated_bytes: u64,
    pub active_sessions_bytes: u64,
    pub cached_tokens_bytes: u64,
    pub metadata_bytes: u64,
    pub agent_count: usize,
    pub session_count: usize,
    pub cache_entries: usize,
    pub memory_efficiency: f64,
    pub garbage_collection_cycles: u64,
    pub last_gc_duration_ms: u64,
}

/// Memory optimization configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub max_memory_mb: u64,
    pub session_memory_limit_mb: u64,
    pub cache_memory_limit_mb: u64,
    pub gc_threshold_mb: u64,
    pub gc_interval_minutes: u64,
    pub agent_session_timeout_minutes: u64,
    pub weak_reference_cleanup_minutes: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 500,                    // 500MB total limit
            session_memory_limit_mb: 50,           // 50MB per session
            cache_memory_limit_mb: 100,            // 100MB for caching
            gc_threshold_mb: 400,                  // GC when 400MB used
            gc_interval_minutes: 5,                // GC every 5 minutes
            agent_session_timeout_minutes: 30,     // Session timeout 30 minutes
            weak_reference_cleanup_minutes: 10,    // Cleanup weak refs every 10 minutes
        }
    }
}

/// Agent session memory footprint
#[derive(Debug, Clone)]
pub struct AgentSessionMemory {
    pub agent_id: String,
    pub session_id: String,
    pub allocated_bytes: u64,
    pub token_cache_bytes: u64,
    pub metadata_bytes: u64,
    pub created_at: SystemTime,
    pub last_accessed: SystemTime,
    pub access_count: u32,
}

impl AgentSessionMemory {
    fn calculate_total_size(&self) -> u64 {
        self.allocated_bytes + self.token_cache_bytes + self.metadata_bytes
    }
}

/// Memory pool for efficient allocation and deallocation
#[derive(Debug)]
struct MemoryPool {
    allocated_sessions: HashMap<String, AgentSessionMemory>,
    session_references: HashMap<String, Weak<AgentSessionMemory>>,
    size_index: BTreeMap<u64, Vec<String>>, // Size -> Session IDs
    total_allocated: u64,
}

impl MemoryPool {
    fn new() -> Self {
        Self {
            allocated_sessions: HashMap::new(),
            session_references: HashMap::new(),
            size_index: BTreeMap::new(),
            total_allocated: 0,
        }
    }

    fn allocate_session(&mut self, session: AgentSessionMemory) -> Result<(), MemoryError> {
        let session_size = session.calculate_total_size();
        let session_id = session.session_id.clone();

        // Add to main storage
        self.allocated_sessions.insert(session_id.clone(), session);
        
        // Add to size index
        self.size_index
            .entry(session_size)
            .or_insert_with(Vec::new)
            .push(session_id.clone());

        self.total_allocated += session_size;
        
        Ok(())
    }

    fn deallocate_session(&mut self, session_id: &str) -> Option<AgentSessionMemory> {
        if let Some(session) = self.allocated_sessions.remove(session_id) {
            let session_size = session.calculate_total_size();
            
            // Remove from size index
            if let Some(size_list) = self.size_index.get_mut(&session_size) {
                size_list.retain(|id| id != session_id);
                if size_list.is_empty() {
                    self.size_index.remove(&session_size);
                }
            }

            // Remove weak reference
            self.session_references.remove(session_id);
            
            self.total_allocated = self.total_allocated.saturating_sub(session_size);
            Some(session)
        } else {
            None
        }
    }

    fn get_largest_sessions(&self, count: usize) -> Vec<String> {
        let mut result = Vec::new();
        
        // Iterate from largest to smallest
        for (_, session_ids) in self.size_index.iter().rev() {
            for session_id in session_ids {
                result.push(session_id.clone());
                if result.len() >= count {
                    return result;
                }
            }
        }
        
        result
    }

    fn get_oldest_sessions(&self, count: usize) -> Vec<String> {
        let mut sessions: Vec<_> = self.allocated_sessions
            .values()
            .collect();
        
        sessions.sort_by_key(|s| s.last_accessed);
        
        sessions.into_iter()
            .take(count)
            .map(|s| s.session_id.clone())
            .collect()
    }
}

/// Memory optimization engine
#[derive(Debug)]
pub struct MemoryOptimizer {
    config: MemoryConfig,
    memory_pool: Arc<RwLock<MemoryPool>>,
    stats: Arc<RwLock<MemoryStats>>,
    last_gc: Arc<RwLock<Instant>>,
}

impl MemoryOptimizer {
    /// Create new memory optimizer
    pub fn new() -> Self {
        Self::with_config(MemoryConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: MemoryConfig) -> Self {
        Self {
            config,
            memory_pool: Arc::new(RwLock::new(MemoryPool::new())),
            stats: Arc::new(RwLock::new(MemoryStats {
                total_allocated_bytes: 0,
                active_sessions_bytes: 0,
                cached_tokens_bytes: 0,
                metadata_bytes: 0,
                agent_count: 0,
                session_count: 0,
                cache_entries: 0,
                memory_efficiency: 100.0,
                garbage_collection_cycles: 0,
                last_gc_duration_ms: 0,
            })),
            last_gc: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Allocate memory for a new agent session
    pub async fn allocate_agent_session(
        &self,
        agent_id: &str,
        estimated_memory_mb: u64,
    ) -> Result<String, MemoryError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let estimated_bytes = estimated_memory_mb * 1024 * 1024;

        // Check if allocation would exceed limits
        let current_total = {
            let pool_guard = self.memory_pool.read().await;
            pool_guard.total_allocated
        };

        let max_bytes = self.config.max_memory_mb * 1024 * 1024;
        if current_total + estimated_bytes > max_bytes {
            // Try garbage collection first
            self.force_garbage_collection().await?;
            
            // Check again after GC
            let pool_guard = self.memory_pool.read().await;
            if pool_guard.total_allocated + estimated_bytes > max_bytes {
                return Err(MemoryError::OutOfMemory {
                    requested: estimated_bytes,
                    available: max_bytes.saturating_sub(pool_guard.total_allocated),
                });
            }
        }

        // Check session-specific limit
        let session_limit_bytes = self.config.session_memory_limit_mb * 1024 * 1024;
        if estimated_bytes > session_limit_bytes {
            return Err(MemoryError::SessionLimitExceeded {
                requested: estimated_bytes,
                limit: session_limit_bytes,
            });
        }

        // Create session memory tracking
        let session_memory = AgentSessionMemory {
            agent_id: agent_id.to_string(),
            session_id: session_id.clone(),
            allocated_bytes: estimated_bytes,
            token_cache_bytes: 0,
            metadata_bytes: 1024, // Base metadata size
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
        };

        // Allocate in pool
        {
            let mut pool_guard = self.memory_pool.write().await;
            pool_guard.allocate_session(session_memory)?;
        }

        // Update statistics
        self.update_stats().await;

        // Check if GC is needed
        self.maybe_trigger_gc().await;

        Ok(session_id)
    }

    /// Release memory for an agent session
    pub async fn deallocate_agent_session(&self, session_id: &str) -> Result<u64, MemoryError> {
        let released_memory = {
            let mut pool_guard = self.memory_pool.write().await;
            if let Some(session) = pool_guard.deallocate_session(session_id) {
                session.calculate_total_size()
            } else {
                return Err(MemoryError::SessionNotFound(session_id.to_string()));
            }
        };

        // Update statistics
        self.update_stats().await;

        Ok(released_memory)
    }

    /// Update memory usage for a session (e.g., token cache growth)
    pub async fn update_session_memory(
        &self,
        session_id: &str,
        additional_bytes: u64,
        memory_type: MemoryType,
    ) -> Result<(), MemoryError> {
        let mut pool_guard = self.memory_pool.write().await;
        
        if let Some(session) = pool_guard.allocated_sessions.get_mut(session_id) {
            match memory_type {
                MemoryType::TokenCache => session.token_cache_bytes += additional_bytes,
                MemoryType::Metadata => session.metadata_bytes += additional_bytes,
                MemoryType::General => session.allocated_bytes += additional_bytes,
            }
            
            session.last_accessed = SystemTime::now();
            session.access_count += 1;
            pool_guard.total_allocated += additional_bytes;
        } else {
            return Err(MemoryError::SessionNotFound(session_id.to_string()));
        }

        drop(pool_guard);

        // Update statistics
        self.update_stats().await;

        Ok(())
    }

    /// Get current memory statistics
    pub async fn get_stats(&self) -> MemoryStats {
        self.stats.read().await.clone()
    }

    /// Get memory usage for a specific session
    pub async fn get_session_memory(&self, session_id: &str) -> Option<AgentSessionMemory> {
        let pool_guard = self.memory_pool.read().await;
        pool_guard.allocated_sessions.get(session_id).cloned()
    }

    /// Get memory health report
    pub async fn get_health_report(&self) -> MemoryHealthReport {
        let stats = self.get_stats().await;
        let pool_guard = self.memory_pool.read().await;
        
        let memory_utilization = stats.total_allocated_bytes as f64 / 
            (self.config.max_memory_mb * 1024 * 1024) as f64;
        
        let average_session_size = if stats.session_count > 0 {
            stats.active_sessions_bytes / stats.session_count as u64
        } else {
            0
        };

        MemoryHealthReport {
            is_healthy: memory_utilization < 0.8 && stats.memory_efficiency > 70.0,
            memory_utilization,
            average_session_size_mb: average_session_size / (1024 * 1024),
            fragmentation_ratio: self.calculate_fragmentation_ratio(&pool_guard),
            recommendations: Self::generate_memory_recommendations(&stats, memory_utilization),
        }
    }

    /// Force garbage collection
    pub async fn force_garbage_collection(&self) -> Result<MemoryGCResult, MemoryError> {
        let gc_start = Instant::now();
        
        let (removed_sessions, bytes_freed) = {
            let mut pool_guard = self.memory_pool.write().await;
            self.perform_gc(&mut pool_guard).await
        };

        let gc_duration = gc_start.elapsed();

        // Update GC statistics
        {
            let mut stats_guard = self.stats.write().await;
            stats_guard.garbage_collection_cycles += 1;
            stats_guard.last_gc_duration_ms = gc_duration.as_millis() as u64;
        }

        {
            let mut last_gc_guard = self.last_gc.write().await;
            *last_gc_guard = Instant::now();
        }

        // Update overall statistics
        self.update_stats().await;

        Ok(MemoryGCResult {
            sessions_removed: removed_sessions,
            bytes_freed,
            duration_ms: gc_duration.as_millis() as u64,
        })
    }

    /// Check if garbage collection should be triggered
    async fn maybe_trigger_gc(&self) {
        let should_gc = {
            let pool_guard = self.memory_pool.read().await;
            let threshold_bytes = self.config.gc_threshold_mb * 1024 * 1024;
            pool_guard.total_allocated > threshold_bytes
        };

        if should_gc {
            let _ = self.force_garbage_collection().await;
        }
    }

    /// Perform garbage collection
    async fn perform_gc(&self, pool: &mut MemoryPool) -> (usize, u64) {
        let mut removed_sessions = 0;
        let mut bytes_freed = 0;

        // Find expired sessions
        let timeout_duration = Duration::from_secs(self.config.agent_session_timeout_minutes * 60);
        let now = SystemTime::now();
        
        let expired_sessions: Vec<String> = pool
            .allocated_sessions
            .values()
            .filter(|session| {
                now.duration_since(session.last_accessed)
                    .unwrap_or(Duration::ZERO) > timeout_duration
            })
            .map(|session| session.session_id.clone())
            .collect();

        // Remove expired sessions
        for session_id in expired_sessions {
            if let Some(session) = pool.deallocate_session(&session_id) {
                bytes_freed += session.calculate_total_size();
                removed_sessions += 1;
            }
        }

        // If still over threshold, remove largest sessions
        let threshold_bytes = self.config.gc_threshold_mb * 1024 * 1024;
        if pool.total_allocated > threshold_bytes {
            let largest_sessions = pool.get_largest_sessions(5);
            for session_id in largest_sessions {
                if pool.total_allocated <= threshold_bytes {
                    break;
                }
                if let Some(session) = pool.deallocate_session(&session_id) {
                    bytes_freed += session.calculate_total_size();
                    removed_sessions += 1;
                }
            }
        }

        (removed_sessions, bytes_freed)
    }

    /// Update memory statistics
    async fn update_stats(&self) {
        let pool_guard = self.memory_pool.read().await;
        let mut stats_guard = self.stats.write().await;

        stats_guard.total_allocated_bytes = pool_guard.total_allocated;
        stats_guard.session_count = pool_guard.allocated_sessions.len();
        
        // Calculate breakdown
        let mut active_sessions_bytes = 0;
        let mut cached_tokens_bytes = 0;
        let mut metadata_bytes = 0;
        let mut agent_ids = std::collections::HashSet::new();

        for session in pool_guard.allocated_sessions.values() {
            active_sessions_bytes += session.allocated_bytes;
            cached_tokens_bytes += session.token_cache_bytes;
            metadata_bytes += session.metadata_bytes;
            agent_ids.insert(&session.agent_id);
        }

        stats_guard.active_sessions_bytes = active_sessions_bytes;
        stats_guard.cached_tokens_bytes = cached_tokens_bytes;
        stats_guard.metadata_bytes = metadata_bytes;
        stats_guard.agent_count = agent_ids.len();

        // Calculate efficiency
        let max_memory = self.config.max_memory_mb * 1024 * 1024;
        if max_memory > 0 {
            stats_guard.memory_efficiency = 
                ((max_memory - pool_guard.total_allocated) as f64 / max_memory as f64) * 100.0;
        }
    }

    /// Calculate memory fragmentation ratio
    fn calculate_fragmentation_ratio(&self, pool: &MemoryPool) -> f64 {
        if pool.allocated_sessions.is_empty() {
            return 0.0;
        }

        let total_sessions = pool.allocated_sessions.len();
        let size_buckets = pool.size_index.len();
        
        // Higher fragmentation when many different sizes
        size_buckets as f64 / total_sessions as f64
    }

    /// Generate memory optimization recommendations
    fn generate_memory_recommendations(stats: &MemoryStats, utilization: f64) -> Vec<String> {
        let mut recommendations = Vec::new();

        if utilization > 0.9 {
            recommendations.push("Memory utilization is very high - consider increasing memory limits".to_string());
        }

        if stats.memory_efficiency < 50.0 {
            recommendations.push("Low memory efficiency - increase garbage collection frequency".to_string());
        }

        if stats.session_count > 20 {
            recommendations.push("High number of active sessions - consider reducing session timeout".to_string());
        }

        if stats.cached_tokens_bytes > stats.active_sessions_bytes {
            recommendations.push("Token cache is using more memory than sessions - optimize cache size".to_string());
        }

        if stats.last_gc_duration_ms > 5000 {
            recommendations.push("Garbage collection is slow - consider optimizing GC strategy".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("Memory usage is optimized".to_string());
        }

        recommendations
    }

    /// Start background memory management tasks
    pub async fn start_background_tasks(&self) {
        let optimizer = self.clone();
        tokio::spawn(async move {
            let mut gc_interval = tokio::time::interval(
                Duration::from_secs(optimizer.config.gc_interval_minutes * 60)
            );
            
            loop {
                gc_interval.tick().await;
                let _ = optimizer.force_garbage_collection().await;
            }
        });
    }
}

impl Clone for MemoryOptimizer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            memory_pool: Arc::clone(&self.memory_pool),
            stats: Arc::clone(&self.stats),
            last_gc: Arc::clone(&self.last_gc),
        }
    }
}

/// Memory types for tracking
#[derive(Debug, Clone)]
pub enum MemoryType {
    General,
    TokenCache,
    Metadata,
}

/// Memory optimization errors
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Out of memory: requested {requested} bytes, available {available} bytes")]
    OutOfMemory { requested: u64, available: u64 },
    
    #[error("Session memory limit exceeded: requested {requested} bytes, limit {limit} bytes")]
    SessionLimitExceeded { requested: u64, limit: u64 },
    
    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

/// Garbage collection result
#[derive(Debug, Clone, Serialize)]
pub struct MemoryGCResult {
    pub sessions_removed: usize,
    pub bytes_freed: u64,
    pub duration_ms: u64,
}

/// Memory health report
#[derive(Debug, Clone, Serialize)]
pub struct MemoryHealthReport {
    pub is_healthy: bool,
    pub memory_utilization: f64,
    pub average_session_size_mb: u64,
    pub fragmentation_ratio: f64,
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[tokio::test]
    async fn test_memory_optimizer_creation() {
        let optimizer = MemoryOptimizer::new();
        let stats = optimizer.get_stats().await;
        assert_eq!(stats.total_allocated_bytes, 0);
        assert_eq!(stats.session_count, 0);
    }

    #[tokio::test]
    async fn test_session_allocation() {
        let optimizer = MemoryOptimizer::new();
        
        let session_id = optimizer
            .allocate_agent_session("test_agent", 10) // 10MB
            .await
            .unwrap();
        
        assert!(!session_id.is_empty());
        
        let stats = optimizer.get_stats().await;
        assert_eq!(stats.session_count, 1);
        assert!(stats.total_allocated_bytes > 0);
    }

    #[tokio::test]
    async fn test_session_deallocation() {
        let optimizer = MemoryOptimizer::new();
        
        let session_id = optimizer
            .allocate_agent_session("test_agent", 10)
            .await
            .unwrap();
        
        let freed_bytes = optimizer
            .deallocate_agent_session(&session_id)
            .await
            .unwrap();
        
        assert!(freed_bytes > 0);
        
        let stats = optimizer.get_stats().await;
        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.total_allocated_bytes, 0);
    }

    #[tokio::test]
    async fn test_memory_limits() {
        let config = MemoryConfig {
            max_memory_mb: 20, // Very small limit for testing
            session_memory_limit_mb: 10,
            ..Default::default()
        };
        
        let optimizer = MemoryOptimizer::with_config(config);
        
        // First allocation should succeed
        let session1 = optimizer
            .allocate_agent_session("agent1", 8)
            .await
            .unwrap();
        
        // Second allocation should succeed
        let session2 = optimizer
            .allocate_agent_session("agent2", 8)
            .await
            .unwrap();
        
        // Third allocation should fail (would exceed total limit)
        let result = optimizer
            .allocate_agent_session("agent3", 8)
            .await;
        
        assert!(result.is_err());
        
        // Session limit test
        let large_session_result = optimizer
            .allocate_agent_session("large_agent", 15) // Exceeds session limit
            .await;
        
        assert!(large_session_result.is_err());
    }

    #[tokio::test]
    async fn test_memory_update() {
        let optimizer = MemoryOptimizer::new();
        
        let session_id = optimizer
            .allocate_agent_session("test_agent", 10)
            .await
            .unwrap();
        
        let initial_stats = optimizer.get_stats().await;
        let initial_memory = initial_stats.total_allocated_bytes;
        
        // Update memory usage
        optimizer
            .update_session_memory(&session_id, 1024 * 1024, MemoryType::TokenCache)
            .await
            .unwrap();
        
        let updated_stats = optimizer.get_stats().await;
        assert!(updated_stats.total_allocated_bytes > initial_memory);
        assert!(updated_stats.cached_tokens_bytes > 0);
    }

    #[tokio::test]
    async fn test_garbage_collection() {
        let config = MemoryConfig {
            agent_session_timeout_minutes: 0, // Immediate timeout for testing
            ..Default::default()
        };
        
        let optimizer = MemoryOptimizer::with_config(config);
        
        // Allocate some sessions
        let _session1 = optimizer.allocate_agent_session("agent1", 10).await.unwrap();
        let _session2 = optimizer.allocate_agent_session("agent2", 10).await.unwrap();
        
        // Wait a bit to ensure timeout
        sleep(TokioDuration::from_millis(100)).await;
        
        // Force garbage collection
        let gc_result = optimizer.force_garbage_collection().await.unwrap();
        
        assert!(gc_result.sessions_removed > 0);
        assert!(gc_result.bytes_freed > 0);
        
        let stats = optimizer.get_stats().await;
        assert_eq!(stats.session_count, 0);
    }

    #[tokio::test]
    async fn test_health_report() {
        let optimizer = MemoryOptimizer::new();
        
        // Allocate some memory
        let _session = optimizer.allocate_agent_session("test_agent", 10).await.unwrap();
        
        let health_report = optimizer.get_health_report().await;
        assert!(health_report.is_healthy);
        assert!(health_report.memory_utilization > 0.0);
        assert!(!health_report.recommendations.is_empty());
    }
}