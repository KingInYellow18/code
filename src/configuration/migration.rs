//! Configuration migration utilities
//! 
//! Handles migration from existing auth.json formats to the unified configuration
//! system while preserving user data and maintaining backward compatibility.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;

use super::unified_storage::{UnifiedAuthJson, OpenAIAuthData, OpenAITokenData, StorageError};
use super::auth_config::ProviderType;

/// Configuration migrator for handling legacy auth.json formats
#[derive(Debug, Clone)]
pub struct ConfigMigrator {
    codex_home: PathBuf,
    backup_dir: PathBuf,
    migration_log: PathBuf,
}

impl ConfigMigrator {
    /// Create new configuration migrator
    pub fn new(codex_home: &Path) -> Result<Self, MigrationError> {
        let backup_dir = codex_home.join("backups");
        let migration_log = codex_home.join("migration.log");
        
        // Ensure backup directory exists
        fs::create_dir_all(&backup_dir)?;
        
        Ok(Self {
            codex_home: codex_home.to_path_buf(),
            backup_dir,
            migration_log,
        })
    }

    /// Check if migration is needed
    pub fn needs_migration(&self) -> Result<bool, MigrationError> {
        let auth_file = self.codex_home.join("auth.json");
        
        if !auth_file.exists() {
            return Ok(false);
        }

        // Read the file and check format
        let content = fs::read_to_string(&auth_file)?;
        
        // Try parsing as current unified format
        if serde_json::from_str::<UnifiedAuthJson>(&content).is_ok() {
            return Ok(false);
        }

        // Try parsing as legacy format
        if serde_json::from_str::<LegacyAuthJson>(&content).is_ok() {
            return Ok(true);
        }

        // Unknown format - might need manual intervention
        Err(MigrationError::UnknownFormat)
    }

    /// Create a backup before migration
    pub async fn create_backup(&self) -> Result<BackupHandle, MigrationError> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_id = format!("migration_{}", timestamp);
        let backup_path = self.backup_dir.join(format!("{}.json", backup_id));
        
        let auth_file = self.codex_home.join("auth.json");
        if auth_file.exists() {
            fs::copy(&auth_file, &backup_path)?;
        }

        // Also backup config.toml if it exists
        let config_file = self.codex_home.join("config.toml");
        if config_file.exists() {
            let config_backup_path = self.backup_dir.join(format!("{}_config.toml", backup_id));
            fs::copy(&config_file, &config_backup_path)?;
        }

        let backup_handle = BackupHandle {
            id: backup_id,
            auth_backup_path: backup_path,
            config_backup_path: config_file.exists().then(|| {
                self.backup_dir.join(format!("{}_config.toml", timestamp))
            }),
            created_at: Utc::now(),
        };

        self.log_migration_event(&format!("Created backup: {}", backup_handle.id))?;

        Ok(backup_handle)
    }

    /// Perform the migration
    pub async fn migrate(&self) -> Result<MigrationResult, MigrationError> {
        let auth_file = self.codex_home.join("auth.json");
        
        if !auth_file.exists() {
            return Ok(MigrationResult {
                strategy: MigrationStrategy::NoMigrationNeeded,
                migrated_providers: Vec::new(),
                warnings: Vec::new(),
            });
        }

        let content = fs::read_to_string(&auth_file)?;
        
        // Determine migration strategy
        let strategy = self.determine_migration_strategy(&content)?;
        let mut warnings = Vec::new();
        let mut migrated_providers = Vec::new();

        let unified_auth = match strategy {
            MigrationStrategy::LegacyFormat => {
                self.migrate_legacy_format(&content, &mut warnings, &mut migrated_providers)?
            }
            MigrationStrategy::PartialUnified => {
                self.migrate_partial_unified(&content, &mut warnings, &mut migrated_providers)?
            }
            MigrationStrategy::CustomFormat => {
                self.migrate_custom_format(&content, &mut warnings, &mut migrated_providers)?
            }
            MigrationStrategy::NoMigrationNeeded => {
                return Ok(MigrationResult {
                    strategy,
                    migrated_providers,
                    warnings,
                });
            }
        };

        // Save the migrated configuration
        let storage = super::unified_storage::UnifiedAuthStorage::new(&self.codex_home)?;
        storage.save(&unified_auth).map_err(|e| MigrationError::StorageError(e))?;

        self.log_migration_event(&format!("Migration completed using strategy: {:?}", strategy))?;

        Ok(MigrationResult {
            strategy,
            migrated_providers,
            warnings,
        })
    }

    /// Restore from backup
    pub async fn restore_backup(&self, backup: BackupHandle) -> Result<(), MigrationError> {
        let auth_file = self.codex_home.join("auth.json");
        
        // Restore auth.json
        if backup.auth_backup_path.exists() {
            fs::copy(&backup.auth_backup_path, &auth_file)?;
        }

        // Restore config.toml if it was backed up
        if let Some(config_backup_path) = &backup.config_backup_path {
            if config_backup_path.exists() {
                let config_file = self.codex_home.join("config.toml");
                fs::copy(config_backup_path, &config_file)?;
            }
        }

        self.log_migration_event(&format!("Restored backup: {}", backup.id))?;

        Ok(())
    }

    /// Get migration history
    pub fn get_migration_history(&self) -> Result<Vec<MigrationLogEntry>, MigrationError> {
        if !self.migration_log.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.migration_log)?;
        let mut entries = Vec::new();

        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<MigrationLogEntry>(line) {
                entries.push(entry);
            }
        }

        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(entries)
    }

    /// Clean up old backups (keep last 5)
    pub fn cleanup_old_backups(&self) -> Result<usize, MigrationError> {
        let mut backups = fs::read_dir(&self.backup_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with("migration_")
                    && entry.file_name().to_string_lossy().ends_with(".json")
            })
            .collect::<Vec<_>>();

        // Sort by creation time (newest first)
        backups.sort_by(|a, b| {
            b.metadata().unwrap().created().unwrap()
                .cmp(&a.metadata().unwrap().created().unwrap())
        });

        // Keep the 5 most recent backups
        let mut removed_count = 0;
        for backup in backups.into_iter().skip(5) {
            fs::remove_file(backup.path())?;
            removed_count += 1;
        }

        Ok(removed_count)
    }

    // Private helper methods
    fn determine_migration_strategy(&self, content: &str) -> Result<MigrationStrategy, MigrationError> {
        // Try parsing as unified format first
        if serde_json::from_str::<UnifiedAuthJson>(content).is_ok() {
            return Ok(MigrationStrategy::NoMigrationNeeded);
        }

        // Try legacy format
        if serde_json::from_str::<LegacyAuthJson>(content).is_ok() {
            return Ok(MigrationStrategy::LegacyFormat);
        }

        // Try partial unified (might have some new fields but missing others)
        if serde_json::from_str::<PartialUnifiedAuthJson>(content).is_ok() {
            return Ok(MigrationStrategy::PartialUnified);
        }

        // Custom or unknown format
        Ok(MigrationStrategy::CustomFormat)
    }

    fn migrate_legacy_format(
        &self,
        content: &str,
        warnings: &mut Vec<String>,
        migrated_providers: &mut Vec<ProviderType>,
    ) -> Result<UnifiedAuthJson, MigrationError> {
        let legacy: LegacyAuthJson = serde_json::from_str(content)?;
        
        let openai_auth = if legacy.openai_api_key.is_some() || legacy.tokens.is_some() {
            migrated_providers.push(ProviderType::OpenAI);
            Some(OpenAIAuthData {
                api_key: legacy.openai_api_key,
                tokens: legacy.tokens,
            })
        } else {
            None
        };

        // Check for deprecated fields
        if legacy.last_refresh.is_some() {
            warnings.push("last_refresh field is deprecated and has been moved to token metadata".to_string());
        }

        Ok(UnifiedAuthJson {
            version: 2,
            openai_auth,
            claude_auth: None,
            preferred_provider: ProviderType::OpenAI,
            last_provider_check: None,
            last_subscription_check: None,
            provider_capabilities: HashMap::new(),
            metadata: super::unified_storage::AuthMetadata {
                created_at: Utc::now(),
                updated_at: Utc::now(),
                migration_source: Some("legacy_auth_json".to_string()),
            },
        })
    }

    fn migrate_partial_unified(
        &self,
        content: &str,
        warnings: &mut Vec<String>,
        migrated_providers: &mut Vec<ProviderType>,
    ) -> Result<UnifiedAuthJson, MigrationError> {
        let partial: PartialUnifiedAuthJson = serde_json::from_str(content)?;
        
        if partial.openai_auth.is_some() {
            migrated_providers.push(ProviderType::OpenAI);
        }
        if partial.claude_auth.is_some() {
            migrated_providers.push(ProviderType::Claude);
        }

        Ok(UnifiedAuthJson {
            version: 2,
            openai_auth: partial.openai_auth,
            claude_auth: partial.claude_auth,
            preferred_provider: partial.preferred_provider.unwrap_or(ProviderType::OpenAI),
            last_provider_check: partial.last_provider_check,
            last_subscription_check: None, // New field
            provider_capabilities: HashMap::new(), // New field
            metadata: super::unified_storage::AuthMetadata {
                created_at: Utc::now(),
                updated_at: Utc::now(),
                migration_source: Some("partial_unified_format".to_string()),
            },
        })
    }

    fn migrate_custom_format(
        &self,
        content: &str,
        warnings: &mut Vec<String>,
        _migrated_providers: &mut Vec<ProviderType>,
    ) -> Result<UnifiedAuthJson, MigrationError> {
        warnings.push("Unknown auth.json format detected. Creating minimal configuration.".to_string());
        warnings.push("Please reconfigure authentication providers manually.".to_string());
        
        // Try to extract any recognizable OpenAI API key
        if content.contains("OPENAI_API_KEY") || content.contains("sk-") {
            warnings.push("Detected possible OpenAI API key - manual verification required.".to_string());
        }

        Ok(UnifiedAuthJson::default())
    }

    fn log_migration_event(&self, message: &str) -> Result<(), MigrationError> {
        let entry = MigrationLogEntry {
            timestamp: Utc::now(),
            message: message.to_string(),
        };

        let log_line = serde_json::to_string(&entry)?;
        let log_content = if self.migration_log.exists() {
            let existing = fs::read_to_string(&self.migration_log)?;
            format!("{}\n{}", existing, log_line)
        } else {
            log_line
        };

        fs::write(&self.migration_log, log_content)?;
        Ok(())
    }
}

/// Migration strategies
#[derive(Debug, Clone, PartialEq)]
pub enum MigrationStrategy {
    /// No migration needed - already in current format
    NoMigrationNeeded,
    /// Migrate from legacy auth.json format
    LegacyFormat,
    /// Migrate from partially unified format (missing some new fields)
    PartialUnified,
    /// Custom or unknown format - best effort migration
    CustomFormat,
}

/// Migration result information
#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub strategy: MigrationStrategy,
    pub migrated_providers: Vec<ProviderType>,
    pub warnings: Vec<String>,
}

/// Backup handle for migration rollback
#[derive(Debug, Clone)]
pub struct BackupHandle {
    pub id: String,
    pub auth_backup_path: PathBuf,
    pub config_backup_path: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
}

/// Migration log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationLogEntry {
    timestamp: DateTime<Utc>,
    message: String,
}

/// Legacy auth.json format (existing Code project format)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyAuthJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<OpenAITokenData>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

/// Partial unified format (intermediate migration state)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PartialUnifiedAuthJson {
    pub version: Option<u32>,
    pub openai_auth: Option<OpenAIAuthData>,
    pub claude_auth: Option<super::unified_storage::ClaudeAuthData>,
    pub preferred_provider: Option<ProviderType>,
    pub last_provider_check: Option<DateTime<Utc>>,
    // Missing newer fields like last_subscription_check, provider_capabilities, metadata
}

/// Migration error types
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Unknown auth.json format")]
    UnknownFormat,
    
    #[error("Migration failed: {0}")]
    MigrationFailed(String),
    
    #[error("Backup operation failed: {0}")]
    BackupFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_migration_strategy_detection() {
        let temp_dir = tempdir().unwrap();
        let migrator = ConfigMigrator::new(temp_dir.path()).unwrap();

        // Test legacy format
        let legacy_content = r#"{
            "OPENAI_API_KEY": "sk-test",
            "tokens": null,
            "last_refresh": null
        }"#;
        let strategy = migrator.determine_migration_strategy(legacy_content).unwrap();
        assert_eq!(strategy, MigrationStrategy::LegacyFormat);

        // Test unified format
        let unified_content = r#"{
            "version": 2,
            "openai_auth": null,
            "claude_auth": null,
            "preferred_provider": "openai",
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        }"#;
        let strategy = migrator.determine_migration_strategy(unified_content).unwrap();
        assert_eq!(strategy, MigrationStrategy::NoMigrationNeeded);
    }

    #[tokio::test]
    async fn test_legacy_migration() {
        let temp_dir = tempdir().unwrap();
        let migrator = ConfigMigrator::new(temp_dir.path()).unwrap();

        let legacy_content = r#"{
            "OPENAI_API_KEY": "sk-test123",
            "tokens": {
                "access_token": "access_test",
                "refresh_token": "refresh_test",
                "account_id": "account_123"
            },
            "last_refresh": "2024-01-01T00:00:00Z"
        }"#;

        let mut warnings = Vec::new();
        let mut migrated_providers = Vec::new();

        let result = migrator.migrate_legacy_format(
            legacy_content,
            &mut warnings,
            &mut migrated_providers,
        ).unwrap();

        assert_eq!(result.version, 2);
        assert!(result.openai_auth.is_some());
        assert!(result.claude_auth.is_none());
        assert_eq!(result.preferred_provider, ProviderType::OpenAI);
        assert_eq!(migrated_providers, vec![ProviderType::OpenAI]);
        assert!(!warnings.is_empty()); // Should warn about deprecated last_refresh
    }

    #[tokio::test]
    async fn test_backup_and_restore() {
        let temp_dir = tempdir().unwrap();
        let migrator = ConfigMigrator::new(temp_dir.path()).unwrap();

        // Create a test auth.json file
        let auth_file = temp_dir.path().join("auth.json");
        let test_content = r#"{"OPENAI_API_KEY": "sk-test"}"#;
        fs::write(&auth_file, test_content).unwrap();

        // Create backup
        let backup = migrator.create_backup().await.unwrap();
        assert!(backup.auth_backup_path.exists());

        // Modify the original file
        fs::write(&auth_file, "modified content").unwrap();

        // Restore from backup
        migrator.restore_backup(backup).await.unwrap();

        // Verify restoration
        let restored_content = fs::read_to_string(&auth_file).unwrap();
        assert_eq!(restored_content, test_content);
    }

    #[test]
    fn test_needs_migration_detection() {
        let temp_dir = tempdir().unwrap();
        let migrator = ConfigMigrator::new(temp_dir.path()).unwrap();

        // No auth.json file
        assert!(!migrator.needs_migration().unwrap());

        // Create legacy auth.json
        let auth_file = temp_dir.path().join("auth.json");
        let legacy_content = r#"{"OPENAI_API_KEY": "sk-test"}"#;
        fs::write(&auth_file, legacy_content).unwrap();

        assert!(migrator.needs_migration().unwrap());

        // Create unified auth.json
        let unified_content = r#"{
            "version": 2,
            "openai_auth": null,
            "claude_auth": null,
            "preferred_provider": "openai",
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        }"#;
        fs::write(&auth_file, unified_content).unwrap();

        assert!(!migrator.needs_migration().unwrap());
    }

    #[test]
    fn test_migration_log() {
        let temp_dir = tempdir().unwrap();
        let migrator = ConfigMigrator::new(temp_dir.path()).unwrap();

        // Log some events
        migrator.log_migration_event("Test event 1").unwrap();
        migrator.log_migration_event("Test event 2").unwrap();

        // Read back the history
        let history = migrator.get_migration_history().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].message, "Test event 1");
        assert_eq!(history[1].message, "Test event 2");
    }
}