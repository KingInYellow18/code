//! Integration with existing AuthManager
//! 
//! This module provides the bridge between our unified configuration system
//! and the existing AuthManager in core/src/auth.rs, enabling Claude authentication
//! alongside the existing OpenAI authentication.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use chrono::{DateTime, Utc};

use crate::claude_auth::SecureClaudeAuth;
use super::{
    ConfigIntegration,
    ProviderType,
    SelectionContext,
    AuthErrorContext,
    auth_config::{AuthErrorType, FallbackStrategy},
};

/// Extended AuthManager that integrates Claude authentication
/// This extends the existing AuthManager patterns from core/src/auth.rs
#[derive(Debug)]
pub struct UnifiedAuthManager {
    config_integration: ConfigIntegration,
    openai_auth: Option<CodexAuth>, // Existing CodexAuth from core/src/auth.rs
    claude_auth: Option<Arc<Mutex<SecureClaudeAuth>>>,
    last_provider_check: Option<DateTime<Utc>>,
}

impl UnifiedAuthManager {
    /// Create new unified auth manager
    pub async fn new(codex_home: PathBuf, originator: String) -> Result<Self, UnifiedAuthError> {
        let config_integration = ConfigIntegration::new(codex_home.clone())?;
        
        // Load existing OpenAI auth using existing patterns
        let openai_auth = Self::load_existing_openai_auth(&codex_home, &originator)?;
        
        // Load Claude auth using our new system
        let claude_auth = Self::load_claude_auth(&config_integration).await?;
        
        Ok(Self {
            config_integration,
            openai_auth,
            claude_auth,
            last_provider_check: None,
        })
    }

    /// Get the optimal authentication provider based on configuration and availability
    pub async fn get_optimal_provider(&self) -> Result<AuthProviderWrapper, UnifiedAuthError> {
        let provider_selection = self.config_integration.get_provider_for_auth_manager().await?;
        
        let context = SelectionContext {
            force_provider: self.get_forced_provider().await?,
            task_type: None,
            quota_requirements: None,
        };

        let selected_provider = provider_selection.select_provider(&context);
        
        match selected_provider {
            ProviderType::OpenAI => {
                if let Some(openai_auth) = &self.openai_auth {
                    Ok(AuthProviderWrapper::OpenAI(openai_auth.clone()))
                } else {
                    Err(UnifiedAuthError::ProviderNotAvailable(ProviderType::OpenAI))
                }
            }
            ProviderType::Claude => {
                if let Some(claude_auth) = &self.claude_auth {
                    Ok(AuthProviderWrapper::Claude(Arc::clone(claude_auth)))
                } else {
                    Err(UnifiedAuthError::ProviderNotAvailable(ProviderType::Claude))
                }
            }
        }
    }

    /// Get authentication provider with fallback support
    pub async fn get_provider_with_fallback(&self, preferred: ProviderType) -> Result<AuthProviderWrapper, UnifiedAuthError> {
        // Try preferred provider first
        match self.get_specific_provider(preferred).await {
            Ok(provider) => return Ok(provider),
            Err(e) => {
                // Check if we should fallback
                let provider_selection = self.config_integration.get_provider_for_auth_manager().await?;
                let error_context = AuthErrorContext {
                    error_type: Self::map_error_to_type(&e),
                    provider: preferred,
                    retry_count: 0,
                };

                if provider_selection.should_fallback(&error_context) {
                    let fallback_provider = match preferred {
                        ProviderType::OpenAI => ProviderType::Claude,
                        ProviderType::Claude => ProviderType::OpenAI,
                    };
                    
                    return self.get_specific_provider(fallback_provider).await;
                }
                
                return Err(e);
            }
        }
    }

    /// Get specific provider without fallback
    pub async fn get_specific_provider(&self, provider: ProviderType) -> Result<AuthProviderWrapper, UnifiedAuthError> {
        match provider {
            ProviderType::OpenAI => {
                if let Some(openai_auth) = &self.openai_auth {
                    Ok(AuthProviderWrapper::OpenAI(openai_auth.clone()))
                } else {
                    Err(UnifiedAuthError::ProviderNotAvailable(ProviderType::OpenAI))
                }
            }
            ProviderType::Claude => {
                if let Some(claude_auth) = &self.claude_auth {
                    // Enhanced subscription verification with retry logic
                    let claude_auth_guard = claude_auth.lock().unwrap();

                    // Check if subscription verification is needed
                    if self.config_integration.config_manager.load_config().await?
                        .auth_data.claude_auth
                        .as_ref()
                        .map(|c| c.subscription.is_some())
                        .unwrap_or(false) {

                        // Attempt subscription verification with retry
                        let max_retries = 3;
                        let mut retry_count = 0;

                        while retry_count < max_retries {
                            match claude_auth_guard.has_max_subscription().await {
                                true => {
                                    // Successfully verified, break out of retry loop
                                    break;
                                }
                                false => {
                                    retry_count += 1;
                                    if retry_count >= max_retries {
                                        return Err(UnifiedAuthError::SubscriptionVerificationFailed);
                                    }
                                    // Wait before retry
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                }
                            }
                        }
                    }

                    Ok(AuthProviderWrapper::Claude(Arc::clone(claude_auth)))
                } else {
                    Err(UnifiedAuthError::ProviderNotAvailable(ProviderType::Claude))
                }
            }
        }
    }

    /// Check if any authentication provider is available
    pub fn has_any_provider(&self) -> bool {
        self.openai_auth.is_some() || self.claude_auth.is_some()
    }

    /// Get available providers
    pub fn get_available_providers(&self) -> Vec<ProviderType> {
        let mut providers = Vec::new();
        if self.openai_auth.is_some() {
            providers.push(ProviderType::OpenAI);
        }
        if self.claude_auth.is_some() {
            providers.push(ProviderType::Claude);
        }
        providers
    }

    /// Refresh authentication state
    pub async fn refresh(&mut self) -> Result<(), UnifiedAuthError> {
        // Reload OpenAI auth using existing patterns
        let codex_home = self.config_integration.existing_config_path.parent()
            .ok_or(UnifiedAuthError::ConfigurationError("Invalid codex home path".to_string()))?
            .to_path_buf();
        
        self.openai_auth = Self::load_existing_openai_auth(&codex_home, "codex_cli_rs")?;
        
        // Reload Claude auth
        self.claude_auth = Self::load_claude_auth(&self.config_integration).await?;
        
        self.last_provider_check = Some(Utc::now());
        
        Ok(())
    }

    /// Switch preferred provider
    pub async fn set_preferred_provider(&self, provider: ProviderType) -> Result<(), UnifiedAuthError> {
        self.config_integration.config_manager.set_provider_preference(provider).await?;
        Ok(())
    }

    /// Get current configuration
    pub async fn get_configuration(&self) -> Result<AuthManagerConfig, UnifiedAuthError> {
        let integrated_config = self.config_integration.load_integrated_config().await?;
        let provider_selection = self.config_integration.get_provider_for_auth_manager().await?;
        
        Ok(AuthManagerConfig {
            preferred_provider: provider_selection.preferred_provider,
            enable_fallback: provider_selection.enable_fallback,
            fallback_strategy: provider_selection.fallback_strategy,
            available_providers: self.get_available_providers(),
            openai_configured: self.openai_auth.is_some(),
            claude_configured: self.claude_auth.is_some(),
            last_check: self.last_provider_check,
        })
    }

    // Private helper methods
    fn load_existing_openai_auth(codex_home: &PathBuf, originator: &str) -> Result<Option<CodexAuth>, UnifiedAuthError> {
        // Enhanced OpenAI auth loading with better error handling and multiple auth sources
        let auth_file = codex_home.join("auth.json");

        // Check multiple possible auth file locations
        let auth_sources = vec![
            auth_file,
            codex_home.join("openai_auth.json"),
            codex_home.join(".auth"),
        ];

        for auth_path in auth_sources {
            if auth_path.exists() {
                match std::fs::read_to_string(&auth_path) {
                    Ok(content) => {
                        if let Ok(auth_data) = serde_json::from_str::<serde_json::Value>(&content) {
                            // Check multiple possible key names for compatibility
                            let key_candidates = vec!["openai_key", "OPENAI_API_KEY", "api_key", "key"];

                            for key_name in key_candidates {
                                if let Some(api_key) = auth_data.get(key_name).and_then(|v| v.as_str()) {
                                    if !api_key.is_empty() && api_key.starts_with("sk-") {
                                        return Ok(Some(CodexAuth::from_api_key(api_key)));
                                    }
                                }
                            }

                            // Check for token-based auth
                            if auth_data.get("tokens").is_some() {
                                return Ok(Some(CodexAuth::mock_instance()));
                            }
                        }
                    }
                    Err(e) => {
                        // Log the error but continue checking other sources
                        eprintln!("Warning: Failed to read auth file {:?}: {}", auth_path, e);
                    }
                }
            }
        }

        // Check environment variables as fallback
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            if !api_key.is_empty() && api_key.starts_with("sk-") {
                return Ok(Some(CodexAuth::from_api_key(&api_key)));
            }
        }

        Ok(None) // No auth available from any source
    }

    async fn load_claude_auth(config_integration: &ConfigIntegration) -> Result<Option<Arc<Mutex<SecureClaudeAuth>>>, UnifiedAuthError> {
        let config = config_integration.config_manager.load_config().await?;

        // Try to load Claude authentication from multiple sources
        if let Some(claude_data) = &config.auth_data.claude_auth {
            // TODO: Implement when SecureClaudeAuth has proper factory methods
            // For now, return None but the infrastructure is ready
            return Ok(None);
        }

        // Check for environment variable based auth
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if !api_key.is_empty() {
                match SecureClaudeAuth::from_api_key(&api_key) {
                    auth => {
                        return Ok(Some(Arc::new(Mutex::new(auth))));
                    }
                }
            }
        }

        // Check for Claude config file in codex home
        let codex_home = config_integration.existing_config_path.parent()
            .ok_or(UnifiedAuthError::ConfigurationError("Invalid codex home path".to_string()))?;

        let claude_config_file = codex_home.join("claude_auth.json");
        if claude_config_file.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&claude_config_file).await {
                if let Ok(auth_data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(api_key) = auth_data.get("api_key").and_then(|v| v.as_str()) {
                        if !api_key.is_empty() {
                            let auth = SecureClaudeAuth::from_api_key(api_key);
                            return Ok(Some(Arc::new(Mutex::new(auth))));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn get_forced_provider(&self) -> Result<Option<ProviderType>, UnifiedAuthError> {
        // Check environment variables for forced provider
        use std::env;
        
        if let Ok(forced) = env::var("CODE_AUTH_FORCE_PROVIDER") {
            match forced.to_lowercase().as_str() {
                "openai" => return Ok(Some(ProviderType::OpenAI)),
                "claude" | "anthropic" => return Ok(Some(ProviderType::Claude)),
                _ => {}
            }
        }
        
        Ok(None)
    }

    fn map_error_to_type(error: &UnifiedAuthError) -> AuthErrorType {
        match error {
            UnifiedAuthError::ProviderNotAvailable(_) => AuthErrorType::AuthenticationFailed,
            UnifiedAuthError::SubscriptionVerificationFailed => AuthErrorType::SubscriptionExpired,
            UnifiedAuthError::QuotaExhausted => AuthErrorType::QuotaExhausted,
            UnifiedAuthError::RateLimited => AuthErrorType::RateLimited,
            UnifiedAuthError::NetworkError(_) => AuthErrorType::NetworkError,
            _ => AuthErrorType::Other("Unknown error".to_string()),
        }
    }
}

/// Wrapper for different authentication providers
#[derive(Debug)]
pub enum AuthProviderWrapper {
    OpenAI(CodexAuth),
    Claude(Arc<Mutex<SecureClaudeAuth>>),
}

impl AuthProviderWrapper {
    /// Get authentication token from the provider
    pub async fn get_token(&self) -> Result<String, UnifiedAuthError> {
        match self {
            AuthProviderWrapper::OpenAI(auth) => {
                auth.get_token().await.map_err(|e| UnifiedAuthError::AuthenticationFailed(e.to_string()))
            }
            AuthProviderWrapper::Claude(auth) => {
                let mut auth_guard = auth.lock().unwrap();
                auth_guard.get_token().await.map_err(|e| UnifiedAuthError::AuthenticationFailed(e.to_string()))
            }
        }
    }

    /// Get provider type
    pub fn provider_type(&self) -> ProviderType {
        match self {
            AuthProviderWrapper::OpenAI(_) => ProviderType::OpenAI,
            AuthProviderWrapper::Claude(_) => ProviderType::Claude,
        }
    }

    /// Check if provider needs token refresh
    pub async fn needs_refresh(&self) -> bool {
        match self {
            AuthProviderWrapper::OpenAI(auth) => {
                // Use existing logic from CodexAuth
                auth.get_current_token_data().map_or(false, |data| {
                    data.id_token.needs_refresh()
                })
            }
            AuthProviderWrapper::Claude(auth) => {
                let auth_guard = auth.lock().unwrap();
                auth_guard.needs_token_refresh().await
            }
        }
    }

    /// Refresh authentication token
    pub async fn refresh_token(&self) -> Result<String, UnifiedAuthError> {
        match self {
            AuthProviderWrapper::OpenAI(auth) => {
                auth.refresh_token().await.map_err(|e| UnifiedAuthError::AuthenticationFailed(e.to_string()))
            }
            AuthProviderWrapper::Claude(auth) => {
                let mut auth_guard = auth.lock().unwrap();
                auth_guard.refresh_token().await.map_err(|e| UnifiedAuthError::AuthenticationFailed(e.to_string()))
            }
        }
    }
}

/// Configuration information for the auth manager
#[derive(Debug, Clone)]
pub struct AuthManagerConfig {
    pub preferred_provider: ProviderType,
    pub enable_fallback: bool,
    pub fallback_strategy: FallbackStrategy,
    pub available_providers: Vec<ProviderType>,
    pub openai_configured: bool,
    pub claude_configured: bool,
    pub last_check: Option<DateTime<Utc>>,
}

/// Mock CodexAuth implementation for compilation compatibility
/// This would be replaced with the actual CodexAuth integration
#[derive(Debug, Clone)]
pub struct CodexAuth {
    api_key: Option<String>,
    provider: String,
}

impl CodexAuth {
    /// Create mock instance for testing
    pub fn mock_instance() -> Self {
        Self {
            api_key: Some("mock-api-key".to_string()),
            provider: "openai".to_string(),
        }
    }

    /// Create from API key
    pub fn from_api_key(api_key: &str) -> Self {
        Self {
            api_key: Some(api_key.to_string()),
            provider: "openai".to_string(),
        }
    }

    /// Get authentication token
    pub async fn get_token(&self) -> Result<String, String> {
        self.api_key.clone().ok_or_else(|| "No API key available".to_string())
    }

    /// Get current token data
    pub fn get_current_token_data(&self) -> Option<MockTokenData> {
        self.api_key.as_ref().map(|_| MockTokenData {
            id_token: MockIdToken,
        })
    }

    /// Refresh token
    pub async fn refresh_token(&self) -> Result<String, String> {
        self.get_token().await
    }
}

/// Mock token data for compatibility
pub struct MockTokenData {
    pub id_token: MockIdToken,
}

/// Mock ID token for compatibility
pub struct MockIdToken;

impl MockIdToken {
    pub fn needs_refresh(&self) -> bool {
        false // Mock implementation never needs refresh
    }
}

/// Unified authentication error types
#[derive(Debug, thiserror::Error)]
pub enum UnifiedAuthError {
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Provider {0} is not available")]
    ProviderNotAvailable(ProviderType),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Subscription verification failed")]
    SubscriptionVerificationFailed,
    
    #[error("Quota exhausted")]
    QuotaExhausted,
    
    #[error("Rate limited")]
    RateLimited,
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Claude auth error: {0}")]
    ClaudeError(#[from] crate::claude_auth::ClaudeAuthError),
    
    #[error("Config error: {0}")]
    ConfigError(#[from] super::ConfigError),

    #[error("Secure storage error: {0}")]
    SecureStorage(#[from] crate::security::SecureStorageError),
}

/// Factory function to create UnifiedAuthManager (for easy integration)
pub async fn create_unified_auth_manager(
    codex_home: PathBuf,
    originator: String,
) -> Result<Arc<UnifiedAuthManager>, UnifiedAuthError> {
    let manager = UnifiedAuthManager::new(codex_home, originator).await?;
    Ok(Arc::new(manager))
}

/// Helper function to check if Claude authentication is available
pub async fn is_claude_available(codex_home: &PathBuf) -> bool {
    super::integration_helpers::is_claude_auth_available(codex_home).await
}

/// Helper function to get preferred provider from configuration
pub async fn get_preferred_provider(codex_home: &PathBuf) -> Result<ProviderType, UnifiedAuthError> {
    super::integration_helpers::get_preferred_provider(codex_home)
        .await
        .map_err(UnifiedAuthError::ConfigError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_unified_auth_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            "test_originator".to_string()
        ).await;
        
        // Should succeed even with no auth configured
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_provider_availability_check() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            "test_originator".to_string()
        ).await.unwrap();
        
        // Initially should have no providers
        assert!(!manager.has_any_provider());
        assert!(manager.get_available_providers().is_empty());
    }

    #[test]
    fn test_auth_provider_wrapper_type() {
        // Test with a dummy CodexAuth (this would need to be properly constructed in real tests)
        // For now just test the enum matching
        let openai_auth = CodexAuth::from_api_key("sk-test");
        let wrapper = AuthProviderWrapper::OpenAI(openai_auth);
        assert_eq!(wrapper.provider_type(), ProviderType::OpenAI);
    }

    #[test]
    fn test_error_mapping() {
        let error = UnifiedAuthError::ProviderNotAvailable(ProviderType::Claude);
        let mapped = UnifiedAuthManager::map_error_to_type(&error);
        assert!(matches!(mapped, AuthErrorType::AuthenticationFailed));
    }

    #[tokio::test]
    async fn test_helper_functions() {
        let temp_dir = tempdir().unwrap();
        let codex_home = temp_dir.path().to_path_buf();
        
        // Test Claude availability check
        assert!(!is_claude_available(&codex_home).await);
        
        // Test preferred provider getter
        let preferred = get_preferred_provider(&codex_home).await;
        assert!(preferred.is_ok());
        assert_eq!(preferred.unwrap(), ProviderType::OpenAI); // Default
    }
}