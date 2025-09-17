use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration};
use thiserror::Error;

use crate::security::{
    SecureTokenStorage, SecureOAuthFlow, OAuthSecurityManager,
    SessionSecurityManager, SecurityError, audit_logger, AuditLogError,
    SecureStorageError, SessionSecurityError, OAuthSecurityError
};

/// Enhanced secure Claude authentication with comprehensive security measures
#[derive(Debug)]
pub struct SecureClaudeAuth {
    client_id: String,
    redirect_uri: String,
    storage: SecureTokenStorage,
    oauth_manager: OAuthSecurityManager,
    session_manager: SessionSecurityManager,
    config: ClaudeAuthConfig,
}

#[derive(Debug, Error)]
pub enum ClaudeAuthError {
    #[error("Security error: {0}")]
    Security(#[from] SecurityError),
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("Token validation failed: {0}")]
    TokenValidationFailed(String),
    #[error("Subscription verification failed: {0}")]
    SubscriptionVerificationFailed(String),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Audit error: {0}")]
    Audit(String),
    #[error("OAuth security error: {0}")]
    OAuth(String),
    #[error("Secure storage error: {0}")]
    SecureStorage(#[from] SecureStorageError),
    #[error("Audit log error: {0}")]
    AuditLog(#[from] AuditLogError),
    #[error("Session security error: {0}")]
    SessionSecurity(#[from] SessionSecurityError),
    #[error("OAuth security error: {0}")]
    OAuthSecurity(#[from] OAuthSecurityError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAuthConfig {
    pub client_id: String,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub subscription_endpoint: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub require_max_subscription: bool,
    pub enable_subscription_check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub token_type: String,
    pub expires_at: DateTime<Utc>,
    pub subscription_tier: Option<String>,
    pub account_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSubscriptionInfo {
    pub tier: String,
    pub usage_limit: Option<u64>,
    pub usage_current: Option<u64>,
    pub reset_date: Option<DateTime<Utc>>,
    pub features: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationResult {
    pub success: bool,
    pub tokens: Option<ClaudeTokenData>,
    pub subscription: Option<ClaudeSubscriptionInfo>,
    pub session_id: String,
    pub error: Option<String>,
}

impl Default for ClaudeAuthConfig {
    fn default() -> Self {
        Self {
            client_id: "claude_code_client".to_string(),
            auth_endpoint: "https://auth.anthropic.com/oauth/authorize".to_string(),
            token_endpoint: "https://auth.anthropic.com/oauth/token".to_string(),
            subscription_endpoint: "https://api.anthropic.com/v1/subscription".to_string(),
            redirect_uri: "http://localhost:1456/auth/callback".to_string(),
            scopes: vec!["api".to_string(), "subscription".to_string()],
            require_max_subscription: false,
            enable_subscription_check: true,
        }
    }
}

impl SecureClaudeAuth {
    /// Create new secure Claude authentication instance
    pub fn new(
        config: ClaudeAuthConfig,
        storage_path: PathBuf,
    ) -> Result<Self, ClaudeAuthError> {
        let storage = SecureTokenStorage::new(storage_path)?;
        let oauth_manager = OAuthSecurityManager::new(3); // Max 3 concurrent flows
        let session_manager = SessionSecurityManager::new(Default::default());

        Ok(Self {
            client_id: config.client_id.clone(),
            redirect_uri: config.redirect_uri.clone(),
            storage,
            oauth_manager,
            session_manager,
            config,
        })
    }

    /// Start OAuth authentication flow with enhanced security
    pub fn start_oauth_flow(&mut self) -> Result<String, ClaudeAuthError> {
        // Start secure OAuth flow
        let session_id = self.oauth_manager.start_flow(
            self.config.client_id.clone(),
            self.config.redirect_uri.clone(),
        )?;

        // Get the OAuth flow
        let flow = self.oauth_manager.get_flow(&session_id)
            .ok_or_else(|| ClaudeAuthError::AuthenticationFailed("Failed to create OAuth flow".to_string()))?;

        // Generate authorization URL
        let auth_request = flow.generate_authorization_url(
            &self.config.auth_endpoint,
            &self.config.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        )?;

        // Log OAuth start event
        audit_logger::log_audit_event(audit_logger::AuditEvent {
            timestamp: Utc::now(),
            event_type: audit_logger::AuthEventType::OAuthStart,
            user_id: None,
            session_id: Some(session_id.clone()),
            client_id: Some(self.config.client_id.clone()),
            ip_address: None,
            user_agent: None,
            success: true,
            error_message: None,
            metadata: serde_json::json!({
                "auth_url": auth_request.authorization_url,
                "scopes": self.config.scopes
            }),
            severity: audit_logger::Severity::Info,
        })?;

        Ok(auth_request.authorization_url)
    }

    /// Handle OAuth callback with security validation
    pub async fn handle_oauth_callback(
        &mut self,
        session_id: &str,
        code: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<AuthenticationResult, ClaudeAuthError> {
        // Get OAuth flow
        let flow = self.oauth_manager.complete_flow(session_id)
            .ok_or_else(|| ClaudeAuthError::AuthenticationFailed("OAuth session not found".to_string()))?;

        // Validate callback parameters
        let token_request = flow.validate_callback(code, state, error)
            .map_err(|e| ClaudeAuthError::OAuth(e.to_string()))?;

        // Exchange code for tokens
        let tokens = self.exchange_authorization_code(&token_request).await?;

        // Verify subscription if required
        let subscription = if self.config.enable_subscription_check {
            self.verify_subscription(&tokens.access_token).await.ok()
        } else {
            None
        };

        // Check subscription requirements
        if self.config.require_max_subscription {
            if let Some(ref sub) = subscription {
                if sub.tier != "max" && sub.tier != "pro" {
                    let error_msg = format!("Required subscription tier not met: got {}, need max or pro", sub.tier);
                    
                    // Log subscription verification failure
                    audit_logger::log_audit_event(audit_logger::AuditEvent {
                        timestamp: Utc::now(),
                        event_type: audit_logger::AuthEventType::OAuthError,
                        user_id: tokens.user_id.clone(),
                        session_id: Some(session_id.to_string()),
                        client_id: Some(self.config.client_id.clone()),
                        ip_address: None,
                        user_agent: None,
                        success: false,
                        error_message: Some(error_msg.clone()),
                        metadata: serde_json::json!({"subscription_tier": sub.tier}),
                        severity: audit_logger::Severity::Warning,
                    })?;

                    return Ok(AuthenticationResult {
                        success: false,
                        tokens: None,
                        subscription,
                        session_id: session_id.to_string(),
                        error: Some(error_msg),
                    });
                }
            } else {
                return Err(ClaudeAuthError::SubscriptionVerificationFailed(
                    "Could not verify subscription status".to_string()
                ));
            }
        }

        // Store tokens securely
        let storage_tokens = crate::security::secure_token_storage::TokenData {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
            id_token: tokens.id_token.clone(),
            expires_at: tokens.expires_at,
            account_id: tokens.account_id.clone(),
            provider: "claude".to_string(),
        };
        self.storage.store_tokens(&storage_tokens)?;

        // Create secure session
        let session_context = crate::security::session_security::SessionValidationContext {
            ip_address: None, // Would be populated from request context
            user_agent: None, // Would be populated from request context
            requested_scopes: self.config.scopes.clone(),
            current_time: Utc::now(),
        };

        let session = self.session_manager.create_session(
            tokens.user_id.clone().unwrap_or_default(),
            self.config.client_id.clone(),
            self.config.scopes.clone(),
            &session_context,
        )?;

        // Log successful authentication
        audit_logger::log_login_success(
            tokens.user_id.clone(),
            Some(session.session_id.clone()),
            Some(self.config.client_id.clone()),
            None,
        )?;

        Ok(AuthenticationResult {
            success: true,
            tokens: Some(tokens),
            subscription,
            session_id: session.session_id,
            error: None,
        })
    }

    /// Refresh tokens securely
    pub async fn refresh_tokens(&mut self, session_id: &str) -> Result<ClaudeTokenData, ClaudeAuthError> {
        // Get stored tokens
        let stored_tokens = self.storage.retrieve_tokens()?
            .ok_or_else(|| ClaudeAuthError::TokenValidationFailed("No stored tokens found".to_string()))?;

        // Prepare refresh request
        let refresh_request = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": stored_tokens.refresh_token,
            "client_id": self.config.client_id,
        });

        // Make token refresh request
        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.token_endpoint)
            .header("Content-Type", "application/json")
            .json(&refresh_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_msg = format!("Token refresh failed: {}", response.status());
            
            // Log token refresh failure
            audit_logger::log_audit_event(audit_logger::AuditEvent {
                timestamp: Utc::now(),
                event_type: audit_logger::AuthEventType::TokenRefresh,
                user_id: stored_tokens.account_id.clone(),
                session_id: Some(session_id.to_string()),
                client_id: Some(self.config.client_id.clone()),
                ip_address: None,
                user_agent: None,
                success: false,
                error_message: Some(error_msg.clone()),
                metadata: serde_json::json!({}),
                severity: audit_logger::Severity::Warning,
            })?;

            return Err(ClaudeAuthError::TokenValidationFailed(error_msg));
        }

        let token_response: serde_json::Value = response.json().await?;
        
        // Parse new tokens
        let new_tokens = ClaudeTokenData {
            access_token: token_response["access_token"]
                .as_str()
                .ok_or_else(|| ClaudeAuthError::TokenValidationFailed("Missing access token".to_string()))?
                .to_string(),
            refresh_token: token_response["refresh_token"]
                .as_str()
                .unwrap_or(&stored_tokens.refresh_token)
                .to_string(),
            id_token: token_response["id_token"]
                .as_str()
                .unwrap_or(&stored_tokens.id_token)
                .to_string(),
            token_type: token_response["token_type"]
                .as_str()
                .unwrap_or("Bearer")
                .to_string(),
            expires_at: Utc::now() + Duration::seconds(
                token_response["expires_in"]
                    .as_i64()
                    .unwrap_or(3600)
            ),
            subscription_tier: None, // Will be populated by subscription check
            account_id: stored_tokens.account_id.clone(),
            user_id: None, // Would be extracted from ID token
        };

        // Store updated tokens
        let storage_tokens = crate::security::secure_token_storage::TokenData {
            access_token: new_tokens.access_token.clone(),
            refresh_token: new_tokens.refresh_token.clone(),
            id_token: new_tokens.id_token.clone(),
            expires_at: new_tokens.expires_at,
            account_id: new_tokens.account_id.clone(),
            provider: "claude".to_string(),
        };
        self.storage.store_tokens(&storage_tokens)?;

        // Log successful token refresh
        audit_logger::log_audit_event(audit_logger::AuditEvent {
            timestamp: Utc::now(),
            event_type: audit_logger::AuthEventType::TokenRefresh,
            user_id: new_tokens.account_id.clone(),
            session_id: Some(session_id.to_string()),
            client_id: Some(self.config.client_id.clone()),
            ip_address: None,
            user_agent: None,
            success: true,
            error_message: None,
            metadata: serde_json::json!({}),
            severity: audit_logger::Severity::Info,
        })?;

        Ok(new_tokens)
    }

    /// Verify Claude subscription status
    pub async fn verify_subscription(&self, access_token: &str) -> Result<ClaudeSubscriptionInfo, ClaudeAuthError> {
        let client = reqwest::Client::new();
        let response = client
            .get(&self.config.subscription_endpoint)
            .bearer_auth(access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClaudeAuthError::SubscriptionVerificationFailed(
                format!("Subscription check failed: {}", response.status())
            ));
        }

        let subscription_data: serde_json::Value = response.json().await?;
        
        Ok(ClaudeSubscriptionInfo {
            tier: subscription_data["tier"]
                .as_str()
                .unwrap_or("free")
                .to_string(),
            usage_limit: subscription_data["usage_limit"].as_u64(),
            usage_current: subscription_data["usage_current"].as_u64(),
            reset_date: subscription_data["reset_date"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            features: subscription_data["features"]
                .as_array()
                .map(|arr| arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect())
                .unwrap_or_default(),
            active: subscription_data["active"]
                .as_bool()
                .unwrap_or(false),
        })
    }

    /// Logout and clear all stored tokens
    pub fn logout(&mut self, session_id: Option<&str>) -> Result<(), ClaudeAuthError> {
        // Delete stored tokens
        self.storage.delete_tokens()?;

        // Destroy session if provided
        if let Some(sid) = session_id {
            self.session_manager.destroy_session(sid).ok();
        }

        // Log logout event
        audit_logger::log_audit_event(audit_logger::AuditEvent {
            timestamp: Utc::now(),
            event_type: audit_logger::AuthEventType::Logout,
            user_id: None,
            session_id: session_id.map(|s| s.to_string()),
            client_id: Some(self.config.client_id.clone()),
            ip_address: None,
            user_agent: None,
            success: true,
            error_message: None,
            metadata: serde_json::json!({}),
            severity: audit_logger::Severity::Info,
        })?;

        Ok(())
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.storage.tokens_exist()
    }

    /// Get stored tokens if available
    pub fn get_stored_tokens(&self) -> Result<Option<ClaudeTokenData>, ClaudeAuthError> {
        if let Some(tokens) = self.storage.retrieve_tokens()? {
            Ok(Some(ClaudeTokenData {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                id_token: tokens.id_token,
                token_type: "Bearer".to_string(),
                expires_at: tokens.expires_at,
                subscription_tier: None,
                account_id: tokens.account_id,
                user_id: None,
            }))
        } else {
            Ok(None)
        }
    }

    /// Create ClaudeAuth from API key
    pub fn from_api_key(api_key: &str) -> Self {
        let config = ClaudeAuthConfig {
            client_id: "api_key_client".to_string(),
            auth_endpoint: "".to_string(),
            token_endpoint: "".to_string(),
            subscription_endpoint: "https://api.anthropic.com/v1/subscription".to_string(),
            redirect_uri: "".to_string(),
            scopes: vec!["api".to_string()],
            require_max_subscription: false,
            enable_subscription_check: true,
        };

        let storage_path = std::env::temp_dir().join("claude_api_tokens.json");
        let mut auth = Self::new(config, storage_path).expect("Failed to create auth instance");

        // Store API key as access token
        let token_data = crate::security::secure_token_storage::TokenData {
            access_token: api_key.to_string(),
            refresh_token: "".to_string(),
            id_token: "".to_string(),
            expires_at: Utc::now() + Duration::days(365), // API keys don't expire
            account_id: None,
            provider: "claude".to_string(),
        };
        auth.storage.store_tokens(&token_data).ok();

        auth
    }

    /// Create ClaudeAuth from OAuth tokens
    pub fn from_oauth_tokens(
        access_token: String,
        refresh_token: String,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, ClaudeAuthError> {
        let config = ClaudeAuthConfig::default();
        let storage_path = std::env::temp_dir().join("claude_oauth_tokens.json");
        let mut auth = Self::new(config, storage_path)?;

        // Store OAuth tokens
        let token_data = crate::security::secure_token_storage::TokenData {
            access_token,
            refresh_token,
            id_token: "".to_string(),
            expires_at,
            account_id: None,
            provider: "claude".to_string(),
        };
        auth.storage.store_tokens(&token_data)?;

        Ok(auth)
    }

    /// Check if has max subscription
    pub async fn has_max_subscription(&self) -> bool {
        if let Ok(Some(tokens)) = self.get_stored_tokens() {
            if let Ok(subscription) = self.verify_subscription(&tokens.access_token).await {
                return subscription.tier == "max" || subscription.tier == "pro";
            }
        }
        false
    }

    /// Get authentication token
    pub async fn get_token(&mut self) -> Result<String, ClaudeAuthError> {
        if let Some(tokens) = self.get_stored_tokens()? {
            // Check if token needs refresh
            if tokens.expires_at <= Utc::now() + Duration::minutes(5) {
                // Token is expiring soon, try to refresh
                if !tokens.refresh_token.is_empty() {
                    let new_tokens = self.refresh_tokens("default_session").await?;
                    return Ok(new_tokens.access_token);
                }
            }
            Ok(tokens.access_token)
        } else {
            Err(ClaudeAuthError::AuthenticationFailed("No tokens available".to_string()))
        }
    }

    /// Check if token refresh is needed
    pub async fn needs_token_refresh(&self) -> bool {
        if let Ok(Some(tokens)) = self.get_stored_tokens() {
            return tokens.expires_at <= Utc::now() + Duration::minutes(5);
        }
        true // If no tokens, refresh is needed
    }

    /// Refresh token
    pub async fn refresh_token(&mut self) -> Result<String, ClaudeAuthError> {
        let new_tokens = self.refresh_tokens("default_session").await?;
        Ok(new_tokens.access_token)
    }

    /// Exchange authorization code for tokens
    async fn exchange_authorization_code(
        &self,
        token_request: &crate::security::oauth_security::TokenExchangeRequest,
    ) -> Result<ClaudeTokenData, ClaudeAuthError> {
        let exchange_request = serde_json::json!({
            "grant_type": "authorization_code",
            "code": token_request.code,
            "redirect_uri": token_request.redirect_uri,
            "client_id": self.config.client_id,
            "code_verifier": token_request.code_verifier,
        });

        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.token_endpoint)
            .header("Content-Type", "application/json")
            .json(&exchange_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ClaudeAuthError::AuthenticationFailed(
                format!("Token exchange failed: {}", response.status())
            ));
        }

        let token_response: serde_json::Value = response.json().await?;
        
        Ok(ClaudeTokenData {
            access_token: token_response["access_token"]
                .as_str()
                .ok_or_else(|| ClaudeAuthError::TokenValidationFailed("Missing access token".to_string()))?
                .to_string(),
            refresh_token: token_response["refresh_token"]
                .as_str()
                .ok_or_else(|| ClaudeAuthError::TokenValidationFailed("Missing refresh token".to_string()))?
                .to_string(),
            id_token: token_response["id_token"]
                .as_str()
                .ok_or_else(|| ClaudeAuthError::TokenValidationFailed("Missing ID token".to_string()))?
                .to_string(),
            token_type: token_response["token_type"]
                .as_str()
                .unwrap_or("Bearer")
                .to_string(),
            expires_at: Utc::now() + Duration::seconds(
                token_response["expires_in"]
                    .as_i64()
                    .unwrap_or(3600)
            ),
            subscription_tier: None,
            account_id: None, // Would be extracted from ID token
            user_id: None,    // Would be extracted from ID token
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_secure_claude_auth_creation() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("claude_tokens.json");
        let config = ClaudeAuthConfig::default();
        
        let auth = SecureClaudeAuth::new(config, storage_path).unwrap();
        assert!(!auth.is_authenticated()); // No tokens stored initially
    }

    #[test]
    fn test_oauth_flow_start() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("claude_tokens.json");
        let config = ClaudeAuthConfig::default();
        
        let mut auth = SecureClaudeAuth::new(config, storage_path).unwrap();
        let auth_url = auth.start_oauth_flow().unwrap();
        
        assert!(auth_url.contains("oauth/authorize"));
        assert!(auth_url.contains("code_challenge"));
        assert!(auth_url.contains("state"));
    }

    #[test]
    fn test_subscription_info_parsing() {
        let subscription_json = serde_json::json!({
            "tier": "max",
            "usage_limit": 1000000,
            "usage_current": 50000,
            "features": ["unlimited_messages", "priority_access"],
            "active": true
        });

        let subscription: ClaudeSubscriptionInfo = serde_json::from_value(subscription_json).unwrap();
        assert_eq!(subscription.tier, "max");
        assert_eq!(subscription.usage_limit, Some(1000000));
        assert!(subscription.active);
    }
}