/// # Core Migration Logic
/// 
/// Implements the actual migration from OpenAI-only authentication to unified Claude+OpenAI system.
/// Preserves all existing data while adding Claude authentication capabilities.

use super::{BackupHandle, MigrationConfig, MigrationError, MigrationResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Result of a migration operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    pub success: bool,
    pub migrated_providers: Vec<String>,
    pub preserved_data: Vec<String>,
    pub created_files: Vec<String>,
    pub migration_duration: chrono::Duration,
    pub warnings: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Original OpenAI auth.json structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginalAuthJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

/// Token data structure from original auth
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub id_token: serde_json::Value,
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: Option<String>,
}

/// New unified authentication structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAuthJson {
    #[serde(default = "default_version")]
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub migration_info: MigrationInfo,
    pub providers: HashMap<String, ProviderAuth>,
    pub preferences: AuthPreferences,
}

fn default_version() -> String {
    "2.0".to_string()
}

/// Migration information tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationInfo {
    pub migrated_from_version: String,
    pub migration_date: DateTime<Utc>,
    pub backup_id: String,
    pub migration_tool_version: String,
    pub preserved_data_types: Vec<String>,
}

/// Provider-specific authentication data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderAuth {
    #[serde(rename = "openai")]
    OpenAI {
        api_key: Option<String>,
        oauth_tokens: Option<OpenAITokens>,
        last_refresh: Option<DateTime<Utc>>,
        account_id: Option<String>,
        plan_type: Option<String>,
        enabled: bool,
    },
    #[serde(rename = "claude")]
    Claude {
        api_key: Option<String>,
        oauth_tokens: Option<ClaudeTokens>,
        subscription_tier: Option<String>,
        last_verified: Option<DateTime<Utc>>,
        enabled: bool,
    },
}

/// OpenAI token structure in unified format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITokens {
    pub id_token: serde_json::Value,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Claude token structure for future OAuth support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub subscription_tier: String,
}

/// User authentication preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthPreferences {
    pub preferred_provider: Option<String>,
    pub fallback_enabled: bool,
    pub auto_refresh_tokens: bool,
    pub quota_monitoring_enabled: bool,
    pub provider_selection_strategy: ProviderSelectionStrategy,
}

/// Strategy for selecting authentication provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderSelectionStrategy {
    #[serde(rename = "prefer_claude")]
    PreferClaude,
    #[serde(rename = "prefer_openai")]
    PreferOpenAI,
    #[serde(rename = "cost_optimized")]
    CostOptimized,
    #[serde(rename = "user_choice")]
    UserChoice,
    #[serde(rename = "adaptive")]
    Adaptive,
}

impl Default for AuthPreferences {
    fn default() -> Self {
        Self {
            preferred_provider: None,
            fallback_enabled: true,
            auto_refresh_tokens: true,
            quota_monitoring_enabled: true,
            provider_selection_strategy: ProviderSelectionStrategy::Adaptive,
        }
    }
}

/// Main migration implementation
#[derive(Debug)]
pub struct AuthMigrator {
    codex_home: PathBuf,
    config: MigrationConfig,
}

impl AuthMigrator {
    /// Create a new migrator instance
    pub fn new(codex_home: &Path, config: &MigrationConfig) -> Self {
        Self {
            codex_home: codex_home.to_path_buf(),
            config: config.clone(),
        }
    }

    /// Execute migration to unified format
    pub async fn migrate_to_unified_format(&self, backup_handle: &BackupHandle) -> MigrationResult<MigrationResult> {
        let start_time = Utc::now();
        let mut result = MigrationResult {
            success: false,
            migrated_providers: Vec::new(),
            preserved_data: Vec::new(),
            created_files: Vec::new(),
            migration_duration: chrono::Duration::zero(),
            warnings: Vec::new(),
            metadata: HashMap::new(),
        };

        // Load original auth.json
        let original_auth = match self.load_original_auth().await {
            Ok(auth) => auth,
            Err(e) => {
                result.metadata.insert("error".to_string(), e.to_string());
                return Ok(result);
            }
        };

        // Create unified auth structure
        let unified_auth = self.create_unified_auth(&original_auth, backup_handle).await?;

        // Preserve original auth.json as backup
        self.preserve_original_auth().await?;
        result.preserved_data.push("original_auth.json".to_string());

        // Write unified auth file
        self.write_unified_auth(&unified_auth).await?;
        result.created_files.push("unified_auth.json".to_string());

        // Update main auth.json to point to unified system
        self.update_main_auth_file(&unified_auth).await?;
        result.created_files.push("auth.json".to_string());

        // Create Claude auth placeholder
        self.create_claude_auth_placeholder().await?;
        result.created_files.push("claude_auth.json".to_string());

        // Update migration tracking
        result.success = true;
        result.migrated_providers.push("openai".to_string());
        result.migration_duration = Utc::now() - start_time;
        
        // Add metadata
        result.metadata.insert("migration_version".to_string(), "2.0".to_string());
        result.metadata.insert("backup_id".to_string(), backup_handle.id.clone());
        result.metadata.insert("migration_timestamp".to_string(), start_time.to_rfc3339());

        if self.config.verbose_logging {
            println!("Migration completed successfully in {:?}", result.migration_duration);
        }

        Ok(result)
    }

    /// Load the original auth.json file
    async fn load_original_auth(&self) -> MigrationResult<OriginalAuthJson> {
        let auth_file = self.codex_home.join("auth.json");
        if !auth_file.exists() {
            return Err(MigrationError::ExtensionFailed(
                "Original auth.json file not found".to_string()
            ));
        }

        let content = tokio::fs::read_to_string(&auth_file).await?;
        let auth: OriginalAuthJson = serde_json::from_str(&content)
            .map_err(|e| MigrationError::ExtensionFailed(
                format!("Failed to parse original auth.json: {}", e)
            ))?;

        Ok(auth)
    }

    /// Create unified authentication structure from original data
    async fn create_unified_auth(&self, original: &OriginalAuthJson, backup_handle: &BackupHandle) -> MigrationResult<UnifiedAuthJson> {
        let now = Utc::now();
        let mut providers = HashMap::new();

        // Migrate OpenAI authentication data
        let openai_auth = ProviderAuth::OpenAI {
            api_key: original.openai_api_key.clone(),
            oauth_tokens: original.tokens.as_ref().map(|tokens| OpenAITokens {
                id_token: tokens.id_token.clone(),
                access_token: tokens.access_token.clone(),
                refresh_token: tokens.refresh_token.clone(),
                expires_at: None, // Will be calculated from token data
            }),
            last_refresh: original.last_refresh,
            account_id: original.tokens.as_ref().and_then(|t| t.account_id.clone()),
            plan_type: None, // Will be extracted from token data
            enabled: true,
        };

        providers.insert("openai".to_string(), openai_auth);

        // Create placeholder for Claude authentication
        let claude_auth = ProviderAuth::Claude {
            api_key: None,
            oauth_tokens: None,
            subscription_tier: None,
            last_verified: None,
            enabled: false, // Disabled until user configures it
        };

        providers.insert("claude".to_string(), claude_auth);

        Ok(UnifiedAuthJson {
            version: "2.0".to_string(),
            created_at: now,
            last_updated: now,
            migration_info: MigrationInfo {
                migrated_from_version: "1.0".to_string(),
                migration_date: now,
                backup_id: backup_handle.id.clone(),
                migration_tool_version: env!("CARGO_PKG_VERSION").to_string(),
                preserved_data_types: vec!["openai_tokens".to_string(), "openai_api_key".to_string()],
            },
            providers,
            preferences: AuthPreferences::default(),
        })
    }

    /// Preserve original auth.json as backup
    async fn preserve_original_auth(&self) -> MigrationResult<()> {
        let auth_file = self.codex_home.join("auth.json");
        let backup_file = self.codex_home.join("auth.json.pre_migration");

        if auth_file.exists() {
            tokio::fs::copy(&auth_file, &backup_file).await?;
            
            // Set secure permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = tokio::fs::metadata(&backup_file).await?.permissions();
                perms.set_mode(0o600);
                tokio::fs::set_permissions(&backup_file, perms).await?;
            }
        }

        Ok(())
    }

    /// Write the unified authentication file
    async fn write_unified_auth(&self, unified_auth: &UnifiedAuthJson) -> MigrationResult<()> {
        let unified_file = self.codex_home.join("unified_auth.json");
        let content = serde_json::to_string_pretty(unified_auth)?;
        
        tokio::fs::write(&unified_file, content).await?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&unified_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&unified_file, perms).await?;
        }

        Ok(())
    }

    /// Update main auth.json to point to unified system
    async fn update_main_auth_file(&self, unified_auth: &UnifiedAuthJson) -> MigrationResult<()> {
        let auth_file = self.codex_home.join("auth.json");
        
        // Create a bridge structure that maintains backward compatibility
        let bridge_auth = self.create_bridge_auth(unified_auth).await?;
        let content = serde_json::to_string_pretty(&bridge_auth)?;
        
        tokio::fs::write(&auth_file, content).await?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&auth_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&auth_file, perms).await?;
        }

        Ok(())
    }

    /// Create a bridge auth structure for backward compatibility
    async fn create_bridge_auth(&self, unified_auth: &UnifiedAuthJson) -> MigrationResult<serde_json::Value> {
        // Extract OpenAI data for backward compatibility
        let openai_provider = unified_auth.providers.get("openai")
            .ok_or_else(|| MigrationError::ExtensionFailed("OpenAI provider not found".to_string()))?;

        match openai_provider {
            ProviderAuth::OpenAI { api_key, oauth_tokens, last_refresh, .. } => {
                let mut bridge = serde_json::Map::new();
                
                // Preserve original structure for backward compatibility
                if let Some(api_key) = api_key {
                    bridge.insert("OPENAI_API_KEY".to_string(), serde_json::Value::String(api_key.clone()));
                }

                if let Some(tokens) = oauth_tokens {
                    let token_obj = serde_json::json!({
                        "id_token": tokens.id_token,
                        "access_token": tokens.access_token,
                        "refresh_token": tokens.refresh_token
                    });
                    bridge.insert("tokens".to_string(), token_obj);
                }

                if let Some(last_refresh) = last_refresh {
                    bridge.insert("last_refresh".to_string(), 
                        serde_json::Value::String(last_refresh.to_rfc3339()));
                }

                // Add migration metadata
                bridge.insert("_migration_version".to_string(), 
                    serde_json::Value::String("2.0".to_string()));
                bridge.insert("_unified_auth_available".to_string(), 
                    serde_json::Value::Bool(true));

                Ok(serde_json::Value::Object(bridge))
            }
            _ => Err(MigrationError::ExtensionFailed("Invalid OpenAI provider structure".to_string()))
        }
    }

    /// Create Claude auth placeholder file
    async fn create_claude_auth_placeholder(&self) -> MigrationResult<()> {
        let claude_file = self.codex_home.join("claude_auth.json");
        
        let placeholder = serde_json::json!({
            "version": "2.0",
            "enabled": false,
            "setup_required": true,
            "setup_instructions": [
                "Run 'code auth login --provider claude' to set up Claude authentication",
                "Or add your Claude API key with 'code auth add-key --provider claude --key <your-key>'"
            ],
            "created_at": Utc::now().to_rfc3339()
        });

        let content = serde_json::to_string_pretty(&placeholder)?;
        tokio::fs::write(&claude_file, content).await?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&claude_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&claude_file, perms).await?;
        }

        Ok(())
    }

    /// Validate migration by checking data integrity
    pub async fn validate_migration(&self) -> MigrationResult<ValidationResult> {
        let mut result = ValidationResult {
            is_valid: true,
            preserved_openai_auth: false,
            unified_auth_created: false,
            backward_compatibility: false,
            claude_placeholder_created: false,
            errors: Vec::new(),
        };

        // Check if unified auth file was created
        let unified_file = self.codex_home.join("unified_auth.json");
        result.unified_auth_created = unified_file.exists();
        if !result.unified_auth_created {
            result.errors.push("Unified auth file not created".to_string());
            result.is_valid = false;
        }

        // Check backward compatibility
        let auth_file = self.codex_home.join("auth.json");
        if auth_file.exists() {
            match self.validate_backward_compatibility().await {
                Ok(compat) => result.backward_compatibility = compat,
                Err(e) => {
                    result.errors.push(format!("Backward compatibility check failed: {}", e));
                    result.is_valid = false;
                }
            }
        }

        // Check if OpenAI data was preserved
        if result.unified_auth_created {
            match self.validate_openai_preservation().await {
                Ok(preserved) => result.preserved_openai_auth = preserved,
                Err(e) => {
                    result.errors.push(format!("OpenAI data preservation check failed: {}", e));
                    result.is_valid = false;
                }
            }
        }

        // Check Claude placeholder
        let claude_file = self.codex_home.join("claude_auth.json");
        result.claude_placeholder_created = claude_file.exists();

        Ok(result)
    }

    /// Validate backward compatibility with existing systems
    async fn validate_backward_compatibility(&self) -> MigrationResult<bool> {
        let auth_file = self.codex_home.join("auth.json");
        let content = tokio::fs::read_to_string(&auth_file).await?;
        let auth_data: serde_json::Value = serde_json::from_str(&content)?;

        // Check if original fields are preserved
        let has_openai_key = auth_data.get("OPENAI_API_KEY").is_some();
        let has_tokens = auth_data.get("tokens").is_some();
        let has_migration_marker = auth_data.get("_migration_version").is_some();

        Ok((has_openai_key || has_tokens) && has_migration_marker)
    }

    /// Validate that OpenAI authentication data was preserved
    async fn validate_openai_preservation(&self) -> MigrationResult<bool> {
        let unified_file = self.codex_home.join("unified_auth.json");
        let content = tokio::fs::read_to_string(&unified_file).await?;
        let unified_auth: UnifiedAuthJson = serde_json::from_str(&content)?;

        if let Some(ProviderAuth::OpenAI { api_key, oauth_tokens, .. }) = unified_auth.providers.get("openai") {
            Ok(api_key.is_some() || oauth_tokens.is_some())
        } else {
            Ok(false)
        }
    }

    /// Rollback migration if needed
    pub async fn rollback_migration(&self, backup_handle: &BackupHandle) -> MigrationResult<()> {
        if self.config.verbose_logging {
            println!("Rolling back migration using backup: {}", backup_handle.id);
        }

        // Restore original auth.json from backup
        let backup_manager = super::BackupManager::new(&self.codex_home, &self.config);
        backup_manager.restore_from_backup(backup_handle).await
            .map_err(|e| MigrationError::RollbackFailed(format!("Failed to restore backup: {}", e)))?;

        // Remove migration artifacts
        let files_to_remove = [
            "unified_auth.json",
            "claude_auth.json",
            "auth.json.pre_migration",
        ];

        for file in &files_to_remove {
            let file_path = self.codex_home.join(file);
            if file_path.exists() {
                tokio::fs::remove_file(&file_path).await
                    .map_err(|e| MigrationError::RollbackFailed(
                        format!("Failed to remove {}: {}", file, e)
                    ))?;
            }
        }

        if self.config.verbose_logging {
            println!("Migration rollback completed successfully");
        }

        Ok(())
    }
}

/// Validation result for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub preserved_openai_auth: bool,
    pub unified_auth_created: bool,
    pub backward_compatibility: bool,
    pub claude_placeholder_created: bool,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_migration_with_api_key_only() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let migrator = AuthMigrator::new(temp_dir.path(), &config);

        // Create original auth.json with API key only
        let auth_file = temp_dir.path().join("auth.json");
        let original_auth = OriginalAuthJson {
            openai_api_key: Some("sk-test-key".to_string()),
            tokens: None,
            last_refresh: None,
        };
        let content = serde_json::to_string_pretty(&original_auth).unwrap();
        tokio::fs::write(&auth_file, content).await.unwrap();

        // Create a mock backup handle
        let backup_handle = BackupHandle {
            id: "test-backup".to_string(),
            created_at: Utc::now(),
            file_path: temp_dir.path().join("backup.json"),
            metadata: super::super::backup_manager::BackupMetadata {
                original_file_size: 100,
                auth_mode: "ApiKey".to_string(),
                has_tokens: false,
                has_api_key: true,
                backup_version: "1.0".to_string(),
                system_info: HashMap::new(),
            },
            encrypted: false,
            checksum: "test-checksum".to_string(),
        };

        // Execute migration
        let result = migrator.migrate_to_unified_format(&backup_handle).await.unwrap();
        assert!(result.success);
        assert!(result.migrated_providers.contains(&"openai".to_string()));

        // Validate migration files were created
        assert!(temp_dir.path().join("unified_auth.json").exists());
        assert!(temp_dir.path().join("claude_auth.json").exists());
        assert!(temp_dir.path().join("auth.json.pre_migration").exists());

        // Validate unified auth content
        let unified_content = tokio::fs::read_to_string(temp_dir.path().join("unified_auth.json")).await.unwrap();
        let unified_auth: UnifiedAuthJson = serde_json::from_str(&unified_content).unwrap();
        
        assert_eq!(unified_auth.version, "2.0");
        assert!(unified_auth.providers.contains_key("openai"));
        assert!(unified_auth.providers.contains_key("claude"));

        if let Some(ProviderAuth::OpenAI { api_key, .. }) = unified_auth.providers.get("openai") {
            assert_eq!(api_key.as_ref().unwrap(), "sk-test-key");
        } else {
            panic!("OpenAI provider not found or invalid");
        }
    }

    #[tokio::test]
    async fn test_migration_with_oauth_tokens() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let migrator = AuthMigrator::new(temp_dir.path(), &config);

        // Create original auth.json with OAuth tokens
        let auth_file = temp_dir.path().join("auth.json");
        let original_auth = OriginalAuthJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: serde_json::json!({"sub": "user123"}),
                access_token: "access-token".to_string(),
                refresh_token: "refresh-token".to_string(),
                account_id: Some("account123".to_string()),
            }),
            last_refresh: Some(Utc::now()),
        };
        let content = serde_json::to_string_pretty(&original_auth).unwrap();
        tokio::fs::write(&auth_file, content).await.unwrap();

        // Create mock backup handle
        let backup_handle = BackupHandle {
            id: "test-backup-oauth".to_string(),
            created_at: Utc::now(),
            file_path: temp_dir.path().join("backup.json"),
            metadata: super::super::backup_manager::BackupMetadata {
                original_file_size: 200,
                auth_mode: "ChatGPT".to_string(),
                has_tokens: true,
                has_api_key: false,
                backup_version: "1.0".to_string(),
                system_info: HashMap::new(),
            },
            encrypted: false,
            checksum: "test-checksum-oauth".to_string(),
        };

        // Execute migration
        let result = migrator.migrate_to_unified_format(&backup_handle).await.unwrap();
        assert!(result.success);

        // Validate OAuth tokens were preserved
        let unified_content = tokio::fs::read_to_string(temp_dir.path().join("unified_auth.json")).await.unwrap();
        let unified_auth: UnifiedAuthJson = serde_json::from_str(&unified_content).unwrap();

        if let Some(ProviderAuth::OpenAI { oauth_tokens, account_id, .. }) = unified_auth.providers.get("openai") {
            assert!(oauth_tokens.is_some());
            assert_eq!(account_id.as_ref().unwrap(), "account123");
            
            let tokens = oauth_tokens.as_ref().unwrap();
            assert_eq!(tokens.access_token, "access-token");
            assert_eq!(tokens.refresh_token, "refresh-token");
        } else {
            panic!("OpenAI provider tokens not preserved");
        }
    }

    #[tokio::test]
    async fn test_backward_compatibility() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let migrator = AuthMigrator::new(temp_dir.path(), &config);

        // Create and migrate auth
        let auth_file = temp_dir.path().join("auth.json");
        let original_auth = OriginalAuthJson {
            openai_api_key: Some("sk-test".to_string()),
            tokens: None,
            last_refresh: None,
        };
        let content = serde_json::to_string_pretty(&original_auth).unwrap();
        tokio::fs::write(&auth_file, content).await.unwrap();

        let backup_handle = BackupHandle {
            id: "test-compat".to_string(),
            created_at: Utc::now(),
            file_path: temp_dir.path().join("backup.json"),
            metadata: super::super::backup_manager::BackupMetadata {
                original_file_size: 50,
                auth_mode: "ApiKey".to_string(),
                has_tokens: false,
                has_api_key: true,
                backup_version: "1.0".to_string(),
                system_info: HashMap::new(),
            },
            encrypted: false,
            checksum: "test-compat-checksum".to_string(),
        };

        migrator.migrate_to_unified_format(&backup_handle).await.unwrap();

        // Test backward compatibility
        let validation = migrator.validate_migration().await.unwrap();
        assert!(validation.is_valid);
        assert!(validation.backward_compatibility);
        assert!(validation.preserved_openai_auth);

        // Verify original structure is still accessible
        let auth_content = tokio::fs::read_to_string(&auth_file).await.unwrap();
        let auth_data: serde_json::Value = serde_json::from_str(&auth_content).unwrap();
        
        assert!(auth_data.get("OPENAI_API_KEY").is_some());
        assert!(auth_data.get("_migration_version").is_some());
    }
}