//! Claude authentication module with enhanced security
//! 
//! This module provides secure Claude authentication with comprehensive security measures
//! including PKCE OAuth flows, encrypted token storage, audit logging, and session management.

pub mod secure_claude_auth;

pub use secure_claude_auth::{
    SecureClaudeAuth,
    ClaudeAuthError,
    ClaudeAuthConfig,
    ClaudeTokenData,
    ClaudeSubscriptionInfo,
    AuthenticationResult,
};

// Type aliases for backwards compatibility and simplified imports
pub type ClaudeAuth = SecureClaudeAuth;

/// Claude authentication mode enum for configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ClaudeAuthMode {
    /// API key based authentication
    ApiKey,
    /// OAuth based authentication
    OAuth,
    /// Session token based authentication
    SessionToken,
}

use std::path::PathBuf;
use crate::security::SecurityConfig;

/// Initialize Claude authentication with security configuration
pub fn init_claude_auth(
    config: ClaudeAuthConfig,
    security_config: Option<SecurityConfig>,
) -> Result<SecureClaudeAuth, ClaudeAuthError> {
    let security_cfg = security_config.unwrap_or_default();
    let storage_path = security_cfg.token_storage_path
        .parent()
        .unwrap_or(&PathBuf::from("."))
        .join("claude_tokens.json");
    
    SecureClaudeAuth::new(config, storage_path)
}

/// Create default Claude authentication configuration
pub fn default_claude_config() -> ClaudeAuthConfig {
    ClaudeAuthConfig::default()
}

/// Create Claude authentication configuration for production
pub fn production_claude_config(client_id: String, redirect_uri: String) -> ClaudeAuthConfig {
    ClaudeAuthConfig {
        client_id,
        redirect_uri,
        require_max_subscription: true,
        enable_subscription_check: true,
        auth_endpoint: "https://auth.anthropic.com/oauth/authorize".to_string(),
        token_endpoint: "https://auth.anthropic.com/oauth/token".to_string(),
        subscription_endpoint: "https://api.anthropic.com/v1/subscription".to_string(),
        scopes: vec!["api".to_string(), "subscription".to_string()],
    }
}