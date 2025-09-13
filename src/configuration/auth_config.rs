//! Authentication configuration structures
//! 
//! Defines the configuration schema for unified authentication supporting
//! both OpenAI and Claude providers with intelligent selection strategies.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration};
use std::fmt;

/// Core authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthConfig {
    /// Preferred authentication provider
    pub preferred_provider: ProviderType,
    
    /// Enable automatic fallback between providers
    pub enable_fallback: bool,
    
    /// Strategy for selecting providers when both are available
    pub provider_preference: ProviderPreference,
    
    /// Fallback strategy when preferred provider fails
    pub fallback_strategy: FallbackStrategy,
    
    /// How often to check Claude subscription status
    pub subscription_check_interval: Duration,
    
    /// Enable automatic subscription status checking
    pub enable_subscription_check: bool,
    
    /// Maximum time to wait for authentication operations
    pub auth_timeout: Duration,
    
    /// Enable automatic token refresh
    pub auto_refresh_tokens: bool,
    
    /// Last time provider availability was checked
    pub last_provider_check: Option<DateTime<Utc>>,
    
    /// Cache provider capabilities for this duration
    pub provider_cache_duration: Duration,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            preferred_provider: ProviderType::OpenAI,
            enable_fallback: true,
            provider_preference: ProviderPreference::PreferClaude,
            fallback_strategy: FallbackStrategy::Automatic,
            subscription_check_interval: Duration::hours(24),
            enable_subscription_check: true,
            auth_timeout: Duration::seconds(30),
            auto_refresh_tokens: true,
            last_provider_check: None,
            provider_cache_duration: Duration::minutes(15),
        }
    }
}

impl AuthConfig {
    /// Create configuration optimized for Claude Max users
    pub fn claude_max_optimized() -> Self {
        Self {
            preferred_provider: ProviderType::Claude,
            provider_preference: ProviderPreference::PreferClaude,
            subscription_check_interval: Duration::hours(6), // More frequent checks
            enable_subscription_check: true,
            fallback_strategy: FallbackStrategy::OnQuotaExhausted,
            ..Default::default()
        }
    }

    /// Create configuration optimized for OpenAI users
    pub fn openai_optimized() -> Self {
        Self {
            preferred_provider: ProviderType::OpenAI,
            provider_preference: ProviderPreference::PreferOpenAI,
            subscription_check_interval: Duration::hours(24),
            enable_subscription_check: false,
            fallback_strategy: FallbackStrategy::Manual,
            ..Default::default()
        }
    }

    /// Create configuration for cost-conscious usage
    pub fn cost_optimized() -> Self {
        Self {
            preferred_provider: ProviderType::Claude,
            provider_preference: ProviderPreference::CostOptimized,
            subscription_check_interval: Duration::hours(1), // Frequent quota checks
            enable_subscription_check: true,
            fallback_strategy: FallbackStrategy::OnQuotaExhausted,
            ..Default::default()
        }
    }

    /// Check if subscription verification is needed
    pub fn needs_subscription_check(&self) -> bool {
        if !self.enable_subscription_check {
            return false;
        }

        match self.last_provider_check {
            Some(last_check) => Utc::now() - last_check > self.subscription_check_interval,
            None => true,
        }
    }

    /// Update the last provider check timestamp
    pub fn update_provider_check(&mut self) {
        self.last_provider_check = Some(Utc::now());
    }

    /// Check if provider capabilities cache is valid
    pub fn is_cache_valid(&self) -> bool {
        match self.last_provider_check {
            Some(last_check) => Utc::now() - last_check < self.provider_cache_duration,
            None => false,
        }
    }
}

/// Authentication provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "claude")]
    Claude,
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::Claude => write!(f, "claude"),
        }
    }
}

impl From<&str> for ProviderType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" | "anthropic" => ProviderType::Claude,
            _ => ProviderType::OpenAI,
        }
    }
}

/// Provider selection preferences
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderPreference {
    /// Always prefer Claude when available
    #[serde(rename = "prefer_claude")]
    PreferClaude,
    
    /// Always prefer OpenAI when available
    #[serde(rename = "prefer_openai")]
    PreferOpenAI,
    
    /// Choose based on cost optimization
    #[serde(rename = "cost_optimized")]
    CostOptimized,
    
    /// Choose based on performance characteristics
    #[serde(rename = "performance_optimized")]
    PerformanceOptimized,
    
    /// Choose based on quota availability
    #[serde(rename = "quota_optimized")]
    QuotaOptimized,
    
    /// Use explicit user preference
    #[serde(rename = "user_preference")]
    UserPreference(ProviderType),
}

impl Default for ProviderPreference {
    fn default() -> Self {
        ProviderPreference::PreferClaude
    }
}

impl ProviderPreference {
    /// Get the preferred provider type from this preference
    pub fn get_preferred_provider(&self) -> Option<ProviderType> {
        match self {
            ProviderPreference::PreferClaude => Some(ProviderType::Claude),
            ProviderPreference::PreferOpenAI => Some(ProviderType::OpenAI),
            ProviderPreference::UserPreference(provider) => Some(*provider),
            _ => None, // Dynamic preferences don't have a fixed preference
        }
    }

    /// Check if this preference requires runtime evaluation
    pub fn is_dynamic(&self) -> bool {
        matches!(
            self,
            ProviderPreference::CostOptimized
                | ProviderPreference::PerformanceOptimized
                | ProviderPreference::QuotaOptimized
        )
    }
}

/// Fallback strategies when preferred provider fails
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FallbackStrategy {
    /// Automatically fallback to the other provider
    #[serde(rename = "automatic")]
    Automatic,
    
    /// Only fallback when quota is exhausted
    #[serde(rename = "on_quota_exhausted")]
    OnQuotaExhausted,
    
    /// Only fallback on authentication errors
    #[serde(rename = "on_auth_error")]
    OnAuthError,
    
    /// Never automatically fallback (manual intervention required)
    #[serde(rename = "manual")]
    Manual,
    
    /// Custom fallback with specific conditions
    #[serde(rename = "conditional")]
    Conditional {
        on_quota_exhausted: bool,
        on_auth_error: bool,
        on_rate_limit: bool,
        on_network_error: bool,
    },
}

impl Default for FallbackStrategy {
    fn default() -> Self {
        FallbackStrategy::Automatic
    }
}

impl FallbackStrategy {
    /// Check if fallback should occur for the given error type
    pub fn should_fallback(&self, error_type: &AuthErrorType) -> bool {
        match self {
            FallbackStrategy::Automatic => true,
            FallbackStrategy::OnQuotaExhausted => {
                matches!(error_type, AuthErrorType::QuotaExhausted)
            }
            FallbackStrategy::OnAuthError => {
                matches!(error_type, AuthErrorType::AuthenticationFailed)
            }
            FallbackStrategy::Manual => false,
            FallbackStrategy::Conditional {
                on_quota_exhausted,
                on_auth_error,
                on_rate_limit,
                on_network_error,
            } => match error_type {
                AuthErrorType::QuotaExhausted => *on_quota_exhausted,
                AuthErrorType::AuthenticationFailed => *on_auth_error,
                AuthErrorType::RateLimited => *on_rate_limit,
                AuthErrorType::NetworkError => *on_network_error,
                _ => false,
            },
        }
    }
}

/// Subscription checking configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionCheckConfig {
    /// Enable automatic subscription checking
    pub enabled: bool,
    
    /// How often to check subscription status
    pub check_interval: Duration,
    
    /// Require Claude Max subscription for Claude provider
    pub require_max_subscription: bool,
    
    /// Fallback to API key if subscription check fails
    pub fallback_to_api_key: bool,
    
    /// Cache subscription status for this duration
    pub cache_duration: Duration,
}

impl Default for SubscriptionCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::hours(24),
            require_max_subscription: false,
            fallback_to_api_key: true,
            cache_duration: Duration::hours(1),
        }
    }
}

/// Types of authentication errors for fallback decisions
#[derive(Debug, Clone, PartialEq)]
pub enum AuthErrorType {
    AuthenticationFailed,
    QuotaExhausted,
    RateLimited,
    NetworkError,
    SubscriptionExpired,
    InvalidCredentials,
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert_eq!(config.preferred_provider, ProviderType::OpenAI);
        assert!(config.enable_fallback);
        assert!(config.enable_subscription_check);
    }

    #[test]
    fn test_claude_max_optimized_config() {
        let config = AuthConfig::claude_max_optimized();
        assert_eq!(config.preferred_provider, ProviderType::Claude);
        assert_eq!(config.provider_preference, ProviderPreference::PreferClaude);
        assert_eq!(config.subscription_check_interval, Duration::hours(6));
    }

    #[test]
    fn test_provider_type_from_string() {
        assert_eq!(ProviderType::from("claude"), ProviderType::Claude);
        assert_eq!(ProviderType::from("anthropic"), ProviderType::Claude);
        assert_eq!(ProviderType::from("openai"), ProviderType::OpenAI);
        assert_eq!(ProviderType::from("unknown"), ProviderType::OpenAI);
    }

    #[test]
    fn test_fallback_strategy_decisions() {
        let automatic = FallbackStrategy::Automatic;
        assert!(automatic.should_fallback(&AuthErrorType::QuotaExhausted));
        assert!(automatic.should_fallback(&AuthErrorType::AuthenticationFailed));

        let manual = FallbackStrategy::Manual;
        assert!(!manual.should_fallback(&AuthErrorType::QuotaExhausted));

        let on_quota = FallbackStrategy::OnQuotaExhausted;
        assert!(on_quota.should_fallback(&AuthErrorType::QuotaExhausted));
        assert!(!on_quota.should_fallback(&AuthErrorType::AuthenticationFailed));
    }

    #[test]
    fn test_provider_preference_dynamic() {
        assert!(ProviderPreference::CostOptimized.is_dynamic());
        assert!(!ProviderPreference::PreferClaude.is_dynamic());
        assert!(!ProviderPreference::UserPreference(ProviderType::OpenAI).is_dynamic());
    }

    #[test]
    fn test_subscription_check_timing() {
        let mut config = AuthConfig::default();
        
        // Should need check initially
        assert!(config.needs_subscription_check());
        
        // Update check timestamp
        config.update_provider_check();
        
        // Should not need check immediately after
        assert!(!config.needs_subscription_check());
    }

    #[test]
    fn test_config_serialization() {
        let config = AuthConfig::claude_max_optimized();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: AuthConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.preferred_provider, deserialized.preferred_provider);
    }
}