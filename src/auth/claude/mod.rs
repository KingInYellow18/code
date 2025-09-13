/// # Claude Authentication Module
/// 
/// Provides comprehensive Claude authentication support including API keys,
/// OAuth tokens, subscription management, and quota tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Claude authentication modes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClaudeAuthMode {
    /// Claude Max subscription with OAuth
    MaxSubscription,
    /// Claude Pro subscription with OAuth
    ProSubscription,
    /// Direct API key authentication
    ApiKey,
}

/// Claude authentication structure
#[derive(Debug, Clone)]
pub struct ClaudeAuth {
    pub mode: ClaudeAuthMode,
    pub subscription_tier: Option<String>,
    pub api_key: Option<String>,
    pub oauth_tokens: Option<ClaudeTokenData>,
    pub client: reqwest::Client,
    pub quota_manager: Arc<RwLock<ClaudeQuotaManager>>,
}

/// Claude OAuth token data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub subscription_tier: String,
    pub token_type: String,
    pub scope: Vec<String>,
}

/// Claude subscription information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSubscription {
    pub tier: String,
    pub features: Vec<String>,
    pub quota_limit: u64,
    pub quota_used: u64,
    pub quota_reset_date: DateTime<Utc>,
    pub active: bool,
}

/// Quota management for Claude usage
#[derive(Debug, Clone)]
pub struct ClaudeQuotaManager {
    pub daily_limit: u64,
    pub current_usage: u64,
    pub concurrent_limit: u16,
    pub active_agents: HashMap<String, AgentQuota>,
    pub last_reset: DateTime<Utc>,
}

/// Agent-specific quota allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQuota {
    pub agent_id: String,
    pub allocated_tokens: u64,
    pub used_tokens: u64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Claude authentication errors
#[derive(Debug, thiserror::Error)]
pub enum ClaudeAuthError {
    #[error("Subscription expired or invalid")]
    SubscriptionExpired,
    
    #[error("Quota exceeded: requested {requested}, available {available}")]
    QuotaExceeded { requested: u64, available: u64 },
    
    #[error("Invalid credentials")]
    InvalidCredentials,
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("OAuth error: {0}")]
    OAuthError(String),
    
    #[error("Concurrent limit exceeded")]
    ConcurrentLimitExceeded,
}

impl ClaudeAuth {
    /// Create Claude auth from codex home directory
    pub fn from_codex_home(
        codex_home: &Path,
        preferred_auth_mode: ClaudeAuthMode,
        originator: &str,
    ) -> std::io::Result<Option<Self>> {
        let claude_auth_file = codex_home.join("claude_auth.json");
        let client = reqwest::Client::builder()
            .user_agent(format!("CodeProject/{} ({})", env!("CARGO_PKG_VERSION"), originator))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        if !claude_auth_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&claude_auth_file)?;
        let auth_data: serde_json::Value = serde_json::from_str(&content)?;

        // Check if setup is required
        if auth_data.get("setup_required").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Ok(None);
        }

        let quota_manager = Arc::new(RwLock::new(ClaudeQuotaManager::default()));

        // Try to load API key
        if let Some(api_key) = auth_data.get("api_key").and_then(|v| v.as_str()) {
            return Ok(Some(Self {
                mode: ClaudeAuthMode::ApiKey,
                subscription_tier: auth_data.get("subscription_tier")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                api_key: Some(api_key.to_string()),
                oauth_tokens: None,
                client,
                quota_manager,
            }));
        }

        // Try to load OAuth tokens
        if let Some(tokens_data) = auth_data.get("oauth_tokens") {
            let tokens: ClaudeTokenData = serde_json::from_value(tokens_data.clone())?;
            
            let mode = match tokens.subscription_tier.as_str() {
                "max" => ClaudeAuthMode::MaxSubscription,
                "pro" => ClaudeAuthMode::ProSubscription,
                _ => ClaudeAuthMode::ApiKey,
            };

            return Ok(Some(Self {
                mode,
                subscription_tier: Some(tokens.subscription_tier.clone()),
                api_key: None,
                oauth_tokens: Some(tokens),
                client,
                quota_manager,
            }));
        }

        Ok(None)
    }

    /// Get authentication token
    pub async fn get_token(&self) -> Result<String, ClaudeAuthError> {
        match &self.mode {
            ClaudeAuthMode::ApiKey => {
                self.api_key.clone()
                    .ok_or(ClaudeAuthError::InvalidCredentials)
            }
            ClaudeAuthMode::MaxSubscription | ClaudeAuthMode::ProSubscription => {
                if let Some(tokens) = &self.oauth_tokens {
                    if tokens.expires_at > Utc::now() {
                        Ok(tokens.access_token.clone())
                    } else {
                        // Token expired, try to refresh
                        self.refresh_oauth_token().await
                    }
                } else {
                    Err(ClaudeAuthError::InvalidCredentials)
                }
            }
        }
    }

    /// Check if user has Claude Max subscription
    pub async fn has_max_subscription(&self) -> bool {
        match self.verify_subscription().await {
            Ok(subscription) => subscription.tier == "max" && subscription.active,
            Err(_) => false,
        }
    }

    /// Verify Claude subscription status
    pub async fn verify_subscription(&self) -> Result<ClaudeSubscription, ClaudeAuthError> {
        let token = self.get_token().await?;
        
        let response = self.client
            .get("https://api.anthropic.com/v1/subscription")
            .bearer_auth(&token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClaudeAuthError::SubscriptionExpired);
        }

        let subscription_data: serde_json::Value = response.json().await?;
        
        Ok(ClaudeSubscription {
            tier: subscription_data.get("tier")
                .and_then(|v| v.as_str())
                .unwrap_or("free")
                .to_string(),
            features: subscription_data.get("features")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect())
                .unwrap_or_default(),
            quota_limit: subscription_data.get("quota_limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            quota_used: subscription_data.get("quota_used")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            quota_reset_date: subscription_data.get("quota_reset_date")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| Utc::now() + chrono::Duration::days(1)),
            active: subscription_data.get("active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    /// Refresh OAuth token
    async fn refresh_oauth_token(&self) -> Result<String, ClaudeAuthError> {
        let tokens = self.oauth_tokens.as_ref()
            .ok_or(ClaudeAuthError::InvalidCredentials)?;
        
        let refresh_token = tokens.refresh_token.as_ref()
            .ok_or(ClaudeAuthError::InvalidCredentials)?;

        let refresh_request = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": "code_project_client_id", // Would be configured
        });

        let response = self.client
            .post("https://auth.anthropic.com/oauth/token")
            .header("Content-Type", "application/json")
            .json(&refresh_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClaudeAuthError::OAuthError("Token refresh failed".to_string()));
        }

        let token_response: serde_json::Value = response.json().await?;
        let new_access_token = token_response.get("access_token")
            .and_then(|v| v.as_str())
            .ok_or(ClaudeAuthError::OAuthError("No access token in response".to_string()))?;

        Ok(new_access_token.to_string())
    }

    /// Allocate quota for an agent
    pub async fn allocate_agent_quota(&self, agent_id: &str, estimated_usage: u64) -> Result<AgentQuota, ClaudeAuthError> {
        let mut quota_manager = self.quota_manager.write().await;
        quota_manager.allocate_quota(agent_id, estimated_usage).await
    }

    /// Release quota from an agent
    pub async fn release_agent_quota(&self, agent_id: &str) -> Result<u64, ClaudeAuthError> {
        let mut quota_manager = self.quota_manager.write().await;
        quota_manager.release_quota(agent_id).await
    }

    /// Get remaining quota
    pub async fn get_remaining_quota(&self) -> Result<u64, ClaudeAuthError> {
        let quota_manager = self.quota_manager.read().await;
        Ok(quota_manager.get_remaining_quota())
    }

    /// Setup Claude authentication with API key
    pub async fn setup_with_api_key(codex_home: &Path, api_key: &str) -> Result<(), ClaudeAuthError> {
        let claude_auth_file = codex_home.join("claude_auth.json");
        
        // Verify API key works
        let client = reqwest::Client::new();
        let test_response = client
            .post("https://api.anthropic.com/v1/messages")
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 10,
                "messages": [{"role": "user", "content": "test"}]
            }))
            .send()
            .await?;

        if !test_response.status().is_success() {
            return Err(ClaudeAuthError::InvalidCredentials);
        }

        // Create auth file
        let auth_data = serde_json::json!({
            "version": "2.0",
            "enabled": true,
            "setup_required": false,
            "auth_mode": "api_key",
            "api_key": api_key,
            "created_at": Utc::now().to_rfc3339(),
            "last_verified": Utc::now().to_rfc3339()
        });

        let content = serde_json::to_string_pretty(&auth_data)?;
        tokio::fs::write(&claude_auth_file, content).await?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&claude_auth_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&claude_auth_file, perms).await?;
        }

        Ok(())
    }

    /// Setup Claude authentication with OAuth
    pub async fn setup_with_oauth(codex_home: &Path, tokens: ClaudeTokenData) -> Result<(), ClaudeAuthError> {
        let claude_auth_file = codex_home.join("claude_auth.json");
        
        let auth_data = serde_json::json!({
            "version": "2.0",
            "enabled": true,
            "setup_required": false,
            "auth_mode": "oauth",
            "oauth_tokens": tokens,
            "subscription_tier": tokens.subscription_tier,
            "created_at": Utc::now().to_rfc3339(),
            "last_verified": Utc::now().to_rfc3339()
        });

        let content = serde_json::to_string_pretty(&auth_data)?;
        tokio::fs::write(&claude_auth_file, content).await?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&claude_auth_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&claude_auth_file, perms).await?;
        }

        Ok(())
    }
}

impl ClaudeQuotaManager {
    /// Allocate quota for an agent
    pub async fn allocate_quota(&mut self, agent_id: &str, estimated_usage: u64) -> Result<AgentQuota, ClaudeAuthError> {
        // Check if we have enough quota remaining
        let remaining = self.get_remaining_quota();
        if remaining < estimated_usage {
            return Err(ClaudeAuthError::QuotaExceeded {
                requested: estimated_usage,
                available: remaining,
            });
        }

        // Check concurrent agent limit
        if self.active_agents.len() >= self.concurrent_limit as usize {
            return Err(ClaudeAuthError::ConcurrentLimitExceeded);
        }

        // Create quota allocation
        let quota = AgentQuota {
            agent_id: agent_id.to_string(),
            allocated_tokens: estimated_usage,
            used_tokens: 0,
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(2),
        };

        self.active_agents.insert(agent_id.to_string(), quota.clone());
        self.current_usage += estimated_usage;

        Ok(quota)
    }

    /// Release quota from an agent
    pub async fn release_quota(&mut self, agent_id: &str) -> Result<u64, ClaudeAuthError> {
        if let Some(quota) = self.active_agents.remove(agent_id) {
            let unused = quota.allocated_tokens.saturating_sub(quota.used_tokens);
            self.current_usage = self.current_usage.saturating_sub(unused);
            Ok(quota.used_tokens)
        } else {
            Ok(0)
        }
    }

    /// Get remaining quota
    pub fn get_remaining_quota(&self) -> u64 {
        self.daily_limit.saturating_sub(self.current_usage)
    }

    /// Update agent usage
    pub fn update_agent_usage(&mut self, agent_id: &str, tokens_used: u64) {
        if let Some(quota) = self.active_agents.get_mut(agent_id) {
            quota.used_tokens += tokens_used;
        }
    }

    /// Check if quota reset is needed
    pub fn should_reset_quota(&self) -> bool {
        Utc::now() - self.last_reset > chrono::Duration::days(1)
    }

    /// Reset daily quota
    pub fn reset_daily_quota(&mut self) {
        self.current_usage = 0;
        self.active_agents.clear();
        self.last_reset = Utc::now();
    }
}

impl Default for ClaudeQuotaManager {
    fn default() -> Self {
        Self {
            daily_limit: 1_000_000, // 1M tokens per day (example)
            current_usage: 0,
            concurrent_limit: 10,
            active_agents: HashMap::new(),
            last_reset: Utc::now(),
        }
    }
}

/// Claude OAuth flow implementation
pub struct ClaudeOAuthFlow {
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    scopes: Vec<String>,
    client: reqwest::Client,
}

impl ClaudeOAuthFlow {
    /// Create new OAuth flow
    pub fn new(client_id: String, redirect_uri: String) -> Self {
        let client = reqwest::Client::new();
        let scopes = vec!["api".to_string(), "subscription".to_string()];

        Self {
            client_id,
            client_secret: None,
            redirect_uri,
            scopes,
            client,
        }
    }

    /// Generate authorization URL
    pub fn generate_auth_url(&self, state: &str) -> String {
        let scope = self.scopes.join(" ");
        format!(
            "https://auth.anthropic.com/oauth/authorize?client_id={}&redirect_uri={}&scope={}&response_type=code&state={}",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(&scope),
            urlencoding::encode(state)
        )
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str) -> Result<ClaudeTokenData, ClaudeAuthError> {
        let token_request = serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": self.client_id,
            "code": code,
            "redirect_uri": self.redirect_uri
        });

        let response = self.client
            .post("https://auth.anthropic.com/oauth/token")
            .header("Content-Type", "application/json")
            .json(&token_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClaudeAuthError::OAuthError("Token exchange failed".to_string()));
        }

        let token_response: serde_json::Value = response.json().await?;
        
        let access_token = token_response.get("access_token")
            .and_then(|v| v.as_str())
            .ok_or(ClaudeAuthError::OAuthError("No access token".to_string()))?;

        let expires_in = token_response.get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600);

        let subscription_tier = token_response.get("subscription_tier")
            .and_then(|v| v.as_str())
            .unwrap_or("free");

        Ok(ClaudeTokenData {
            access_token: access_token.to_string(),
            refresh_token: token_response.get("refresh_token")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            expires_at: Utc::now() + chrono::Duration::seconds(expires_in as i64),
            subscription_tier: subscription_tier.to_string(),
            token_type: token_response.get("token_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Bearer")
                .to_string(),
            scope: token_response.get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.split(' ').map(|s| s.to_string()).collect())
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_claude_auth_from_api_key() {
        let temp_dir = tempdir().unwrap();
        
        // Setup API key auth
        ClaudeAuth::setup_with_api_key(temp_dir.path(), "sk-test-key").await.unwrap();
        
        // Load auth
        let auth = ClaudeAuth::from_codex_home(temp_dir.path(), ClaudeAuthMode::ApiKey, "test").unwrap();
        assert!(auth.is_some());
        
        let auth = auth.unwrap();
        assert_eq!(auth.mode, ClaudeAuthMode::ApiKey);
        assert_eq!(auth.api_key.as_ref().unwrap(), "sk-test-key");
    }

    #[tokio::test]
    async fn test_quota_management() {
        let mut quota_manager = ClaudeQuotaManager::default();
        
        // Allocate quota
        let quota = quota_manager.allocate_quota("agent1", 1000).await.unwrap();
        assert_eq!(quota.allocated_tokens, 1000);
        assert_eq!(quota_manager.get_remaining_quota(), quota_manager.daily_limit - 1000);
        
        // Release quota
        let used = quota_manager.release_quota("agent1").await.unwrap();
        assert_eq!(used, 0); // No tokens used
        assert_eq!(quota_manager.get_remaining_quota(), quota_manager.daily_limit);
    }

    #[tokio::test]
    async fn test_oauth_flow() {
        let oauth_flow = ClaudeOAuthFlow::new(
            "test_client_id".to_string(),
            "http://localhost:3000/callback".to_string()
        );
        
        let auth_url = oauth_flow.generate_auth_url("test_state");
        assert!(auth_url.contains("test_client_id"));
        assert!(auth_url.contains("test_state"));
        assert!(auth_url.contains("auth.anthropic.com"));
    }

    #[test]
    fn test_quota_manager_concurrent_limits() {
        let mut quota_manager = ClaudeQuotaManager::default();
        quota_manager.concurrent_limit = 2;
        
        // Fill up concurrent slots
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            quota_manager.allocate_quota("agent1", 100).await.unwrap();
            quota_manager.allocate_quota("agent2", 100).await.unwrap();
            
            // Third allocation should fail
            let result = quota_manager.allocate_quota("agent3", 100).await;
            assert!(matches!(result, Err(ClaudeAuthError::ConcurrentLimitExceeded)));
        });
    }
}