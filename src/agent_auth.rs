use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Duration;
use serde::Serialize;

use crate::claude_quota::{ClaudeQuotaManager, ClaudeAuthType, AgentQuotaAllocation};

/// Agent authentication environment setup
#[derive(Debug, Clone)]
pub struct AgentAuthEnvironment {
    /// Environment variables for agent execution
    pub env_vars: HashMap<String, String>,
    /// Authentication type being used
    pub auth_type: ClaudeAuthType,
    /// Quota allocation for this agent
    pub quota_allocation: Option<AgentQuotaAllocation>,
    /// Session tracking ID
    pub session_id: String,
}

/// Agent authentication coordinator for Claude Code environments
pub struct AgentAuthCoordinator {
    /// Quota manager for tracking usage
    quota_manager: Arc<ClaudeQuotaManager>,
    
    /// Active agent authentications
    active_auth_sessions: Arc<RwLock<HashMap<String, AgentAuthEnvironment>>>,
    
    /// Base environment variables to inherit
    base_env: HashMap<String, String>,
    
    /// Authentication manager for Claude authentication
    auth_manager: Option<Arc<AuthManager>>,
}

impl AgentAuthCoordinator {
    /// Create new authentication coordinator
    pub fn new(quota_manager: Arc<ClaudeQuotaManager>) -> Self {
        let base_env = Self::detect_base_claude_env();
        
        Self {
            quota_manager,
            active_auth_sessions: Arc::new(RwLock::new(HashMap::new())),
            base_env,
            auth_manager: None,
        }
    }

    /// Create new authentication coordinator with auth manager
    pub fn new_with_auth_manager(
        quota_manager: Arc<ClaudeQuotaManager>, 
        auth_manager: Arc<AuthManager>
    ) -> Self {
        let base_env = Self::detect_base_claude_env();
        
        Self {
            quota_manager,
            active_auth_sessions: Arc::new(RwLock::new(HashMap::new())),
            base_env,
            auth_manager: Some(auth_manager),
        }
    }
    
    /// Detect Claude authentication from environment
    fn detect_base_claude_env() -> HashMap<String, String> {
        let mut env = HashMap::new();
        
        // Check for Claude API credentials
        if let Ok(claude_key) = std::env::var("CLAUDE_API_KEY") {
            env.insert("CLAUDE_API_KEY".to_string(), claude_key.clone());
            env.insert("ANTHROPIC_API_KEY".to_string(), claude_key);
        }
        
        if let Ok(anthropic_key) = std::env::var("ANTHROPIC_API_KEY") {
            env.insert("ANTHROPIC_API_KEY".to_string(), anthropic_key.clone());
            env.insert("CLAUDE_API_KEY".to_string(), anthropic_key);
        }
        
        // Claude base URL if configured
        if let Ok(base_url) = std::env::var("ANTHROPIC_BASE_URL") {
            env.insert("ANTHROPIC_BASE_URL".to_string(), base_url.clone());
            env.insert("CLAUDE_BASE_URL".to_string(), base_url);
        }
        
        if let Ok(claude_base_url) = std::env::var("CLAUDE_BASE_URL") {
            env.insert("CLAUDE_BASE_URL".to_string(), claude_base_url.clone());
            env.insert("ANTHROPIC_BASE_URL".to_string(), claude_base_url);
        }
        
        // Add Claude-specific environment optimizations
        env.insert("DISABLE_AUTOUPDATER".to_string(), "1".to_string());
        env.insert("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(), "1".to_string());
        env.insert("DISABLE_ERROR_REPORTING".to_string(), "1".to_string());
        
        env
    }
    
    /// Setup Claude authentication for a new agent with enhanced authentication
    pub async fn setup_claude_agent_auth(&self, agent_id: &str) -> Result<AgentAuthEnvironment, String> {
        // Check if agent can be allocated quota
        if !self.quota_manager.can_allocate_agent().await.map_err(|e| format!("Quota check error: {}", e))? {
            return Err("Cannot allocate Claude quota: limits reached".to_string());
        }
        
        // Try to get authentication from the auth manager first
        let mut env_vars = self.base_env.clone();
        let mut auth_type = ClaudeAuthType::ApiKey { 
            requests_per_minute: 60, 
            tokens_per_minute: 100_000 
        };

        if let Some(ref auth_manager) = self.auth_manager {
            // Use the new Claude authentication system
            if let Some(claude_auth) = auth_manager.claude_auth() {
                match claude_auth.get_token().await {
                    Ok(token) => {
                        // Set Claude authentication token
                        env_vars.insert("ANTHROPIC_API_KEY".to_string(), token);
                        env_vars.insert("CLAUDE_API_KEY".to_string(), claude_auth.api_key.unwrap_or_default());
                        
                        // Set subscription information
                        if let Some(tier) = claude_auth.get_subscription_tier() {
                            env_vars.insert("CLAUDE_SUBSCRIPTION_TIER".to_string(), tier.clone());
                            
                            // Update auth type based on subscription
                            auth_type = match tier.as_str() {
                                "max" => {
                                    env_vars.insert("CLAUDE_MAX_USER".to_string(), "true".to_string());
                                    ClaudeAuthType::Max { daily_limit: 500_000 } // Claude Max typical limits
                                },
                                "pro" => {
                                    env_vars.insert("CLAUDE_PRO_USER".to_string(), "true".to_string());
                                    ClaudeAuthType::ApiKey { requests_per_minute: 300, tokens_per_minute: 200_000 }
                                },
                                _ => ClaudeAuthType::ApiKey { requests_per_minute: 60, tokens_per_minute: 100_000 }
                            };
                        }

                        // Check if user has Max subscription
                        if claude_auth.has_max_subscription().await {
                            env_vars.insert("CLAUDE_MAX_VERIFIED".to_string(), "true".to_string());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get Claude token: {}, falling back to environment detection", e);
                    }
                }
            } else if let Some(AuthProvider::Claude(claude_auth)) = auth_manager.get_optimal_provider().await {
                // Fallback to optimal provider if direct Claude auth not available
                if let Ok(token) = claude_auth.get_token().await {
                    env_vars.insert("ANTHROPIC_API_KEY".to_string(), token);
                    if let Some(api_key) = &claude_auth.api_key {
                        env_vars.insert("CLAUDE_API_KEY".to_string(), api_key.clone());
                    }
                }
            }
        }
        
        // Allocate quota for this agent based on determined auth type
        let quota_allocation = self.quota_manager.allocate_agent_quota(agent_id).await
            .map_err(|e| format!("Quota allocation error: {}", e))?;
        
        // Add quota-specific environment variables
        env_vars.insert(
            "CLAUDE_AGENT_ID".to_string(),
            agent_id.to_string(),
        );
        env_vars.insert(
            "CLAUDE_SESSION_ID".to_string(),
            quota_allocation.agent_id.clone(),
        );
        env_vars.insert(
            "CLAUDE_MAX_REQUESTS_PER_MINUTE".to_string(),
            quota_allocation.max_requests_per_minute.to_string(),
        );
        env_vars.insert(
            "CLAUDE_MAX_TOKENS_ALLOCATED".to_string(),
            quota_allocation.max_tokens_allocated.to_string(),
        );
        
        let auth_env = AgentAuthEnvironment {
            env_vars,
            auth_type,
            quota_allocation: Some(quota_allocation),
            session_id: agent_id.to_string(),
        };
        
        // Store active session
        self.active_auth_sessions.write().await.insert(
            agent_id.to_string(), 
            auth_env.clone()
        );
        
        Ok(auth_env)
    }
    
    /// Release authentication for an agent
    pub async fn release_agent_auth(&self, agent_id: &str) -> Result<(), String> {
        // Remove from active sessions
        self.active_auth_sessions.write().await.remove(agent_id);
        
        // Release quota
        self.quota_manager.release_agent_quota(agent_id).await?;
        
        Ok(())
    }
    
    /// Update agent activity for quota tracking
    pub async fn update_agent_activity(
        &self, 
        agent_id: &str, 
        tokens_used: u64, 
        requests_made: u32
    ) -> Result<(), String> {
        self.quota_manager.update_agent_activity(agent_id, tokens_used, requests_made).await
    }
    
    /// Check if agent can make a request
    pub async fn can_agent_make_request(&self, agent_id: &str) -> Result<bool, String> {
        self.quota_manager.can_agent_make_request(agent_id).await
    }
    
    /// Get agent authentication details
    pub async fn get_agent_auth(&self, agent_id: &str) -> Option<AgentAuthEnvironment> {
        self.active_auth_sessions.read().await.get(agent_id).cloned()
    }
    
    /// Get all active agent authentications
    pub async fn get_active_agents(&self) -> Vec<String> {
        self.active_auth_sessions.read().await.keys().cloned().collect()
    }
    
    /// Cleanup inactive agent sessions
    pub async fn cleanup_inactive_agents(&self, max_inactive: Duration) -> usize {
        let cleaned = self.quota_manager.cleanup_inactive_sessions(max_inactive).await;
        
        // Also cleanup our local sessions
        let mut sessions = self.active_auth_sessions.write().await;
        let active_quota_agents = self.quota_manager.get_quota_status().await.active_agent_ids;
        
        sessions.retain(|agent_id, _| active_quota_agents.contains(agent_id));
        
        cleaned
    }
    
    /// Check Claude authentication health
    pub async fn check_auth_health(&self) -> ClaudeAuthHealthStatus {
        let quota_status = self.quota_manager.get_quota_status().await;
        let active_sessions = self.active_auth_sessions.read().await;
        
        let healthy = match quota_status.auth_type {
            ClaudeAuthType::Max { daily_limit } => {
                quota_status.current_usage < (daily_limit as f64 * 0.9) as u64
            },
            ClaudeAuthType::ApiKey { requests_per_minute, .. } => {
                quota_status.current_requests_per_minute < (requests_per_minute as f64 * 0.8) as u16
            },
        };
        
        ClaudeAuthHealthStatus {
            healthy,
            active_agent_count: active_sessions.len(),
            quota_usage_percentage: match quota_status.auth_type {
                ClaudeAuthType::Max { daily_limit } => {
                    (quota_status.current_usage as f64 / daily_limit as f64 * 100.0) as u8
                },
                ClaudeAuthType::ApiKey { .. } => 0, // API keys don't have daily limits
            },
            rate_limit_usage_percentage: match quota_status.auth_type {
                ClaudeAuthType::ApiKey { requests_per_minute, .. } => {
                    (quota_status.current_requests_per_minute as f64 / requests_per_minute as f64 * 100.0) as u8
                },
                ClaudeAuthType::Max { .. } => 0,
            },
            can_allocate_new_agents: quota_status.active_agent_count < quota_status.concurrent_limit as usize,
        }
    }
}

/// Claude authentication health status
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeAuthHealthStatus {
    pub healthy: bool,
    pub active_agent_count: usize,
    pub quota_usage_percentage: u8,
    pub rate_limit_usage_percentage: u8,
    pub can_allocate_new_agents: bool,
}

/// Create a global authentication coordinator
lazy_static::lazy_static! {
    static ref GLOBAL_AUTH_COORDINATOR: Arc<RwLock<Option<Arc<AgentAuthCoordinator>>>> = 
        Arc::new(RwLock::new(None));
}

/// Initialize global Claude authentication coordinator
pub async fn init_claude_auth_coordinator() -> Result<Arc<AgentAuthCoordinator>, String> {
    let auth_type = ClaudeAuthType::detect_from_env()
        .ok_or_else(|| "No Claude authentication detected in environment".to_string())?;
    
    let quota_manager = match auth_type {
        ClaudeAuthType::Max { daily_limit } => {
            Arc::new(ClaudeQuotaManager::new_max_subscription(daily_limit, 5))
        },
        ClaudeAuthType::ApiKey { requests_per_minute, tokens_per_minute } => {
            Arc::new(ClaudeQuotaManager::new_api_key(requests_per_minute, tokens_per_minute, 8))
        },
    };
    
    let coordinator = Arc::new(AgentAuthCoordinator::new(quota_manager));
    
    *GLOBAL_AUTH_COORDINATOR.write().await = Some(coordinator.clone());
    
    Ok(coordinator)
}

/// Get global authentication coordinator
pub async fn get_claude_auth_coordinator() -> Option<Arc<AgentAuthCoordinator>> {
    GLOBAL_AUTH_COORDINATOR.read().await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::claude_quota::ClaudeQuotaManager;

    #[tokio::test]
    async fn test_agent_auth_setup() {
        let quota_manager = Arc::new(ClaudeQuotaManager::new_max_subscription(10000, 3));
        let coordinator = AgentAuthCoordinator::new(quota_manager);
        
        let agent_id = "test_agent";
        let auth_env = coordinator.setup_claude_agent_auth(agent_id).await;
        
        assert!(auth_env.is_ok());
        let env = auth_env.unwrap();
        
        assert!(env.env_vars.contains_key("CLAUDE_AGENT_ID"));
        assert!(env.env_vars.contains_key("CLAUDE_SESSION_ID"));
        assert!(env.quota_allocation.is_some());
        
        // Cleanup
        coordinator.release_agent_auth(agent_id).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_concurrent_agent_limits() {
        let quota_manager = Arc::new(ClaudeQuotaManager::new_max_subscription(10000, 2));
        let coordinator = AgentAuthCoordinator::new(quota_manager);
        
        // Should be able to allocate 2 agents
        let agent1 = coordinator.setup_claude_agent_auth("agent1").await;
        let agent2 = coordinator.setup_claude_agent_auth("agent2").await;
        
        assert!(agent1.is_ok());
        assert!(agent2.is_ok());
        
        // Third agent should fail
        let agent3 = coordinator.setup_claude_agent_auth("agent3").await;
        assert!(agent3.is_err());
        
        // Cleanup
        coordinator.release_agent_auth("agent1").await.unwrap();
        coordinator.release_agent_auth("agent2").await.unwrap();
    }
}