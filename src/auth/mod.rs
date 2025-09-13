/// # Unified Authentication System
/// 
/// This module provides a comprehensive authentication system supporting both OpenAI and Claude
/// authentication with seamless migration, intelligent provider selection, and robust fallback mechanisms.
/// 
/// ## Features
/// 
/// - **Zero-downtime migration** from OpenAI-only to unified Claude+OpenAI authentication
/// - **Intelligent provider selection** based on subscription status, quotas, and user preferences
/// - **Comprehensive backup and rollback** mechanisms for safe migration
/// - **Quota management** for Claude Max subscriptions with multi-agent support
/// - **Backward compatibility** with existing OpenAI authentication workflows
/// - **Adaptive learning** from usage patterns to optimize provider selection
/// 
/// ## Architecture
/// 
/// ```
/// ┌─────────────────────────────────────────────────────────────┐
/// │                  Unified Auth Manager                       │
/// │  ├─ Provider Selection Logic                               │
/// │  ├─ Usage Statistics & Learning                            │
/// │  └─ Configuration Management                               │
/// ├─────────────────────────────────────────────────────────────┤
/// │  OpenAI Provider        │  Claude Provider                  │
/// │  ├─ ChatGPT OAuth       │  ├─ Claude Max OAuth             │
/// │  ├─ API Key Auth        │  ├─ API Key Auth                 │
/// │  └─ Token Management    │  └─ Quota Management             │
/// ├─────────────────────────────────────────────────────────────┤
/// │                    Migration System                         │
/// │  ├─ Backup Manager      │  ├─ Rollback Manager             │
/// │  ├─ Migration Logic     │  └─ Validation & Testing         │
/// └─────────────────────────────────────────────────────────────┘
/// ```
/// 
/// ## Usage Examples
/// 
/// ### Basic Setup
/// 
/// ```rust
/// use crate::auth::{UnifiedAuthManager, ProviderSelectionStrategy, AuthContext, TaskType, Priority};
/// 
/// // Create unified auth manager
/// let auth_manager = UnifiedAuthManager::new(
///     codex_home_path,
///     ProviderSelectionStrategy::PreferClaude
/// ).await?;
/// 
/// // Get optimal provider for a task
/// let context = AuthContext {
///     task_type: TaskType::CodeGeneration,
///     estimated_tokens: Some(2000),
///     priority: Priority::High,
///     user_preference: None,
///     required_features: vec![],
/// };
/// 
/// let auth_token = auth_manager.get_auth_token(&context).await?;
/// ```
/// 
/// ### Migration
/// 
/// ```rust
/// use crate::auth::migration::{MigrationCoordinator, MigrationConfig};
/// 
/// // Execute migration from OpenAI-only to unified system
/// let config = MigrationConfig::default();
/// let mut coordinator = MigrationCoordinator::new(codex_home_path, config);
/// 
/// let migration_result = coordinator.execute_migration().await?;
/// if migration_result.phase == MigrationPhase::Completed {
///     println!("Migration successful!");
/// }
/// ```
/// 
/// ### Claude Authentication Setup
/// 
/// ```rust
/// use crate::auth::claude::ClaudeAuth;
/// 
/// // Setup with API key
/// ClaudeAuth::setup_with_api_key(&codex_home, "sk-ant-api03-...").await?;
/// 
/// // Or setup with OAuth tokens (from web flow)
/// ClaudeAuth::setup_with_oauth(&codex_home, claude_tokens).await?;
/// ```

pub mod claude;
pub mod unified;
pub mod migration;

// Re-export main types for convenient access
pub use claude::{ClaudeAuth, ClaudeAuthMode, ClaudeAuthError, ClaudeTokenData, ClaudeSubscription};
pub use unified::{
    UnifiedAuthManager, ProviderType, ProviderSelectionStrategy, AuthContext, AuthProvider,
    TaskType, Priority, ProviderStatus, UnifiedAuthError, UnifiedAuthConfig,
};
pub use migration::{
    MigrationCoordinator, MigrationConfig, MigrationProgress, MigrationPhase, MigrationError,
    MigrationResult as MigrationOpResult,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Main authentication manager that provides a unified interface
/// for both migration and ongoing authentication operations
#[derive(Debug)]
pub struct AuthenticationManager {
    codex_home: PathBuf,
    unified_manager: Option<UnifiedAuthManager>,
    migration_coordinator: Option<migration::MigrationCoordinator>,
    config: AuthManagerConfig,
}

/// Configuration for the main authentication manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthManagerConfig {
    /// Enable automatic migration detection and prompts
    pub auto_migration_detection: bool,
    /// Default provider selection strategy
    pub default_strategy: ProviderSelectionStrategy,
    /// Migration configuration
    pub migration_config: migration::MigrationConfig,
    /// Unified auth configuration
    pub unified_config: UnifiedAuthConfig,
    /// Enable verbose logging
    pub verbose_logging: bool,
}

impl Default for AuthManagerConfig {
    fn default() -> Self {
        Self {
            auto_migration_detection: true,
            default_strategy: ProviderSelectionStrategy::Adaptive,
            migration_config: migration::MigrationConfig::default(),
            unified_config: UnifiedAuthConfig::default(),
            verbose_logging: false,
        }
    }
}

/// Overall authentication system status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSystemStatus {
    /// Whether the system is ready to use
    pub ready: bool,
    /// Whether migration is needed
    pub migration_needed: bool,
    /// Current migration progress (if any)
    pub migration_progress: Option<MigrationProgress>,
    /// Available providers and their status
    pub provider_status: HashMap<ProviderType, ProviderStatus>,
    /// System health indicators
    pub health: SystemHealth,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

/// System health indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// All critical components working
    pub healthy: bool,
    /// Individual component status
    pub components: HashMap<String, ComponentHealth>,
    /// Any warnings or issues
    pub warnings: Vec<String>,
}

/// Individual component health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub last_check: DateTime<Utc>,
    pub error_message: Option<String>,
}

/// Health status levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

impl AuthenticationManager {
    /// Create a new authentication manager
    pub async fn new(codex_home: PathBuf) -> Result<Self, UnifiedAuthError> {
        Self::with_config(codex_home, AuthManagerConfig::default()).await
    }

    /// Create with custom configuration
    pub async fn with_config(codex_home: PathBuf, config: AuthManagerConfig) -> Result<Self, UnifiedAuthError> {
        let mut manager = Self {
            codex_home,
            unified_manager: None,
            migration_coordinator: None,
            config,
        };

        // Initialize based on current system state
        manager.initialize().await?;

        Ok(manager)
    }

    /// Initialize the authentication manager
    async fn initialize(&mut self) -> Result<(), UnifiedAuthError> {
        // Check if migration is needed
        if self.config.auto_migration_detection {
            let migration_coordinator = migration::MigrationCoordinator::new(
                self.codex_home.clone(),
                self.config.migration_config.clone()
            );

            if migration_coordinator.is_migration_needed().await.unwrap_or(false) {
                if self.config.verbose_logging {
                    println!("Migration needed - setting up migration coordinator");
                }
                self.migration_coordinator = Some(migration_coordinator);
                return Ok(());
            }
        }

        // Initialize unified manager
        let unified_manager = UnifiedAuthManager::with_config(
            self.codex_home.clone(),
            self.config.default_strategy.clone(),
            self.config.unified_config.clone()
        ).await?;

        self.unified_manager = Some(unified_manager);

        if self.config.verbose_logging {
            println!("Unified authentication manager initialized");
        }

        Ok(())
    }

    /// Get system status
    pub async fn get_system_status(&self) -> Result<AuthSystemStatus, UnifiedAuthError> {
        let migration_needed = if let Some(coordinator) = &self.migration_coordinator {
            coordinator.is_migration_needed().await.unwrap_or(false)
        } else {
            false
        };

        let migration_progress = if let Some(coordinator) = &self.migration_coordinator {
            coordinator.get_progress().await.ok().flatten()
        } else {
            None
        };

        let provider_status = if let Some(manager) = &self.unified_manager {
            manager.get_provider_status_summary().await
        } else {
            HashMap::new()
        };

        let health = self.assess_system_health(&provider_status).await;
        let ready = !migration_needed && health.healthy;

        Ok(AuthSystemStatus {
            ready,
            migration_needed,
            migration_progress,
            provider_status,
            health,
            last_updated: Utc::now(),
        })
    }

    /// Execute migration if needed
    pub async fn execute_migration_if_needed(&mut self) -> Result<Option<MigrationProgress>, UnifiedAuthError> {
        if let Some(mut coordinator) = self.migration_coordinator.take() {
            match coordinator.execute_migration().await {
                Ok(progress) => {
                    // Migration successful, initialize unified manager
                    self.unified_manager = Some(
                        UnifiedAuthManager::with_config(
                            self.codex_home.clone(),
                            self.config.default_strategy.clone(),
                            self.config.unified_config.clone()
                        ).await?
                    );
                    
                    Ok(Some(progress))
                }
                Err(e) => {
                    // Migration failed, keep coordinator for potential retry
                    self.migration_coordinator = Some(coordinator);
                    Err(UnifiedAuthError::ConfigError(format!("Migration failed: {}", e)))
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Get authentication token for a given context
    pub async fn get_auth_token(&self, context: &AuthContext) -> Result<String, UnifiedAuthError> {
        if let Some(manager) = &self.unified_manager {
            manager.get_auth_token(context).await
        } else {
            Err(UnifiedAuthError::ConfigError("System not ready - migration may be needed".to_string()))
        }
    }

    /// Get optimal provider for a context
    pub async fn get_optimal_provider(&self, context: &AuthContext) -> Result<AuthProvider, UnifiedAuthError> {
        if let Some(manager) = &self.unified_manager {
            manager.get_optimal_provider(context).await
        } else {
            Err(UnifiedAuthError::ConfigError("System not ready - migration may be needed".to_string()))
        }
    }

    /// Record usage statistics for learning
    pub async fn record_usage(&self, provider_type: ProviderType, context: &AuthContext, success: bool, response_time_ms: f64) {
        if let Some(manager) = &self.unified_manager {
            manager.record_usage(provider_type, context, success, response_time_ms).await;
        }
    }

    /// Add Claude authentication
    pub async fn add_claude_auth(&mut self, setup_type: ClaudeSetupType) -> Result<(), UnifiedAuthError> {
        match setup_type {
            ClaudeSetupType::ApiKey(api_key) => {
                ClaudeAuth::setup_with_api_key(&self.codex_home, &api_key).await
                    .map_err(|e| UnifiedAuthError::ClaudeError(e))?;
            }
            ClaudeSetupType::OAuth(tokens) => {
                ClaudeAuth::setup_with_oauth(&self.codex_home, tokens).await
                    .map_err(|e| UnifiedAuthError::ClaudeError(e))?;
            }
        }

        // Refresh unified manager if available
        if let Some(manager) = &self.unified_manager {
            manager.refresh_all_provider_status().await?;
        }

        Ok(())
    }

    /// Remove provider authentication
    pub async fn remove_provider(&mut self, provider_type: ProviderType) -> Result<(), UnifiedAuthError> {
        match provider_type {
            ProviderType::Claude => {
                let claude_file = self.codex_home.join("claude_auth.json");
                if claude_file.exists() {
                    tokio::fs::remove_file(claude_file).await
                        .map_err(|e| UnifiedAuthError::IoError(e))?;
                }
            }
            ProviderType::OpenAI => {
                // For OpenAI, we might want to preserve for backward compatibility
                // This could be implemented as disabling rather than removing
                return Err(UnifiedAuthError::ConfigError(
                    "Cannot remove OpenAI provider - use logout instead".to_string()
                ));
            }
        }

        // Update unified manager
        if let Some(manager) = &self.unified_manager {
            manager.remove_provider(&provider_type).await;
        }

        Ok(())
    }

    /// Assess overall system health
    async fn assess_system_health(&self, provider_status: &HashMap<ProviderType, ProviderStatus>) -> SystemHealth {
        let mut components = HashMap::new();
        let mut warnings = Vec::new();
        let mut healthy = true;

        // Check file system
        let fs_health = if self.codex_home.exists() && self.codex_home.is_dir() {
            ComponentHealth {
                status: HealthStatus::Healthy,
                last_check: Utc::now(),
                error_message: None,
            }
        } else {
            healthy = false;
            ComponentHealth {
                status: HealthStatus::Critical,
                last_check: Utc::now(),
                error_message: Some("Codex home directory not accessible".to_string()),
            }
        };
        components.insert("filesystem".to_string(), fs_health);

        // Check providers
        let mut any_provider_available = false;
        for (provider_type, status) in provider_status {
            let component_name = format!("provider_{:?}", provider_type).to_lowercase();
            
            let health_status = if status.available && status.authenticated {
                any_provider_available = true;
                if status.error_message.is_some() {
                    warnings.push(format!("{:?} provider has warnings", provider_type));
                    HealthStatus::Warning
                } else {
                    HealthStatus::Healthy
                }
            } else if status.available {
                HealthStatus::Warning
            } else {
                HealthStatus::Critical
            };

            components.insert(component_name, ComponentHealth {
                status: health_status,
                last_check: status.last_verified.unwrap_or_else(|| Utc::now()),
                error_message: status.error_message.clone(),
            });
        }

        // Overall health requires at least one working provider
        if !any_provider_available {
            healthy = false;
            warnings.push("No authentication providers available".to_string());
        }

        // Check migration status
        if self.migration_coordinator.is_some() {
            warnings.push("Migration pending - some features may be limited".to_string());
        }

        SystemHealth {
            healthy,
            components,
            warnings,
        }
    }

    /// Get migration status if migration coordinator is available
    pub async fn get_migration_status(&self) -> Result<Option<migration::MigrationStatusSummary>, UnifiedAuthError> {
        if let Some(coordinator) = &self.migration_coordinator {
            coordinator.get_status_summary().await
                .map(Some)
                .map_err(|e| UnifiedAuthError::ConfigError(e.to_string()))
        } else {
            Ok(None)
        }
    }

    /// Force refresh of all provider status
    pub async fn refresh_provider_status(&self) -> Result<(), UnifiedAuthError> {
        if let Some(manager) = &self.unified_manager {
            manager.refresh_all_provider_status().await
        } else {
            Ok(())
        }
    }

    /// Update authentication strategy
    pub fn set_provider_strategy(&mut self, strategy: ProviderSelectionStrategy) {
        self.config.default_strategy = strategy.clone();
        if let Some(manager) = &mut self.unified_manager {
            manager.set_strategy(strategy);
        }
    }

    /// Check if system is ready for normal operation
    pub async fn is_ready(&self) -> bool {
        match self.get_system_status().await {
            Ok(status) => status.ready,
            Err(_) => false,
        }
    }
}

/// Claude authentication setup types
#[derive(Debug, Clone)]
pub enum ClaudeSetupType {
    ApiKey(String),
    OAuth(ClaudeTokenData),
}

/// Convenience functions for common authentication patterns
pub mod convenience {
    use super::*;

    /// Create a simple code generation context
    pub fn code_generation_context(estimated_tokens: Option<u64>) -> AuthContext {
        AuthContext {
            task_type: TaskType::CodeGeneration,
            estimated_tokens,
            priority: Priority::Medium,
            user_preference: None,
            required_features: vec![],
        }
    }

    /// Create an agent execution context
    pub fn agent_execution_context(estimated_tokens: u64, priority: Priority) -> AuthContext {
        AuthContext {
            task_type: TaskType::AgentExecution,
            estimated_tokens: Some(estimated_tokens),
            priority,
            user_preference: None,
            required_features: vec!["multi_agent".to_string()],
        }
    }

    /// Create a batch processing context
    pub fn batch_processing_context(estimated_tokens: u64) -> AuthContext {
        AuthContext {
            task_type: TaskType::Batch,
            estimated_tokens: Some(estimated_tokens),
            priority: Priority::Low,
            user_preference: None,
            required_features: vec!["high_throughput".to_string()],
        }
    }

    /// Create an interactive context (for real-time applications)
    pub fn interactive_context() -> AuthContext {
        AuthContext {
            task_type: TaskType::Interactive,
            estimated_tokens: Some(500), // Typically smaller for interactive use
            priority: Priority::High,
            user_preference: None,
            required_features: vec!["low_latency".to_string()],
        }
    }

    /// Quick setup for development/testing
    pub async fn setup_for_development(codex_home: PathBuf) -> Result<AuthenticationManager, UnifiedAuthError> {
        let mut config = AuthManagerConfig::default();
        config.verbose_logging = true;
        config.auto_migration_detection = true;
        config.migration_config.validate_tokens_before_migration = false; // Skip token validation in dev
        
        AuthenticationManager::with_config(codex_home, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_authentication_manager_creation() {
        let temp_dir = tempdir().unwrap();
        
        // Create basic auth.json for testing
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();
        
        let auth_manager = AuthenticationManager::new(temp_dir.path().to_path_buf()).await.unwrap();
        
        let status = auth_manager.get_system_status().await.unwrap();
        assert!(status.provider_status.contains_key(&ProviderType::OpenAI));
    }

    #[tokio::test]
    async fn test_migration_detection() {
        let temp_dir = tempdir().unwrap();
        
        // Create legacy OpenAI-only auth.json
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();
        
        let mut config = AuthManagerConfig::default();
        config.auto_migration_detection = true;
        
        let auth_manager = AuthenticationManager::with_config(temp_dir.path().to_path_buf(), config).await.unwrap();
        
        let status = auth_manager.get_system_status().await.unwrap();
        // Should detect migration need if unified_auth.json doesn't exist
        assert_eq!(status.migration_needed, !temp_dir.path().join("unified_auth.json").exists());
    }

    #[tokio::test]
    async fn test_convenience_functions() {
        let context = convenience::code_generation_context(Some(2000));
        assert_eq!(context.estimated_tokens, Some(2000));
        assert!(matches!(context.task_type, TaskType::CodeGeneration));

        let context = convenience::agent_execution_context(5000, Priority::High);
        assert_eq!(context.estimated_tokens, Some(5000));
        assert_eq!(context.priority as u8, Priority::High as u8);
        assert!(context.required_features.contains(&"multi_agent".to_string()));
    }

    #[tokio::test]
    async fn test_system_health_assessment() {
        let temp_dir = tempdir().unwrap();
        tokio::fs::create_dir_all(temp_dir.path()).await.unwrap();
        
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();
        
        let auth_manager = AuthenticationManager::new(temp_dir.path().to_path_buf()).await.unwrap();
        let status = auth_manager.get_system_status().await.unwrap();
        
        // Should have filesystem component
        assert!(status.health.components.contains_key("filesystem"));
        
        // Should have at least one provider
        assert!(!status.provider_status.is_empty());
    }
}