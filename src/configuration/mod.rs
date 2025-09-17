//! Configuration management module for Claude authentication integration
//! 
//! This module provides unified configuration management that extends the existing
//! Code project configuration system to support both OpenAI and Claude authentication
//! providers while maintaining backward compatibility.

pub mod auth_config;
pub mod unified_storage;
pub mod migration;
pub mod validation;
pub mod environment;
pub mod integration;
pub mod auth_manager_integration;

pub use auth_config::{
    AuthConfig, 
    ProviderType, 
    ProviderPreference, 
    FallbackStrategy,
    SubscriptionCheckConfig,
};

pub use unified_storage::{
    UnifiedAuthJson,
    UnifiedAuthStorage,
    StorageError,
    AuthData,
    OpenAIAuthData,
    ClaudeAuthData,
};

pub use migration::{
    ConfigMigrator,
    MigrationError,
    MigrationStrategy,
    BackupHandle,
};

pub use validation::{
    ConfigValidator,
    ValidationError,
    ValidationResult,
    ValidationRule,
};

pub use environment::{
    EnvironmentOverrides,
    EnvironmentConfig,
    EnvironmentError,
};

pub use integration::{
    ConfigIntegration,
    IntegratedConfig,
    ProviderSelection,
    SelectionContext,
    AuthErrorContext,
    integration_helpers,
};

pub use auth_manager_integration::{
    UnifiedAuthManager,
    AuthProviderWrapper,
    AuthManagerConfig,
    UnifiedAuthError,
    create_unified_auth_manager,
    is_claude_available,
    get_preferred_provider,
};

use std::path::PathBuf;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};

/// Configuration manager that integrates Claude authentication with existing Code project config
#[derive(Debug, Clone)]
pub struct UnifiedConfigManager {
    pub base_config_path: PathBuf,
    pub auth_storage: UnifiedAuthStorage,
    pub migrator: ConfigMigrator,
    pub validator: ConfigValidator,
    pub env_config: EnvironmentConfig,
}

impl UnifiedConfigManager {
    /// Create new configuration manager with default settings
    pub fn new(codex_home: PathBuf) -> Result<Self, ConfigError> {
        let auth_storage = UnifiedAuthStorage::new(&codex_home)?;
        let migrator = ConfigMigrator::new(&codex_home)?;
        let validator = ConfigValidator::new();
        let env_config = EnvironmentConfig::new();
        
        Ok(Self {
            base_config_path: codex_home.join("config.toml"),
            auth_storage,
            migrator,
            validator,
            env_config,
        })
    }

    /// Load configuration with migration and validation
    pub async fn load_config(&self) -> Result<UnifiedConfig, ConfigError> {
        // Check if migration is needed
        if self.migrator.needs_migration()? {
            let backup = self.migrator.create_backup().await?;
            match self.migrator.migrate().await {
                Ok(_) => tracing::info!("Configuration migrated successfully"),
                Err(e) => {
                    tracing::error!("Migration failed: {}", e);
                    self.migrator.restore_backup(backup).await?;
                    return Err(ConfigError::MigrationFailed(e));
                }
            }
        }

        // Load base configuration
        let mut config = self.load_base_config()?;
        
        // Apply environment overrides
        self.env_config.apply_overrides(&mut config)?;
        
        // Validate configuration
        self.validator.validate(&config)?;
        
        Ok(config)
    }

    /// Save configuration changes
    pub async fn save_config(&self, config: &UnifiedConfig) -> Result<(), ConfigError> {
        // Validate before saving
        self.validator.validate(config)?;
        
        // Save unified auth data
        self.auth_storage.save(&config.auth_data)?;
        
        // Update base configuration if needed
        self.save_base_config(config)?;
        
        Ok(())
    }

    /// Get current provider preference
    pub fn get_provider_preference(&self) -> Result<ProviderType, ConfigError> {
        let config = self.load_base_config()?;
        Ok(config.auth.preferred_provider)
    }

    /// Set provider preference
    pub async fn set_provider_preference(&self, provider: ProviderType) -> Result<(), ConfigError> {
        let mut config = self.load_base_config()?;
        config.auth.preferred_provider = provider;
        config.auth.last_provider_check = Some(Utc::now());
        self.save_config(&config).await
    }

    /// Check if Claude subscription verification is needed
    pub fn needs_subscription_check(&self) -> Result<bool, ConfigError> {
        let config = self.load_base_config()?;
        
        if !config.auth.enable_subscription_check {
            return Ok(false);
        }

        match config.auth_data.last_subscription_check {
            Some(last_check) => {
                let check_interval = config.auth.subscription_check_interval;
                Ok(Utc::now() - last_check > check_interval)
            }
            None => Ok(true),
        }
    }

    /// Update subscription check timestamp
    pub async fn update_subscription_check(&self) -> Result<(), ConfigError> {
        let mut config = self.load_base_config()?;
        config.auth_data.last_subscription_check = Some(Utc::now());
        self.save_config(&config).await
    }

    // Private helper methods
    fn load_base_config(&self) -> Result<UnifiedConfig, ConfigError> {
        if !self.base_config_path.exists() {
            return Ok(UnifiedConfig::default());
        }

        let content = std::fs::read_to_string(&self.base_config_path)?;
        let base_config: BaseConfig = toml::from_str(&content)?;
        
        // Load auth data separately
        let auth_data = self.auth_storage.load()?;
        
        Ok(UnifiedConfig {
            auth: base_config.auth.unwrap_or_default(),
            auth_data,
            // Copy other fields as needed
        })
    }

    fn save_base_config(&self, config: &UnifiedConfig) -> Result<(), ConfigError> {
        // Create directory if it doesn't exist
        if let Some(parent) = self.base_config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Convert to base config format
        let base_config = BaseConfig {
            auth: Some(config.auth.clone()),
        };

        let content = toml::to_string_pretty(&base_config)?;
        std::fs::write(&self.base_config_path, content)
            .map_err(|e| ConfigError::Io(e))?;
        
        Ok(())
    }
}

/// Unified configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedConfig {
    pub auth: AuthConfig,
    #[serde(skip)]
    pub auth_data: UnifiedAuthJson,
}

impl Default for UnifiedConfig {
    fn default() -> Self {
        Self {
            auth: AuthConfig::default(),
            auth_data: UnifiedAuthJson::default(),
        }
    }
}

/// Base configuration for TOML serialization (extends existing patterns)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BaseConfig {
    pub auth: Option<AuthConfig>,
    // Note: Other existing config fields would be preserved here
    // This integrates with the existing config.toml structure
}

/// Configuration error types
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("TOML deserialization error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Secure storage error: {0}")]
    SecureStorage(#[from] crate::security::SecureStorageError),
    
    #[error("Migration failed: {0}")]
    MigrationFailed(#[from] MigrationError),
    
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    
    #[error("Environment error: {0}")]
    Environment(#[from] EnvironmentError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_config_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf());
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_default_config_loading() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
        let config = manager.load_config().await.unwrap();
        assert_eq!(config.auth.preferred_provider, ProviderType::OpenAI);
    }

    #[tokio::test]
    async fn test_provider_preference_setting() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        // Set Claude preference
        manager.set_provider_preference(ProviderType::Claude).await.unwrap();
        
        // Verify it was saved
        let preference = manager.get_provider_preference().unwrap();
        assert_eq!(preference, ProviderType::Claude);
    }
}