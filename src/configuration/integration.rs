//! Integration with existing Code project configuration system
//! 
//! This module provides seamless integration with the existing config.toml
//! and AuthManager systems while adding Claude authentication support.

use std::path::PathBuf;
use chrono::Duration;
use serde::{Deserialize, Serialize};

use super::{
    UnifiedConfigManager, 
    UnifiedConfig, 
    ConfigError,
    auth_config::{AuthConfig, ProviderType, ProviderPreference},
    unified_storage::UnifiedAuthJson,
};

/// Extended configuration that integrates with existing Config struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedConfigToml {
    // Existing fields would be preserved here
    // This is just the auth extension
    pub auth: Option<AuthConfigToml>,
}

/// TOML-serializable auth configuration (integrates with existing config.toml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfigToml {
    /// Preferred authentication provider
    #[serde(default)]
    pub preferred_provider: Option<String>,
    
    /// Enable automatic fallback between providers
    #[serde(default)]
    pub enable_fallback: Option<bool>,
    
    /// Provider selection strategy
    #[serde(default)]
    pub provider_preference: Option<String>,
    
    /// Fallback strategy when preferred provider fails
    #[serde(default)]
    pub fallback_strategy: Option<String>,
    
    /// Subscription check interval in minutes
    #[serde(default)]
    pub subscription_check_interval_minutes: Option<u64>,
    
    /// Enable automatic subscription status checking
    #[serde(default)]
    pub enable_subscription_check: Option<bool>,
    
    /// Authentication timeout in seconds
    #[serde(default)]
    pub auth_timeout_seconds: Option<u64>,
    
    /// Enable automatic token refresh
    #[serde(default)]
    pub auto_refresh_tokens: Option<bool>,
    
    /// Cache provider capabilities for this many minutes
    #[serde(default)]
    pub provider_cache_duration_minutes: Option<u64>,
}

impl From<AuthConfig> for AuthConfigToml {
    fn from(config: AuthConfig) -> Self {
        Self {
            preferred_provider: Some(config.preferred_provider.to_string()),
            enable_fallback: Some(config.enable_fallback),
            provider_preference: Some(format!("{:?}", config.provider_preference).to_lowercase()),
            fallback_strategy: Some(format!("{:?}", config.fallback_strategy).to_lowercase()),
            subscription_check_interval_minutes: Some(config.subscription_check_interval.num_minutes() as u64),
            enable_subscription_check: Some(config.enable_subscription_check),
            auth_timeout_seconds: Some(config.auth_timeout.num_seconds() as u64),
            auto_refresh_tokens: Some(config.auto_refresh_tokens),
            provider_cache_duration_minutes: Some(config.provider_cache_duration.num_minutes() as u64),
        }
    }
}

impl From<AuthConfigToml> for AuthConfig {
    fn from(toml: AuthConfigToml) -> Self {
        let mut config = AuthConfig::default();
        
        if let Some(provider) = toml.preferred_provider {
            config.preferred_provider = provider.as_str().into();
        }
        
        if let Some(fallback) = toml.enable_fallback {
            config.enable_fallback = fallback;
        }
        
        if let Some(preference) = toml.provider_preference {
            config.provider_preference = match preference.as_str() {
                "prefer_claude" => ProviderPreference::PreferClaude,
                "prefer_openai" => ProviderPreference::PreferOpenAI,
                "cost_optimized" => ProviderPreference::CostOptimized,
                "performance_optimized" => ProviderPreference::PerformanceOptimized,
                "quota_optimized" => ProviderPreference::QuotaOptimized,
                _ => ProviderPreference::PreferClaude,
            };
        }
        
        if let Some(strategy) = toml.fallback_strategy {
            config.fallback_strategy = match strategy.as_str() {
                "automatic" => super::auth_config::FallbackStrategy::Automatic,
                "manual" => super::auth_config::FallbackStrategy::Manual,
                "on_quota_exhausted" => super::auth_config::FallbackStrategy::OnQuotaExhausted,
                "on_auth_error" => super::auth_config::FallbackStrategy::OnAuthError,
                _ => super::auth_config::FallbackStrategy::Automatic,
            };
        }
        
        if let Some(interval) = toml.subscription_check_interval_minutes {
            config.subscription_check_interval = Duration::minutes(interval as i64);
        }
        
        if let Some(check) = toml.enable_subscription_check {
            config.enable_subscription_check = check;
        }
        
        if let Some(timeout) = toml.auth_timeout_seconds {
            config.auth_timeout = Duration::seconds(timeout as i64);
        }
        
        if let Some(refresh) = toml.auto_refresh_tokens {
            config.auto_refresh_tokens = refresh;
        }
        
        if let Some(cache) = toml.provider_cache_duration_minutes {
            config.provider_cache_duration = Duration::minutes(cache as i64);
        }
        
        config
    }
}

/// Integration layer for existing Config struct
pub struct ConfigIntegration {
    config_manager: UnifiedConfigManager,
    existing_config_path: PathBuf,
}

impl ConfigIntegration {
    /// Create new integration layer
    pub fn new(codex_home: PathBuf) -> Result<Self, ConfigError> {
        let config_manager = UnifiedConfigManager::new(codex_home.clone())?;
        let existing_config_path = codex_home.join("config.toml");
        
        Ok(Self {
            config_manager,
            existing_config_path,
        })
    }

    /// Load configuration that integrates with existing config.toml
    pub async fn load_integrated_config(&self) -> Result<IntegratedConfig, ConfigError> {
        // Load our unified config
        let unified_config = self.config_manager.load_config().await?;
        
        // Load existing config.toml if it exists
        let existing_config = self.load_existing_config()?;
        
        Ok(IntegratedConfig {
            unified: unified_config,
            existing: existing_config,
        })
    }

    /// Save configuration back to both systems
    pub async fn save_integrated_config(&self, config: &IntegratedConfig) -> Result<(), ConfigError> {
        // Save unified config
        self.config_manager.save_config(&config.unified).await?;
        
        // Update existing config.toml with auth section
        self.update_existing_config(&config.unified.auth)?;
        
        Ok(())
    }

    /// Extend existing Config struct with auth provider selection
    pub async fn extend_existing_config_struct(&self, existing_config: &mut ExistingConfig) -> Result<(), ConfigError> {
        let unified_config = self.config_manager.load_config().await?;
        
        // Add our auth configuration to the existing config
        existing_config.auth_config = Some(unified_config.auth);
        existing_config.auth_data = Some(unified_config.auth_data);
        
        Ok(())
    }

    /// Get provider selection for existing AuthManager
    pub async fn get_provider_for_auth_manager(&self) -> Result<ProviderSelection, ConfigError> {
        let config = self.config_manager.load_config().await?;
        
        Ok(ProviderSelection {
            preferred_provider: config.auth.preferred_provider,
            enable_fallback: config.auth.enable_fallback,
            fallback_strategy: config.auth.fallback_strategy,
            openai_available: config.auth_data.openai_auth.is_some(),
            claude_available: config.auth_data.claude_auth.is_some(),
        })
    }

    /// Check if Claude subscription verification is needed
    pub async fn needs_claude_subscription_check(&self) -> Result<bool, ConfigError> {
        let config = self.config_manager.load_config().await?;
        
        // Only check if Claude is configured and subscription checking is enabled
        if config.auth_data.claude_auth.is_none() || !config.auth.enable_subscription_check {
            return Ok(false);
        }
        
        Ok(config.auth.needs_subscription_check())
    }

    /// Update subscription check timestamp
    pub async fn update_subscription_check_timestamp(&self) -> Result<(), ConfigError> {
        self.config_manager.update_subscription_check().await
    }

    // Private helper methods
    fn load_existing_config(&self) -> Result<Option<ExistingConfigToml>, ConfigError> {
        if !self.existing_config_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&self.existing_config_path)?;
        let existing: ExistingConfigToml = toml::from_str(&content)
            .map_err(|e| ConfigError::Toml(e))?;
        
        Ok(Some(existing))
    }

    fn update_existing_config(&self, auth_config: &AuthConfig) -> Result<(), ConfigError> {
        // Read existing config.toml
        let mut existing_content = if self.existing_config_path.exists() {
            std::fs::read_to_string(&self.existing_config_path)?
        } else {
            String::new()
        };

        // Parse as TOML document for editing
        let mut doc = existing_content.parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|_| toml_edit::DocumentMut::new());

        // Add/update auth section
        let auth_toml = AuthConfigToml::from(auth_config.clone());
        
        // Convert to TOML value
        let auth_value = toml::to_string(&auth_toml)
            .map_err(|e| ConfigError::Toml(e.into()))?;
        let auth_table: toml_edit::Table = auth_value.parse()
            .map_err(|_| ConfigError::Toml(toml::de::Error::custom("Failed to parse auth config")))?;

        doc["auth"] = toml_edit::Item::Table(auth_table);

        // Write back to file
        std::fs::write(&self.existing_config_path, doc.to_string())?;
        
        Ok(())
    }
}

/// Integrated configuration combining unified and existing systems
#[derive(Debug, Clone)]
pub struct IntegratedConfig {
    pub unified: UnifiedConfig,
    pub existing: Option<ExistingConfigToml>,
}

/// Placeholder for existing config.toml structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExistingConfigToml {
    // Existing fields would be preserved here
    // This is just a placeholder for integration
    pub model: Option<String>,
    pub approval_policy: Option<String>,
    pub auth: Option<AuthConfigToml>,
    // ... other existing fields
}

/// Extended version of existing Config struct
#[derive(Debug, Clone)]
pub struct ExistingConfig {
    // Existing fields would be here
    pub model: String,
    pub approval_policy: String,
    // ... other existing fields
    
    // New auth integration fields
    pub auth_config: Option<AuthConfig>,
    pub auth_data: Option<UnifiedAuthJson>,
}

/// Provider selection information for AuthManager integration
#[derive(Debug, Clone)]
pub struct ProviderSelection {
    pub preferred_provider: ProviderType,
    pub enable_fallback: bool,
    pub fallback_strategy: super::auth_config::FallbackStrategy,
    pub openai_available: bool,
    pub claude_available: bool,
}

impl ProviderSelection {
    /// Determine which provider to use based on current configuration
    pub fn select_provider(&self, context: &SelectionContext) -> ProviderType {
        // If force provider is set, use it
        if let Some(forced) = context.force_provider {
            return forced;
        }

        // Check if preferred provider is available
        let preferred_available = match self.preferred_provider {
            ProviderType::OpenAI => self.openai_available,
            ProviderType::Claude => self.claude_available,
        };

        if preferred_available {
            return self.preferred_provider;
        }

        // Fallback logic
        if self.enable_fallback {
            match self.preferred_provider {
                ProviderType::OpenAI if self.claude_available => ProviderType::Claude,
                ProviderType::Claude if self.openai_available => ProviderType::OpenAI,
                _ => self.preferred_provider, // Return preferred even if not available
            }
        } else {
            self.preferred_provider
        }
    }

    /// Check if fallback should occur for the given error
    pub fn should_fallback(&self, error: &AuthErrorContext) -> bool {
        if !self.enable_fallback {
            return false;
        }

        self.fallback_strategy.should_fallback(&error.error_type)
    }
}

/// Context for provider selection
#[derive(Debug, Clone)]
pub struct SelectionContext {
    pub force_provider: Option<ProviderType>,
    pub task_type: Option<String>,
    pub quota_requirements: Option<u64>,
}

/// Error context for fallback decisions
#[derive(Debug, Clone)]
pub struct AuthErrorContext {
    pub error_type: super::auth_config::AuthErrorType,
    pub provider: ProviderType,
    pub retry_count: u32,
}

/// Helper functions for existing code integration
pub mod integration_helpers {
    use super::*;

    /// Check if Claude authentication is available and valid
    pub async fn is_claude_auth_available(codex_home: &std::path::Path) -> bool {
        if let Ok(integration) = ConfigIntegration::new(codex_home.to_path_buf()) {
            if let Ok(config) = integration.config_manager.load_config().await {
                return config.auth_data.claude_auth.is_some();
            }
        }
        false
    }

    /// Get the currently preferred provider
    pub async fn get_preferred_provider(codex_home: &std::path::Path) -> Result<ProviderType, ConfigError> {
        let integration = ConfigIntegration::new(codex_home.to_path_buf())?;
        let config = integration.config_manager.load_config().await?;
        Ok(config.auth.preferred_provider)
    }

    /// Set the preferred provider
    pub async fn set_preferred_provider(codex_home: &std::path::Path, provider: ProviderType) -> Result<(), ConfigError> {
        let integration = ConfigIntegration::new(codex_home.to_path_buf())?;
        integration.config_manager.set_provider_preference(provider).await
    }

    /// Check if subscription verification is needed
    pub async fn check_subscription_verification_needed(codex_home: &std::path::Path) -> Result<bool, ConfigError> {
        let integration = ConfigIntegration::new(codex_home.to_path_buf())?;
        integration.needs_claude_subscription_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_auth_config_toml_conversion() {
        let auth_config = AuthConfig::default();
        let auth_toml = AuthConfigToml::from(auth_config.clone());
        let converted_back = AuthConfig::from(auth_toml);
        
        assert_eq!(auth_config.preferred_provider, converted_back.preferred_provider);
        assert_eq!(auth_config.enable_fallback, converted_back.enable_fallback);
    }

    #[tokio::test]
    async fn test_config_integration_creation() {
        let temp_dir = tempdir().unwrap();
        let integration = ConfigIntegration::new(temp_dir.path().to_path_buf());
        assert!(integration.is_ok());
    }

    #[test]
    fn test_provider_selection() {
        let selection = ProviderSelection {
            preferred_provider: ProviderType::Claude,
            enable_fallback: true,
            fallback_strategy: super::auth_config::FallbackStrategy::Automatic,
            openai_available: true,
            claude_available: false,
        };

        let context = SelectionContext {
            force_provider: None,
            task_type: None,
            quota_requirements: None,
        };

        // Should fallback to OpenAI since Claude is not available
        assert_eq!(selection.select_provider(&context), ProviderType::OpenAI);
    }

    #[test]
    fn test_provider_selection_with_force() {
        let selection = ProviderSelection {
            preferred_provider: ProviderType::Claude,
            enable_fallback: true,
            fallback_strategy: super::auth_config::FallbackStrategy::Automatic,
            openai_available: true,
            claude_available: false,
        };

        let context = SelectionContext {
            force_provider: Some(ProviderType::OpenAI),
            task_type: None,
            quota_requirements: None,
        };

        // Should use forced provider
        assert_eq!(selection.select_provider(&context), ProviderType::OpenAI);
    }

    #[test]
    fn test_fallback_decision() {
        let selection = ProviderSelection {
            preferred_provider: ProviderType::Claude,
            enable_fallback: true,
            fallback_strategy: super::auth_config::FallbackStrategy::OnQuotaExhausted,
            openai_available: true,
            claude_available: true,
        };

        let quota_error = AuthErrorContext {
            error_type: super::auth_config::AuthErrorType::QuotaExhausted,
            provider: ProviderType::Claude,
            retry_count: 0,
        };

        let auth_error = AuthErrorContext {
            error_type: super::auth_config::AuthErrorType::AuthenticationFailed,
            provider: ProviderType::Claude,
            retry_count: 0,
        };

        assert!(selection.should_fallback(&quota_error));
        assert!(!selection.should_fallback(&auth_error));
    }

    #[tokio::test]
    async fn test_integration_helpers() {
        let temp_dir = tempdir().unwrap();
        
        // Test with no configuration
        assert!(!integration_helpers::is_claude_auth_available(temp_dir.path()).await);
        
        // Test preferred provider getter
        let result = integration_helpers::get_preferred_provider(temp_dir.path()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ProviderType::OpenAI); // Default
    }
}