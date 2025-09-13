/// # Unified Authentication Manager
/// 
/// Provides a single interface for managing both OpenAI and Claude authentication,
/// with intelligent provider selection and seamless fallback mechanisms.

use super::claude::{ClaudeAuth, ClaudeAuthMode, ClaudeAuthError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Provider types supported by the unified system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    OpenAI,
    Claude,
}

/// Authentication provider wrapper
#[derive(Debug, Clone)]
pub enum AuthProvider {
    OpenAI(OpenAIAuth),
    Claude(ClaudeAuth),
}

/// OpenAI authentication (simplified wrapper for existing types)
#[derive(Debug, Clone)]
pub struct OpenAIAuth {
    // This would wrap the existing CodexAuth from the original codebase
    pub mode: String, // "ChatGPT" or "ApiKey"
    pub api_key: Option<String>,
    pub has_tokens: bool,
}

/// Provider selection strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderSelectionStrategy {
    /// Prefer Claude Max if available, fallback to OpenAI
    PreferClaude,
    /// Prefer OpenAI, fallback to Claude
    PreferOpenAI,
    /// Choose based on cost optimization
    CostOptimized,
    /// Use user's explicit choice
    UserChoice(ProviderType),
    /// Adaptive selection based on usage patterns
    Adaptive,
    /// Best available subscription (Max > Pro > API Key)
    BestSubscription,
}

/// Authentication context for provider selection
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub task_type: TaskType,
    pub estimated_tokens: Option<u64>,
    pub priority: Priority,
    pub user_preference: Option<ProviderType>,
    pub required_features: Vec<String>,
}

/// Types of tasks that may influence provider selection
#[derive(Debug, Clone)]
pub enum TaskType {
    CodeGeneration,
    Analysis,
    AgentExecution,
    LongRunning,
    Interactive,
    Batch,
}

/// Task priority levels
#[derive(Debug, Clone)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Provider capabilities and status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider_type: ProviderType,
    pub available: bool,
    pub authenticated: bool,
    pub subscription_tier: Option<String>,
    pub quota_remaining: Option<u64>,
    pub rate_limit_status: RateLimitStatus,
    pub last_verified: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Rate limiting status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub requests_remaining: Option<u32>,
    pub tokens_remaining: Option<u64>,
    pub reset_time: Option<DateTime<Utc>>,
    pub current_usage: f64, // Percentage of limit used
}

/// Unified authentication manager
#[derive(Debug)]
pub struct UnifiedAuthManager {
    codex_home: PathBuf,
    strategy: ProviderSelectionStrategy,
    providers: Arc<RwLock<HashMap<ProviderType, AuthProvider>>>,
    status_cache: Arc<RwLock<HashMap<ProviderType, ProviderStatus>>>,
    usage_stats: Arc<RwLock<UsageStats>>,
    config: UnifiedAuthConfig,
}

/// Configuration for unified authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAuthConfig {
    pub enable_fallback: bool,
    pub cache_status_duration_seconds: u64,
    pub auto_refresh_tokens: bool,
    pub monitor_quota: bool,
    pub load_balance_agents: bool,
    pub max_concurrent_claude_agents: u16,
    pub preference_learning_enabled: bool,
}

impl Default for UnifiedAuthConfig {
    fn default() -> Self {
        Self {
            enable_fallback: true,
            cache_status_duration_seconds: 300, // 5 minutes
            auto_refresh_tokens: true,
            monitor_quota: true,
            load_balance_agents: true,
            max_concurrent_claude_agents: 10,
            preference_learning_enabled: true,
        }
    }
}

/// Usage statistics for learning user preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub provider_usage: HashMap<ProviderType, ProviderUsage>,
    pub task_type_preferences: HashMap<String, ProviderType>,
    pub success_rates: HashMap<ProviderType, f64>,
    pub average_response_times: HashMap<ProviderType, f64>,
    pub total_requests: u64,
    pub last_updated: DateTime<Utc>,
}

/// Usage statistics per provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub requests_count: u64,
    pub tokens_used: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub average_response_time_ms: f64,
    pub last_used: DateTime<Utc>,
}

impl Default for UsageStats {
    fn default() -> Self {
        Self {
            provider_usage: HashMap::new(),
            task_type_preferences: HashMap::new(),
            success_rates: HashMap::new(),
            average_response_times: HashMap::new(),
            total_requests: 0,
            last_updated: Utc::now(),
        }
    }
}

impl UnifiedAuthManager {
    /// Create a new unified authentication manager
    pub async fn new(codex_home: PathBuf, strategy: ProviderSelectionStrategy) -> Result<Self, UnifiedAuthError> {
        let config = UnifiedAuthConfig::default();
        Self::with_config(codex_home, strategy, config).await
    }

    /// Create with custom configuration
    pub async fn with_config(
        codex_home: PathBuf, 
        strategy: ProviderSelectionStrategy, 
        config: UnifiedAuthConfig
    ) -> Result<Self, UnifiedAuthError> {
        let mut manager = Self {
            codex_home,
            strategy,
            providers: Arc::new(RwLock::new(HashMap::new())),
            status_cache: Arc::new(RwLock::new(HashMap::new())),
            usage_stats: Arc::new(RwLock::new(UsageStats::default())),
            config,
        };

        // Load existing providers
        manager.load_providers().await?;
        
        // Initialize status cache
        manager.refresh_all_provider_status().await?;

        // Load usage statistics
        manager.load_usage_stats().await?;

        Ok(manager)
    }

    /// Get the optimal provider for a given context
    pub async fn get_optimal_provider(&self, context: &AuthContext) -> Result<AuthProvider, UnifiedAuthError> {
        match self.strategy {
            ProviderSelectionStrategy::PreferClaude => {
                self.get_provider_with_fallback(ProviderType::Claude, ProviderType::OpenAI, context).await
            }
            ProviderSelectionStrategy::PreferOpenAI => {
                self.get_provider_with_fallback(ProviderType::OpenAI, ProviderType::Claude, context).await
            }
            ProviderSelectionStrategy::UserChoice(ref provider_type) => {
                self.get_specific_provider(provider_type.clone()).await
            }
            ProviderSelectionStrategy::CostOptimized => {
                self.get_cost_optimized_provider(context).await
            }
            ProviderSelectionStrategy::Adaptive => {
                self.get_adaptive_provider(context).await
            }
            ProviderSelectionStrategy::BestSubscription => {
                self.get_best_subscription_provider(context).await
            }
        }
    }

    /// Get provider with fallback logic
    async fn get_provider_with_fallback(
        &self, 
        primary: ProviderType, 
        fallback: ProviderType, 
        context: &AuthContext
    ) -> Result<AuthProvider, UnifiedAuthError> {
        // Try primary provider first
        if let Ok(provider) = self.get_specific_provider(primary.clone()).await {
            if self.is_provider_suitable(&provider, context).await? {
                return Ok(provider);
            }
        }

        // Fallback to secondary provider if enabled
        if self.config.enable_fallback {
            if let Ok(provider) = self.get_specific_provider(fallback).await {
                if self.is_provider_suitable(&provider, context).await? {
                    return Ok(provider);
                }
            }
        }

        Err(UnifiedAuthError::NoSuitableProvider)
    }

    /// Get specific provider by type
    async fn get_specific_provider(&self, provider_type: ProviderType) -> Result<AuthProvider, UnifiedAuthError> {
        let providers = self.providers.read().await;
        providers.get(&provider_type)
            .cloned()
            .ok_or(UnifiedAuthError::ProviderNotAvailable(provider_type))
    }

    /// Get cost-optimized provider
    async fn get_cost_optimized_provider(&self, context: &AuthContext) -> Result<AuthProvider, UnifiedAuthError> {
        let status_cache = self.status_cache.read().await;
        
        // Prefer Claude Max for high-volume tasks (free usage)
        if let Some(claude_status) = status_cache.get(&ProviderType::Claude) {
            if claude_status.subscription_tier.as_ref().map(|t| t == "max").unwrap_or(false) {
                if let Some(quota_remaining) = claude_status.quota_remaining {
                    if quota_remaining > context.estimated_tokens.unwrap_or(1000) {
                        return self.get_specific_provider(ProviderType::Claude).await;
                    }
                }
            }
        }

        // Fall back to OpenAI or Claude API key based on estimated cost
        if let Some(estimated_tokens) = context.estimated_tokens {
            if estimated_tokens < 10000 { // Small tasks - use Claude API key
                if let Ok(provider) = self.get_specific_provider(ProviderType::Claude).await {
                    return Ok(provider);
                }
            }
        }

        // Default to OpenAI
        self.get_specific_provider(ProviderType::OpenAI).await
    }

    /// Get adaptive provider based on usage patterns
    async fn get_adaptive_provider(&self, context: &AuthContext) -> Result<AuthProvider, UnifiedAuthError> {
        if !self.config.preference_learning_enabled {
            return self.get_best_subscription_provider(context).await;
        }

        let usage_stats = self.usage_stats.read().await;
        
        // Check if we have a learned preference for this task type
        let task_type_key = format!("{:?}", context.task_type);
        if let Some(preferred_provider) = usage_stats.task_type_preferences.get(&task_type_key) {
            if let Ok(provider) = self.get_specific_provider(preferred_provider.clone()).await {
                if self.is_provider_suitable(&provider, context).await? {
                    return Ok(provider);
                }
            }
        }

        // No learned preference, use best subscription
        self.get_best_subscription_provider(context).await
    }

    /// Get provider with best subscription tier
    async fn get_best_subscription_provider(&self, context: &AuthContext) -> Result<AuthProvider, UnifiedAuthError> {
        let status_cache = self.status_cache.read().await;
        
        // Priority: Claude Max > Claude Pro > OpenAI > Claude API Key
        let priority_order = [
            (ProviderType::Claude, Some("max".to_string())),
            (ProviderType::Claude, Some("pro".to_string())),
            (ProviderType::OpenAI, None),
            (ProviderType::Claude, None), // API key
        ];

        for (provider_type, required_tier) in &priority_order {
            if let Some(status) = status_cache.get(provider_type) {
                if status.available && status.authenticated {
                    // Check subscription tier if required
                    if let Some(required) = required_tier {
                        if status.subscription_tier.as_ref() == Some(required) {
                            if let Ok(provider) = self.get_specific_provider(provider_type.clone()).await {
                                if self.is_provider_suitable(&provider, context).await? {
                                    return Ok(provider);
                                }
                            }
                        }
                    } else {
                        // No specific tier required
                        if let Ok(provider) = self.get_specific_provider(provider_type.clone()).await {
                            if self.is_provider_suitable(&provider, context).await? {
                                return Ok(provider);
                            }
                        }
                    }
                }
            }
        }

        Err(UnifiedAuthError::NoSuitableProvider)
    }

    /// Check if provider is suitable for the given context
    async fn is_provider_suitable(&self, provider: &AuthProvider, context: &AuthContext) -> Result<bool, UnifiedAuthError> {
        match provider {
            AuthProvider::Claude(claude_auth) => {
                // Check quota if we have an estimate
                if let Some(estimated_tokens) = context.estimated_tokens {
                    let remaining_quota = claude_auth.get_remaining_quota().await
                        .map_err(|e| UnifiedAuthError::ClaudeError(e))?;
                    
                    if remaining_quota < estimated_tokens {
                        return Ok(false);
                    }
                }

                // Check concurrent limits for agent tasks
                if matches!(context.task_type, TaskType::AgentExecution) {
                    let quota_manager = claude_auth.quota_manager.read().await;
                    if quota_manager.active_agents.len() >= self.config.max_concurrent_claude_agents as usize {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            AuthProvider::OpenAI(_) => {
                // For OpenAI, we assume it's suitable if authenticated
                Ok(true)
            }
        }
    }

    /// Load providers from disk
    async fn load_providers(&mut self) -> Result<(), UnifiedAuthError> {
        let mut providers = HashMap::new();

        // Load OpenAI authentication (using existing logic)
        if let Some(openai_auth) = self.load_openai_auth().await? {
            providers.insert(ProviderType::OpenAI, AuthProvider::OpenAI(openai_auth));
        }

        // Load Claude authentication
        if let Some(claude_auth) = ClaudeAuth::from_codex_home(&self.codex_home, ClaudeAuthMode::MaxSubscription, "unified_auth") {
            providers.insert(ProviderType::Claude, AuthProvider::Claude(claude_auth));
        }

        *self.providers.write().await = providers;
        Ok(())
    }

    /// Load OpenAI authentication (simplified)
    async fn load_openai_auth(&self) -> Result<Option<OpenAIAuth>, UnifiedAuthError> {
        let auth_file = self.codex_home.join("auth.json");
        if !auth_file.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&auth_file).await?;
        let auth_data: serde_json::Value = serde_json::from_str(&content)?;

        let api_key = auth_data.get("OPENAI_API_KEY")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let has_tokens = auth_data.get("tokens").is_some();

        let mode = if has_tokens && api_key.is_none() {
            "ChatGPT".to_string()
        } else {
            "ApiKey".to_string()
        };

        Ok(Some(OpenAIAuth {
            mode,
            api_key,
            has_tokens,
        }))
    }

    /// Refresh status for all providers
    pub async fn refresh_all_provider_status(&self) -> Result<(), UnifiedAuthError> {
        let providers = self.providers.read().await;
        let mut status_updates = HashMap::new();

        for (provider_type, provider) in providers.iter() {
            let status = self.get_provider_status(provider).await;
            status_updates.insert(provider_type.clone(), status);
        }

        *self.status_cache.write().await = status_updates;
        Ok(())
    }

    /// Get status for a specific provider
    async fn get_provider_status(&self, provider: &AuthProvider) -> ProviderStatus {
        match provider {
            AuthProvider::Claude(claude_auth) => {
                let mut status = ProviderStatus {
                    provider_type: ProviderType::Claude,
                    available: true,
                    authenticated: false,
                    subscription_tier: claude_auth.subscription_tier.clone(),
                    quota_remaining: None,
                    rate_limit_status: RateLimitStatus {
                        requests_remaining: None,
                        tokens_remaining: None,
                        reset_time: None,
                        current_usage: 0.0,
                    },
                    last_verified: Some(Utc::now()),
                    error_message: None,
                };

                // Test authentication
                match claude_auth.get_token().await {
                    Ok(_) => {
                        status.authenticated = true;
                        
                        // Get quota information
                        if let Ok(remaining) = claude_auth.get_remaining_quota().await {
                            status.quota_remaining = Some(remaining);
                        }
                    }
                    Err(e) => {
                        status.error_message = Some(e.to_string());
                    }
                }

                status
            }
            AuthProvider::OpenAI(openai_auth) => {
                ProviderStatus {
                    provider_type: ProviderType::OpenAI,
                    available: true,
                    authenticated: openai_auth.api_key.is_some() || openai_auth.has_tokens,
                    subscription_tier: None,
                    quota_remaining: None,
                    rate_limit_status: RateLimitStatus {
                        requests_remaining: None,
                        tokens_remaining: None,
                        reset_time: None,
                        current_usage: 0.0,
                    },
                    last_verified: Some(Utc::now()),
                    error_message: None,
                }
            }
        }
    }

    /// Get authentication token from optimal provider
    pub async fn get_auth_token(&self, context: &AuthContext) -> Result<String, UnifiedAuthError> {
        let provider = self.get_optimal_provider(context).await?;
        
        match provider {
            AuthProvider::Claude(claude_auth) => {
                claude_auth.get_token().await
                    .map_err(|e| UnifiedAuthError::ClaudeError(e))
            }
            AuthProvider::OpenAI(openai_auth) => {
                openai_auth.api_key
                    .ok_or(UnifiedAuthError::NoValidToken)
            }
        }
    }

    /// Record usage for learning
    pub async fn record_usage(&self, provider_type: ProviderType, context: &AuthContext, success: bool, response_time_ms: f64) {
        if !self.config.preference_learning_enabled {
            return;
        }

        let mut usage_stats = self.usage_stats.write().await;
        
        // Update provider usage
        let provider_usage = usage_stats.provider_usage.entry(provider_type.clone()).or_insert_with(|| ProviderUsage {
            requests_count: 0,
            tokens_used: 0,
            success_count: 0,
            error_count: 0,
            average_response_time_ms: 0.0,
            last_used: Utc::now(),
        });

        provider_usage.requests_count += 1;
        provider_usage.last_used = Utc::now();
        
        if success {
            provider_usage.success_count += 1;
        } else {
            provider_usage.error_count += 1;
        }

        // Update average response time
        provider_usage.average_response_time_ms = 
            (provider_usage.average_response_time_ms * (provider_usage.requests_count - 1) as f64 + response_time_ms) / 
            provider_usage.requests_count as f64;

        // Update task type preferences (only for successful requests)
        if success {
            let task_type_key = format!("{:?}", context.task_type);
            usage_stats.task_type_preferences.insert(task_type_key, provider_type.clone());
        }

        // Update success rates
        let success_rate = provider_usage.success_count as f64 / provider_usage.requests_count as f64;
        usage_stats.success_rates.insert(provider_type, success_rate);

        usage_stats.total_requests += 1;
        usage_stats.last_updated = Utc::now();

        // Save to disk periodically
        if usage_stats.total_requests % 10 == 0 {
            let _ = self.save_usage_stats().await;
        }
    }

    /// Load usage statistics from disk
    async fn load_usage_stats(&self) -> Result<(), UnifiedAuthError> {
        let stats_file = self.codex_home.join("auth_usage_stats.json");
        if !stats_file.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&stats_file).await?;
        let stats: UsageStats = serde_json::from_str(&content)?;
        *self.usage_stats.write().await = stats;

        Ok(())
    }

    /// Save usage statistics to disk
    async fn save_usage_stats(&self) -> Result<(), UnifiedAuthError> {
        let stats_file = self.codex_home.join("auth_usage_stats.json");
        let stats = self.usage_stats.read().await;
        let content = serde_json::to_string_pretty(&*stats)?;
        tokio::fs::write(&stats_file, content).await?;
        Ok(())
    }

    /// Get current provider status
    pub async fn get_provider_status_summary(&self) -> HashMap<ProviderType, ProviderStatus> {
        self.status_cache.read().await.clone()
    }

    /// Switch strategy
    pub fn set_strategy(&mut self, strategy: ProviderSelectionStrategy) {
        self.strategy = strategy;
    }

    /// Add or update provider
    pub async fn add_provider(&self, provider_type: ProviderType, provider: AuthProvider) {
        self.providers.write().await.insert(provider_type.clone(), provider);
        // Refresh status for the new provider
        let _ = self.refresh_all_provider_status().await;
    }

    /// Remove provider
    pub async fn remove_provider(&self, provider_type: &ProviderType) {
        self.providers.write().await.remove(provider_type);
        self.status_cache.write().await.remove(provider_type);
    }
}

/// Unified authentication errors
#[derive(Debug, thiserror::Error)]
pub enum UnifiedAuthError {
    #[error("No suitable provider available")]
    NoSuitableProvider,
    
    #[error("Provider not available: {0:?}")]
    ProviderNotAvailable(ProviderType),
    
    #[error("No valid authentication token")]
    NoValidToken,
    
    #[error("Claude authentication error: {0}")]
    ClaudeError(ClaudeAuthError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_unified_auth_manager_creation() {
        let temp_dir = tempdir().unwrap();
        
        // Create minimal auth.json for testing
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();
        
        let manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            ProviderSelectionStrategy::PreferOpenAI
        ).await.unwrap();

        let status = manager.get_provider_status_summary().await;
        assert!(status.contains_key(&ProviderType::OpenAI));
    }

    #[tokio::test]
    async fn test_provider_selection_strategies() {
        let temp_dir = tempdir().unwrap();
        
        // Create auth files
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();
        
        let mut manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            ProviderSelectionStrategy::UserChoice(ProviderType::OpenAI)
        ).await.unwrap();

        let context = AuthContext {
            task_type: TaskType::CodeGeneration,
            estimated_tokens: Some(1000),
            priority: Priority::Medium,
            user_preference: None,
            required_features: Vec::new(),
        };

        // Test user choice strategy
        let provider = manager.get_optimal_provider(&context).await.unwrap();
        assert!(matches!(provider, AuthProvider::OpenAI(_)));

        // Test strategy switching
        manager.set_strategy(ProviderSelectionStrategy::PreferOpenAI);
        let provider = manager.get_optimal_provider(&context).await.unwrap();
        assert!(matches!(provider, AuthProvider::OpenAI(_)));
    }

    #[tokio::test]
    async fn test_usage_stats_recording() {
        let temp_dir = tempdir().unwrap();
        
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();
        
        let manager = UnifiedAuthManager::new(
            temp_dir.path().to_path_buf(),
            ProviderSelectionStrategy::Adaptive
        ).await.unwrap();

        let context = AuthContext {
            task_type: TaskType::CodeGeneration,
            estimated_tokens: Some(500),
            priority: Priority::Medium,
            user_preference: None,
            required_features: Vec::new(),
        };

        // Record usage
        manager.record_usage(ProviderType::OpenAI, &context, true, 250.0).await;
        
        let usage_stats = manager.usage_stats.read().await;
        assert!(usage_stats.provider_usage.contains_key(&ProviderType::OpenAI));
        
        let openai_usage = &usage_stats.provider_usage[&ProviderType::OpenAI];
        assert_eq!(openai_usage.requests_count, 1);
        assert_eq!(openai_usage.success_count, 1);
    }
}