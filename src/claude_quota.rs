use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU16, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashSet;
use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Claude authentication type for quota management
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClaudeAuthType {
    /// Claude Max subscription with daily limits
    Max { daily_limit: u64 },
    /// API key with rate limits
    ApiKey { 
        requests_per_minute: u16,
        tokens_per_minute: u32,
    },
}

/// Claude quota allocation for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQuotaAllocation {
    pub agent_id: String,
    pub auth_type: String, // Serialized ClaudeAuthType
    pub allocated_at: DateTime<Utc>,
    pub max_requests_per_minute: u16,
    pub max_tokens_allocated: u32,
    pub current_usage: u64,
}

/// Agent session tracking for coordination
#[derive(Debug, Clone)]
pub struct AgentSession {
    pub agent_id: String,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub estimated_tokens_used: u64,
    pub requests_made: u32,
}

/// Main Claude quota manager for multi-agent coordination
pub struct ClaudeQuotaManager {
    /// Current total usage across all agents
    current_usage: Arc<AtomicU64>,
    
    /// Daily usage limit (for Claude Max)
    daily_limit: u64,
    
    /// Maximum concurrent agents allowed
    concurrent_limit: u16,
    
    /// Set of currently active agent IDs
    active_agents: Arc<RwLock<HashSet<String>>>,
    
    /// Detailed agent sessions for coordination
    agent_sessions: Arc<RwLock<HashMap<String, AgentSession>>>,
    
    /// Per-agent quota allocations
    quota_allocations: Arc<RwLock<HashMap<String, AgentQuotaAllocation>>>,
    
    /// Authentication type configuration
    auth_type: ClaudeAuthType,
    
    /// Rate limiting: requests per minute counter
    requests_this_minute: Arc<AtomicU16>,
    last_minute_reset: Arc<RwLock<DateTime<Utc>>>,
}

impl ClaudeQuotaManager {
    /// Create new quota manager with Claude Max subscription limits
    pub fn new_max_subscription(daily_limit: u64, concurrent_limit: u16) -> Self {
        Self {
            current_usage: Arc::new(AtomicU64::new(0)),
            daily_limit,
            concurrent_limit,
            active_agents: Arc::new(RwLock::new(HashSet::new())),
            agent_sessions: Arc::new(RwLock::new(HashMap::new())),
            quota_allocations: Arc::new(RwLock::new(HashMap::new())),
            auth_type: ClaudeAuthType::Max { daily_limit },
            requests_this_minute: Arc::new(AtomicU16::new(0)),
            last_minute_reset: Arc::new(RwLock::new(Utc::now())),
        }
    }
    
    /// Create new quota manager with API key rate limits
    pub fn new_api_key(requests_per_minute: u16, tokens_per_minute: u32, concurrent_limit: u16) -> Self {
        Self {
            current_usage: Arc::new(AtomicU64::new(0)),
            daily_limit: u64::MAX, // No daily limit for API keys
            concurrent_limit,
            active_agents: Arc::new(RwLock::new(HashSet::new())),
            agent_sessions: Arc::new(RwLock::new(HashMap::new())),
            quota_allocations: Arc::new(RwLock::new(HashMap::new())),
            auth_type: ClaudeAuthType::ApiKey { requests_per_minute, tokens_per_minute },
            requests_this_minute: Arc::new(AtomicU16::new(0)),
            last_minute_reset: Arc::new(RwLock::new(Utc::now())),
        }
    }
    
    /// Check if we can allocate quota for a new agent
    pub async fn can_allocate_agent(&self) -> Result<bool, String> {
        let active_count = self.active_agents.read().await.len();
        
        if active_count >= self.concurrent_limit as usize {
            return Ok(false);
        }
        
        // Check daily usage for Claude Max
        match &self.auth_type {
            ClaudeAuthType::Max { daily_limit } => {
                let current = self.current_usage.load(Ordering::Relaxed);
                Ok(current < *daily_limit)
            }
            ClaudeAuthType::ApiKey { .. } => {
                // API keys don't have daily limits, just rate limits
                Ok(true)
            }
        }
    }
    
    /// Allocate quota for a new agent session
    pub async fn allocate_agent_quota(&self, agent_id: &str) -> Result<AgentQuotaAllocation, String> {
        if !self.can_allocate_agent().await? {
            return Err("Cannot allocate quota: limits reached".to_string());
        }
        
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        
        // Calculate per-agent allocation based on auth type and concurrent limit
        let (max_requests_per_minute, max_tokens_allocated, auth_type_str) = match &self.auth_type {
            ClaudeAuthType::Max { daily_limit } => {
                let tokens_per_agent = daily_limit / (self.concurrent_limit as u64).max(1);
                (60u16, tokens_per_agent as u32, "max".to_string())
            }
            ClaudeAuthType::ApiKey { requests_per_minute, tokens_per_minute } => {
                let req_per_agent = requests_per_minute / self.concurrent_limit.max(1);
                let tokens_per_agent = tokens_per_minute / (self.concurrent_limit as u32).max(1);
                (req_per_agent, tokens_per_agent, "api_key".to_string())
            }
        };
        
        let allocation = AgentQuotaAllocation {
            agent_id: agent_id.to_string(),
            auth_type: auth_type_str,
            allocated_at: now,
            max_requests_per_minute,
            max_tokens_allocated,
            current_usage: 0,
        };
        
        let session = AgentSession {
            agent_id: agent_id.to_string(),
            session_id: session_id.clone(),
            started_at: now,
            last_activity: now,
            estimated_tokens_used: 0,
            requests_made: 0,
        };
        
        // Store allocation and session
        self.quota_allocations.write().await.insert(agent_id.to_string(), allocation.clone());
        self.agent_sessions.write().await.insert(agent_id.to_string(), session);
        self.active_agents.write().await.insert(agent_id.to_string());
        
        Ok(allocation)
    }
    
    /// Release quota allocation for an agent
    pub async fn release_agent_quota(&self, agent_id: &str) -> Result<(), String> {
        let mut active_agents = self.active_agents.write().await;
        let mut agent_sessions = self.agent_sessions.write().await;
        let mut quota_allocations = self.quota_allocations.write().await;
        
        if let Some(session) = agent_sessions.remove(agent_id) {
            // Update total usage
            self.current_usage.fetch_add(session.estimated_tokens_used, Ordering::Relaxed);
        }
        
        active_agents.remove(agent_id);
        quota_allocations.remove(agent_id);
        
        Ok(())
    }
    
    /// Update agent activity and usage
    pub async fn update_agent_activity(&self, agent_id: &str, tokens_used: u64, requests_made: u32) -> Result<(), String> {
        let mut agent_sessions = self.agent_sessions.write().await;
        
        if let Some(session) = agent_sessions.get_mut(agent_id) {
            session.last_activity = Utc::now();
            session.estimated_tokens_used += tokens_used;
            session.requests_made += requests_made;
            
            // Update global rate limiting
            self.requests_this_minute.fetch_add(requests_made as u16, Ordering::Relaxed);
            
            Ok(())
        } else {
            Err(format!("Agent {} not found in active sessions", agent_id))
        }
    }
    
    /// Check if agent can make a request (rate limiting)
    pub async fn can_agent_make_request(&self, _agent_id: &str) -> Result<bool, String> {
        // Reset rate limit counter if a minute has passed
        self.reset_rate_limit_if_needed().await;
        
        match &self.auth_type {
            ClaudeAuthType::ApiKey { requests_per_minute, .. } => {
                let current_requests = self.requests_this_minute.load(Ordering::Relaxed);
                Ok(current_requests < *requests_per_minute)
            }
            ClaudeAuthType::Max { .. } => {
                // Claude Max has more flexible rate limits
                Ok(true)
            }
        }
    }
    
    /// Reset rate limiting counters if needed
    async fn reset_rate_limit_if_needed(&self) {
        let mut last_reset = self.last_minute_reset.write().await;
        let now = Utc::now();
        
        if now - *last_reset >= Duration::minutes(1) {
            self.requests_this_minute.store(0, Ordering::Relaxed);
            *last_reset = now;
        }
    }
    
    /// Get current quota status
    pub async fn get_quota_status(&self) -> ClaudeQuotaStatus {
        let active_agents = self.active_agents.read().await;
        let agent_sessions = self.agent_sessions.read().await;
        let _quota_allocations = self.quota_allocations.read().await;
        
        let total_estimated_usage: u64 = agent_sessions.values()
            .map(|s| s.estimated_tokens_used)
            .sum();
        
        let active_agent_count = active_agents.len();
        let current_requests_per_minute = self.requests_this_minute.load(Ordering::Relaxed);
        
        ClaudeQuotaStatus {
            auth_type: self.auth_type.clone(),
            daily_limit: self.daily_limit,
            current_usage: self.current_usage.load(Ordering::Relaxed),
            estimated_session_usage: total_estimated_usage,
            active_agent_count,
            concurrent_limit: self.concurrent_limit,
            current_requests_per_minute,
            active_agent_ids: active_agents.iter().cloned().collect(),
        }
    }
    
    /// Get agent session details
    pub async fn get_agent_session(&self, agent_id: &str) -> Option<AgentSession> {
        self.agent_sessions.read().await.get(agent_id).cloned()
    }
    
    /// Cleanup expired or inactive sessions
    pub async fn cleanup_inactive_sessions(&self, max_inactive_duration: Duration) -> usize {
        let mut cleaned_count = 0;
        let now = Utc::now();
        let mut agent_sessions = self.agent_sessions.write().await;
        let mut active_agents = self.active_agents.write().await;
        let mut quota_allocations = self.quota_allocations.write().await;
        
        let inactive_agents: Vec<String> = agent_sessions
            .iter()
            .filter(|(_, session)| now - session.last_activity > max_inactive_duration)
            .map(|(agent_id, _)| agent_id.clone())
            .collect();
        
        for agent_id in inactive_agents {
            if let Some(session) = agent_sessions.remove(&agent_id) {
                self.current_usage.fetch_add(session.estimated_tokens_used, Ordering::Relaxed);
                cleaned_count += 1;
            }
            active_agents.remove(&agent_id);
            quota_allocations.remove(&agent_id);
        }
        
        cleaned_count
    }
}

/// Current quota status snapshot
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeQuotaStatus {
    pub auth_type: ClaudeAuthType,
    pub daily_limit: u64,
    pub current_usage: u64,
    pub estimated_session_usage: u64,
    pub active_agent_count: usize,
    pub concurrent_limit: u16,
    pub current_requests_per_minute: u16,
    pub active_agent_ids: Vec<String>,
}

impl ClaudeAuthType {
    /// Parse from environment variables and subscription type
    pub fn detect_from_env() -> Option<Self> {
        // Check if we have Claude API credentials
        if std::env::var("CLAUDE_API_KEY").is_ok() || std::env::var("ANTHROPIC_API_KEY").is_ok() {
            // Default to API key limits if we have keys
            Some(ClaudeAuthType::ApiKey {
                requests_per_minute: 60,
                tokens_per_minute: 100_000,
            })
        } else {
            // Try to detect Claude Max subscription
            // This would require additional logic to check subscription status
            None
        }
    }
}

// Implement Serialize for ClaudeAuthType
impl Serialize for ClaudeAuthType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ClaudeAuthType::Max { daily_limit } => {
                serializer.serialize_str(&format!("max:{}", daily_limit))
            }
            ClaudeAuthType::ApiKey { requests_per_minute, tokens_per_minute } => {
                serializer.serialize_str(&format!("api_key:{}:{}", requests_per_minute, tokens_per_minute))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_quota_allocation() {
        let manager = ClaudeQuotaManager::new_max_subscription(10000, 3);
        
        // Should be able to allocate agents up to concurrent limit
        for i in 0..3 {
            let agent_id = format!("agent_{}", i);
            let result = manager.allocate_agent_quota(&agent_id).await;
            assert!(result.is_ok());
        }
        
        // Fourth agent should fail
        let result = manager.allocate_agent_quota("agent_3").await;
        assert!(result.is_err());
        
        // Release one agent
        manager.release_agent_quota("agent_0").await.unwrap();
        
        // Now should be able to allocate again
        let result = manager.allocate_agent_quota("agent_3").await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_rate_limiting() {
        let manager = ClaudeQuotaManager::new_api_key(5, 1000, 2);
        
        let agent_id = "test_agent";
        manager.allocate_agent_quota(agent_id).await.unwrap();
        
        // Should be able to make requests up to rate limit
        for _ in 0..5 {
            assert!(manager.can_agent_make_request(agent_id).await.unwrap());
            manager.update_agent_activity(agent_id, 10, 1).await.unwrap();
        }
        
        // Sixth request should be rate limited
        assert!(!manager.can_agent_make_request(agent_id).await.unwrap());
    }
}