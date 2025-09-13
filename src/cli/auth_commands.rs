//! Extended authentication commands with multi-provider support
//! 
//! This module extends the existing CLI authentication system to support
//! multiple providers (OpenAI and Claude) with intelligent provider selection
//! and comprehensive quota management.

use clap::{Parser, Subcommand, ValueEnum};
use codex_common::CliConfigOverrides;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::claude_auth::{SecureClaudeAuth, ClaudeAuthConfig, ClaudeAuthError};

/// Authentication provider types
#[derive(Debug, Clone, ValueEnum, Serialize, Deserialize)]
pub enum AuthProvider {
    /// OpenAI provider (ChatGPT OAuth or API key)
    #[value(name = "openai")]
    OpenAI,
    /// Claude provider (Claude Max OAuth or API key)
    #[value(name = "claude")]
    Claude,
    /// Automatically select best provider
    #[value(name = "auto")]
    Auto,
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthProvider::OpenAI => write!(f, "openai"),
            AuthProvider::Claude => write!(f, "claude"),
            AuthProvider::Auto => write!(f, "auto"),
        }
    }
}

/// Extended login command with provider support
#[derive(Debug, Parser)]
pub struct ExtendedLoginCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// API key for provider (if using API key authentication)
    #[arg(long = "api-key", value_name = "API_KEY")]
    pub api_key: Option<String>,

    /// Authentication provider to use
    #[arg(long = "provider", value_enum, default_value_t = AuthProvider::Auto)]
    pub provider: AuthProvider,

    /// Force re-authentication even if already logged in
    #[arg(long = "force")]
    pub force: bool,

    #[command(subcommand)]
    pub action: Option<ExtendedLoginSubcommand>,
}

/// Extended authentication subcommands
#[derive(Debug, Subcommand)]
pub enum ExtendedLoginSubcommand {
    /// Show login status for all or specific provider
    Status {
        /// Show status for specific provider
        #[arg(long = "provider", value_enum)]
        provider: Option<AuthProvider>,
        /// Show detailed information including quotas
        #[arg(long = "detailed")]
        detailed: bool,
    },
    /// List all available authentication providers
    Providers {
        /// Show only active providers
        #[arg(long = "active-only")]
        active_only: bool,
    },
    /// Switch active provider
    Switch {
        /// Provider to switch to
        #[arg(value_enum)]
        provider: AuthProvider,
        /// Force switch even if target provider not authenticated
        #[arg(long = "force")]
        force: bool,
    },
    /// Show quota information for Claude Max users
    Quota {
        /// Provider to check quota for
        #[arg(long = "provider", value_enum, default_value_t = AuthProvider::Claude)]
        provider: AuthProvider,
        /// Show detailed quota breakdown
        #[arg(long = "detailed")]
        detailed: bool,
    },
    /// Test authentication with provider
    Test {
        /// Provider to test
        #[arg(long = "provider", value_enum, default_value_t = AuthProvider::Auto)]
        provider: AuthProvider,
    },
}

/// Authentication status information
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthStatus {
    pub provider: AuthProvider,
    pub authenticated: bool,
    pub user_info: Option<UserInfo>,
    pub subscription_info: Option<SubscriptionInfo>,
    pub quota_info: Option<QuotaInfo>,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub tier: String,
    pub features: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub daily_limit: Option<u64>,
    pub current_usage: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_time: Option<chrono::DateTime<chrono::Utc>>,
    pub percentage_used: Option<f64>,
}

/// Provider capabilities information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider: AuthProvider,
    pub name: String,
    pub description: String,
    pub auth_methods: Vec<String>,
    pub features: Vec<String>,
    pub requires_subscription: bool,
    pub supports_quota_management: bool,
}

/// Unified authentication manager for CLI operations
pub struct UnifiedAuthManager {
    config_overrides: CliConfigOverrides,
    claude_auth: Option<SecureClaudeAuth>,
    preferred_provider: AuthProvider,
}

impl UnifiedAuthManager {
    /// Create new unified authentication manager
    pub fn new(config_overrides: CliConfigOverrides) -> Result<Self, Box<dyn std::error::Error>> {
        let claude_config = ClaudeAuthConfig::default();
        let claude_auth = match SecureClaudeAuth::new(
            claude_config,
            std::env::home_dir()
                .unwrap_or_default()
                .join(".codex")
                .join("claude_tokens.json")
        ) {
            Ok(auth) => Some(auth),
            Err(e) => {
                eprintln!("Warning: Failed to initialize Claude authentication: {}", e);
                None
            }
        };

        Ok(Self {
            config_overrides,
            claude_auth,
            preferred_provider: AuthProvider::Auto,
        })
    }

    /// Get authentication status for all providers
    pub async fn get_auth_status(&self, provider_filter: Option<AuthProvider>) -> Result<Vec<AuthStatus>, Box<dyn std::error::Error>> {
        let mut statuses = Vec::new();

        // Check OpenAI authentication status
        if provider_filter.is_none() || matches!(provider_filter, Some(AuthProvider::OpenAI) | Some(AuthProvider::Auto)) {
            let openai_status = self.get_openai_auth_status().await?;
            statuses.push(openai_status);
        }

        // Check Claude authentication status
        if provider_filter.is_none() || matches!(provider_filter, Some(AuthProvider::Claude) | Some(AuthProvider::Auto)) {
            let claude_status = self.get_claude_auth_status().await?;
            statuses.push(claude_status);
        }

        Ok(statuses)
    }

    /// Get available provider capabilities
    pub fn get_provider_capabilities(&self, active_only: bool) -> Vec<ProviderCapabilities> {
        let mut capabilities = Vec::new();

        // OpenAI capabilities
        let openai_active = self.is_openai_authenticated();
        if !active_only || openai_active {
            capabilities.push(ProviderCapabilities {
                provider: AuthProvider::OpenAI,
                name: "OpenAI".to_string(),
                description: "OpenAI GPT models with ChatGPT OAuth or API key authentication".to_string(),
                auth_methods: vec!["OAuth (ChatGPT)".to_string(), "API Key".to_string()],
                features: vec!["Chat completions".to_string(), "Code generation".to_string(), "Text analysis".to_string()],
                requires_subscription: false,
                supports_quota_management: false,
            });
        }

        // Claude capabilities
        let claude_active = self.is_claude_authenticated();
        if !active_only || claude_active {
            capabilities.push(ProviderCapabilities {
                provider: AuthProvider::Claude,
                name: "Anthropic Claude".to_string(),
                description: "Claude AI models with Claude Max OAuth or API key authentication".to_string(),
                auth_methods: vec!["OAuth (Claude Max)".to_string(), "API Key".to_string()],
                features: vec!["Chat completions".to_string(), "Code analysis".to_string(), "Long context".to_string(), "Constitutional AI".to_string()],
                requires_subscription: false,
                supports_quota_management: true,
            });
        }

        capabilities
    }

    /// Switch active provider
    pub async fn switch_provider(&mut self, provider: AuthProvider, force: bool) -> Result<(), Box<dyn std::error::Error>> {
        // Validate that target provider is authenticated (unless forced)
        if !force {
            match provider {
                AuthProvider::OpenAI => {
                    if !self.is_openai_authenticated() {
                        return Err("OpenAI provider is not authenticated. Use --force to switch anyway.".into());
                    }
                }
                AuthProvider::Claude => {
                    if !self.is_claude_authenticated() {
                        return Err("Claude provider is not authenticated. Use --force to switch anyway.".into());
                    }
                }
                AuthProvider::Auto => {
                    // Auto is always valid
                }
            }
        }

        self.preferred_provider = provider;
        self.save_provider_preference()?;
        Ok(())
    }

    /// Get quota information for Claude provider
    pub async fn get_claude_quota(&self, detailed: bool) -> Result<Option<QuotaInfo>, Box<dyn std::error::Error>> {
        if let Some(ref claude_auth) = self.claude_auth {
            if let Some(tokens) = claude_auth.get_stored_tokens()? {
                match claude_auth.verify_subscription(&tokens.access_token).await {
                    Ok(subscription) => {
                        let percentage_used = if let (Some(current), Some(limit)) = (subscription.usage_current, subscription.usage_limit) {
                            Some((current as f64 / limit as f64) * 100.0)
                        } else {
                            None
                        };

                        let remaining = if let (Some(current), Some(limit)) = (subscription.usage_current, subscription.usage_limit) {
                            Some(limit.saturating_sub(current))
                        } else {
                            None
                        };

                        Ok(Some(QuotaInfo {
                            daily_limit: subscription.usage_limit,
                            current_usage: subscription.usage_current,
                            remaining,
                            reset_time: subscription.reset_date,
                            percentage_used,
                        }))
                    }
                    Err(_) => Ok(None),
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Test authentication with specified provider
    pub async fn test_authentication(&self, provider: AuthProvider) -> Result<bool, Box<dyn std::error::Error>> {
        match provider {
            AuthProvider::OpenAI => Ok(self.test_openai_auth().await?),
            AuthProvider::Claude => Ok(self.test_claude_auth().await?),
            AuthProvider::Auto => {
                // Test the best available provider
                if self.is_claude_authenticated() {
                    Ok(self.test_claude_auth().await?)
                } else if self.is_openai_authenticated() {
                    Ok(self.test_openai_auth().await?)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// Perform Claude authentication
    pub async fn authenticate_claude(&mut self, api_key: Option<String>, force: bool) -> Result<(), ClaudeAuthError> {
        if let Some(ref mut claude_auth) = self.claude_auth {
            // Check if already authenticated and not forcing
            if !force && claude_auth.is_authenticated() {
                return Err(ClaudeAuthError::AuthenticationFailed(
                    "Already authenticated with Claude. Use --force to re-authenticate.".to_string()
                ));
            }

            if let Some(key) = api_key {
                // API key authentication
                // Note: This would need to be implemented in SecureClaudeAuth
                todo!("Implement API key authentication for Claude")
            } else {
                // OAuth authentication
                let auth_url = claude_auth.start_oauth_flow()?;
                
                println!("Opening Claude authentication in your browser...");
                println!("If your browser doesn't open automatically, visit: {}", auth_url);
                
                // Open browser
                if let Err(e) = open::that(&auth_url) {
                    eprintln!("Failed to open browser: {}. Please visit the URL manually.", e);
                }

                println!("Waiting for authentication completion...");
                // Note: This would need a callback server implementation
                todo!("Implement OAuth callback handling")
            }
        } else {
            Err(ClaudeAuthError::InvalidConfiguration(
                "Claude authentication not available".to_string()
            ))
        }
    }

    // Private helper methods
    async fn get_openai_auth_status(&self) -> Result<AuthStatus, Box<dyn std::error::Error>> {
        // Implementation would use existing OpenAI auth checking logic
        Ok(AuthStatus {
            provider: AuthProvider::OpenAI,
            authenticated: self.is_openai_authenticated(),
            user_info: None, // Would be populated from OpenAI auth
            subscription_info: None,
            quota_info: None,
            last_used: None,
            expires_at: None,
        })
    }

    async fn get_claude_auth_status(&self) -> Result<AuthStatus, Box<dyn std::error::Error>> {
        if let Some(ref claude_auth) = self.claude_auth {
            let authenticated = claude_auth.is_authenticated();
            let mut subscription_info = None;
            let mut quota_info = None;

            if authenticated {
                if let Some(tokens) = claude_auth.get_stored_tokens()? {
                    // Get subscription info
                    if let Ok(subscription) = claude_auth.verify_subscription(&tokens.access_token).await {
                        subscription_info = Some(SubscriptionInfo {
                            tier: subscription.tier.clone(),
                            features: subscription.features.clone(),
                            active: subscription.active,
                        });

                        // Calculate quota info
                        if let (Some(current), Some(limit)) = (subscription.usage_current, subscription.usage_limit) {
                            quota_info = Some(QuotaInfo {
                                daily_limit: Some(limit),
                                current_usage: Some(current),
                                remaining: Some(limit.saturating_sub(current)),
                                reset_time: subscription.reset_date,
                                percentage_used: Some((current as f64 / limit as f64) * 100.0),
                            });
                        }
                    }
                }
            }

            Ok(AuthStatus {
                provider: AuthProvider::Claude,
                authenticated,
                user_info: None, // Would be extracted from ID token
                subscription_info,
                quota_info,
                last_used: None,
                expires_at: None, // Would be from token data
            })
        } else {
            Ok(AuthStatus {
                provider: AuthProvider::Claude,
                authenticated: false,
                user_info: None,
                subscription_info: None,
                quota_info: None,
                last_used: None,
                expires_at: None,
            })
        }
    }

    fn is_openai_authenticated(&self) -> bool {
        // This would use the existing OpenAI auth checking logic
        // For now, return false as placeholder
        false
    }

    fn is_claude_authenticated(&self) -> bool {
        self.claude_auth.as_ref().map_or(false, |auth| auth.is_authenticated())
    }

    async fn test_openai_auth(&self) -> Result<bool, Box<dyn std::error::Error>> {
        // This would test OpenAI authentication by making a simple API call
        Ok(false) // Placeholder
    }

    async fn test_claude_auth(&self) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(ref claude_auth) = self.claude_auth {
            if let Some(tokens) = claude_auth.get_stored_tokens()? {
                // Test by checking subscription (lightweight API call)
                match claude_auth.verify_subscription(&tokens.access_token).await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn save_provider_preference(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Save preferred provider to config file
        let config_path = std::env::home_dir()
            .unwrap_or_default()
            .join(".codex")
            .join("auth_config.json");

        let config = serde_json::json!({
            "preferred_provider": self.preferred_provider
        });

        std::fs::create_dir_all(config_path.parent().unwrap())?;
        std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
        Ok(())
    }
}

/// Format authentication status for display
pub fn format_auth_status(statuses: &[AuthStatus], detailed: bool) -> String {
    let mut output = String::new();
    
    output.push_str("Authentication Status:\n");
    output.push_str("=====================\n\n");

    for status in statuses {
        output.push_str(&format!("Provider: {} ({})\n", 
            status.provider, 
            if status.authenticated { "✓ Authenticated" } else { "✗ Not Authenticated" }
        ));

        if detailed && status.authenticated {
            if let Some(ref subscription) = status.subscription_info {
                output.push_str(&format!("  Subscription: {} ({})\n", 
                    subscription.tier,
                    if subscription.active { "Active" } else { "Inactive" }
                ));
                
                if !subscription.features.is_empty() {
                    output.push_str(&format!("  Features: {}\n", subscription.features.join(", ")));
                }
            }

            if let Some(ref quota) = status.quota_info {
                if let (Some(current), Some(limit)) = (quota.current_usage, quota.daily_limit) {
                    output.push_str(&format!("  Quota: {}/{} ({:.1}%)\n", 
                        current, limit, quota.percentage_used.unwrap_or(0.0)
                    ));
                }
                
                if let Some(reset_time) = quota.reset_time {
                    output.push_str(&format!("  Resets: {}\n", reset_time.format("%Y-%m-%d %H:%M UTC")));
                }
            }

            if let Some(expires_at) = status.expires_at {
                output.push_str(&format!("  Token Expires: {}\n", expires_at.format("%Y-%m-%d %H:%M UTC")));
            }
        }

        output.push('\n');
    }

    output
}

/// Format provider capabilities for display
pub fn format_provider_capabilities(capabilities: &[ProviderCapabilities]) -> String {
    let mut output = String::new();
    
    output.push_str("Available Providers:\n");
    output.push_str("===================\n\n");

    for cap in capabilities {
        output.push_str(&format!("Provider: {}\n", cap.name));
        output.push_str(&format!("  Description: {}\n", cap.description));
        output.push_str(&format!("  Auth Methods: {}\n", cap.auth_methods.join(", ")));
        output.push_str(&format!("  Features: {}\n", cap.features.join(", ")));
        
        if cap.requires_subscription {
            output.push_str("  Requires Subscription: Yes\n");
        }
        
        if cap.supports_quota_management {
            output.push_str("  Quota Management: Supported\n");
        }
        
        output.push('\n');
    }

    output
}

/// Format quota information for display
pub fn format_quota_info(quota: &QuotaInfo, provider: AuthProvider) -> String {
    let mut output = String::new();
    
    output.push_str(&format!("{} Quota Information:\n", provider));
    output.push_str("========================\n\n");

    if let (Some(current), Some(limit)) = (quota.current_usage, quota.daily_limit) {
        let remaining = limit.saturating_sub(current);
        let percentage = (current as f64 / limit as f64) * 100.0;
        
        output.push_str(&format!("Usage: {}/{} tokens ({:.1}%)\n", current, limit, percentage));
        output.push_str(&format!("Remaining: {} tokens\n", remaining));
        
        // Progress bar
        let bar_width = 40;
        let filled = ((current as f64 / limit as f64) * bar_width as f64) as usize;
        let empty = bar_width - filled;
        output.push_str(&format!("Progress: [{}{}]\n", 
            "█".repeat(filled), 
            "░".repeat(empty)
        ));
    }

    if let Some(reset_time) = quota.reset_time {
        output.push_str(&format!("Resets: {}\n", reset_time.format("%Y-%m-%d %H:%M UTC")));
        
        let now = chrono::Utc::now();
        if reset_time > now {
            let duration = reset_time - now;
            let hours = duration.num_hours();
            let minutes = duration.num_minutes() % 60;
            output.push_str(&format!("Time until reset: {}h {}m\n", hours, minutes));
        }
    }

    output
}