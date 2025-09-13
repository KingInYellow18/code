//! CLI module for multi-provider authentication
//! 
//! This module provides extended CLI functionality for managing authentication
//! across multiple providers (OpenAI and Claude) with intelligent provider
//! selection and comprehensive management features.

pub mod auth_commands;
pub mod extended_login;

pub use auth_commands::{
    AuthProvider, ExtendedLoginCommand, ExtendedLoginSubcommand,
    UnifiedAuthManager, AuthStatus, ProviderCapabilities, QuotaInfo,
    format_auth_status, format_provider_capabilities, format_quota_info,
};

pub use extended_login::{
    run_extended_login, run_extended_logout, ExtendedLogoutCommand,
};

/// CLI integration utilities
pub mod integration {
    use super::*;
    use clap::{Parser, Subcommand};
    use codex_common::CliConfigOverrides;

    /// Extended authentication commands for the main CLI
    #[derive(Debug, Subcommand)]
    pub enum AuthCommands {
        /// Extended login with provider support
        #[command(name = "login")]
        Login(ExtendedLoginCommand),
        
        /// Extended logout with provider support  
        #[command(name = "logout")]
        Logout(ExtendedLogoutCommand),
        
        /// Show authentication status
        #[command(name = "status")]
        Status {
            /// Show status for specific provider
            #[arg(long = "provider", value_enum)]
            provider: Option<AuthProvider>,
            /// Show detailed information including quotas
            #[arg(long = "detailed")]
            detailed: bool,
        },
        
        /// List available providers
        #[command(name = "providers")]
        Providers {
            /// Show only active providers
            #[arg(long = "active-only")]
            active_only: bool,
        },
        
        /// Switch active provider
        #[command(name = "switch")]
        Switch {
            /// Provider to switch to
            #[arg(value_enum)]
            provider: AuthProvider,
            /// Force switch even if target provider not authenticated
            #[arg(long = "force")]
            force: bool,
        },
        
        /// Show quota information
        #[command(name = "quota")]
        Quota {
            /// Provider to check quota for
            #[arg(long = "provider", value_enum, default_value_t = AuthProvider::Claude)]
            provider: AuthProvider,
            /// Show detailed quota breakdown
            #[arg(long = "detailed")]
            detailed: bool,
        },
        
        /// Test authentication
        #[command(name = "test")]
        Test {
            /// Provider to test
            #[arg(long = "provider", value_enum, default_value_t = AuthProvider::Auto)]
            provider: AuthProvider,
        },
    }

    /// Main auth command grouping
    #[derive(Debug, Parser)]
    pub struct AuthCommand {
        #[clap(skip)]
        pub config_overrides: CliConfigOverrides,
        
        #[command(subcommand)]
        pub command: AuthCommands,
    }

    /// Execute auth command
    pub async fn execute_auth_command(cmd: AuthCommand) -> ! {
        match cmd.command {
            AuthCommands::Login(login_cmd) => {
                run_extended_login(login_cmd).await
            }
            AuthCommands::Logout(logout_cmd) => {
                run_extended_logout(logout_cmd).await
            }
            AuthCommands::Status { provider, detailed } => {
                let status_cmd = ExtendedLoginCommand {
                    config_overrides: cmd.config_overrides,
                    api_key: None,
                    provider: AuthProvider::Auto,
                    force: false,
                    action: Some(ExtendedLoginSubcommand::Status { provider, detailed }),
                };
                run_extended_login(status_cmd).await
            }
            AuthCommands::Providers { active_only } => {
                let providers_cmd = ExtendedLoginCommand {
                    config_overrides: cmd.config_overrides,
                    api_key: None,
                    provider: AuthProvider::Auto,
                    force: false,
                    action: Some(ExtendedLoginSubcommand::Providers { active_only }),
                };
                run_extended_login(providers_cmd).await
            }
            AuthCommands::Switch { provider, force } => {
                let switch_cmd = ExtendedLoginCommand {
                    config_overrides: cmd.config_overrides,
                    api_key: None,
                    provider: AuthProvider::Auto,
                    force: false,
                    action: Some(ExtendedLoginSubcommand::Switch { provider, force }),
                };
                run_extended_login(switch_cmd).await
            }
            AuthCommands::Quota { provider, detailed } => {
                let quota_cmd = ExtendedLoginCommand {
                    config_overrides: cmd.config_overrides,
                    api_key: None,
                    provider: AuthProvider::Auto,
                    force: false,
                    action: Some(ExtendedLoginSubcommand::Quota { provider, detailed }),
                };
                run_extended_login(quota_cmd).await
            }
            AuthCommands::Test { provider } => {
                let test_cmd = ExtendedLoginCommand {
                    config_overrides: cmd.config_overrides,
                    api_key: None,
                    provider: AuthProvider::Auto,
                    force: false,
                    action: Some(ExtendedLoginSubcommand::Test { provider }),
                };
                run_extended_login(test_cmd).await
            }
        }
    }
}

/// Utilities for backward compatibility
pub mod compat {
    use super::*;
    use codex_common::CliConfigOverrides;

    /// Convert legacy login command to extended login command
    pub fn legacy_to_extended_login(
        config_overrides: CliConfigOverrides,
        api_key: Option<String>,
        action: Option<codex_cli::login::LoginSubcommand>, // This would be the existing subcommand
    ) -> ExtendedLoginCommand {
        let extended_action = action.map(|legacy_action| {
            match legacy_action {
                // This would map existing login subcommands to extended ones
                // codex_cli::login::LoginSubcommand::Status => {
                //     ExtendedLoginSubcommand::Status { 
                //         provider: None, 
                //         detailed: false 
                //     }
                // }
            }
        });

        ExtendedLoginCommand {
            config_overrides,
            api_key,
            provider: AuthProvider::Auto, // Default to auto-selection
            force: false,
            action: extended_action,
        }
    }

    /// Check if extended authentication is available
    pub fn is_extended_auth_available() -> bool {
        // Check if Claude authentication module is available
        std::path::Path::new(&std::env::home_dir().unwrap_or_default())
            .join(".codex")
            .exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_auth_manager_creation() {
        let config_overrides = CliConfigOverrides::default();
        let auth_manager = UnifiedAuthManager::new(config_overrides);
        assert!(auth_manager.is_ok());
    }

    #[tokio::test]
    async fn test_provider_capabilities() {
        let config_overrides = CliConfigOverrides::default();
        let auth_manager = UnifiedAuthManager::new(config_overrides).unwrap();
        
        let capabilities = auth_manager.get_provider_capabilities(false);
        assert!(!capabilities.is_empty());
        
        // Should have at least OpenAI and Claude
        let provider_names: Vec<_> = capabilities.iter()
            .map(|c| c.provider.clone())
            .collect();
        assert!(provider_names.contains(&AuthProvider::OpenAI));
        assert!(provider_names.contains(&AuthProvider::Claude));
    }

    #[test]
    fn test_auth_status_formatting() {
        let status = AuthStatus {
            provider: AuthProvider::Claude,
            authenticated: true,
            user_info: None,
            subscription_info: Some(crate::cli::auth_commands::SubscriptionInfo {
                tier: "max".to_string(),
                features: vec!["unlimited_messages".to_string()],
                active: true,
            }),
            quota_info: Some(QuotaInfo {
                daily_limit: Some(1000000),
                current_usage: Some(50000),
                remaining: Some(950000),
                reset_time: None,
                percentage_used: Some(5.0),
            }),
            last_used: None,
            expires_at: None,
        };

        let formatted = format_auth_status(&[status], true);
        assert!(formatted.contains("✓ Authenticated"));
        assert!(formatted.contains("max"));
        assert!(formatted.contains("5.0%"));
    }
}