# Claude Authentication API Reference

## Overview

This document provides comprehensive API documentation for programmatically using Claude authentication in the Code project. It covers Rust APIs for developers extending Code and HTTP APIs for external integrations.

## Rust API Reference

### Core Authentication Module

#### `ClaudeAuth` Struct

The main authentication structure for Claude integration:

```rust
use codex_core::claude_auth::{ClaudeAuth, ClaudeAuthMode, ClaudeTokenData};

pub struct ClaudeAuth {
    pub mode: ClaudeAuthMode,
    pub subscription_tier: Option<String>,
    pub api_key: Option<String>,
    pub oauth_tokens: Option<ClaudeTokenData>,
    pub client: reqwest::Client,
}

pub enum ClaudeAuthMode {
    MaxSubscription,    // Claude Max subscription with OAuth
    ApiKey,            // Direct API key authentication
    ProSubscription,   // Claude Pro subscription with OAuth
}
```

#### Constructor Methods

```rust
impl ClaudeAuth {
    /// Create from API key
    pub fn from_api_key(api_key: String) -> Self {
        ClaudeAuth {
            mode: ClaudeAuthMode::ApiKey,
            subscription_tier: None,
            api_key: Some(api_key),
            oauth_tokens: None,
            client: reqwest::Client::new(),
        }
    }

    /// Load from codex home directory
    pub fn from_codex_home(
        codex_home: &Path,
        mode: ClaudeAuthMode,
        client_id: &str,
    ) -> Result<Self> {
        // Implementation details...
    }

    /// Create for OAuth subscription
    pub fn from_oauth_tokens(
        tokens: ClaudeTokenData,
        subscription_tier: String,
    ) -> Self {
        ClaudeAuth {
            mode: if subscription_tier == "max" {
                ClaudeAuthMode::MaxSubscription
            } else {
                ClaudeAuthMode::ProSubscription
            },
            subscription_tier: Some(subscription_tier),
            api_key: None,
            oauth_tokens: Some(tokens),
            client: reqwest::Client::new(),
        }
    }
}
```

#### Core Methods

```rust
impl ClaudeAuth {
    /// Get current authentication token
    pub async fn get_token(&self) -> Result<String> {
        match &self.mode {
            ClaudeAuthMode::ApiKey => {
                self.api_key.clone()
                    .ok_or_else(|| AuthError::NoCredentials)
            }
            ClaudeAuthMode::MaxSubscription | ClaudeAuthMode::ProSubscription => {
                if let Some(tokens) = &self.oauth_tokens {
                    if tokens.is_expired() {
                        self.refresh_token().await
                    } else {
                        Ok(tokens.access_token.clone())
                    }
                } else {
                    Err(AuthError::NoCredentials)
                }
            }
        }
    }

    /// Check if user has Claude Max subscription
    pub async fn has_max_subscription(&self) -> bool {
        matches!(self.mode, ClaudeAuthMode::MaxSubscription) ||
        self.subscription_tier.as_deref() == Some("max")
    }

    /// Get detailed subscription information
    pub async fn check_subscription(&self) -> Result<SubscriptionInfo> {
        let token = self.get_token().await?;
        let response = self.client
            .get("https://api.anthropic.com/v1/subscription")
            .bearer_auth(token)
            .send()
            .await?;

        response.json::<SubscriptionInfo>().await
    }

    /// Refresh OAuth tokens
    pub async fn refresh_token(&self) -> Result<String> {
        if let Some(tokens) = &self.oauth_tokens {
            if let Some(refresh_token) = &tokens.refresh_token {
                // Implement token refresh logic
                self.perform_token_refresh(refresh_token).await
            } else {
                Err(AuthError::NoRefreshToken)
            }
        } else {
            Err(AuthError::NoCredentials)
        }
    }

    /// Make authenticated request to Claude API
    pub async fn make_request<T>(&self, endpoint: &str, body: T) -> Result<ClaudeResponse>
    where
        T: serde::Serialize,
    {
        let token = self.get_token().await?;
        let response = self.client
            .post(&format!("https://api.anthropic.com{}", endpoint))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;

        if response.status().is_success() {
            response.json().await.map_err(AuthError::from)
        } else {
            let error: ApiError = response.json().await?;
            Err(AuthError::ApiError(error))
        }
    }
}
```

### Enhanced AuthManager

#### Core Structure

```rust
use codex_core::auth::{AuthManager, AuthProvider, ProviderType};

pub struct AuthManager {
    openai_auth: Option<CodexAuth>,
    claude_auth: Option<ClaudeAuth>,
    preferred_provider: ProviderType,
    codex_home: PathBuf,
}

pub enum AuthProvider {
    OpenAI(CodexAuth),
    Claude(ClaudeAuth),
}

pub enum ProviderType {
    OpenAI,
    Claude,
    Auto,
}
```

#### Enhanced Methods

```rust
impl AuthManager {
    /// Create new AuthManager with both providers
    pub fn new(codex_home: PathBuf, client_id: String) -> Self {
        AuthManager {
            openai_auth: CodexAuth::from_codex_home(&codex_home).ok(),
            claude_auth: ClaudeAuth::from_codex_home(&codex_home, ClaudeAuthMode::MaxSubscription, &client_id).ok(),
            preferred_provider: ProviderType::Auto,
            codex_home,
        }
    }

    /// Get optimal provider based on availability and preferences
    pub async fn get_optimal_provider(&self) -> Option<AuthProvider> {
        match self.preferred_provider {
            ProviderType::Claude => {
                if let Some(claude) = &self.claude_auth {
                    Some(AuthProvider::Claude(claude.clone()))
                } else if let Some(openai) = &self.openai_auth {
                    Some(AuthProvider::OpenAI(openai.clone()))
                } else {
                    None
                }
            }
            ProviderType::OpenAI => {
                if let Some(openai) = &self.openai_auth {
                    Some(AuthProvider::OpenAI(openai.clone()))
                } else if let Some(claude) = &self.claude_auth {
                    Some(AuthProvider::Claude(claude.clone()))
                } else {
                    None
                }
            }
            ProviderType::Auto => {
                // Intelligent selection: Claude Max > Claude Pro > OpenAI > Claude API
                if let Some(claude) = &self.claude_auth {
                    if claude.has_max_subscription().await {
                        return Some(AuthProvider::Claude(claude.clone()));
                    }
                }
                if let Some(openai) = &self.openai_auth {
                    return Some(AuthProvider::OpenAI(openai.clone()));
                }
                if let Some(claude) = &self.claude_auth {
                    return Some(AuthProvider::Claude(claude.clone()));
                }
                None
            }
        }
    }

    /// Get specific provider
    pub fn get_provider(&self, provider_type: ProviderType) -> Option<AuthProvider> {
        match provider_type {
            ProviderType::Claude => {
                self.claude_auth.as_ref()
                    .map(|auth| AuthProvider::Claude(auth.clone()))
            }
            ProviderType::OpenAI => {
                self.openai_auth.as_ref()
                    .map(|auth| AuthProvider::OpenAI(auth.clone()))
            }
            ProviderType::Auto => {
                // Use runtime async version
                None  // Should use get_optimal_provider() instead
            }
        }
    }

    /// Add Claude authentication
    pub fn add_claude_auth(&mut self, claude_auth: ClaudeAuth) {
        self.claude_auth = Some(claude_auth);
    }

    /// Remove Claude authentication
    pub fn remove_claude_auth(&mut self) {
        self.claude_auth = None;
    }

    /// Set preferred provider
    pub fn set_preferred_provider(&mut self, provider: ProviderType) {
        self.preferred_provider = provider;
    }

    /// Check if provider is authenticated
    pub fn is_authenticated(&self, provider: ProviderType) -> bool {
        match provider {
            ProviderType::Claude => self.claude_auth.is_some(),
            ProviderType::OpenAI => self.openai_auth.is_some(),
            ProviderType::Auto => self.claude_auth.is_some() || self.openai_auth.is_some(),
        }
    }
}
```

### Data Structures

#### ClaudeTokenData

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub subscription_tier: String,
    pub scope: Option<String>,
}

impl ClaudeTokenData {
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    pub fn expires_in_minutes(&self) -> i64 {
        (self.expires_at - Utc::now()).num_minutes()
    }

    pub fn needs_refresh(&self, threshold_minutes: i64) -> bool {
        self.expires_in_minutes() <= threshold_minutes
    }
}
```

#### SubscriptionInfo

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub tier: String,              // "max", "pro", "free"
    pub usage_limit: u64,
    pub usage_current: u64,
    pub reset_date: DateTime<Utc>,
    pub features: Vec<String>,
    pub billing_cycle: String,
}

impl SubscriptionInfo {
    pub fn usage_percentage(&self) -> f64 {
        if self.usage_limit == 0 {
            0.0
        } else {
            (self.usage_current as f64 / self.usage_limit as f64) * 100.0
        }
    }

    pub fn is_quota_exceeded(&self) -> bool {
        self.usage_current >= self.usage_limit
    }

    pub fn quota_remaining(&self) -> u64 {
        self.usage_limit.saturating_sub(self.usage_current)
    }
}
```

#### Error Types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("No credentials available")]
    NoCredentials,

    #[error("No refresh token available")]
    NoRefreshToken,

    #[error("Subscription verification failed")]
    SubscriptionError,

    #[error("Quota exceeded: {current}/{limit}")]
    QuotaExceeded { current: u64, limit: u64 },

    #[error("Token expired")]
    TokenExpired,

    #[error("API error: {0}")]
    ApiError(#[from] ApiError),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error_type: String,
    pub message: String,
    pub code: Option<String>,
}
```

### Agent Environment Integration

#### Agent Authentication Coordinator

```rust
use std::collections::HashMap;

pub struct AgentAuthCoordinator {
    auth_manager: AuthManager,
    quota_manager: QuotaManager,
    session_manager: SessionManager,
}

impl AgentAuthCoordinator {
    pub async fn setup_agent_environment(&self, agent_id: &str) -> Result<HashMap<String, String>> {
        let mut env = HashMap::new();
        
        // Get optimal provider for this agent
        if let Some(provider) = self.auth_manager.get_optimal_provider().await {
            match provider {
                AuthProvider::Claude(claude_auth) => {
                    // Set up Claude environment
                    let token = claude_auth.get_token().await?;
                    env.insert("ANTHROPIC_API_KEY".to_string(), token);
                    env.insert("CLAUDE_API_KEY".to_string(), token.clone());
                    
                    // Add Claude-specific metadata
                    if let Some(tier) = &claude_auth.subscription_tier {
                        env.insert("CLAUDE_SUBSCRIPTION_TIER".to_string(), tier.clone());
                        env.insert("CLAUDE_MAX_USER".to_string(), 
                                  (tier == "max").to_string());
                    }
                    
                    // Agent-specific settings
                    env.insert("CLAUDE_AGENT_ID".to_string(), agent_id.to_string());
                    env.insert("CLAUDE_SESSION_ID".to_string(), 
                              self.session_manager.get_session_id(agent_id)?);
                    
                    // Quota information
                    if let Ok(quota) = self.quota_manager.get_agent_quota(agent_id).await {
                        env.insert("CLAUDE_ALLOCATED_QUOTA".to_string(), 
                                  quota.allocated_tokens.to_string());
                    }
                }
                AuthProvider::OpenAI(openai_auth) => {
                    // Set up OpenAI environment (existing logic)
                    let token = openai_auth.get_token().await?;
                    env.insert("OPENAI_API_KEY".to_string(), token);
                }
            }
        }

        // Add common environment variables
        env.insert("CODE_USER_AGENT".to_string(), "Code/1.0".to_string());
        env.insert("DISABLE_AUTO_UPDATE".to_string(), "1".to_string());
        
        Ok(env)
    }

    pub async fn cleanup_agent_environment(&self, agent_id: &str) -> Result<()> {
        self.quota_manager.release_agent_quota(agent_id).await?;
        self.session_manager.end_agent_session(agent_id)?;
        Ok(())
    }
}
```

### OAuth Implementation

#### OAuth Flow Manager

```rust
use oauth2::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};

pub struct ClaudeOAuthManager {
    client_id: ClientId,
    auth_url: String,
    token_url: String,
    redirect_uri: RedirectUrl,
}

impl ClaudeOAuthManager {
    pub fn new(client_id: String, redirect_uri: String) -> Self {
        ClaudeOAuthManager {
            client_id: ClientId::new(client_id),
            auth_url: "https://console.anthropic.com/oauth/authorize".to_string(),
            token_url: "https://console.anthropic.com/oauth/token".to_string(),
            redirect_uri: RedirectUrl::new(redirect_uri).expect("Invalid redirect URI"),
        }
    }

    pub fn generate_auth_url(&self) -> (String, PkceCodeVerifier, CsrfToken) {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let (auth_url, csrf_token) = self.oauth_client()
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("api".to_string()))
            .add_scope(Scope::new("subscription".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        (auth_url.to_string(), pkce_verifier, csrf_token)
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        pkce_verifier: PkceCodeVerifier,
    ) -> Result<ClaudeTokenData> {
        let token_result = self.oauth_client()
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_pkce_verifier(pkce_verifier)
            .request_async(oauth2::reqwest::async_http_client)
            .await?;

        // Convert OAuth2 token to our format
        let subscription_info = self.get_subscription_info(
            token_result.access_token().secret()
        ).await?;

        Ok(ClaudeTokenData {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result.refresh_token()
                .map(|t| t.secret().to_string()),
            expires_at: Utc::now() + chrono::Duration::seconds(
                token_result.expires_in()
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(3600)
            ),
            subscription_tier: subscription_info.tier,
            scope: Some("api subscription".to_string()),
        })
    }

    async fn get_subscription_info(&self, token: &str) -> Result<SubscriptionInfo> {
        let client = reqwest::Client::new();
        let response = client
            .get("https://api.anthropic.com/v1/subscription")
            .bearer_auth(token)
            .send()
            .await?;

        response.json().await.map_err(AuthError::from)
    }
}
```

## HTTP API Reference

### Authentication Endpoints

For external integrations, Code exposes HTTP endpoints for authentication management:

#### Base URL

```
http://localhost:8080/api/v1/auth
```

#### Get Authentication Status

```http
GET /api/v1/auth/status
```

**Response:**
```json
{
  "providers": [
    {
      "name": "claude",
      "authenticated": true,
      "subscription_tier": "max",
      "quota": {
        "used": 50000,
        "limit": 1000000,
        "percentage": 5.0,
        "reset_time": "2025-09-14T00:00:00Z"
      }
    },
    {
      "name": "openai",
      "authenticated": true,
      "method": "chatgpt_oauth"
    }
  ],
  "active_provider": "claude",
  "auto_fallback_enabled": true
}
```

#### Switch Provider

```http
POST /api/v1/auth/switch
Content-Type: application/json

{
  "provider": "claude"  // "claude" | "openai" | "auto"
}
```

**Response:**
```json
{
  "success": true,
  "previous_provider": "openai",
  "current_provider": "claude"
}
```

#### Test Provider Connection

```http
POST /api/v1/auth/test
Content-Type: application/json

{
  "provider": "claude"
}
```

**Response:**
```json
{
  "success": true,
  "provider": "claude",
  "response_time_ms": 150,
  "subscription_verified": true
}
```

#### Get Quota Information

```http
GET /api/v1/auth/quota?provider=claude
```

**Response:**
```json
{
  "provider": "claude",
  "quota": {
    "used": 50000,
    "limit": 1000000,
    "percentage": 5.0,
    "remaining": 950000,
    "reset_time": "2025-09-14T00:00:00Z",
    "time_until_reset": "14h 30m"
  },
  "warnings": {
    "low_quota": false,
    "approaching_limit": false
  }
}
```

### WebSocket API

For real-time updates, Code provides WebSocket endpoints:

#### Connect to Authentication Events

```javascript
const ws = new WebSocket('ws://localhost:8080/api/v1/auth/events');

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('Auth event:', data);
};

// Event types:
// - provider_switched
// - quota_warning
// - token_refreshed
// - authentication_failed
```

**Example Events:**

```json
{
  "type": "quota_warning",
  "provider": "claude",
  "quota_percentage": 85.0,
  "timestamp": "2025-09-13T15:30:00Z"
}

{
  "type": "provider_switched",
  "from": "claude",
  "to": "openai",
  "reason": "quota_exhausted",
  "timestamp": "2025-09-13T15:31:00Z"
}
```

## Usage Examples

### Basic Usage

```rust
use codex_core::claude_auth::ClaudeAuth;
use codex_core::auth::{AuthManager, ProviderType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize auth manager
    let codex_home = dirs::home_dir()
        .unwrap()
        .join(".codex");
    let mut auth_manager = AuthManager::new(codex_home, "code_cli".to_string());

    // Get optimal provider
    if let Some(provider) = auth_manager.get_optimal_provider().await {
        match provider {
            AuthProvider::Claude(claude_auth) => {
                println!("Using Claude authentication");
                let token = claude_auth.get_token().await?;
                println!("Token obtained successfully");

                // Check subscription
                if claude_auth.has_max_subscription().await {
                    println!("User has Claude Max subscription");
                    let quota = claude_auth.check_subscription().await?;
                    println!("Quota usage: {:.1}%", quota.usage_percentage());
                }
            }
            AuthProvider::OpenAI(openai_auth) => {
                println!("Using OpenAI authentication");
                // Handle OpenAI authentication
            }
        }
    } else {
        println!("No authentication available");
    }

    Ok(())
}
```

### Agent Environment Setup

```rust
use codex_core::agent_auth::AgentAuthCoordinator;
use std::collections::HashMap;

async fn setup_claude_agent(agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let coordinator = AgentAuthCoordinator::new()?;
    
    // Setup environment for agent
    let env = coordinator.setup_agent_environment(agent_id).await?;
    
    // Use environment variables in agent process
    for (key, value) in env {
        std::env::set_var(key, value);
    }
    
    // Run agent code...
    
    // Cleanup when done
    coordinator.cleanup_agent_environment(agent_id).await?;
    
    Ok(())
}
```

### OAuth Flow Integration

```rust
use codex_core::claude_auth::ClaudeOAuthManager;

async fn authenticate_with_oauth() -> Result<(), Box<dyn std::error::Error>> {
    let oauth_manager = ClaudeOAuthManager::new(
        "your-client-id".to_string(),
        "http://localhost:1456/callback".to_string(),
    );

    // Generate auth URL
    let (auth_url, pkce_verifier, csrf_token) = oauth_manager.generate_auth_url();
    
    println!("Please visit: {}", auth_url);
    
    // Wait for callback (in real implementation, this would be handled by a server)
    let authorization_code = wait_for_callback().await?;
    
    // Exchange code for tokens
    let tokens = oauth_manager.exchange_code(&authorization_code, pkce_verifier).await?;
    
    // Create Claude auth instance
    let claude_auth = ClaudeAuth::from_oauth_tokens(tokens.clone(), tokens.subscription_tier);
    
    // Save tokens for future use
    save_tokens_to_file(&tokens)?;
    
    Ok(())
}
```

### Configuration Management

```rust
use codex_core::config::AuthConfig;

fn configure_authentication() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AuthConfig::load()?;
    
    // Set preferred provider
    config.preferred_provider = ProviderType::Claude;
    
    // Configure Claude-specific settings
    config.claude.auto_fallback_enabled = true;
    config.claude.quota_warning_threshold = 0.8;
    config.claude.subscription_check_interval = Duration::hours(24);
    
    // Save configuration
    config.save()?;
    
    Ok(())
}
```

## Error Handling Best Practices

### Comprehensive Error Handling

```rust
use codex_core::claude_auth::{AuthError, ClaudeAuth};

async fn robust_authentication() -> Result<String, AuthError> {
    let claude_auth = ClaudeAuth::from_codex_home(
        &codex_home, 
        ClaudeAuthMode::MaxSubscription, 
        "code_cli"
    )?;

    match claude_auth.get_token().await {
        Ok(token) => Ok(token),
        Err(AuthError::TokenExpired) => {
            // Try to refresh token
            claude_auth.refresh_token().await
        }
        Err(AuthError::QuotaExceeded { current, limit }) => {
            // Handle quota exceeded
            eprintln!("Claude quota exceeded: {}/{}", current, limit);
            // Could switch to OpenAI here
            Err(AuthError::QuotaExceeded { current, limit })
        }
        Err(AuthError::NoCredentials) => {
            // Prompt user to authenticate
            eprintln!("No Claude credentials found. Please run: code auth login --provider claude");
            Err(AuthError::NoCredentials)
        }
        Err(e) => Err(e),
    }
}
```

### Fallback Implementation

```rust
async fn get_authenticated_provider() -> Result<AuthProvider, AuthError> {
    let auth_manager = AuthManager::new(codex_home, "code_cli".to_string());
    
    // Try Claude first
    if let Some(AuthProvider::Claude(claude_auth)) = auth_manager.get_provider(ProviderType::Claude) {
        match claude_auth.get_token().await {
            Ok(_) => return Ok(AuthProvider::Claude(claude_auth)),
            Err(AuthError::QuotaExceeded { .. }) => {
                eprintln!("Claude quota exceeded, falling back to OpenAI");
            }
            Err(e) => {
                eprintln!("Claude authentication failed: {}, falling back to OpenAI", e);
            }
        }
    }
    
    // Fallback to OpenAI
    if let Some(openai_provider) = auth_manager.get_provider(ProviderType::OpenAI) {
        Ok(openai_provider)
    } else {
        Err(AuthError::NoCredentials)
    }
}
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_claude_api_key_auth() {
        let claude_auth = ClaudeAuth::from_api_key("sk-ant-api03-test".to_string());
        let token = claude_auth.get_token().await.unwrap();
        assert_eq!(token, "sk-ant-api03-test");
    }

    #[tokio::test]
    async fn test_token_expiration() {
        let expired_tokens = ClaudeTokenData {
            access_token: "test-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Utc::now() - chrono::Duration::hours(1), // Expired
            subscription_tier: "max".to_string(),
            scope: Some("api".to_string()),
        };

        assert!(expired_tokens.is_expired());
        assert!(expired_tokens.needs_refresh(30));
    }

    #[test]
    fn test_subscription_quota_calculation() {
        let subscription = SubscriptionInfo {
            tier: "max".to_string(),
            usage_limit: 1000000,
            usage_current: 750000,
            reset_date: Utc::now() + chrono::Duration::days(1),
            features: vec!["unlimited_messages".to_string()],
            billing_cycle: "monthly".to_string(),
        };

        assert_eq!(subscription.usage_percentage(), 75.0);
        assert_eq!(subscription.quota_remaining(), 250000);
        assert!(!subscription.is_quota_exceeded());
    }
}
```

### Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_dual_provider_setup() {
        let temp_dir = TempDir::new().unwrap();
        let mut auth_manager = AuthManager::new(temp_dir.path().to_path_buf(), "test".to_string());

        // Add both providers
        let claude_auth = ClaudeAuth::from_api_key("test-claude-key".to_string());
        auth_manager.add_claude_auth(claude_auth);

        // Test provider selection
        auth_manager.set_preferred_provider(ProviderType::Claude);
        let provider = auth_manager.get_optimal_provider().await;
        assert!(matches!(provider, Some(AuthProvider::Claude(_))));

        auth_manager.set_preferred_provider(ProviderType::OpenAI);
        let provider = auth_manager.get_optimal_provider().await;
        // Should still return Claude since OpenAI not set up in this test
        assert!(matches!(provider, Some(AuthProvider::Claude(_))));
    }
}
```

This API reference provides comprehensive documentation for both internal Rust APIs and external HTTP integrations. For more examples and advanced usage patterns, see the implementation files in the `codex-rs/core/src/` directory.