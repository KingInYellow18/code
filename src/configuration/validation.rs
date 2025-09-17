//! Configuration validation system
//! 
//! Provides comprehensive validation for authentication configurations,
//! ensuring data integrity and security compliance.

use std::collections::HashSet;
use chrono::{DateTime, Utc, Duration};
use regex::Regex;
use once_cell::sync::Lazy;

use super::auth_config::{AuthConfig, ProviderType, ProviderPreference, FallbackStrategy};
use super::unified_storage::{UnifiedAuthJson, OpenAIAuthData, ClaudeAuthData, AuthData};
use super::UnifiedConfig;

/// Configuration validator
#[derive(Debug)]
pub struct ConfigValidator {
    rules: Vec<Box<dyn ValidationRule>>,
    strict_mode: bool,
}

impl Clone for ConfigValidator {
    fn clone(&self) -> Self {
        let cloned_rules = self.rules.iter()
            .map(|rule| rule.clone_rule())
            .collect();

        Self {
            rules: cloned_rules,
            strict_mode: self.strict_mode,
        }
    }
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigValidator {
    /// Create new validator with default rules
    pub fn new() -> Self {
        let rules: Vec<Box<dyn ValidationRule>> = vec![
            Box::new(BasicIntegrityRule),
            Box::new(AuthenticationRule),
            Box::new(SecurityRule),
            Box::new(TokenValidityRule),
            Box::new(ConfigurationConsistencyRule),
            Box::new(ProviderAvailabilityRule),
        ];

        Self {
            rules,
            strict_mode: false,
        }
    }

    /// Create validator with strict validation enabled
    pub fn new_strict() -> Self {
        let mut validator = Self::new();
        validator.strict_mode = true;
        validator
    }

    /// Add custom validation rule
    pub fn add_rule<R: ValidationRule + 'static>(&mut self, rule: R) {
        self.rules.push(Box::new(rule));
    }

    /// Validate configuration
    pub fn validate(&self, config: &UnifiedConfig) -> Result<ValidationResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        let context = ValidationContext {
            config,
            strict_mode: self.strict_mode,
        };

        for rule in &self.rules {
            match rule.validate(&context) {
                Ok(result) => {
                    issues.extend(result.issues);
                    warnings.extend(result.warnings);
                    recommendations.extend(result.recommendations);
                }
                Err(e) => {
                    issues.push(format!("Validation rule failed: {}", e));
                }
            }
        }

        let is_valid = issues.is_empty();
        let severity = if !is_valid {
            ValidationSeverity::Error
        } else if !warnings.is_empty() {
            ValidationSeverity::Warning
        } else {
            ValidationSeverity::Valid
        };

        Ok(ValidationResult {
            is_valid,
            severity,
            issues,
            warnings,
            recommendations,
        })
    }

    /// Quick validation check (errors only)
    pub fn quick_validate(&self, config: &UnifiedConfig) -> Result<bool, ValidationError> {
        let result = self.validate(config)?;
        Ok(result.is_valid)
    }

    /// Validate specific auth data
    pub fn validate_auth_data(&self, auth_data: &UnifiedAuthJson) -> ValidationResult {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();

        // Check version compatibility
        if auth_data.version < 2 {
            warnings.push("Using older auth format version. Consider migrating.".to_string());
        }

        // Check for expired tokens
        if let Some(openai_auth) = &auth_data.openai_auth {
            if openai_auth.needs_refresh() {
                warnings.push("OpenAI tokens need refresh".to_string());
            }
        }

        if let Some(claude_auth) = &auth_data.claude_auth {
            if claude_auth.needs_refresh() {
                warnings.push("Claude tokens need refresh".to_string());
            }
        }

        // Check for authentication availability
        if auth_data.openai_auth.is_none() && auth_data.claude_auth.is_none() {
            issues.push("No authentication providers configured".to_string());
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            severity: if issues.is_empty() { 
                ValidationSeverity::Valid 
            } else { 
                ValidationSeverity::Error 
            },
            issues,
            warnings,
            recommendations: Vec::new(),
        }
    }
}

/// Validation context
pub struct ValidationContext<'a> {
    pub config: &'a UnifiedConfig,
    pub strict_mode: bool,
}

/// Validation rule trait
pub trait ValidationRule: Send + Sync + std::fmt::Debug {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError>;
    fn name(&self) -> &'static str;
    fn priority(&self) -> u8 { 50 } // Lower number = higher priority

    /// Clone the validation rule for trait object cloning
    fn clone_rule(&self) -> Box<dyn ValidationRule>;
}

/// Basic integrity validation rule
#[derive(Debug, Clone)]
struct BasicIntegrityRule;

impl ValidationRule for BasicIntegrityRule {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();

        // Check for basic configuration completeness
        let config = &context.config;

        // Validate timeout values
        if config.auth.auth_timeout < Duration::seconds(1) {
            issues.push("Auth timeout is too low (minimum 1 second)".to_string());
        }
        if config.auth.auth_timeout > Duration::minutes(10) {
            warnings.push("Auth timeout is very high (over 10 minutes)".to_string());
        }

        // Validate subscription check interval
        if config.auth.subscription_check_interval < Duration::minutes(5) {
            warnings.push("Subscription check interval is very frequent (under 5 minutes)".to_string());
        }

        Ok(RuleResult {
            issues,
            warnings,
            recommendations: Vec::new(),
        })
    }

    fn name(&self) -> &'static str {
        "BasicIntegrity"
    }

    fn priority(&self) -> u8 {
        10
    }

    fn clone_rule(&self) -> Box<dyn ValidationRule> {
        Box::new(self.clone())
    }
}

/// Authentication validation rule
#[derive(Debug, Clone)]
struct AuthenticationRule;

impl ValidationRule for AuthenticationRule {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        let auth_data = &context.config.auth_data;

        // Check if at least one provider is configured
        if auth_data.openai_auth.is_none() && auth_data.claude_auth.is_none() {
            issues.push("No authentication providers configured".to_string());
            recommendations.push("Configure at least one authentication provider".to_string());
        }

        // Validate OpenAI authentication
        if let Some(openai_auth) = &auth_data.openai_auth {
            if let Err(validation_issues) = self.validate_openai_auth(openai_auth) {
                issues.extend(validation_issues);
            }
        }

        // Validate Claude authentication
        if let Some(claude_auth) = &auth_data.claude_auth {
            if let Err(validation_issues) = self.validate_claude_auth(claude_auth) {
                issues.extend(validation_issues);
            }
        }

        // Check preferred provider is actually available
        let preferred = context.config.auth.preferred_provider;
        match preferred {
            ProviderType::OpenAI if auth_data.openai_auth.is_none() => {
                warnings.push("Preferred provider (OpenAI) is not configured".to_string());
                recommendations.push("Either configure OpenAI authentication or change preferred provider".to_string());
            }
            ProviderType::Claude if auth_data.claude_auth.is_none() => {
                warnings.push("Preferred provider (Claude) is not configured".to_string());
                recommendations.push("Either configure Claude authentication or change preferred provider".to_string());
            }
            _ => {}
        }

        Ok(RuleResult {
            issues,
            warnings,
            recommendations,
        })
    }

    fn name(&self) -> &'static str {
        "Authentication"
    }

    fn priority(&self) -> u8 {
        20
    }

    fn clone_rule(&self) -> Box<dyn ValidationRule> {
        Box::new(self.clone())
    }
}

impl AuthenticationRule {
    fn validate_openai_auth(&self, auth: &OpenAIAuthData) -> Result<(), Vec<String>> {
        let mut issues = Vec::new();

        // Validate API key format
        if let Some(api_key) = &auth.api_key {
            if !self.is_valid_openai_api_key(api_key) {
                issues.push("OpenAI API key format appears invalid".to_string());
            }
        }

        // Check that either API key or tokens exist
        if auth.api_key.is_none() && auth.tokens.is_none() {
            issues.push("OpenAI authentication has neither API key nor tokens".to_string());
        }

        if issues.is_empty() { Ok(()) } else { Err(issues) }
    }

    fn validate_claude_auth(&self, auth: &ClaudeAuthData) -> Result<(), Vec<String>> {
        let mut issues = Vec::new();

        // Validate API key format
        if let Some(api_key) = &auth.api_key {
            if !self.is_valid_claude_api_key(api_key) {
                issues.push("Claude API key format appears invalid".to_string());
            }
        }

        // Check that either API key or tokens exist
        if auth.api_key.is_none() && auth.tokens.is_none() {
            issues.push("Claude authentication has neither API key nor tokens".to_string());
        }

        if issues.is_empty() { Ok(()) } else { Err(issues) }
    }

    fn is_valid_openai_api_key(&self, key: &str) -> bool {
        static OPENAI_KEY_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^sk-[A-Za-z0-9]{48,}$").unwrap()
        });
        OPENAI_KEY_REGEX.is_match(key)
    }

    fn is_valid_claude_api_key(&self, key: &str) -> bool {
        static CLAUDE_KEY_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^sk-ant-[A-Za-z0-9]{48,}$").unwrap()
        });
        CLAUDE_KEY_REGEX.is_match(key)
    }
}

/// Security validation rule
#[derive(Debug, Clone)]
struct SecurityRule;

impl ValidationRule for SecurityRule {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        // Check for security best practices
        if !context.config.auth.auto_refresh_tokens {
            warnings.push("Automatic token refresh is disabled".to_string());
            recommendations.push("Enable automatic token refresh for better security".to_string());
        }

        // Check for fallback configuration in strict mode
        if context.strict_mode && context.config.auth.enable_fallback {
            warnings.push("Fallback is enabled in strict mode".to_string());
        }

        // Validate subscription checking is enabled for Claude
        if let Some(_claude_auth) = &context.config.auth_data.claude_auth {
            if !context.config.auth.enable_subscription_check {
                recommendations.push("Enable subscription checking for Claude authentication".to_string());
            }
        }

        Ok(RuleResult {
            issues,
            warnings,
            recommendations,
        })
    }

    fn name(&self) -> &'static str {
        "Security"
    }

    fn priority(&self) -> u8 {
        30
    }

    fn clone_rule(&self) -> Box<dyn ValidationRule> {
        Box::new(self.clone())
    }
}

/// Token validity validation rule
#[derive(Debug, Clone)]
struct TokenValidityRule;

impl ValidationRule for TokenValidityRule {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        let auth_data = &context.config.auth_data;

        // Check OpenAI tokens
        if let Some(openai_auth) = &auth_data.openai_auth {
            if let Some(tokens) = &openai_auth.tokens {
                if let Some(expires_at) = tokens.expires_at {
                    let now = Utc::now();
                    if now > expires_at {
                        issues.push("OpenAI tokens have expired".to_string());
                    } else if now > expires_at - Duration::hours(24) {
                        warnings.push("OpenAI tokens will expire within 24 hours".to_string());
                        recommendations.push("Refresh OpenAI tokens soon".to_string());
                    }
                }
            }
        }

        // Check Claude tokens
        if let Some(claude_auth) = &auth_data.claude_auth {
            if let Some(tokens) = &claude_auth.tokens {
                if let Some(expires_at) = tokens.expires_at {
                    let now = Utc::now();
                    if now > expires_at {
                        issues.push("Claude tokens have expired".to_string());
                    } else if now > expires_at - Duration::hours(24) {
                        warnings.push("Claude tokens will expire within 24 hours".to_string());
                        recommendations.push("Refresh Claude tokens soon".to_string());
                    }
                }
            }
        }

        Ok(RuleResult {
            issues,
            warnings,
            recommendations,
        })
    }

    fn name(&self) -> &'static str {
        "TokenValidity"
    }

    fn priority(&self) -> u8 {
        40
    }

    fn clone_rule(&self) -> Box<dyn ValidationRule> {
        Box::new(self.clone())
    }
}

/// Configuration consistency validation rule
#[derive(Debug, Clone)]
struct ConfigurationConsistencyRule;

impl ValidationRule for ConfigurationConsistencyRule {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        let config = &context.config;

        // Check consistency between preferred provider and provider preference
        match (&config.auth.preferred_provider, &config.auth.provider_preference) {
            (ProviderType::OpenAI, ProviderPreference::PreferClaude) => {
                warnings.push("Preferred provider (OpenAI) conflicts with provider preference (PreferClaude)".to_string());
                recommendations.push("Align preferred provider with provider preference".to_string());
            }
            (ProviderType::Claude, ProviderPreference::PreferOpenAI) => {
                warnings.push("Preferred provider (Claude) conflicts with provider preference (PreferOpenAI)".to_string());
                recommendations.push("Align preferred provider with provider preference".to_string());
            }
            _ => {}
        }

        // Check fallback configuration consistency
        if !config.auth.enable_fallback && config.auth.fallback_strategy != FallbackStrategy::Manual {
            warnings.push("Fallback is disabled but fallback strategy is not set to Manual".to_string());
        }

        // Check subscription checking consistency
        if config.auth.enable_subscription_check && config.auth_data.claude_auth.is_none() {
            warnings.push("Subscription checking is enabled but Claude authentication is not configured".to_string());
        }

        Ok(RuleResult {
            issues,
            warnings,
            recommendations,
        })
    }

    fn name(&self) -> &'static str {
        "ConfigurationConsistency"
    }

    fn priority(&self) -> u8 {
        50
    }

    fn clone_rule(&self) -> Box<dyn ValidationRule> {
        Box::new(self.clone())
    }
}

/// Provider availability validation rule
#[derive(Debug, Clone)]
struct ProviderAvailabilityRule;

impl ValidationRule for ProviderAvailabilityRule {
    fn validate(&self, context: &ValidationContext) -> Result<RuleResult, ValidationError> {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        let auth_data = &context.config.auth_data;

        // Count available providers
        let mut available_providers = HashSet::new();
        
        if let Some(openai_auth) = &auth_data.openai_auth {
            if openai_auth.is_authenticated() {
                available_providers.insert(ProviderType::OpenAI);
            }
        }

        if let Some(claude_auth) = &auth_data.claude_auth {
            if claude_auth.is_authenticated() {
                available_providers.insert(ProviderType::Claude);
            }
        }

        // Validate provider availability
        if available_providers.is_empty() {
            issues.push("No authentication providers are available".to_string());
            recommendations.push("Configure at least one authentication provider".to_string());
        } else if available_providers.len() == 1 && context.config.auth.enable_fallback {
            warnings.push("Fallback is enabled but only one provider is available".to_string());
            recommendations.push("Configure additional providers for true fallback capability".to_string());
        }

        // Check if preferred provider is available
        if !available_providers.contains(&context.config.auth.preferred_provider) {
            if context.strict_mode {
                issues.push(format!("Preferred provider ({}) is not available", context.config.auth.preferred_provider));
            } else {
                warnings.push(format!("Preferred provider ({}) is not available", context.config.auth.preferred_provider));
            }
        }

        Ok(RuleResult {
            issues,
            warnings,
            recommendations,
        })
    }

    fn name(&self) -> &'static str {
        "ProviderAvailability"
    }

    fn priority(&self) -> u8 {
        60
    }

    fn clone_rule(&self) -> Box<dyn ValidationRule> {
        Box::new(self.clone())
    }
}

/// Validation result from a single rule
#[derive(Debug, Clone)]
struct RuleResult {
    issues: Vec<String>,
    warnings: Vec<String>,
    recommendations: Vec<String>,
}

/// Overall validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub severity: ValidationSeverity,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

impl ValidationResult {
    /// Check if there are any issues or warnings
    pub fn has_problems(&self) -> bool {
        !self.issues.is_empty() || !self.warnings.is_empty()
    }

    /// Get a summary of the validation result
    pub fn summary(&self) -> String {
        match self.severity {
            ValidationSeverity::Valid => "Configuration is valid".to_string(),
            ValidationSeverity::Warning => format!("Configuration has {} warnings", self.warnings.len()),
            ValidationSeverity::Error => format!("Configuration has {} errors", self.issues.len()),
        }
    }
}

/// Validation severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationSeverity {
    Valid,
    Warning,
    Error,
}

/// Validation error types
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Validation rule error: {0}")]
    RuleError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Internal validation error: {0}")]
    InternalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::{UnifiedConfig, auth_config::AuthConfig};

    fn create_test_config() -> UnifiedConfig {
        UnifiedConfig {
            auth: AuthConfig::default(),
            auth_data: UnifiedAuthJson::default(),
        }
    }

    #[test]
    fn test_basic_validation() {
        let validator = ConfigValidator::new();
        let config = create_test_config();
        
        let result = validator.validate(&config).unwrap();
        assert!(!result.is_valid); // Should fail due to no auth providers
    }

    #[test]
    fn test_openai_key_validation() {
        let rule = AuthenticationRule;
        
        assert!(rule.is_valid_openai_api_key("sk-1234567890123456789012345678901234567890123456789"));
        assert!(!rule.is_valid_openai_api_key("sk-short"));
        assert!(!rule.is_valid_openai_api_key("invalid-key"));
    }

    #[test]
    fn test_claude_key_validation() {
        let rule = AuthenticationRule;
        
        assert!(rule.is_valid_claude_api_key("sk-ant-1234567890123456789012345678901234567890123456789"));
        assert!(!rule.is_valid_claude_api_key("sk-ant-short"));
        assert!(!rule.is_valid_claude_api_key("sk-1234567890123456789012345678901234567890123456789"));
    }

    #[test]
    fn test_validation_severity() {
        assert!(ValidationSeverity::Error > ValidationSeverity::Warning);
        assert!(ValidationSeverity::Warning > ValidationSeverity::Valid);
    }

    #[test]
    fn test_validation_result_summary() {
        let result = ValidationResult {
            is_valid: false,
            severity: ValidationSeverity::Error,
            issues: vec!["Test error".to_string()],
            warnings: Vec::new(),
            recommendations: Vec::new(),
        };
        
        assert_eq!(result.summary(), "Configuration has 1 errors");
        assert!(result.has_problems());
    }

    #[test]
    fn test_strict_mode_validation() {
        let validator = ConfigValidator::new_strict();
        assert!(validator.strict_mode);
    }
}