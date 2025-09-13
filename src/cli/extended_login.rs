//! Extended login functionality with multi-provider support
//! 
//! This module provides the CLI command handlers for the extended authentication
//! system that supports both OpenAI and Claude providers.

use crate::cli::auth_commands::{
    ExtendedLoginCommand, ExtendedLoginSubcommand, AuthProvider, 
    UnifiedAuthManager, format_auth_status, format_provider_capabilities, format_quota_info
};
use codex_common::CliConfigOverrides;

/// Run extended login command with provider support
pub async fn run_extended_login(mut cmd: ExtendedLoginCommand) -> ! {
    let result = execute_extended_login(&mut cmd).await;
    
    match result {
        Ok(()) => {
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Authentication error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Execute extended login command logic
async fn execute_extended_login(cmd: &mut ExtendedLoginCommand) -> Result<(), Box<dyn std::error::Error>> {
    let mut auth_manager = UnifiedAuthManager::new(cmd.config_overrides.clone())?;

    match &cmd.action {
        Some(ExtendedLoginSubcommand::Status { provider, detailed }) => {
            handle_status_command(&auth_manager, provider.clone(), *detailed).await
        }
        Some(ExtendedLoginSubcommand::Providers { active_only }) => {
            handle_providers_command(&auth_manager, *active_only).await
        }
        Some(ExtendedLoginSubcommand::Switch { provider, force }) => {
            handle_switch_command(&mut auth_manager, provider.clone(), *force).await
        }
        Some(ExtendedLoginSubcommand::Quota { provider, detailed }) => {
            handle_quota_command(&auth_manager, provider.clone(), *detailed).await
        }
        Some(ExtendedLoginSubcommand::Test { provider }) => {
            handle_test_command(&auth_manager, provider.clone()).await
        }
        None => {
            // Main login flow
            handle_login_command(&mut auth_manager, cmd).await
        }
    }
}

/// Handle status subcommand
async fn handle_status_command(
    auth_manager: &UnifiedAuthManager, 
    provider: Option<AuthProvider>, 
    detailed: bool
) -> Result<(), Box<dyn std::error::Error>> {
    let statuses = auth_manager.get_auth_status(provider).await?;
    let output = format_auth_status(&statuses, detailed);
    println!("{}", output);
    Ok(())
}

/// Handle providers subcommand
async fn handle_providers_command(
    auth_manager: &UnifiedAuthManager, 
    active_only: bool
) -> Result<(), Box<dyn std::error::Error>> {
    let capabilities = auth_manager.get_provider_capabilities(active_only);
    let output = format_provider_capabilities(&capabilities);
    println!("{}", output);
    Ok(())
}

/// Handle switch subcommand
async fn handle_switch_command(
    auth_manager: &mut UnifiedAuthManager, 
    provider: AuthProvider, 
    force: bool
) -> Result<(), Box<dyn std::error::Error>> {
    auth_manager.switch_provider(provider.clone(), force).await?;
    println!("Successfully switched to {} provider", provider);
    Ok(())
}

/// Handle quota subcommand
async fn handle_quota_command(
    auth_manager: &UnifiedAuthManager, 
    provider: AuthProvider, 
    detailed: bool
) -> Result<(), Box<dyn std::error::Error>> {
    match provider {
        AuthProvider::Claude => {
            if let Some(quota) = auth_manager.get_claude_quota(detailed).await? {
                let output = format_quota_info(&quota, provider);
                println!("{}", output);
            } else {
                println!("No quota information available for Claude provider.");
                println!("Make sure you're authenticated with Claude Max subscription.");
            }
        }
        AuthProvider::OpenAI => {
            println!("Quota management is not available for OpenAI provider.");
            println!("OpenAI uses token-based billing rather than subscription quotas.");
        }
        AuthProvider::Auto => {
            // Try Claude first, then fall back to explaining limitations
            if let Some(quota) = auth_manager.get_claude_quota(detailed).await? {
                let output = format_quota_info(&quota, AuthProvider::Claude);
                println!("{}", output);
            } else {
                println!("No quota information available.");
                println!("Claude provider: Not authenticated or no Max subscription");
                println!("OpenAI provider: Uses token-based billing");
            }
        }
    }
    Ok(())
}

/// Handle test subcommand
async fn handle_test_command(
    auth_manager: &UnifiedAuthManager, 
    provider: AuthProvider
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing authentication for {} provider...", provider);
    
    let result = auth_manager.test_authentication(provider.clone()).await?;
    
    if result {
        println!("✓ {} provider authentication test successful", provider);
    } else {
        println!("✗ {} provider authentication test failed", provider);
        
        match provider {
            AuthProvider::OpenAI => {
                println!("Try: code auth login --provider openai");
            }
            AuthProvider::Claude => {
                println!("Try: code auth login --provider claude");
            }
            AuthProvider::Auto => {
                println!("Try authenticating with a specific provider first:");
                println!("  code auth login --provider openai");
                println!("  code auth login --provider claude");
            }
        }
    }
    
    Ok(())
}

/// Handle main login command
async fn handle_login_command(
    auth_manager: &mut UnifiedAuthManager, 
    cmd: &ExtendedLoginCommand
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd.provider {
        AuthProvider::OpenAI => {
            handle_openai_login(auth_manager, cmd).await
        }
        AuthProvider::Claude => {
            handle_claude_login(auth_manager, cmd).await
        }
        AuthProvider::Auto => {
            handle_auto_login(auth_manager, cmd).await
        }
    }
}

/// Handle OpenAI login
async fn handle_openai_login(
    _auth_manager: &mut UnifiedAuthManager,
    cmd: &ExtendedLoginCommand
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting OpenAI authentication...");
    
    if let Some(ref api_key) = cmd.api_key {
        // Use existing OpenAI API key login logic
        println!("Using API key authentication for OpenAI");
        
        // Call existing login function
        let config = load_config_or_exit(cmd.config_overrides.clone());
        match codex_cli::login::login_with_api_key(&config.codex_home, api_key) {
            Ok(_) => {
                println!("✓ Successfully authenticated with OpenAI using API key");
                Ok(())
            }
            Err(e) => {
                Err(format!("OpenAI API key authentication failed: {}", e).into())
            }
        }
    } else {
        // Use existing ChatGPT OAuth login logic
        println!("Using ChatGPT OAuth authentication for OpenAI");
        
        let config = load_config_or_exit(cmd.config_overrides.clone());
        match codex_cli::login::login_with_chatgpt(
            config.codex_home,
            config.responses_originator_header.clone(),
        ).await {
            Ok(_) => {
                println!("✓ Successfully authenticated with OpenAI using ChatGPT OAuth");
                Ok(())
            }
            Err(e) => {
                Err(format!("OpenAI ChatGPT OAuth authentication failed: {}", e).into())
            }
        }
    }
}

/// Handle Claude login
async fn handle_claude_login(
    auth_manager: &mut UnifiedAuthManager,
    cmd: &ExtendedLoginCommand
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Claude authentication...");
    
    auth_manager.authenticate_claude(cmd.api_key.clone(), cmd.force).await?;
    println!("✓ Successfully authenticated with Claude");
    Ok(())
}

/// Handle automatic provider selection login
async fn handle_auto_login(
    auth_manager: &mut UnifiedAuthManager,
    cmd: &ExtendedLoginCommand
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Auto-selecting best provider for authentication...");
    
    // Strategy: Try Claude first (better for development), fall back to OpenAI
    let claude_result = handle_claude_login(auth_manager, cmd).await;
    
    match claude_result {
        Ok(()) => {
            println!("✓ Successfully authenticated with Claude (auto-selected)");
            Ok(())
        }
        Err(claude_error) => {
            println!("Claude authentication failed, trying OpenAI...");
            eprintln!("Claude error: {}", claude_error);
            
            let openai_result = handle_openai_login(auth_manager, cmd).await;
            
            match openai_result {
                Ok(()) => {
                    println!("✓ Successfully authenticated with OpenAI (fallback)");
                    Ok(())
                }
                Err(openai_error) => {
                    eprintln!("OpenAI error: {}", openai_error);
                    Err("Both Claude and OpenAI authentication failed".into())
                }
            }
        }
    }
}

/// Load configuration (using existing logic)
fn load_config_or_exit(cli_config_overrides: CliConfigOverrides) -> codex_core::config::Config {
    let cli_overrides = match cli_config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {}", e);
            std::process::exit(1);
        }
    };

    let config_overrides = codex_core::config::ConfigOverrides::default();
    match codex_core::config::Config::load_with_cli_overrides(cli_overrides, config_overrides) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            std::process::exit(1);
        }
    }
}

/// Extended logout command with provider support
#[derive(Debug, clap::Parser)]
pub struct ExtendedLogoutCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Specific provider to logout from
    #[arg(long = "provider", value_enum)]
    pub provider: Option<AuthProvider>,

    /// Logout from all providers
    #[arg(long = "all")]
    pub all: bool,
}

/// Run extended logout command
pub async fn run_extended_logout(cmd: ExtendedLogoutCommand) -> ! {
    let result = execute_extended_logout(&cmd).await;
    
    match result {
        Ok(()) => {
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Logout error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Execute extended logout command logic
async fn execute_extended_logout(cmd: &ExtendedLogoutCommand) -> Result<(), Box<dyn std::error::Error>> {
    let mut auth_manager = UnifiedAuthManager::new(cmd.config_overrides.clone())?;

    match (&cmd.provider, cmd.all) {
        (Some(AuthProvider::OpenAI), false) => {
            logout_openai(&cmd.config_overrides)?;
            println!("✓ Logged out from OpenAI provider");
        }
        (Some(AuthProvider::Claude), false) => {
            logout_claude(&mut auth_manager)?;
            println!("✓ Logged out from Claude provider");
        }
        (Some(AuthProvider::Auto), false) | (None, true) | (None, false) => {
            // Logout from all providers
            let mut success_count = 0;
            let mut error_count = 0;

            // Logout from OpenAI
            match logout_openai(&cmd.config_overrides) {
                Ok(()) => {
                    println!("✓ Logged out from OpenAI provider");
                    success_count += 1;
                }
                Err(e) => {
                    eprintln!("Failed to logout from OpenAI: {}", e);
                    error_count += 1;
                }
            }

            // Logout from Claude
            match logout_claude(&mut auth_manager) {
                Ok(()) => {
                    println!("✓ Logged out from Claude provider");
                    success_count += 1;
                }
                Err(e) => {
                    eprintln!("Failed to logout from Claude: {}", e);
                    error_count += 1;
                }
            }

            if success_count > 0 {
                println!("Logged out from {} provider(s)", success_count);
            }
            if error_count > 0 {
                eprintln!("Failed to logout from {} provider(s)", error_count);
            }
        }
        (Some(provider), true) => {
            return Err(format!("Cannot specify both --provider {} and --all", provider).into());
        }
    }

    Ok(())
}

/// Logout from OpenAI provider
fn logout_openai(config_overrides: &CliConfigOverrides) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config_or_exit(config_overrides.clone());
    
    match codex_cli::login::logout(&config.codex_home) {
        Ok(true) => Ok(()),
        Ok(false) => Err("Not logged in to OpenAI".into()),
        Err(e) => Err(format!("OpenAI logout failed: {}", e).into()),
    }
}

/// Logout from Claude provider
fn logout_claude(auth_manager: &mut UnifiedAuthManager) -> Result<(), Box<dyn std::error::Error>> {
    // Implementation would call claude_auth.logout()
    // For now, just remove the token file
    let token_path = std::env::home_dir()
        .unwrap_or_default()
        .join(".codex")
        .join("claude_tokens.json");
    
    if token_path.exists() {
        std::fs::remove_file(&token_path)?;
        Ok(())
    } else {
        Err("Not logged in to Claude".into())
    }
}