//! Unified authentication storage system
//! 
//! Provides a storage abstraction that can handle both OpenAI and Claude
//! authentication data while maintaining compatibility with existing auth.json format.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Unified authentication storage that handles multiple providers
#[derive(Debug, Clone)]
pub struct UnifiedAuthStorage {
    storage_path: PathBuf,
    backup_path: PathBuf,
    encryption_enabled: bool,
}

impl UnifiedAuthStorage {
    /// Create new storage instance
    pub fn new(codex_home: &Path) -> Result<Self, StorageError> {
        let storage_path = codex_home.join("auth.json");
        let backup_path = codex_home.join("auth.json.backup");
        
        // Ensure directory exists
        if let Some(parent) = storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        Ok(Self {
            storage_path,
            backup_path,
            encryption_enabled: false, // Can be enabled for enhanced security
        })
    }

    /// Load unified authentication data
    pub fn load(&self) -> Result<UnifiedAuthJson, StorageError> {
        if !self.storage_path.exists() {
            return Ok(UnifiedAuthJson::default());
        }

        let content = fs::read_to_string(&self.storage_path)?;
        
        // Try to parse as unified format first
        if let Ok(unified) = serde_json::from_str::<UnifiedAuthJson>(&content) {
            return Ok(unified);
        }

        // Fallback to legacy format and migrate
        if let Ok(legacy) = serde_json::from_str::<LegacyAuthJson>(&content) {
            tracing::info!("Migrating legacy auth.json format");
            let unified = self.migrate_from_legacy(legacy)?;
            self.save(&unified)?; // Save in new format
            return Ok(unified);
        }

        Err(StorageError::InvalidFormat("Could not parse auth.json in any known format".into()))
    }

    /// Save unified authentication data
    pub fn save(&self, data: &UnifiedAuthJson) -> Result<(), StorageError> {
        // Create backup of existing file
        if self.storage_path.exists() {
            fs::copy(&self.storage_path, &self.backup_path)?;
        }

        // Serialize data
        let content = if self.encryption_enabled {
            self.encrypt_data(data)?
        } else {
            serde_json::to_string_pretty(data)?
        };

        // Write atomically using temporary file
        let temp_path = self.storage_path.with_extension("tmp");
        {
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }

        // Set secure permissions (0o600)
        #[cfg(unix)]
        {
            let permissions = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&temp_path, permissions)?;
        }

        // Atomic rename
        fs::rename(temp_path, &self.storage_path)?;

        Ok(())
    }

    /// Check if storage file exists
    pub fn exists(&self) -> bool {
        self.storage_path.exists()
    }

    /// Get the size of the storage file
    pub fn size(&self) -> Result<u64, StorageError> {
        let metadata = fs::metadata(&self.storage_path)?;
        Ok(metadata.len())
    }

    /// Create a backup with timestamp
    pub fn create_timestamped_backup(&self) -> Result<PathBuf, StorageError> {
        if !self.storage_path.exists() {
            return Err(StorageError::FileNotFound);
        }

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = self.storage_path.with_file_name(
            format!("auth_{}.json.backup", timestamp)
        );

        fs::copy(&self.storage_path, &backup_path)?;
        Ok(backup_path)
    }

    /// Restore from backup
    pub fn restore_from_backup(&self, backup_path: &Path) -> Result<(), StorageError> {
        if !backup_path.exists() {
            return Err(StorageError::FileNotFound);
        }

        fs::copy(backup_path, &self.storage_path)?;
        Ok(())
    }

    /// Validate stored data integrity
    pub fn validate(&self) -> Result<ValidationResult, StorageError> {
        let data = self.load()?;
        let mut issues = Vec::new();

        // Check for expired tokens
        if let Some(openai_data) = &data.openai_auth {
            if let Some(tokens) = &openai_data.tokens {
                if let Some(expires_at) = tokens.expires_at {
                    if Utc::now() > expires_at {
                        issues.push("OpenAI tokens have expired".to_string());
                    }
                }
            }
        }

        if let Some(claude_data) = &data.claude_auth {
            if let Some(tokens) = &claude_data.tokens {
                if let Some(expires_at) = tokens.expires_at {
                    if Utc::now() > expires_at {
                        issues.push("Claude tokens have expired".to_string());
                    }
                }
            }
        }

        // Check for missing required fields
        if data.openai_auth.is_none() && data.claude_auth.is_none() {
            issues.push("No authentication providers configured".to_string());
        }

        Ok(ValidationResult {
            is_valid: issues.is_empty(),
            issues,
        })
    }

    // Private helper methods
    fn migrate_from_legacy(&self, legacy: LegacyAuthJson) -> Result<UnifiedAuthJson, StorageError> {
        let openai_auth = Some(OpenAIAuthData {
            api_key: legacy.openai_api_key,
            tokens: legacy.tokens,
        });

        Ok(UnifiedAuthJson {
            version: 2,
            openai_auth,
            claude_auth: None,
            preferred_provider: crate::ProviderType::OpenAI,
            last_provider_check: None,
            last_subscription_check: None,
            provider_capabilities: HashMap::new(),
            metadata: AuthMetadata {
                created_at: Utc::now(),
                updated_at: Utc::now(),
                migration_source: Some("legacy_auth_json".to_string()),
            },
        })
    }

    fn encrypt_data(&self, data: &UnifiedAuthJson) -> Result<String, StorageError> {
        // TODO: Implement encryption using a secure key derivation
        // For now, just serialize normally
        serde_json::to_string_pretty(data)
            .map_err(|e| StorageError::SerializationError(e.to_string()))
    }
}

/// Unified authentication data structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnifiedAuthJson {
    /// Storage format version for migration compatibility
    #[serde(default = "default_version")]
    pub version: u32,
    
    /// OpenAI authentication data
    pub openai_auth: Option<OpenAIAuthData>,
    
    /// Claude authentication data
    pub claude_auth: Option<ClaudeAuthData>,
    
    /// Currently preferred provider
    pub preferred_provider: crate::ProviderType,
    
    /// Last time provider availability was checked
    pub last_provider_check: Option<DateTime<Utc>>,
    
    /// Last time subscription status was checked
    pub last_subscription_check: Option<DateTime<Utc>>,
    
    /// Cached provider capabilities
    #[serde(default)]
    pub provider_capabilities: HashMap<String, ProviderCapabilities>,
    
    /// Storage metadata
    #[serde(default)]
    pub metadata: AuthMetadata,
}

impl Default for UnifiedAuthJson {
    fn default() -> Self {
        Self {
            version: 2,
            openai_auth: None,
            claude_auth: None,
            preferred_provider: crate::ProviderType::OpenAI,
            last_provider_check: None,
            last_subscription_check: None,
            provider_capabilities: HashMap::new(),
            metadata: AuthMetadata::default(),
        }
    }
}

/// OpenAI authentication data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIAuthData {
    #[serde(rename = "OPENAI_API_KEY")]
    pub api_key: Option<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<OpenAITokenData>,
}

/// Claude authentication data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeAuthData {
    pub api_key: Option<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<ClaudeTokenData>,
    
    /// Subscription information
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription: Option<ClaudeSubscriptionInfo>,
}

/// OpenAI token data structure (compatible with existing format)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAITokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub account_id: Option<String>,
}

/// Claude token data structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_type: String,
    pub scope: Option<String>,
}

/// Claude subscription information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeSubscriptionInfo {
    pub tier: String, // "free", "pro", "max"
    pub usage_limit: Option<u64>,
    pub usage_current: Option<u64>,
    pub reset_date: Option<DateTime<Utc>>,
    pub features: Vec<String>,
    pub last_checked: DateTime<Utc>,
}

/// Provider capabilities cache
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCapabilities {
    pub available: bool,
    pub subscription_active: bool,
    pub quota_remaining: Option<u64>,
    pub rate_limit_info: Option<RateLimitInfo>,
    pub last_checked: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Rate limiting information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitInfo {
    pub requests_per_minute: Option<u32>,
    pub tokens_per_minute: Option<u32>,
    pub concurrent_requests: Option<u32>,
}

/// Storage metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthMetadata {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub migration_source: Option<String>,
}

impl Default for AuthMetadata {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            updated_at: now,
            migration_source: None,
        }
    }
}

/// Legacy auth.json format for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyAuthJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<OpenAITokenData>,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

/// Abstract authentication data trait
pub trait AuthData: Send + Sync {
    fn provider_type(&self) -> crate::ProviderType;
    fn is_authenticated(&self) -> bool;
    fn needs_refresh(&self) -> bool;
    fn expires_at(&self) -> Option<DateTime<Utc>>;
}

impl AuthData for OpenAIAuthData {
    fn provider_type(&self) -> crate::ProviderType {
        crate::ProviderType::OpenAI
    }

    fn is_authenticated(&self) -> bool {
        self.api_key.is_some() || self.tokens.is_some()
    }

    fn needs_refresh(&self) -> bool {
        if let Some(tokens) = &self.tokens {
            if let Some(expires_at) = tokens.expires_at {
                return Utc::now() > expires_at - chrono::Duration::minutes(5);
            }
        }
        false
    }

    fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.tokens.as_ref().and_then(|t| t.expires_at)
    }
}

impl AuthData for ClaudeAuthData {
    fn provider_type(&self) -> crate::ProviderType {
        crate::ProviderType::Claude
    }

    fn is_authenticated(&self) -> bool {
        self.api_key.is_some() || self.tokens.is_some()
    }

    fn needs_refresh(&self) -> bool {
        if let Some(tokens) = &self.tokens {
            if let Some(expires_at) = tokens.expires_at {
                return Utc::now() > expires_at - chrono::Duration::minutes(5);
            }
        }
        false
    }

    fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.tokens.as_ref().and_then(|t| t.expires_at)
    }
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub issues: Vec<String>,
}

/// Storage error types
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("File not found")]
    FileNotFound,
    
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}

fn default_version() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_unified_storage_creation() {
        let temp_dir = tempdir().unwrap();
        let storage = UnifiedAuthStorage::new(temp_dir.path());
        assert!(storage.is_ok());
    }

    #[test]
    fn test_default_unified_auth_json() {
        let auth_json = UnifiedAuthJson::default();
        assert_eq!(auth_json.version, 2);
        assert_eq!(auth_json.preferred_provider, crate::ProviderType::OpenAI);
        assert!(auth_json.openai_auth.is_none());
        assert!(auth_json.claude_auth.is_none());
    }

    #[test]
    fn test_openai_auth_data_is_authenticated() {
        let auth_data = OpenAIAuthData {
            api_key: Some("sk-test".to_string()),
            tokens: None,
        };
        assert!(auth_data.is_authenticated());
        
        let empty_auth_data = OpenAIAuthData {
            api_key: None,
            tokens: None,
        };
        assert!(!empty_auth_data.is_authenticated());
    }

    #[test]
    fn test_claude_auth_data_provider_type() {
        let auth_data = ClaudeAuthData {
            api_key: Some("sk-ant-test".to_string()),
            tokens: None,
            subscription: None,
        };
        assert_eq!(auth_data.provider_type(), crate::ProviderType::Claude);
    }

    #[tokio::test]
    async fn test_storage_save_and_load() {
        let temp_dir = tempdir().unwrap();
        let storage = UnifiedAuthStorage::new(temp_dir.path()).unwrap();
        
        let mut auth_data = UnifiedAuthJson::default();
        auth_data.openai_auth = Some(OpenAIAuthData {
            api_key: Some("sk-test".to_string()),
            tokens: None,
        });
        
        // Save data
        storage.save(&auth_data).unwrap();
        
        // Load data back
        let loaded_data = storage.load().unwrap();
        assert_eq!(loaded_data.openai_auth, auth_data.openai_auth);
    }

    #[test]
    fn test_validation_result() {
        let temp_dir = tempdir().unwrap();
        let storage = UnifiedAuthStorage::new(temp_dir.path()).unwrap();
        
        // Test validation of empty storage
        let result = storage.validate().unwrap();
        assert!(!result.is_valid);
        assert!(!result.issues.is_empty());
    }

    #[test]
    fn test_serialization_compatibility() {
        let auth_data = UnifiedAuthJson {
            version: 2,
            openai_auth: Some(OpenAIAuthData {
                api_key: Some("sk-test".to_string()),
                tokens: None,
            }),
            claude_auth: Some(ClaudeAuthData {
                api_key: Some("sk-ant-test".to_string()),
                tokens: None,
                subscription: None,
            }),
            preferred_provider: crate::ProviderType::Claude,
            last_provider_check: Some(Utc::now()),
            last_subscription_check: None,
            provider_capabilities: HashMap::new(),
            metadata: AuthMetadata::default(),
        };

        // Test JSON serialization
        let json = serde_json::to_string(&auth_data).unwrap();
        let deserialized: UnifiedAuthJson = serde_json::from_str(&json).unwrap();
        assert_eq!(auth_data.version, deserialized.version);
        assert_eq!(auth_data.preferred_provider, deserialized.preferred_provider);
    }
}