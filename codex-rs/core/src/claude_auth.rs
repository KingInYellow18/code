use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration as StdDuration;

/// Claude authentication modes supporting different subscription tiers
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClaudeAuthMode {
    /// Claude Max subscription with OAuth
    MaxSubscription,
    /// Claude Pro subscription with OAuth  
    ProSubscription,
    /// Direct API key authentication
    ApiKey,
}

/// Claude token data structure for OAuth flows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub subscription_tier: String,
    pub scope: Option<String>,
}

/// Claude authentication configuration
#[derive(Debug, Clone)]
pub struct ClaudeAuth {
    pub mode: ClaudeAuthMode,
    pub api_key: Option<String>,
    pub oauth_tokens: Arc<Mutex<Option<ClaudeTokenData>>>,
    pub auth_file: PathBuf,
    pub client: reqwest::Client,
    pub subscription_info: Arc<Mutex<Option<SubscriptionInfo>>>,
}

/// Claude subscription information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub tier: String,           // "max", "pro", "free"
    pub usage_limit: u64,       // Daily token limit
    pub usage_current: u64,     // Current usage
    pub reset_date: DateTime<Utc>,
    pub features: Vec<String>,
    pub concurrent_limit: Option<u16>, // Max concurrent agents
}

/// OAuth PKCE configuration for Claude
#[derive(Debug, Clone)]
pub struct ClaudeOAuthConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

impl Default for ClaudeOAuthConfig {
    fn default() -> Self {
        Self {
            client_id: "code_project_client_id".to_string(), // TODO: Register with Anthropic
            auth_url: "https://auth.anthropic.com/oauth/authorize".to_string(),
            token_url: "https://auth.anthropic.com/oauth/token".to_string(),
            scopes: vec!["api".to_string(), "subscription".to_string()],
            redirect_uri: "http://localhost:1456/callback".to_string(),
        }
    }
}

/// Claude authentication storage format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAuthJson {
    #[serde(rename = "ANTHROPIC_API_KEY")]
    pub api_key: Option<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_tokens: Option<ClaudeTokenData>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_info: Option<SubscriptionInfo>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_mode: Option<ClaudeAuthMode>,
}

impl ClaudeAuth {
    /// Create new Claude authentication instance
    pub fn new(codex_home: &std::path::Path, client: reqwest::Client) -> Self {
        Self {
            mode: ClaudeAuthMode::ApiKey,
            api_key: None,
            oauth_tokens: Arc::new(Mutex::new(None)),
            auth_file: codex_home.join("claude_auth.json"),
            client,
            subscription_info: Arc::new(Mutex::new(None)),
        }
    }

    /// Load Claude authentication from codex home directory
    pub fn from_codex_home(
        codex_home: &std::path::Path,
        preferred_mode: ClaudeAuthMode,
        _originator: &str,
    ) -> Result<Option<Self>, std::io::Error> {
        let client = crate::default_client::create_client(_originator);
        let auth_file = get_claude_auth_file(codex_home);
        
        if auth_file.exists() {
            // Try to load from file
            let content = std::fs::read_to_string(&auth_file)?;
            let auth_data: ClaudeAuthJson = serde_json::from_str(&content)?;
            
            let claude_auth = Self {
                mode: auth_data.preferred_mode.unwrap_or(preferred_mode),
                api_key: auth_data.api_key,
                oauth_tokens: Arc::new(Mutex::new(auth_data.oauth_tokens)),
                auth_file,
                client,
                subscription_info: Arc::new(Mutex::new(auth_data.subscription_info)),
            };
            
            Ok(Some(claude_auth))
        } else if let Some(api_key) = read_claude_api_key_from_env() {
            // Fallback to environment variable
            Ok(Some(Self::from_api_key(&api_key, client)))
        } else {
            Ok(None)
        }
    }

    /// Create Claude auth from API key
    pub fn from_api_key(api_key: &str, client: reqwest::Client) -> Self {
        Self {
            mode: ClaudeAuthMode::ApiKey,
            api_key: Some(api_key.to_string()),
            oauth_tokens: Arc::new(Mutex::new(None)),
            auth_file: PathBuf::new(),
            client,
            subscription_info: Arc::new(Mutex::new(None)),
        }
    }

    /// Get current authentication token
    pub async fn get_token(&self) -> Result<String, std::io::Error> {
        match self.mode {
            ClaudeAuthMode::ApiKey => {
                self.api_key.clone()
                    .ok_or_else(|| std::io::Error::other("Claude API key not available"))
            }
            ClaudeAuthMode::MaxSubscription | ClaudeAuthMode::ProSubscription => {
                let tokens = self.oauth_tokens.lock()
                    .map_err(|_| std::io::Error::other("Failed to lock OAuth tokens"))?;
                
                if let Some(token_data) = tokens.as_ref() {
                    if token_data.expires_at > Utc::now() + Duration::minutes(5) {
                        Ok(token_data.access_token.clone())
                    } else {
                        drop(tokens);
                        self.refresh_oauth_token().await
                    }
                } else {
                    Err(std::io::Error::other("Claude OAuth tokens not available"))
                }
            }
        }
    }

    /// Refresh OAuth token if needed
    pub async fn refresh_oauth_token(&self) -> Result<String, std::io::Error> {
        let refresh_token = {
            let tokens = self.oauth_tokens.lock()
                .map_err(|_| std::io::Error::other("Failed to lock OAuth tokens"))?;
            
            tokens.as_ref()
                .and_then(|t| t.refresh_token.clone())
                .ok_or_else(|| std::io::Error::other("No refresh token available"))?
        };

        let refresh_response = self.try_refresh_token(refresh_token).await
            .map_err(std::io::Error::other)?;

        // Update stored tokens
        {
            let mut tokens = self.oauth_tokens.lock()
                .map_err(|_| std::io::Error::other("Failed to lock OAuth tokens"))?;
            *tokens = Some(refresh_response.clone());
        }

        // Persist to file
        self.save_auth_data().await?;

        Ok(refresh_response.access_token)
    }

    /// Check Claude subscription status
    pub async fn check_subscription(&self) -> Result<SubscriptionInfo, std::io::Error> {
        let token = self.get_token().await?;
        
        let response = self.client
            .get("https://api.anthropic.com/v1/subscription")
            .bearer_auth(&token)
            .send()
            .await
            .map_err(std::io::Error::other)?;

        if response.status().is_success() {
            let subscription_info: SubscriptionInfo = response
                .json()
                .await
                .map_err(std::io::Error::other)?;

            // Cache subscription info
            {
                let mut sub_info = self.subscription_info.lock()
                    .map_err(|_| std::io::Error::other("Failed to lock subscription info"))?;
                *sub_info = Some(subscription_info.clone());
            }

            Ok(subscription_info)
        } else {
            Err(std::io::Error::other(format!(
                "Failed to check Claude subscription: {}",
                response.status()
            )))
        }
    }

    /// Get cached subscription info or fetch if needed
    pub async fn get_subscription_info(&self) -> Result<SubscriptionInfo, std::io::Error> {
        {
            let sub_info = self.subscription_info.lock()
                .map_err(|_| std::io::Error::other("Failed to lock subscription info"))?;
            
            if let Some(info) = sub_info.as_ref() {
                // Return cached if still valid (refresh every hour)
                if info.reset_date > Utc::now() - Duration::hours(1) {
                    return Ok(info.clone());
                }
            }
        }

        // Fetch fresh subscription info
        self.check_subscription().await
    }

    /// Check if user has Claude Max subscription
    pub async fn has_max_subscription(&self) -> Result<bool, std::io::Error> {
        let sub_info = self.get_subscription_info().await?;
        Ok(sub_info.tier == "max")
    }

    /// Check if user has any paid Claude subscription (Pro or Max)
    pub async fn has_paid_subscription(&self) -> Result<bool, std::io::Error> {
        let sub_info = self.get_subscription_info().await?;
        Ok(sub_info.tier == "max" || sub_info.tier == "pro")
    }

    /// Load Claude authentication from file
    pub async fn load_from_file(auth_file: &std::path::Path, client: reqwest::Client) -> Result<Option<Self>, std::io::Error> {
        if !auth_file.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(auth_file).await?;
        let auth_data: ClaudeAuthJson = serde_json::from_str(&content)?;

        let mut claude_auth = Self {
            mode: auth_data.preferred_mode.unwrap_or(ClaudeAuthMode::ApiKey),
            api_key: auth_data.api_key,
            oauth_tokens: Arc::new(Mutex::new(auth_data.oauth_tokens)),
            auth_file: auth_file.to_path_buf(),
            client,
            subscription_info: Arc::new(Mutex::new(auth_data.subscription_info)),
        };

        // Determine auth mode based on available credentials
        claude_auth.mode = claude_auth.determine_auth_mode();

        Ok(Some(claude_auth))
    }

    /// Save authentication data to file
    pub async fn save_auth_data(&self) -> Result<(), std::io::Error> {
        let auth_data = ClaudeAuthJson {
            api_key: self.api_key.clone(),
            oauth_tokens: self.oauth_tokens.lock()
                .map_err(|_| std::io::Error::other("Failed to lock OAuth tokens"))?
                .clone(),
            subscription_info: self.subscription_info.lock()
                .map_err(|_| std::io::Error::other("Failed to lock subscription info"))?
                .clone(),
            last_refresh: Some(Utc::now()),
            preferred_mode: Some(self.mode.clone()),
        };

        let json_data = serde_json::to_string_pretty(&auth_data)?;
        
        // Ensure secure permissions (similar to OpenAI auth.json)
        tokio::fs::write(&self.auth_file, json_data).await?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&self.auth_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&self.auth_file, perms).await?;
        }

        Ok(())
    }

    /// Determine the best authentication mode based on available credentials
    fn determine_auth_mode(&self) -> ClaudeAuthMode {
        // Check if we have OAuth tokens
        if let Ok(tokens) = self.oauth_tokens.lock() {
            if let Some(token_data) = tokens.as_ref() {
                return match token_data.subscription_tier.as_str() {
                    "max" => ClaudeAuthMode::MaxSubscription,
                    "pro" => ClaudeAuthMode::ProSubscription,
                    _ => ClaudeAuthMode::ApiKey,
                };
            }
        }

        // Fallback to API key if available
        if self.api_key.is_some() {
            ClaudeAuthMode::ApiKey
        } else {
            ClaudeAuthMode::ApiKey // Default
        }
    }

    /// Try to refresh OAuth token
    async fn try_refresh_token(&self, refresh_token: String) -> Result<ClaudeTokenData, Box<dyn std::error::Error + Send + Sync>> {
        let oauth_config = ClaudeOAuthConfig::default();
        
        let refresh_request = ClaudeRefreshRequest {
            client_id: oauth_config.client_id,
            grant_type: "refresh_token".to_string(),
            refresh_token,
        };

        let response = self.client
            .post(&oauth_config.token_url)
            .header("Content-Type", "application/json")
            .json(&refresh_request)
            .timeout(StdDuration::from_secs(30))
            .send()
            .await?;

        if response.status().is_success() {
            let refresh_response: ClaudeRefreshResponse = response.json().await?;
            
            Ok(ClaudeTokenData {
                access_token: refresh_response.access_token,
                refresh_token: refresh_response.refresh_token,
                expires_at: Utc::now() + Duration::seconds(refresh_response.expires_in.unwrap_or(3600) as i64),
                subscription_tier: refresh_response.subscription_tier.unwrap_or_default(),
                scope: refresh_response.scope,
            })
        } else {
            Err(format!("Failed to refresh Claude token: {}", response.status()).into())
        }
    }
}

/// Claude OAuth refresh request
#[derive(Serialize)]
struct ClaudeRefreshRequest {
    client_id: String,
    grant_type: String,
    refresh_token: String,
}

/// Claude OAuth refresh response
#[derive(Deserialize)]
struct ClaudeRefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    subscription_tier: Option<String>,
    scope: Option<String>,
}

/// Check if Claude authentication is available via environment variable
pub fn read_claude_api_key_from_env() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .or_else(|_| std::env::var("CLAUDE_API_KEY"))
        .ok()
        .filter(|s| !s.is_empty())
}

/// Get Claude auth file path
pub fn get_claude_auth_file(codex_home: &std::path::Path) -> PathBuf {
    codex_home.join("claude_auth.json")
}

/// Login with Claude API key
pub async fn login_with_claude_api_key(codex_home: &std::path::Path, api_key: &str) -> Result<(), std::io::Error> {
    let auth_data = ClaudeAuthJson {
        api_key: Some(api_key.to_string()),
        oauth_tokens: None,
        subscription_info: None,
        last_refresh: None,
        preferred_mode: Some(ClaudeAuthMode::ApiKey),
    };

    let auth_file = get_claude_auth_file(codex_home);
    let json_data = serde_json::to_string_pretty(&auth_data)?;
    
    tokio::fs::write(&auth_file, json_data).await?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&auth_file).await?.permissions();
        perms.set_mode(0o600);
        tokio::fs::set_permissions(&auth_file, perms).await?;
    }

    Ok(())
}

/// Delete Claude authentication file
pub async fn logout_claude(codex_home: &std::path::Path) -> Result<bool, std::io::Error> {
    let auth_file = get_claude_auth_file(codex_home);
    match tokio::fs::remove_file(&auth_file).await {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_claude_auth_api_key() {
        let client = reqwest::Client::new();
        let claude_auth = ClaudeAuth::from_api_key("test-key", client);
        
        assert_eq!(claude_auth.mode, ClaudeAuthMode::ApiKey);
        assert_eq!(claude_auth.api_key, Some("test-key".to_string()));
    }

    #[tokio::test]
    async fn test_claude_auth_file_operations() {
        let temp_dir = tempdir().unwrap();
        let auth_file = temp_dir.path().join("claude_auth.json");
        
        // Test saving and loading
        let client = reqwest::Client::new();
        let mut claude_auth = ClaudeAuth::new(temp_dir.path(), client.clone());
        claude_auth.api_key = Some("test-api-key".to_string());
        
        claude_auth.save_auth_data().await.unwrap();
        assert!(auth_file.exists());
        
        let loaded_auth = ClaudeAuth::load_from_file(&auth_file, client).await.unwrap();
        assert!(loaded_auth.is_some());
        
        let loaded = loaded_auth.unwrap();
        assert_eq!(loaded.api_key, Some("test-api-key".to_string()));
    }

    #[tokio::test]
    async fn test_claude_login_logout() {
        let temp_dir = tempdir().unwrap();
        
        // Test login
        login_with_claude_api_key(temp_dir.path(), "test-key").await.unwrap();
        let auth_file = get_claude_auth_file(temp_dir.path());
        assert!(auth_file.exists());
        
        // Test logout
        let removed = logout_claude(temp_dir.path()).await.unwrap();
        assert!(removed);
        assert!(!auth_file.exists());
    }

    #[test]
    fn test_env_var_reading() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-env-key");
        let key = read_claude_api_key_from_env();
        assert_eq!(key, Some("test-env-key".to_string()));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}