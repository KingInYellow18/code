use std::path::PathBuf;
use std::sync::Arc;
use crate::auth::{AuthManager as OpenAIAuthManager};
use crate::claude_auth::{ClaudeAuth, ClaudeAuthMode, read_claude_api_key_from_env};
use codex_protocol::mcp_protocol::AuthMode as OpenAIAuthMode;

/// Provider types for the unified authentication system
#[derive(Debug, Clone, PartialEq)]
pub enum AuthProvider {
    OpenAI,
    Claude,
}

/// Selection strategy for choosing between authentication providers
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderSelectionStrategy {
    /// Always prefer Claude if available, fallback to OpenAI
    PreferClaude,
    /// Always prefer OpenAI if available, fallback to Claude
    PreferOpenAI,
    /// Choose based on subscription status (Claude Max > OpenAI Pro > Claude Pro > API keys)
    IntelligentSelection,
    /// Use user's explicit preference
    UserPreference(AuthProvider),
}

/// Unified authentication manager that coordinates between OpenAI and Claude providers
#[derive(Debug)]
pub struct UnifiedAuthManager {
    openai_manager: Arc<OpenAIAuthManager>,
    claude_auth: Option<ClaudeAuth>,
    codex_home: PathBuf,
    selection_strategy: ProviderSelectionStrategy,
    preferred_provider: Option<AuthProvider>,
    client: reqwest::Client,
}

impl UnifiedAuthManager {
    /// Create new unified authentication manager
    pub fn new(
        codex_home: PathBuf,
        preferred_auth_mode: OpenAIAuthMode,
        originator: String,
        selection_strategy: ProviderSelectionStrategy,
    ) -> Self {
        let client = crate::default_client::create_client(&originator);
        let openai_manager = OpenAIAuthManager::shared(
            codex_home.clone(), 
            preferred_auth_mode, 
            originator
        );

        Self {
            openai_manager,
            claude_auth: None,
            codex_home,
            selection_strategy,
            preferred_provider: None,
            client,
        }
    }

    /// Initialize Claude authentication if available
    pub async fn initialize_claude_auth(&mut self) -> Result<(), std::io::Error> {
        // Try to load from file first
        let claude_auth_file = crate::claude_auth::get_claude_auth_file(&self.codex_home);
        
        if let Ok(Some(claude_auth)) = ClaudeAuth::load_from_file(&claude_auth_file, self.client.clone()).await {
            self.claude_auth = Some(claude_auth);
        } else if let Some(api_key) = read_claude_api_key_from_env() {
            // Fallback to environment variable
            self.claude_auth = Some(ClaudeAuth::from_api_key(&api_key, self.client.clone()));
        }

        Ok(())
    }

    /// Get the optimal authentication provider based on current strategy
    pub async fn select_optimal_provider(&self) -> Result<AuthProvider, std::io::Error> {
        match &self.selection_strategy {
            ProviderSelectionStrategy::UserPreference(provider) => Ok(provider.clone()),
            ProviderSelectionStrategy::PreferClaude => {
                if self.is_claude_available().await {
                    Ok(AuthProvider::Claude)
                } else if self.is_openai_available() {
                    Ok(AuthProvider::OpenAI)
                } else {
                    Err(std::io::Error::other("No authentication provider available"))
                }
            }
            ProviderSelectionStrategy::PreferOpenAI => {
                if self.is_openai_available() {
                    Ok(AuthProvider::OpenAI)
                } else if self.is_claude_available().await {
                    Ok(AuthProvider::Claude)
                } else {
                    Err(std::io::Error::other("No authentication provider available"))
                }
            }
            ProviderSelectionStrategy::IntelligentSelection => {
                self.intelligent_provider_selection().await
            }
        }
    }

    /// Intelligent provider selection based on subscription status and capabilities
    async fn intelligent_provider_selection(&self) -> Result<AuthProvider, std::io::Error> {
        let claude_available = self.is_claude_available().await;
        let openai_available = self.is_openai_available();

        // If only one provider is available, use it
        if claude_available && !openai_available {
            return Ok(AuthProvider::Claude);
        }
        if openai_available && !claude_available {
            return Ok(AuthProvider::OpenAI);
        }
        if !claude_available && !openai_available {
            return Err(std::io::Error::other("No authentication provider available"));
        }

        // Both providers available - use intelligent selection
        
        // Check Claude subscription status
        if let Some(claude_auth) = &self.claude_auth {
            match claude_auth.has_max_subscription().await {
                Ok(true) => {
                    // Claude Max subscription is the premium option
                    return Ok(AuthProvider::Claude);
                }
                Ok(false) => {
                    // Check if Claude has Pro subscription
                    match claude_auth.has_paid_subscription().await {
                        Ok(true) => {
                            // Claude Pro available, check OpenAI status
                            if let Some(openai_auth) = self.openai_manager.auth() {
                                match openai_auth.get_plan_type() {
                                    Some(plan) if plan.contains("pro") || plan.contains("team") => {
                                        // Both have paid plans, prefer Claude for better rate limits
                                        return Ok(AuthProvider::Claude);
                                    }
                                    Some(plan) if plan.contains("plus") => {
                                        // OpenAI Plus vs Claude Pro - prefer Claude
                                        return Ok(AuthProvider::Claude);
                                    }
                                    _ => {
                                        // Claude Pro vs OpenAI API key - prefer Claude Pro
                                        return Ok(AuthProvider::Claude);
                                    }
                                }
                            }
                            return Ok(AuthProvider::Claude);
                        }
                        Ok(false) => {
                            // Claude API key only - check OpenAI subscription
                            if let Some(openai_auth) = self.openai_manager.auth() {
                                if openai_auth.mode == OpenAIAuthMode::ChatGPT {
                                    // OpenAI subscription vs Claude API key - prefer OpenAI
                                    return Ok(AuthProvider::OpenAI);
                                }
                            }
                            // Both API keys - prefer OpenAI for better ecosystem support
                            return Ok(AuthProvider::OpenAI);
                        }
                        Err(_) => {
                            // Can't determine Claude subscription - fallback to OpenAI
                            return Ok(AuthProvider::OpenAI);
                        }
                    }
                }
                Err(_) => {
                    // Can't check Claude subscription - fallback to OpenAI
                    return Ok(AuthProvider::OpenAI);
                }
            }
        }

        // Default fallback
        Ok(AuthProvider::OpenAI)
    }

    /// Check if Claude authentication is available
    async fn is_claude_available(&self) -> bool {
        if let Some(claude_auth) = &self.claude_auth {
            claude_auth.get_token().await.is_ok()
        } else {
            false
        }
    }

    /// Check if OpenAI authentication is available
    fn is_openai_available(&self) -> bool {
        self.openai_manager.auth().is_some()
    }

    /// Get authentication token for the optimal provider
    pub async fn get_token(&self) -> Result<(AuthProvider, String), std::io::Error> {
        let provider = self.select_optimal_provider().await?;
        
        match provider {
            AuthProvider::OpenAI => {
                let openai_auth = self.openai_manager.auth()
                    .ok_or_else(|| std::io::Error::other("OpenAI authentication not available"))?;
                let token = openai_auth.get_token().await?;
                Ok((AuthProvider::OpenAI, token))
            }
            AuthProvider::Claude => {
                let claude_auth = self.claude_auth.as_ref()
                    .ok_or_else(|| std::io::Error::other("Claude authentication not available"))?;
                let token = claude_auth.get_token().await?;
                Ok((AuthProvider::Claude, token))
            }
        }
    }

    /// Get authentication token for a specific provider
    pub async fn get_token_for_provider(&self, provider: AuthProvider) -> Result<String, std::io::Error> {
        match provider {
            AuthProvider::OpenAI => {
                let openai_auth = self.openai_manager.auth()
                    .ok_or_else(|| std::io::Error::other("OpenAI authentication not available"))?;
                openai_auth.get_token().await
            }
            AuthProvider::Claude => {
                let claude_auth = self.claude_auth.as_ref()
                    .ok_or_else(|| std::io::Error::other("Claude authentication not available"))?;
                claude_auth.get_token().await
            }
        }
    }

    /// Set user preference for provider selection
    pub fn set_preferred_provider(&mut self, provider: AuthProvider) {
        self.preferred_provider = Some(provider.clone());
        self.selection_strategy = ProviderSelectionStrategy::UserPreference(provider);
    }

    /// Get current preferred provider
    pub fn get_preferred_provider(&self) -> Option<&AuthProvider> {
        self.preferred_provider.as_ref()
    }

    /// Login with Claude API key
    pub async fn login_claude_api_key(&mut self, api_key: &str) -> Result<(), std::io::Error> {
        crate::claude_auth::login_with_claude_api_key(&self.codex_home, api_key).await?;
        
        // Reload Claude authentication
        self.initialize_claude_auth().await?;
        
        Ok(())
    }

    /// Login with OpenAI API key (delegate to existing manager)
    pub fn login_openai_api_key(&self, api_key: &str) -> Result<(), std::io::Error> {
        crate::auth::login_with_api_key(&self.codex_home, api_key)
    }

    /// Logout from Claude
    pub async fn logout_claude(&mut self) -> Result<bool, std::io::Error> {
        let removed = crate::claude_auth::logout_claude(&self.codex_home).await?;
        self.claude_auth = None;
        Ok(removed)
    }

    /// Logout from OpenAI (delegate to existing manager)
    pub fn logout_openai(&self) -> Result<bool, std::io::Error> {
        self.openai_manager.logout()
    }

    /// Logout from all providers
    pub async fn logout_all(&mut self) -> Result<(bool, bool), std::io::Error> {
        let claude_removed = self.logout_claude().await?;
        let openai_removed = self.logout_openai()?;
        Ok((claude_removed, openai_removed))
    }

    /// Get provider status information
    pub async fn get_provider_status(&self) -> ProviderStatus {
        let openai_available = self.is_openai_available();
        let claude_available = self.is_claude_available().await;
        
        let openai_mode = if openai_available {
            self.openai_manager.auth().map(|auth| auth.mode)
        } else {
            None
        };

        let claude_mode = if claude_available {
            self.claude_auth.as_ref().map(|auth| auth.mode.clone())
        } else {
            None
        };

        let claude_subscription = if claude_available {
            if let Some(claude_auth) = &self.claude_auth {
                claude_auth.get_subscription_info().await.ok()
            } else {
                None
            }
        } else {
            None
        };

        ProviderStatus {
            openai_available,
            claude_available,
            openai_mode,
            claude_mode,
            claude_subscription,
            current_strategy: self.selection_strategy.clone(),
            preferred_provider: self.preferred_provider.clone(),
        }
    }

    /// Refresh tokens for all providers
    pub async fn refresh_all_tokens(&self) -> Result<(), std::io::Error> {
        // Refresh OpenAI token
        if let Err(e) = self.openai_manager.refresh_token().await {
            eprintln!("Warning: Failed to refresh OpenAI token: {}", e);
        }

        // Refresh Claude token if available
        if let Some(claude_auth) = &self.claude_auth {
            if let Err(e) = claude_auth.refresh_oauth_token().await {
                eprintln!("Warning: Failed to refresh Claude token: {}", e);
            }
        }

        Ok(())
    }

    /// Get OpenAI auth manager for advanced operations
    pub fn openai_manager(&self) -> &Arc<OpenAIAuthManager> {
        &self.openai_manager
    }

    /// Get Claude auth for advanced operations
    pub fn claude_auth(&self) -> Option<&ClaudeAuth> {
        self.claude_auth.as_ref()
    }
}

/// Provider status information
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub openai_available: bool,
    pub claude_available: bool,
    pub openai_mode: Option<OpenAIAuthMode>,
    pub claude_mode: Option<ClaudeAuthMode>,
    pub claude_subscription: Option<crate::claude_auth::SubscriptionInfo>,
    pub current_strategy: ProviderSelectionStrategy,
    pub preferred_provider: Option<AuthProvider>,
}

impl ProviderStatus {
    /// Get a human-readable description of the current authentication status
    pub fn description(&self) -> String {
        let mut parts = Vec::new();

        if self.openai_available {
            if let Some(mode) = &self.openai_mode {
                match mode {
                    OpenAIAuthMode::ChatGPT => parts.push("OpenAI (ChatGPT)".to_string()),
                    OpenAIAuthMode::ApiKey => parts.push("OpenAI (API Key)".to_string()),
                }
            } else {
                parts.push("OpenAI".to_string());
            }
        }

        if self.claude_available {
            if let Some(mode) = &self.claude_mode {
                match mode {
                    ClaudeAuthMode::MaxSubscription => parts.push("Claude (Max)".to_string()),
                    ClaudeAuthMode::ProSubscription => parts.push("Claude (Pro)".to_string()),
                    ClaudeAuthMode::ApiKey => parts.push("Claude (API Key)".to_string()),
                }
            } else {
                parts.push("Claude".to_string());
            }
        }

        if parts.is_empty() {
            "No authentication available".to_string()
        } else {
            format!("Available: {}", parts.join(", "))
        }
    }

    /// Check if any paid subscription is available
    pub fn has_paid_subscription(&self) -> bool {
        // Check OpenAI subscription
        if let Some(OpenAIAuthMode::ChatGPT) = self.openai_mode {
            return true;
        }

        // Check Claude subscription
        if let Some(claude_mode) = &self.claude_mode {
            match claude_mode {
                ClaudeAuthMode::MaxSubscription | ClaudeAuthMode::ProSubscription => return true,
                ClaudeAuthMode::ApiKey => {}
            }
        }

        false
    }

    /// Get the best available provider recommendation
    pub fn recommended_provider(&self) -> Option<AuthProvider> {
        // Prefer Claude Max
        if let Some(ClaudeAuthMode::MaxSubscription) = &self.claude_mode {
            return Some(AuthProvider::Claude);
        }

        // Then OpenAI ChatGPT subscriptions
        if let Some(OpenAIAuthMode::ChatGPT) = &self.openai_mode {
            return Some(AuthProvider::OpenAI);
        }

        // Then Claude Pro
        if let Some(ClaudeAuthMode::ProSubscription) = &self.claude_mode {
            return Some(AuthProvider::Claude);
        }

        // Finally API keys - prefer OpenAI for ecosystem compatibility
        if self.openai_available {
            return Some(AuthProvider::OpenAI);
        }

        if self.claude_available {
            return Some(AuthProvider::Claude);
        }

        None
    }
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
            OpenAIAuthMode::ApiKey,
            "test".to_string(),
            ProviderSelectionStrategy::IntelligentSelection,
        );

        assert_eq!(manager.selection_strategy, ProviderSelectionStrategy::IntelligentSelection);
        assert!(!manager.is_openai_available());
        assert!(!manager.is_claude_available().await);
    }

    #[tokio::test]
    async fn test_provider_selection_strategy() {
        let temp_dir = tempdir().unwrap();
        let mut manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            OpenAIAuthMode::ApiKey,
            "test".to_string(),
            ProviderSelectionStrategy::PreferClaude,
        );

        manager.set_preferred_provider(AuthProvider::OpenAI);
        assert_eq!(manager.get_preferred_provider(), Some(&AuthProvider::OpenAI));
        assert_eq!(manager.selection_strategy, ProviderSelectionStrategy::UserPreference(AuthProvider::OpenAI));
    }

    #[tokio::test]
    async fn test_provider_status() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            OpenAIAuthMode::ApiKey,
            "test".to_string(),
            ProviderSelectionStrategy::IntelligentSelection,
        );

        let status = manager.get_provider_status().await;
        assert!(!status.openai_available);
        assert!(!status.claude_available);
        assert_eq!(status.description(), "No authentication available");
        assert!(!status.has_paid_subscription());
        assert_eq!(status.recommended_provider(), None);
    }
}