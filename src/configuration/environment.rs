//! Environment variable override system
//! 
//! Provides environment variable-based configuration overrides while maintaining
//! security and following the Code project's existing patterns.

use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use chrono::Duration;

use super::auth_config::{AuthConfig, ProviderType, ProviderPreference, FallbackStrategy};
use super::UnifiedConfig;

/// Environment configuration manager
#[derive(Debug, Clone)]
pub struct EnvironmentConfig {
    overrides: EnvironmentOverrides,
    prefix: String,
}

impl EnvironmentConfig {
    /// Create new environment configuration with default prefix
    pub fn new() -> Self {
        Self {
            overrides: EnvironmentOverrides::load(),
            prefix: "CODE_AUTH_".to_string(),
        }
    }

    /// Create with custom prefix
    pub fn with_prefix(prefix: String) -> Self {
        Self {
            overrides: EnvironmentOverrides::load_with_prefix(&prefix),
            prefix,
        }
    }

    /// Apply environment overrides to configuration
    pub fn apply_overrides(&self, config: &mut UnifiedConfig) -> Result<(), EnvironmentError> {
        // Apply auth configuration overrides
        if let Some(provider) = &self.overrides.preferred_provider {
            config.auth.preferred_provider = *provider;
        }

        if let Some(fallback) = self.overrides.enable_fallback {
            config.auth.enable_fallback = fallback;
        }

        if let Some(strategy) = &self.overrides.fallback_strategy {
            config.auth.fallback_strategy = strategy.clone();
        }

        if let Some(check_interval) = self.overrides.subscription_check_interval {
            config.auth.subscription_check_interval = check_interval;
        }

        if let Some(enable_check) = self.overrides.enable_subscription_check {
            config.auth.enable_subscription_check = enable_check;
        }

        if let Some(timeout) = self.overrides.auth_timeout {
            config.auth.auth_timeout = timeout;
        }

        if let Some(auto_refresh) = self.overrides.auto_refresh_tokens {
            config.auth.auto_refresh_tokens = auto_refresh;
        }

        // Apply authentication data overrides
        if let Some(openai_key) = &self.overrides.openai_api_key {
            if config.auth_data.openai_auth.is_none() {
                config.auth_data.openai_auth = Some(super::unified_storage::OpenAIAuthData {
                    api_key: Some(openai_key.clone()),
                    tokens: None,
                });
            } else if let Some(openai_auth) = &mut config.auth_data.openai_auth {
                openai_auth.api_key = Some(openai_key.clone());
            }
        }

        if let Some(claude_key) = &self.overrides.claude_api_key {
            if config.auth_data.claude_auth.is_none() {
                config.auth_data.claude_auth = Some(super::unified_storage::ClaudeAuthData {
                    api_key: Some(claude_key.clone()),
                    tokens: None,
                    subscription: None,
                });
            } else if let Some(claude_auth) = &mut config.auth_data.claude_auth {
                claude_auth.api_key = Some(claude_key.clone());
            }
        }

        Ok(())
    }

    /// Get current environment overrides
    pub fn get_overrides(&self) -> &EnvironmentOverrides {
        &self.overrides
    }

    /// Refresh overrides from environment
    pub fn refresh(&mut self) {
        self.overrides = EnvironmentOverrides::load_with_prefix(&self.prefix);
    }

    /// Check if any overrides are active
    pub fn has_overrides(&self) -> bool {
        self.overrides.has_any_overrides()
    }

    /// Get list of active override variables
    pub fn get_active_variables(&self) -> Vec<String> {
        self.overrides.get_active_variables(&self.prefix)
    }

    /// Validate environment variable values
    pub fn validate_environment(&self) -> Result<(), EnvironmentError> {
        self.overrides.validate()
    }
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Environment variable overrides
#[derive(Debug, Clone, Default)]
pub struct EnvironmentOverrides {
    // Auth configuration overrides
    pub preferred_provider: Option<ProviderType>,
    pub enable_fallback: Option<bool>,
    pub fallback_strategy: Option<FallbackStrategy>,
    pub subscription_check_interval: Option<Duration>,
    pub enable_subscription_check: Option<bool>,
    pub auth_timeout: Option<Duration>,
    pub auto_refresh_tokens: Option<bool>,

    // API key overrides (for development/testing)
    pub openai_api_key: Option<String>,
    pub claude_api_key: Option<String>,
    pub anthropic_api_key: Option<String>, // Alias for claude_api_key

    // Debug and development flags
    pub debug_auth: Option<bool>,
    pub force_provider: Option<ProviderType>,
    pub disable_token_validation: Option<bool>,
}

impl EnvironmentOverrides {
    /// Load overrides from environment variables
    pub fn load() -> Self {
        Self::load_with_prefix("CODE_AUTH_")
    }

    /// Load overrides with custom prefix
    pub fn load_with_prefix(prefix: &str) -> Self {
        let mut overrides = Self::default();

        // Load auth configuration overrides
        overrides.preferred_provider = Self::get_env_provider(&format!("{}PREFERRED_PROVIDER", prefix));
        overrides.enable_fallback = Self::get_env_bool(&format!("{}ENABLE_FALLBACK", prefix));
        overrides.fallback_strategy = Self::get_env_fallback_strategy(&format!("{}FALLBACK_STRATEGY", prefix));
        overrides.subscription_check_interval = Self::get_env_duration(&format!("{}SUBSCRIPTION_CHECK_INTERVAL", prefix));
        overrides.enable_subscription_check = Self::get_env_bool(&format!("{}ENABLE_SUBSCRIPTION_CHECK", prefix));
        overrides.auth_timeout = Self::get_env_duration(&format!("{}AUTH_TIMEOUT", prefix));
        overrides.auto_refresh_tokens = Self::get_env_bool(&format!("{}AUTO_REFRESH_TOKENS", prefix));

        // Load API key overrides
        overrides.openai_api_key = env::var("OPENAI_API_KEY").ok();
        overrides.claude_api_key = env::var("CLAUDE_API_KEY").ok()
            .or_else(|| env::var("ANTHROPIC_API_KEY").ok());
        overrides.anthropic_api_key = env::var("ANTHROPIC_API_KEY").ok();

        // Load debug flags
        overrides.debug_auth = Self::get_env_bool(&format!("{}DEBUG", prefix));
        overrides.force_provider = Self::get_env_provider(&format!("{}FORCE_PROVIDER", prefix));
        overrides.disable_token_validation = Self::get_env_bool(&format!("{}DISABLE_TOKEN_VALIDATION", prefix));

        overrides
    }

    /// Check if any overrides are set
    pub fn has_any_overrides(&self) -> bool {
        self.preferred_provider.is_some()
            || self.enable_fallback.is_some()
            || self.fallback_strategy.is_some()
            || self.subscription_check_interval.is_some()
            || self.enable_subscription_check.is_some()
            || self.auth_timeout.is_some()
            || self.auto_refresh_tokens.is_some()
            || self.openai_api_key.is_some()
            || self.claude_api_key.is_some()
            || self.anthropic_api_key.is_some()
            || self.debug_auth.is_some()
            || self.force_provider.is_some()
            || self.disable_token_validation.is_some()
    }

    /// Get list of active environment variables
    pub fn get_active_variables(&self, prefix: &str) -> Vec<String> {
        let mut variables = Vec::new();

        if self.preferred_provider.is_some() {
            variables.push(format!("{}PREFERRED_PROVIDER", prefix));
        }
        if self.enable_fallback.is_some() {
            variables.push(format!("{}ENABLE_FALLBACK", prefix));
        }
        if self.fallback_strategy.is_some() {
            variables.push(format!("{}FALLBACK_STRATEGY", prefix));
        }
        if self.subscription_check_interval.is_some() {
            variables.push(format!("{}SUBSCRIPTION_CHECK_INTERVAL", prefix));
        }
        if self.enable_subscription_check.is_some() {
            variables.push(format!("{}ENABLE_SUBSCRIPTION_CHECK", prefix));
        }
        if self.auth_timeout.is_some() {
            variables.push(format!("{}AUTH_TIMEOUT", prefix));
        }
        if self.auto_refresh_tokens.is_some() {
            variables.push(format!("{}AUTO_REFRESH_TOKENS", prefix));
        }
        if self.openai_api_key.is_some() {
            variables.push("OPENAI_API_KEY".to_string());
        }
        if self.claude_api_key.is_some() {
            variables.push("CLAUDE_API_KEY".to_string());
        }
        if self.anthropic_api_key.is_some() {
            variables.push("ANTHROPIC_API_KEY".to_string());
        }
        if self.debug_auth.is_some() {
            variables.push(format!("{}DEBUG", prefix));
        }
        if self.force_provider.is_some() {
            variables.push(format!("{}FORCE_PROVIDER", prefix));
        }
        if self.disable_token_validation.is_some() {
            variables.push(format!("{}DISABLE_TOKEN_VALIDATION", prefix));
        }

        variables
    }

    /// Validate environment variable values
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        // Validate timeout values
        if let Some(timeout) = self.auth_timeout {
            if timeout < Duration::seconds(1) {
                return Err(EnvironmentError::InvalidValue(
                    "AUTH_TIMEOUT must be at least 1 second".to_string()
                ));
            }
            if timeout > Duration::minutes(30) {
                return Err(EnvironmentError::InvalidValue(
                    "AUTH_TIMEOUT cannot exceed 30 minutes".to_string()
                ));
            }
        }

        if let Some(interval) = self.subscription_check_interval {
            if interval < Duration::minutes(1) {
                return Err(EnvironmentError::InvalidValue(
                    "SUBSCRIPTION_CHECK_INTERVAL must be at least 1 minute".to_string()
                ));
            }
        }

        // Validate API key formats
        if let Some(openai_key) = &self.openai_api_key {
            if !openai_key.starts_with("sk-") {
                return Err(EnvironmentError::InvalidValue(
                    "OPENAI_API_KEY must start with 'sk-'".to_string()
                ));
            }
        }

        if let Some(claude_key) = &self.claude_api_key {
            if !claude_key.starts_with("sk-ant-") && !claude_key.starts_with("sk-") {
                return Err(EnvironmentError::InvalidValue(
                    "CLAUDE_API_KEY must start with 'sk-ant-' or 'sk-'".to_string()
                ));
            }
        }

        Ok(())
    }

    // Helper methods for parsing environment variables
    fn get_env_bool(key: &str) -> Option<bool> {
        env::var(key).ok().and_then(|v| {
            match v.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            }
        })
    }

    fn get_env_provider(key: &str) -> Option<ProviderType> {
        env::var(key).ok().and_then(|v| {
            match v.to_lowercase().as_str() {
                "openai" => Some(ProviderType::OpenAI),
                "claude" | "anthropic" => Some(ProviderType::Claude),
                _ => None,
            }
        })
    }

    fn get_env_fallback_strategy(key: &str) -> Option<FallbackStrategy> {
        env::var(key).ok().and_then(|v| {
            match v.to_lowercase().as_str() {
                "automatic" => Some(FallbackStrategy::Automatic),
                "manual" => Some(FallbackStrategy::Manual),
                "on_quota_exhausted" => Some(FallbackStrategy::OnQuotaExhausted),
                "on_auth_error" => Some(FallbackStrategy::OnAuthError),
                _ => None,
            }
        })
    }

    fn get_env_duration(key: &str) -> Option<Duration> {
        env::var(key).ok().and_then(|v| {
            // Support formats like "30s", "5m", "1h", "2d"
            if let Some(captures) = regex::Regex::new(r"^(\d+)([smhd])$").unwrap().captures(&v) {
                let number: i64 = captures.get(1)?.as_str().parse().ok()?;
                let unit = captures.get(2)?.as_str();
                
                match unit {
                    "s" => Some(Duration::seconds(number)),
                    "m" => Some(Duration::minutes(number)),
                    "h" => Some(Duration::hours(number)),
                    "d" => Some(Duration::days(number)),
                    _ => None,
                }
            } else {
                // Try parsing as seconds
                v.parse::<i64>().ok().map(Duration::seconds)
            }
        })
    }
}

/// Environment configuration error types
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error("Invalid environment variable value: {0}")]
    InvalidValue(String),
    
    #[error("Missing required environment variable: {0}")]
    MissingVariable(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Configuration conflict: {0}")]
    ConfigConflict(String),
}

/// Environment variable documentation
pub struct EnvironmentVariableDoc {
    pub name: String,
    pub description: String,
    pub example: String,
    pub required: bool,
}

impl EnvironmentConfig {
    /// Get documentation for all supported environment variables
    pub fn get_documentation() -> Vec<EnvironmentVariableDoc> {
        vec![
            EnvironmentVariableDoc {
                name: "CODE_AUTH_PREFERRED_PROVIDER".to_string(),
                description: "Set preferred authentication provider".to_string(),
                example: "openai | claude".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CODE_AUTH_ENABLE_FALLBACK".to_string(),
                description: "Enable automatic fallback between providers".to_string(),
                example: "true | false".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CODE_AUTH_FALLBACK_STRATEGY".to_string(),
                description: "Strategy for provider fallback".to_string(),
                example: "automatic | manual | on_quota_exhausted | on_auth_error".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CODE_AUTH_SUBSCRIPTION_CHECK_INTERVAL".to_string(),
                description: "How often to check Claude subscription status".to_string(),
                example: "1h | 30m | 3600s".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CODE_AUTH_AUTH_TIMEOUT".to_string(),
                description: "Timeout for authentication operations".to_string(),
                example: "30s | 60s".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "OPENAI_API_KEY".to_string(),
                description: "OpenAI API key for authentication".to_string(),
                example: "sk-1234567890...".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CLAUDE_API_KEY".to_string(),
                description: "Claude API key for authentication".to_string(),
                example: "sk-ant-1234567890...".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "ANTHROPIC_API_KEY".to_string(),
                description: "Anthropic API key (alias for CLAUDE_API_KEY)".to_string(),
                example: "sk-ant-1234567890...".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CODE_AUTH_DEBUG".to_string(),
                description: "Enable debug logging for authentication".to_string(),
                example: "true | false".to_string(),
                required: false,
            },
            EnvironmentVariableDoc {
                name: "CODE_AUTH_FORCE_PROVIDER".to_string(),
                description: "Force use of specific provider (overrides selection logic)".to_string(),
                example: "openai | claude".to_string(),
                required: false,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_environment_config_creation() {
        let config = EnvironmentConfig::new();
        assert_eq!(config.prefix, "CODE_AUTH_");
    }

    #[test]
    fn test_environment_config_with_custom_prefix() {
        let config = EnvironmentConfig::with_prefix("CUSTOM_".to_string());
        assert_eq!(config.prefix, "CUSTOM_");
    }

    #[test]
    fn test_bool_parsing() {
        assert_eq!(EnvironmentOverrides::get_env_bool("NONEXISTENT"), None);
        
        env::set_var("TEST_BOOL_TRUE", "true");
        assert_eq!(EnvironmentOverrides::get_env_bool("TEST_BOOL_TRUE"), Some(true));
        
        env::set_var("TEST_BOOL_FALSE", "false");
        assert_eq!(EnvironmentOverrides::get_env_bool("TEST_BOOL_FALSE"), Some(false));
        
        env::set_var("TEST_BOOL_INVALID", "maybe");
        assert_eq!(EnvironmentOverrides::get_env_bool("TEST_BOOL_INVALID"), None);
        
        // Cleanup
        env::remove_var("TEST_BOOL_TRUE");
        env::remove_var("TEST_BOOL_FALSE");
        env::remove_var("TEST_BOOL_INVALID");
    }

    #[test]
    fn test_provider_parsing() {
        env::set_var("TEST_PROVIDER_OPENAI", "openai");
        assert_eq!(EnvironmentOverrides::get_env_provider("TEST_PROVIDER_OPENAI"), Some(ProviderType::OpenAI));
        
        env::set_var("TEST_PROVIDER_CLAUDE", "claude");
        assert_eq!(EnvironmentOverrides::get_env_provider("TEST_PROVIDER_CLAUDE"), Some(ProviderType::Claude));
        
        env::set_var("TEST_PROVIDER_ANTHROPIC", "anthropic");
        assert_eq!(EnvironmentOverrides::get_env_provider("TEST_PROVIDER_ANTHROPIC"), Some(ProviderType::Claude));
        
        env::set_var("TEST_PROVIDER_INVALID", "invalid");
        assert_eq!(EnvironmentOverrides::get_env_provider("TEST_PROVIDER_INVALID"), None);
        
        // Cleanup
        env::remove_var("TEST_PROVIDER_OPENAI");
        env::remove_var("TEST_PROVIDER_CLAUDE");
        env::remove_var("TEST_PROVIDER_ANTHROPIC");
        env::remove_var("TEST_PROVIDER_INVALID");
    }

    #[test]
    fn test_duration_parsing() {
        env::set_var("TEST_DURATION_SECONDS", "30s");
        assert_eq!(EnvironmentOverrides::get_env_duration("TEST_DURATION_SECONDS"), Some(Duration::seconds(30)));
        
        env::set_var("TEST_DURATION_MINUTES", "5m");
        assert_eq!(EnvironmentOverrides::get_env_duration("TEST_DURATION_MINUTES"), Some(Duration::minutes(5)));
        
        env::set_var("TEST_DURATION_HOURS", "2h");
        assert_eq!(EnvironmentOverrides::get_env_duration("TEST_DURATION_HOURS"), Some(Duration::hours(2)));
        
        env::set_var("TEST_DURATION_DAYS", "1d");
        assert_eq!(EnvironmentOverrides::get_env_duration("TEST_DURATION_DAYS"), Some(Duration::days(1)));
        
        env::set_var("TEST_DURATION_PLAIN", "3600");
        assert_eq!(EnvironmentOverrides::get_env_duration("TEST_DURATION_PLAIN"), Some(Duration::seconds(3600)));
        
        // Cleanup
        env::remove_var("TEST_DURATION_SECONDS");
        env::remove_var("TEST_DURATION_MINUTES");
        env::remove_var("TEST_DURATION_HOURS");
        env::remove_var("TEST_DURATION_DAYS");
        env::remove_var("TEST_DURATION_PLAIN");
    }

    #[test]
    fn test_validation() {
        let mut overrides = EnvironmentOverrides::default();
        
        // Valid configuration
        assert!(overrides.validate().is_ok());
        
        // Invalid timeout (too short)
        overrides.auth_timeout = Some(Duration::seconds(0));
        assert!(overrides.validate().is_err());
        
        // Invalid timeout (too long)
        overrides.auth_timeout = Some(Duration::minutes(31));
        assert!(overrides.validate().is_err());
        
        // Valid timeout
        overrides.auth_timeout = Some(Duration::seconds(30));
        assert!(overrides.validate().is_ok());
        
        // Invalid OpenAI key format
        overrides.openai_api_key = Some("invalid-key".to_string());
        assert!(overrides.validate().is_err());
        
        // Valid OpenAI key format
        overrides.openai_api_key = Some("sk-1234567890".to_string());
        assert!(overrides.validate().is_ok());
    }

    #[test]
    fn test_has_overrides() {
        let mut overrides = EnvironmentOverrides::default();
        assert!(!overrides.has_any_overrides());
        
        overrides.preferred_provider = Some(ProviderType::Claude);
        assert!(overrides.has_any_overrides());
    }

    #[test]
    fn test_active_variables() {
        let mut overrides = EnvironmentOverrides::default();
        overrides.preferred_provider = Some(ProviderType::Claude);
        overrides.openai_api_key = Some("sk-test".to_string());
        
        let variables = overrides.get_active_variables("CODE_AUTH_");
        assert!(variables.contains(&"CODE_AUTH_PREFERRED_PROVIDER".to_string()));
        assert!(variables.contains(&"OPENAI_API_KEY".to_string()));
    }

    #[test]
    fn test_environment_documentation() {
        let docs = EnvironmentConfig::get_documentation();
        assert!(!docs.is_empty());
        
        // Check that key variables are documented
        let names: Vec<&str> = docs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"OPENAI_API_KEY"));
        assert!(names.contains(&"CLAUDE_API_KEY"));
        assert!(names.contains(&"CODE_AUTH_PREFERRED_PROVIDER"));
    }
}