use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::claude_auth::SubscriptionInfo;
use crate::unified_auth::{UnifiedAuthManager, AuthProvider};

/// Agent quota allocation information
#[derive(Debug)]
pub struct AgentQuota {
    pub agent_id: String,
    pub allocated_tokens: u64,
    pub used_tokens: AtomicU64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub provider: AuthProvider,
    pub session_id: String,
}

impl Clone for AgentQuota {
    fn clone(&self) -> Self {
        Self {
            agent_id: self.agent_id.clone(),
            allocated_tokens: self.allocated_tokens,
            used_tokens: AtomicU64::new(self.used_tokens.load(std::sync::atomic::Ordering::SeqCst)),
            created_at: self.created_at,
            expires_at: self.expires_at,
            provider: self.provider.clone(),
            session_id: self.session_id.clone(),
        }
    }
}

/// Authentication coordinator for managing agent access to Claude/OpenAI
#[derive(Debug)]
pub struct AgentAuthCoordinator {
    unified_auth: Arc<UnifiedAuthManager>,
    active_quotas: Arc<RwLock<HashMap<String, AgentQuota>>>,
    claude_usage: Arc<AtomicU64>,
    openai_usage: Arc<AtomicU64>,
    daily_limits: DailyLimits,
}

/// Daily usage limits for different providers
#[derive(Debug, Clone)]
pub struct DailyLimits {
    pub claude_max_tokens: u64,
    pub claude_max_concurrent: u16,
    pub openai_tokens: u64,
    pub openai_concurrent: u16,
}

impl Default for DailyLimits {
    fn default() -> Self {
        Self {
            claude_max_tokens: 1_000_000,  // 1M tokens for Claude Max
            claude_max_concurrent: 10,      // Max 10 concurrent Claude agents
            openai_tokens: 500_000,         // 500K tokens for OpenAI
            openai_concurrent: 8,           // Max 8 concurrent OpenAI agents
        }
    }
}

/// Agent authentication request
#[derive(Debug, Clone)]
pub struct AgentAuthRequest {
    pub agent_id: String,
    pub estimated_tokens: u64,
    pub preferred_provider: Option<AuthProvider>,
    pub task_description: String,
}

/// Agent authentication response
#[derive(Debug, Clone)]
pub struct AgentAuthResponse {
    pub provider: AuthProvider,
    pub token: String,
    pub quota: AgentQuota,
    pub subscription_info: Option<SubscriptionInfo>,
}

/// Authentication errors for agents
#[derive(Debug, thiserror::Error)]
pub enum AgentAuthError {
    #[error("No authentication provider available")]
    NoProvider,
    
    #[error("Claude quota exceeded: requested {requested}, available {available}")]
    ClaudeQuotaExceeded { requested: u64, available: u64 },
    
    #[error("OpenAI quota exceeded: requested {requested}, available {available}")]
    OpenAIQuotaExceeded { requested: u64, available: u64 },
    
    #[error("Concurrent agent limit exceeded for {provider:?}: {current}/{max}")]
    ConcurrentLimitExceeded { provider: AuthProvider, current: usize, max: u16 },
    
    #[error("Agent {agent_id} not found")]
    AgentNotFound { agent_id: String },
    
    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl AgentAuthCoordinator {
    /// Create new agent authentication coordinator
    pub fn new(unified_auth: Arc<UnifiedAuthManager>) -> Self {
        Self {
            unified_auth,
            active_quotas: Arc::new(RwLock::new(HashMap::new())),
            claude_usage: Arc::new(AtomicU64::new(0)),
            openai_usage: Arc::new(AtomicU64::new(0)),
            daily_limits: DailyLimits::default(),
        }
    }

    /// Create coordinator with custom limits
    pub fn with_limits(unified_auth: Arc<UnifiedAuthManager>, limits: DailyLimits) -> Self {
        Self {
            unified_auth,
            active_quotas: Arc::new(RwLock::new(HashMap::new())),
            claude_usage: Arc::new(AtomicU64::new(0)),
            openai_usage: Arc::new(AtomicU64::new(0)),
            daily_limits: limits,
        }
    }

    /// Authenticate an agent and allocate resources
    pub async fn authenticate_agent(&self, request: AgentAuthRequest) -> Result<AgentAuthResponse, AgentAuthError> {
        // Check if agent already has allocation
        {
            let quotas = self.active_quotas.read().await;
            if quotas.contains_key(&request.agent_id) {
                return Err(AgentAuthError::AuthenticationFailed {
                    message: format!("Agent {} already has active quota", request.agent_id)
                });
            }
        }

        // Determine optimal provider
        let provider = match &request.preferred_provider {
            Some(p) => p.clone(),
            None => self.unified_auth.select_optimal_provider().await
                .map_err(|e| AgentAuthError::AuthenticationFailed { 
                    message: e.to_string() 
                })?
        };

        // Check quotas and allocate
        let quota = self.allocate_quota(&request, provider.clone()).await?;
        
        // Get authentication token
        let token = self.unified_auth.get_token_for_provider(provider.clone()).await
            .map_err(|e| AgentAuthError::AuthenticationFailed { 
                message: e.to_string() 
            })?;

        // Get subscription info if using Claude
        let subscription_info = if provider == AuthProvider::Claude {
            self.unified_auth.claude_auth()
                .and_then(|_auth| {
                    // This would need to be async, but for now return None
                    None
                })
        } else {
            None
        };

        // Store active quota
        {
            let mut quotas = self.active_quotas.write().await;
            quotas.insert(request.agent_id.clone(), quota.clone());
        }

        Ok(AgentAuthResponse {
            provider,
            token,
            quota,
            subscription_info,
        })
    }

    /// Allocate quota for an agent
    async fn allocate_quota(&self, request: &AgentAuthRequest, provider: AuthProvider) -> Result<AgentQuota, AgentAuthError> {
        let quotas = self.active_quotas.read().await;
        
        // Count current concurrent agents by provider
        let concurrent_count = quotas.values()
            .filter(|q| q.provider == provider)
            .count();

        // Check concurrent limits
        let max_concurrent = match provider {
            AuthProvider::Claude => self.daily_limits.claude_max_concurrent,
            AuthProvider::OpenAI => self.daily_limits.openai_concurrent,
        };

        if concurrent_count >= max_concurrent as usize {
            return Err(AgentAuthError::ConcurrentLimitExceeded {
                provider,
                current: concurrent_count,
                max: max_concurrent,
            });
        }

        // Check daily usage limits
        let (current_usage, daily_limit) = match provider {
            AuthProvider::Claude => (
                self.claude_usage.load(Ordering::Relaxed),
                self.daily_limits.claude_max_tokens
            ),
            AuthProvider::OpenAI => (
                self.openai_usage.load(Ordering::Relaxed),
                self.daily_limits.openai_tokens
            ),
        };

        let available = daily_limit.saturating_sub(current_usage);
        if available < request.estimated_tokens {
            return match provider {
                AuthProvider::Claude => Err(AgentAuthError::ClaudeQuotaExceeded {
                    requested: request.estimated_tokens,
                    available,
                }),
                AuthProvider::OpenAI => Err(AgentAuthError::OpenAIQuotaExceeded {
                    requested: request.estimated_tokens,
                    available,
                }),
            };
        }

        // Allocate quota
        let allocated = std::cmp::min(request.estimated_tokens, available / 2); // Conservative allocation
        
        // Update usage counter
        match provider {
            AuthProvider::Claude => {
                self.claude_usage.fetch_add(allocated, Ordering::Relaxed);
            }
            AuthProvider::OpenAI => {
                self.openai_usage.fetch_add(allocated, Ordering::Relaxed);
            }
        }

        Ok(AgentQuota {
            agent_id: request.agent_id.clone(),
            allocated_tokens: allocated,
            used_tokens: AtomicU64::new(0),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(2), // 2 hour expiry
            provider,
            session_id: Uuid::new_v4().to_string(),
        })
    }

    /// Release quota for a completed agent
    pub async fn release_agent_quota(&self, agent_id: &str) -> Result<u64, AgentAuthError> {
        let mut quotas = self.active_quotas.write().await;
        
        if let Some(quota) = quotas.remove(agent_id) {
            let used = quota.used_tokens.load(Ordering::Relaxed);
            let unused = quota.allocated_tokens.saturating_sub(used);

            // Return unused quota to the pool
            match quota.provider {
                AuthProvider::Claude => {
                    self.claude_usage.fetch_sub(unused, Ordering::Relaxed);
                }
                AuthProvider::OpenAI => {
                    self.openai_usage.fetch_sub(unused, Ordering::Relaxed);
                }
            }

            Ok(used)
        } else {
            Err(AgentAuthError::AgentNotFound {
                agent_id: agent_id.to_string()
            })
        }
    }

    /// Update token usage for an agent
    pub async fn update_agent_usage(&self, agent_id: &str, tokens_used: u64) -> Result<(), AgentAuthError> {
        let quotas = self.active_quotas.read().await;
        
        if let Some(quota) = quotas.get(agent_id) {
            quota.used_tokens.store(tokens_used, Ordering::Relaxed);
            Ok(())
        } else {
            Err(AgentAuthError::AgentNotFound {
                agent_id: agent_id.to_string()
            })
        }
    }

    /// Get quota information for an agent
    pub async fn get_agent_quota(&self, agent_id: &str) -> Option<AgentQuota> {
        let quotas = self.active_quotas.read().await;
        quotas.get(agent_id).map(|q| AgentQuota {
            agent_id: q.agent_id.clone(),
            allocated_tokens: q.allocated_tokens,
            used_tokens: AtomicU64::new(q.used_tokens.load(Ordering::Relaxed)),
            created_at: q.created_at,
            expires_at: q.expires_at,
            provider: q.provider.clone(),
            session_id: q.session_id.clone(),
        })
    }

    /// Get current usage statistics
    pub async fn get_usage_stats(&self) -> UsageStats {
        let quotas = self.active_quotas.read().await;
        
        let claude_active = quotas.values()
            .filter(|q| q.provider == AuthProvider::Claude)
            .count();
        
        let openai_active = quotas.values()
            .filter(|q| q.provider == AuthProvider::OpenAI)
            .count();

        UsageStats {
            claude_tokens_used: self.claude_usage.load(Ordering::Relaxed),
            claude_tokens_limit: self.daily_limits.claude_max_tokens,
            claude_active_agents: claude_active,
            claude_max_concurrent: self.daily_limits.claude_max_concurrent as usize,
            openai_tokens_used: self.openai_usage.load(Ordering::Relaxed),
            openai_tokens_limit: self.daily_limits.openai_tokens,
            openai_active_agents: openai_active,
            openai_max_concurrent: self.daily_limits.openai_concurrent as usize,
        }
    }

    /// Clean up expired quotas
    pub async fn cleanup_expired_quotas(&self) -> usize {
        let mut quotas = self.active_quotas.write().await;
        let now = Utc::now();
        let mut expired_agents = Vec::new();

        // Find expired quotas
        for (agent_id, quota) in quotas.iter() {
            if quota.expires_at <= now {
                expired_agents.push(agent_id.clone());
            }
        }

        // Remove expired quotas and return unused tokens
        for agent_id in &expired_agents {
            if let Some(quota) = quotas.remove(agent_id) {
                let used = quota.used_tokens.load(Ordering::Relaxed);
                let unused = quota.allocated_tokens.saturating_sub(used);

                match quota.provider {
                    AuthProvider::Claude => {
                        self.claude_usage.fetch_sub(unused, Ordering::Relaxed);
                    }
                    AuthProvider::OpenAI => {
                        self.openai_usage.fetch_sub(unused, Ordering::Relaxed);
                    }
                }
            }
        }

        expired_agents.len()
    }

    /// Reset daily usage counters (called at midnight)
    pub async fn reset_daily_usage(&self) {
        self.claude_usage.store(0, Ordering::Relaxed);
        self.openai_usage.store(0, Ordering::Relaxed);
        
        // Also clean up any stale quotas
        self.cleanup_expired_quotas().await;
    }

    /// Check if we can handle a new agent request
    pub async fn can_handle_request(&self, request: &AgentAuthRequest) -> Result<AuthProvider, AgentAuthError> {
        let provider = match &request.preferred_provider {
            Some(p) => p.clone(),
            None => self.unified_auth.select_optimal_provider().await
                .map_err(|e| AgentAuthError::AuthenticationFailed { 
                    message: e.to_string() 
                })?
        };

        // Simulate quota check without allocating
        let quotas = self.active_quotas.read().await;
        let concurrent_count = quotas.values()
            .filter(|q| q.provider == provider)
            .count();

        let max_concurrent = match provider {
            AuthProvider::Claude => self.daily_limits.claude_max_concurrent,
            AuthProvider::OpenAI => self.daily_limits.openai_concurrent,
        };

        if concurrent_count >= max_concurrent as usize {
            return Err(AgentAuthError::ConcurrentLimitExceeded {
                provider,
                current: concurrent_count,
                max: max_concurrent,
            });
        }

        let (current_usage, daily_limit) = match provider {
            AuthProvider::Claude => (
                self.claude_usage.load(Ordering::Relaxed),
                self.daily_limits.claude_max_tokens
            ),
            AuthProvider::OpenAI => (
                self.openai_usage.load(Ordering::Relaxed),
                self.daily_limits.openai_tokens
            ),
        };

        let available = daily_limit.saturating_sub(current_usage);
        if available < request.estimated_tokens {
            return match provider {
                AuthProvider::Claude => Err(AgentAuthError::ClaudeQuotaExceeded {
                    requested: request.estimated_tokens,
                    available,
                }),
                AuthProvider::OpenAI => Err(AgentAuthError::OpenAIQuotaExceeded {
                    requested: request.estimated_tokens,
                    available,
                }),
            };
        }

        Ok(provider)
    }
}

/// Usage statistics for monitoring
#[derive(Debug, Clone)]
pub struct UsageStats {
    pub claude_tokens_used: u64,
    pub claude_tokens_limit: u64,
    pub claude_active_agents: usize,
    pub claude_max_concurrent: usize,
    pub openai_tokens_used: u64,
    pub openai_tokens_limit: u64,
    pub openai_active_agents: usize,
    pub openai_max_concurrent: usize,
}

impl UsageStats {
    /// Get Claude usage percentage (0.0 to 1.0)
    pub fn claude_usage_percentage(&self) -> f64 {
        if self.claude_tokens_limit == 0 {
            0.0
        } else {
            self.claude_tokens_used as f64 / self.claude_tokens_limit as f64
        }
    }

    /// Get OpenAI usage percentage (0.0 to 1.0)
    pub fn openai_usage_percentage(&self) -> f64 {
        if self.openai_tokens_limit == 0 {
            0.0
        } else {
            self.openai_tokens_used as f64 / self.openai_tokens_limit as f64
        }
    }

    /// Check if we're approaching limits (>80%)
    pub fn is_approaching_limits(&self) -> bool {
        self.claude_usage_percentage() > 0.8 || self.openai_usage_percentage() > 0.8
    }

    /// Get recommended provider based on current usage
    pub fn recommended_provider(&self) -> Option<AuthProvider> {
        let claude_available = self.claude_active_agents < self.claude_max_concurrent;
        let openai_available = self.openai_active_agents < self.openai_max_concurrent;

        if !claude_available && !openai_available {
            return None;
        }

        if claude_available && !openai_available {
            return Some(AuthProvider::Claude);
        }

        if openai_available && !claude_available {
            return Some(AuthProvider::OpenAI);
        }

        // Both available - choose based on usage percentage
        if self.claude_usage_percentage() <= self.openai_usage_percentage() {
            Some(AuthProvider::Claude)
        } else {
            Some(AuthProvider::OpenAI)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::unified_auth::ProviderSelectionStrategy;

    fn create_test_unified_auth() -> Arc<UnifiedAuthManager> {
        let temp_dir = tempdir().unwrap();
        Arc::new(UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            crate::auth::AuthMode::ApiKey,
            "test".to_string(),
            ProviderSelectionStrategy::IntelligentSelection,
        ))
    }

    #[tokio::test]
    async fn test_agent_auth_coordinator_creation() {
        let unified_auth = create_test_unified_auth();
        let coordinator = AgentAuthCoordinator::new(unified_auth);
        
        let stats = coordinator.get_usage_stats().await;
        assert_eq!(stats.claude_tokens_used, 0);
        assert_eq!(stats.openai_tokens_used, 0);
        assert_eq!(stats.claude_active_agents, 0);
        assert_eq!(stats.openai_active_agents, 0);
    }

    #[tokio::test]
    async fn test_usage_stats() {
        let unified_auth = create_test_unified_auth();
        let coordinator = AgentAuthCoordinator::new(unified_auth);
        
        let stats = coordinator.get_usage_stats().await;
        assert_eq!(stats.claude_usage_percentage(), 0.0);
        assert_eq!(stats.openai_usage_percentage(), 0.0);
        assert!(!stats.is_approaching_limits());
    }

    #[tokio::test]
    async fn test_quota_cleanup() {
        let unified_auth = create_test_unified_auth();
        let coordinator = AgentAuthCoordinator::new(unified_auth);
        
        let cleaned = coordinator.cleanup_expired_quotas().await;
        assert_eq!(cleaned, 0);
    }
}