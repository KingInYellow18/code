//! Comprehensive integration tests for the configuration management system
//! 
//! Tests the complete configuration system including migration, validation,
//! environment overrides, and integration with existing systems.

use std::fs;
use std::collections::HashMap;
use tempfile::tempdir;
use chrono::{Duration, Utc};

use super::{
    UnifiedConfigManager, ConfigIntegration, ConfigValidator,
    AuthConfig, ProviderType, ProviderPreference, FallbackStrategy,
    UnifiedAuthJson, OpenAIAuthData, ClaudeAuthData,
    MigrationStrategy, ConfigMigrator,
    EnvironmentConfig, EnvironmentOverrides,
    integration_helpers,
};

#[tokio::test]
async fn test_complete_configuration_workflow() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path();
    
    // Step 1: Create configuration manager
    let manager = UnifiedConfigManager::new(codex_home.to_path_buf()).unwrap();
    
    // Step 2: Load default configuration
    let mut config = manager.load_config().await.unwrap();
    assert_eq!(config.auth.preferred_provider, ProviderType::OpenAI);
    assert!(config.auth.enable_fallback);
    
    // Step 3: Modify configuration
    config.auth.preferred_provider = ProviderType::Claude;
    config.auth.provider_preference = ProviderPreference::PreferClaude;
    config.auth.subscription_check_interval = Duration::hours(6);
    
    // Step 4: Save configuration
    manager.save_config(&config).await.unwrap();
    
    // Step 5: Reload and verify persistence
    let reloaded_config = manager.load_config().await.unwrap();
    assert_eq!(reloaded_config.auth.preferred_provider, ProviderType::Claude);
    assert_eq!(reloaded_config.auth.provider_preference, ProviderPreference::PreferClaude);
    assert_eq!(reloaded_config.auth.subscription_check_interval, Duration::hours(6));
}

#[tokio::test]
async fn test_legacy_auth_migration() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path();
    
    // Create legacy auth.json file
    let auth_file = codex_home.join("auth.json");
    let legacy_content = serde_json::json!({
        "OPENAI_API_KEY": "sk-test123456789",
        "tokens": {
            "access_token": "access_test",
            "refresh_token": "refresh_test",
            "account_id": "account_123"
        },
        "last_refresh": "2024-01-01T00:00:00Z"
    });
    
    fs::create_dir_all(codex_home).unwrap();
    fs::write(&auth_file, serde_json::to_string_pretty(&legacy_content).unwrap()).unwrap();
    
    // Create migrator and test migration
    let migrator = ConfigMigrator::new(codex_home).unwrap();
    assert!(migrator.needs_migration().unwrap());
    
    // Perform migration
    let backup = migrator.create_backup().await.unwrap();
    let result = migrator.migrate().await.unwrap();
    
    assert_eq!(result.strategy, MigrationStrategy::LegacyFormat);
    assert_eq!(result.migrated_providers, vec![ProviderType::OpenAI]);
    
    // Verify migrated configuration
    let manager = UnifiedConfigManager::new(codex_home.to_path_buf()).unwrap();
    let config = manager.load_config().await.unwrap();
    
    assert!(config.auth_data.openai_auth.is_some());
    let openai_auth = config.auth_data.openai_auth.unwrap();
    assert_eq!(openai_auth.api_key, Some("sk-test123456789".to_string()));
    assert!(openai_auth.tokens.is_some());
    
    // Test backup restoration
    migrator.restore_backup(backup).await.unwrap();
    let restored_content = fs::read_to_string(&auth_file).unwrap();
    assert_eq!(restored_content, serde_json::to_string_pretty(&legacy_content).unwrap());
}

#[tokio::test]
async fn test_configuration_validation() {
    let temp_dir = tempdir().unwrap();
    let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
    
    // Test valid configuration
    let valid_config = manager.load_config().await.unwrap();
    let validation_result = manager.validator.validate(&valid_config).unwrap();
    assert!(!validation_result.is_valid); // No auth providers configured
    assert!(!validation_result.issues.is_empty());
    
    // Test configuration with auth providers
    let mut config_with_auth = valid_config.clone();
    config_with_auth.auth_data.openai_auth = Some(OpenAIAuthData {
        api_key: Some("sk-test123456789012345678901234567890123456789012345".to_string()),
        tokens: None,
    });
    
    let validation_result = manager.validator.validate(&config_with_auth).unwrap();
    assert!(validation_result.is_valid);
    
    // Test invalid configuration
    let mut invalid_config = config_with_auth.clone();
    invalid_config.auth.auth_timeout = Duration::seconds(0); // Invalid timeout
    
    let validation_result = manager.validator.validate(&invalid_config).unwrap();
    assert!(!validation_result.is_valid);
    assert!(validation_result.issues.iter().any(|issue| issue.contains("timeout")));
}

#[tokio::test]
async fn test_environment_overrides() {
    use std::env;
    
    let temp_dir = tempdir().unwrap();
    
    // Set environment variables
    env::set_var("CODE_AUTH_PREFERRED_PROVIDER", "claude");
    env::set_var("CODE_AUTH_ENABLE_FALLBACK", "false");
    env::set_var("CODE_AUTH_AUTH_TIMEOUT", "60s");
    env::set_var("OPENAI_API_KEY", "sk-env-override-key");
    
    let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
    let config = manager.load_config().await.unwrap();
    
    // Verify environment overrides were applied
    assert_eq!(config.auth.preferred_provider, ProviderType::Claude);
    assert!(!config.auth.enable_fallback);
    assert_eq!(config.auth.auth_timeout, Duration::seconds(60));
    assert!(config.auth_data.openai_auth.is_some());
    assert_eq!(
        config.auth_data.openai_auth.unwrap().api_key,
        Some("sk-env-override-key".to_string())
    );
    
    // Clean up environment variables
    env::remove_var("CODE_AUTH_PREFERRED_PROVIDER");
    env::remove_var("CODE_AUTH_ENABLE_FALLBACK");
    env::remove_var("CODE_AUTH_AUTH_TIMEOUT");
    env::remove_var("OPENAI_API_KEY");
}

#[tokio::test]
async fn test_config_integration_with_existing_system() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path();
    
    // Create existing config.toml
    let config_toml_content = r#"
model = "gpt-4"
approval_policy = "always"

[auth]
preferred_provider = "claude"
enable_fallback = true
subscription_check_interval_minutes = 360
"#;
    
    fs::create_dir_all(codex_home).unwrap();
    fs::write(codex_home.join("config.toml"), config_toml_content).unwrap();
    
    // Test integration
    let integration = ConfigIntegration::new(codex_home.to_path_buf()).unwrap();
    let integrated_config = integration.load_integrated_config().await.unwrap();
    
    assert_eq!(integrated_config.unified.auth.preferred_provider, ProviderType::Claude);
    assert!(integrated_config.unified.auth.enable_fallback);
    assert_eq!(
        integrated_config.unified.auth.subscription_check_interval,
        Duration::hours(6)
    );
    
    // Test provider selection
    let provider_selection = integration.get_provider_for_auth_manager().await.unwrap();
    assert_eq!(provider_selection.preferred_provider, ProviderType::Claude);
    assert!(provider_selection.enable_fallback);
}

#[tokio::test]
async fn test_claude_auth_data_handling() {
    let temp_dir = tempdir().unwrap();
    let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
    
    // Create configuration with Claude auth data
    let mut config = manager.load_config().await.unwrap();
    config.auth_data.claude_auth = Some(ClaudeAuthData {
        api_key: Some("sk-ant-test123456789".to_string()),
        tokens: None,
        subscription: Some(super::unified_storage::ClaudeSubscriptionInfo {
            tier: "max".to_string(),
            usage_limit: Some(1000000),
            usage_current: Some(50000),
            reset_date: Some(Utc::now() + Duration::days(30)),
            features: vec!["api_access".to_string(), "max_tokens".to_string()],
            last_checked: Utc::now(),
        }),
    });
    
    // Save and reload
    manager.save_config(&config).await.unwrap();
    let reloaded_config = manager.load_config().await.unwrap();
    
    assert!(reloaded_config.auth_data.claude_auth.is_some());
    let claude_auth = reloaded_config.auth_data.claude_auth.unwrap();
    assert_eq!(claude_auth.api_key, Some("sk-ant-test123456789".to_string()));
    assert!(claude_auth.subscription.is_some());
    assert_eq!(claude_auth.subscription.unwrap().tier, "max");
}

#[tokio::test]
async fn test_subscription_check_timing() {
    let temp_dir = tempdir().unwrap();
    let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
    
    // Load default config
    let mut config = manager.load_config().await.unwrap();
    
    // Configure Claude with subscription checking
    config.auth.enable_subscription_check = true;
    config.auth.subscription_check_interval = Duration::hours(1);
    config.auth_data.claude_auth = Some(ClaudeAuthData {
        api_key: Some("sk-ant-test".to_string()),
        tokens: None,
        subscription: None,
    });
    
    // Save config
    manager.save_config(&config).await.unwrap();
    
    // Initially should need subscription check
    assert!(manager.needs_subscription_check().unwrap());
    
    // Update subscription check timestamp
    manager.update_subscription_check().await.unwrap();
    
    // Should not need check immediately after update
    assert!(!manager.needs_subscription_check().unwrap());
}

#[tokio::test]
async fn test_provider_fallback_logic() {
    let temp_dir = tempdir().unwrap();
    let integration = ConfigIntegration::new(temp_dir.path().to_path_buf()).unwrap();
    
    // Create configuration with both providers available
    let mut config = integration.config_manager.load_config().await.unwrap();
    config.auth.preferred_provider = ProviderType::Claude;
    config.auth.enable_fallback = true;
    config.auth.fallback_strategy = FallbackStrategy::Automatic;
    
    config.auth_data.openai_auth = Some(OpenAIAuthData {
        api_key: Some("sk-openai-test".to_string()),
        tokens: None,
    });
    config.auth_data.claude_auth = Some(ClaudeAuthData {
        api_key: Some("sk-ant-test".to_string()),
        tokens: None,
        subscription: None,
    });
    
    integration.config_manager.save_config(&config).await.unwrap();
    
    // Test provider selection
    let provider_selection = integration.get_provider_for_auth_manager().await.unwrap();
    assert_eq!(provider_selection.preferred_provider, ProviderType::Claude);
    assert!(provider_selection.openai_available);
    assert!(provider_selection.claude_available);
    
    // Test fallback decision
    let context = super::SelectionContext {
        force_provider: None,
        task_type: None,
        quota_requirements: None,
    };
    
    assert_eq!(provider_selection.select_provider(&context), ProviderType::Claude);
    
    // Test error-based fallback
    let error_context = super::AuthErrorContext {
        error_type: super::auth_config::AuthErrorType::QuotaExhausted,
        provider: ProviderType::Claude,
        retry_count: 0,
    };
    
    assert!(provider_selection.should_fallback(&error_context));
}

#[tokio::test]
async fn test_configuration_backup_and_restore() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path();
    
    // Create initial configuration
    let auth_file = codex_home.join("auth.json");
    let initial_content = serde_json::json!({
        "version": 2,
        "openai_auth": {
            "OPENAI_API_KEY": "sk-initial-key"
        },
        "claude_auth": null,
        "preferred_provider": "openai"
    });
    
    fs::create_dir_all(codex_home).unwrap();
    fs::write(&auth_file, serde_json::to_string_pretty(&initial_content).unwrap()).unwrap();
    
    let migrator = ConfigMigrator::new(codex_home).unwrap();
    
    // Create backup
    let backup = migrator.create_timestamped_backup().unwrap();
    assert!(backup.exists());
    
    // Modify the file
    let modified_content = serde_json::json!({
        "version": 2,
        "openai_auth": {
            "OPENAI_API_KEY": "sk-modified-key"
        },
        "claude_auth": {
            "api_key": "sk-ant-new-key"
        },
        "preferred_provider": "claude"
    });
    
    fs::write(&auth_file, serde_json::to_string_pretty(&modified_content).unwrap()).unwrap();
    
    // Restore from backup
    migrator.restore_from_backup(&backup).unwrap();
    
    // Verify restoration
    let restored_content = fs::read_to_string(&auth_file).unwrap();
    let restored_json: serde_json::Value = serde_json::from_str(&restored_content).unwrap();
    assert_eq!(
        restored_json["openai_auth"]["OPENAI_API_KEY"],
        "sk-initial-key"
    );
    assert_eq!(restored_json["claude_auth"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_integration_helper_functions() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path();
    
    // Test with no Claude auth
    assert!(!integration_helpers::is_claude_auth_available(codex_home).await);
    
    // Test default preferred provider
    let preferred = integration_helpers::get_preferred_provider(codex_home).await.unwrap();
    assert_eq!(preferred, ProviderType::OpenAI);
    
    // Test setting preferred provider
    integration_helpers::set_preferred_provider(codex_home, ProviderType::Claude).await.unwrap();
    let updated_preferred = integration_helpers::get_preferred_provider(codex_home).await.unwrap();
    assert_eq!(updated_preferred, ProviderType::Claude);
    
    // Test subscription check
    let needs_check = integration_helpers::check_subscription_verification_needed(codex_home).await.unwrap();
    assert!(!needs_check); // No Claude auth configured
}

#[tokio::test]
async fn test_environment_validation() {
    use std::env;
    
    // Test valid environment configuration
    env::set_var("CODE_AUTH_AUTH_TIMEOUT", "30s");
    env::set_var("CODE_AUTH_PREFERRED_PROVIDER", "claude");
    env::set_var("OPENAI_API_KEY", "sk-valid-key-format-123456789012345678901234567890");
    
    let env_config = EnvironmentConfig::new();
    assert!(env_config.validate_environment().is_ok());
    
    // Test invalid timeout
    env::set_var("CODE_AUTH_AUTH_TIMEOUT", "0s");
    let env_config = EnvironmentConfig::new();
    assert!(env_config.validate_environment().is_err());
    
    // Test invalid API key format
    env::set_var("CODE_AUTH_AUTH_TIMEOUT", "30s"); // Fix timeout
    env::set_var("OPENAI_API_KEY", "invalid-key-format");
    let env_config = EnvironmentConfig::new();
    assert!(env_config.validate_environment().is_err());
    
    // Clean up
    env::remove_var("CODE_AUTH_AUTH_TIMEOUT");
    env::remove_var("CODE_AUTH_PREFERRED_PROVIDER");
    env::remove_var("OPENAI_API_KEY");
}

#[tokio::test]
async fn test_concurrent_config_access() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path().to_path_buf();
    
    // Create multiple managers accessing the same configuration
    let manager1 = UnifiedConfigManager::new(codex_home.clone()).unwrap();
    let manager2 = UnifiedConfigManager::new(codex_home.clone()).unwrap();
    
    // Test concurrent access
    let handle1 = tokio::spawn(async move {
        let mut config = manager1.load_config().await.unwrap();
        config.auth.preferred_provider = ProviderType::Claude;
        manager1.save_config(&config).await.unwrap();
        "manager1_done"
    });
    
    let handle2 = tokio::spawn(async move {
        let mut config = manager2.load_config().await.unwrap();
        config.auth.enable_fallback = false;
        manager2.save_config(&config).await.unwrap();
        "manager2_done"
    });
    
    let (result1, result2) = tokio::join!(handle1, handle2);
    assert_eq!(result1.unwrap(), "manager1_done");
    assert_eq!(result2.unwrap(), "manager2_done");
    
    // Verify final state
    let final_manager = UnifiedConfigManager::new(codex_home).unwrap();
    let final_config = final_manager.load_config().await.unwrap();
    
    // One of the changes should be preserved (last writer wins)
    assert!(
        final_config.auth.preferred_provider == ProviderType::Claude
        || !final_config.auth.enable_fallback
    );
}

#[tokio::test]
async fn test_storage_format_versioning() {
    let temp_dir = tempdir().unwrap();
    let codex_home = temp_dir.path();
    
    // Create old version format
    let auth_file = codex_home.join("auth.json");
    let old_format = serde_json::json!({
        "version": 1,
        "openai_auth": {
            "OPENAI_API_KEY": "sk-test-key"
        },
        "preferred_provider": "openai"
        // Missing newer fields
    });
    
    fs::create_dir_all(codex_home).unwrap();
    fs::write(&auth_file, serde_json::to_string_pretty(&old_format).unwrap()).unwrap();
    
    // Load with migration
    let manager = UnifiedConfigManager::new(codex_home.to_path_buf()).unwrap();
    let config = manager.load_config().await.unwrap();
    
    // Should load successfully with defaults for missing fields
    assert_eq!(config.auth.preferred_provider, ProviderType::OpenAI);
    assert!(config.auth_data.openai_auth.is_some());
    
    // After saving, should be in current format
    manager.save_config(&config).await.unwrap();
    let saved_content = fs::read_to_string(&auth_file).unwrap();
    let saved_json: serde_json::Value = serde_json::from_str(&saved_content).unwrap();
    assert_eq!(saved_json["version"], 2);
    assert!(saved_json.get("provider_capabilities").is_some());
    assert!(saved_json.get("metadata").is_some());
}

/// Performance test for configuration loading
#[tokio::test]
async fn test_configuration_performance() {
    let temp_dir = tempdir().unwrap();
    let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
    
    // Create a configuration with some data
    let mut config = manager.load_config().await.unwrap();
    config.auth_data.openai_auth = Some(OpenAIAuthData {
        api_key: Some("sk-test".to_string()),
        tokens: None,
    });
    manager.save_config(&config).await.unwrap();
    
    // Measure loading time
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = manager.load_config().await.unwrap();
    }
    let duration = start.elapsed();
    
    // Should be reasonably fast (less than 1 second for 100 loads)
    assert!(duration.as_secs() < 1, "Configuration loading too slow: {:?}", duration);
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_large_configuration_handling() {
        let temp_dir = tempdir().unwrap();
        let manager = UnifiedConfigManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        // Create configuration with large metadata
        let mut config = manager.load_config().await.unwrap();
        
        // Add large provider capabilities cache
        for i in 0..1000 {
            config.auth_data.provider_capabilities.insert(
                format!("provider_{}", i),
                super::unified_storage::ProviderCapabilities {
                    available: true,
                    subscription_active: true,
                    quota_remaining: Some(1000),
                    rate_limit_info: None,
                    last_checked: Utc::now(),
                    expires_at: Utc::now() + Duration::hours(1),
                }
            );
        }
        
        // Should handle large configurations gracefully
        let save_result = manager.save_config(&config).await;
        assert!(save_result.is_ok());
        
        let load_result = manager.load_config().await;
        assert!(load_result.is_ok());
        
        let loaded_config = load_result.unwrap();
        assert_eq!(loaded_config.auth_data.provider_capabilities.len(), 1000);
    }
}