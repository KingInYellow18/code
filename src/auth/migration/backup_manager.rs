/// # Backup Management System
/// 
/// Provides secure backup and restoration capabilities for authentication data.
/// Supports encrypted backups, versioning, and automatic cleanup.

use super::{MigrationConfig, MigrationError, MigrationResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Backup handle for tracking and restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupHandle {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub file_path: PathBuf,
    pub metadata: BackupMetadata,
    pub encrypted: bool,
    pub checksum: String,
}

/// Metadata associated with a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub original_file_size: u64,
    pub auth_mode: String,
    pub has_tokens: bool,
    pub has_api_key: bool,
    pub backup_version: String,
    pub system_info: HashMap<String, String>,
}

/// Backup verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVerification {
    pub is_valid: bool,
    pub checksum_match: bool,
    pub file_exists: bool,
    pub can_decrypt: bool,
    pub metadata_valid: bool,
    pub errors: Vec<String>,
}

/// Backup manager for handling auth data backups
#[derive(Debug)]
pub struct BackupManager {
    codex_home: PathBuf,
    backup_dir: PathBuf,
    config: MigrationConfig,
    encryption_key: Option<[u8; 32]>,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(codex_home: &Path, config: &MigrationConfig) -> Self {
        let backup_dir = codex_home.join(".backups");
        let encryption_key = if config.encrypt_backups {
            Some(Self::derive_encryption_key(codex_home))
        } else {
            None
        };

        Self {
            codex_home: codex_home.to_path_buf(),
            backup_dir,
            config: config.clone(),
            encryption_key,
        }
    }

    /// Create a backup of the current auth.json file
    pub async fn create_backup(&self) -> MigrationResult<BackupHandle> {
        self.ensure_backup_dir().await?;

        let auth_file = self.codex_home.join("auth.json");
        if !auth_file.exists() {
            return Err(MigrationError::BackupFailed(
                "No auth.json file found to backup".to_string()
            ));
        }

        // Generate unique backup ID
        let backup_id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now();
        let backup_filename = format!("auth_backup_{}_{}.json", 
            timestamp.format("%Y%m%d_%H%M%S"), 
            &backup_id[..8]
        );
        let backup_path = self.backup_dir.join(&backup_filename);

        // Read original file
        let auth_content = tokio::fs::read_to_string(&auth_file).await
            .map_err(|e| MigrationError::BackupFailed(format!("Failed to read auth.json: {}", e)))?;

        // Parse to extract metadata
        let auth_data: serde_json::Value = serde_json::from_str(&auth_content)
            .map_err(|e| MigrationError::BackupFailed(format!("Invalid JSON in auth.json: {}", e)))?;

        let metadata = self.extract_backup_metadata(&auth_data, &auth_file).await?;

        // Create backup content
        let backup_content = if self.config.encrypt_backups {
            self.encrypt_content(&auth_content)?
        } else {
            auth_content.into_bytes()
        };

        // Write backup file
        tokio::fs::write(&backup_path, &backup_content).await
            .map_err(|e| MigrationError::BackupFailed(format!("Failed to write backup: {}", e)))?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&backup_path).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&backup_path, perms).await?;
        }

        // Calculate checksum
        let checksum = self.calculate_checksum(&backup_content);

        let handle = BackupHandle {
            id: backup_id,
            created_at: timestamp,
            file_path: backup_path,
            metadata,
            encrypted: self.config.encrypt_backups,
            checksum,
        };

        // Save backup handle
        self.save_backup_handle(&handle).await?;

        if self.config.verbose_logging {
            println!("Created backup: {} at {}", handle.id, handle.file_path.display());
        }

        Ok(handle)
    }

    /// Verify a backup's integrity
    pub async fn verify_backup(&self, handle: &BackupHandle) -> MigrationResult<BackupVerification> {
        let mut verification = BackupVerification {
            is_valid: true,
            checksum_match: false,
            file_exists: false,
            can_decrypt: false,
            metadata_valid: false,
            errors: Vec::new(),
        };

        // Check if file exists
        verification.file_exists = handle.file_path.exists();
        if !verification.file_exists {
            verification.errors.push("Backup file does not exist".to_string());
            verification.is_valid = false;
            return Ok(verification);
        }

        // Verify checksum
        let backup_content = tokio::fs::read(&handle.file_path).await?;
        let calculated_checksum = self.calculate_checksum(&backup_content);
        verification.checksum_match = calculated_checksum == handle.checksum;
        if !verification.checksum_match {
            verification.errors.push("Checksum mismatch - backup may be corrupted".to_string());
            verification.is_valid = false;
        }

        // Test decryption if encrypted
        if handle.encrypted {
            match self.decrypt_content(&backup_content) {
                Ok(_) => verification.can_decrypt = true,
                Err(e) => {
                    verification.errors.push(format!("Cannot decrypt backup: {}", e));
                    verification.is_valid = false;
                }
            }
        } else {
            verification.can_decrypt = true;
        }

        // Validate metadata
        verification.metadata_valid = self.validate_metadata(&handle.metadata);
        if !verification.metadata_valid {
            verification.errors.push("Invalid backup metadata".to_string());
            verification.is_valid = false;
        }

        Ok(verification)
    }

    /// Restore auth.json from a backup
    pub async fn restore_from_backup(&self, handle: &BackupHandle) -> MigrationResult<()> {
        // Verify backup before restoration
        let verification = self.verify_backup(handle).await?;
        if !verification.is_valid {
            return Err(MigrationError::BackupFailed(
                format!("Cannot restore from invalid backup: {:?}", verification.errors)
            ));
        }

        // Read backup content
        let backup_content = tokio::fs::read(&handle.file_path).await?;
        
        // Decrypt if necessary
        let auth_content = if handle.encrypted {
            self.decrypt_content(&backup_content)?
        } else {
            String::from_utf8(backup_content)
                .map_err(|e| MigrationError::BackupFailed(format!("Invalid UTF-8 in backup: {}", e)))?
        };

        // Restore to auth.json
        let auth_file = self.codex_home.join("auth.json");
        
        // Create backup of current file if it exists
        if auth_file.exists() {
            let current_backup = format!("auth.json.pre_restore_{}", Utc::now().format("%Y%m%d_%H%M%S"));
            let current_backup_path = self.codex_home.join(current_backup);
            tokio::fs::copy(&auth_file, current_backup_path).await?;
        }

        // Write restored content
        tokio::fs::write(&auth_file, auth_content).await?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&auth_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&auth_file, perms).await?;
        }

        if self.config.verbose_logging {
            println!("Restored auth.json from backup: {}", handle.id);
        }

        Ok(())
    }

    /// List all available backups
    pub async fn list_backups(&self) -> MigrationResult<Vec<BackupHandle>> {
        if !self.backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut handles = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.backup_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("handle") {
                if let Ok(handle) = self.load_backup_handle(&path).await {
                    handles.push(handle);
                }
            }
        }

        // Sort by creation time (newest first)
        handles.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(handles)
    }

    /// Get backup count
    pub async fn get_backup_count(&self) -> MigrationResult<usize> {
        Ok(self.list_backups().await?.len())
    }

    /// Archive a backup after successful migration
    pub async fn archive_backup(&self, backup_id: &str) -> MigrationResult<()> {
        let handles = self.list_backups().await?;
        let handle = handles.iter()
            .find(|h| h.id == backup_id)
            .ok_or_else(|| MigrationError::BackupFailed(format!("Backup {} not found", backup_id)))?;

        let archive_dir = self.backup_dir.join("archived");
        tokio::fs::create_dir_all(&archive_dir).await?;

        let archived_backup_path = archive_dir.join(handle.file_path.file_name().unwrap());
        let archived_handle_path = archive_dir.join(format!("{}.handle", handle.id));

        // Move backup file and handle to archive
        tokio::fs::rename(&handle.file_path, archived_backup_path).await?;
        
        let handle_path = self.backup_dir.join(format!("{}.handle", handle.id));
        if handle_path.exists() {
            tokio::fs::rename(handle_path, archived_handle_path).await?;
        }

        if self.config.verbose_logging {
            println!("Archived backup: {}", backup_id);
        }

        Ok(())
    }

    /// Clean up old backups based on retention policy
    pub async fn cleanup_old_backups(&self) -> MigrationResult<()> {
        let handles = self.list_backups().await?;
        let retention_cutoff = Utc::now() - chrono::Duration::days(self.config.backup_retention_days as i64);
        let mut removed_count = 0;

        // Remove backups older than retention period (keeping at least one)
        for handle in handles.iter().skip(1) { // Skip the newest backup
            if handle.created_at < retention_cutoff {
                self.delete_backup(&handle.id).await?;
                removed_count += 1;
            }
        }

        // Enforce max backup limit
        if handles.len() > self.config.max_backups {
            let excess_count = handles.len() - self.config.max_backups;
            for handle in handles.iter().skip(self.config.max_backups) {
                self.delete_backup(&handle.id).await?;
                removed_count += 1;
            }
        }

        if self.config.verbose_logging && removed_count > 0 {
            println!("Cleaned up {} old backups", removed_count);
        }

        Ok(())
    }

    /// Delete a specific backup
    async fn delete_backup(&self, backup_id: &str) -> MigrationResult<()> {
        let handles = self.list_backups().await?;
        let handle = handles.iter()
            .find(|h| h.id == backup_id)
            .ok_or_else(|| MigrationError::BackupFailed(format!("Backup {} not found", backup_id)))?;

        // Remove backup file
        if handle.file_path.exists() {
            tokio::fs::remove_file(&handle.file_path).await?;
        }

        // Remove handle file
        let handle_path = self.backup_dir.join(format!("{}.handle", handle.id));
        if handle_path.exists() {
            tokio::fs::remove_file(handle_path).await?;
        }

        Ok(())
    }

    /// Ensure backup directory exists
    async fn ensure_backup_dir(&self) -> MigrationResult<()> {
        if !self.backup_dir.exists() {
            tokio::fs::create_dir_all(&self.backup_dir).await?;
            
            // Set secure permissions on backup directory
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = tokio::fs::metadata(&self.backup_dir).await?.permissions();
                perms.set_mode(0o700);
                tokio::fs::set_permissions(&self.backup_dir, perms).await?;
            }
        }
        Ok(())
    }

    /// Extract metadata from auth data
    async fn extract_backup_metadata(&self, auth_data: &serde_json::Value, auth_file: &Path) -> MigrationResult<BackupMetadata> {
        let file_metadata = tokio::fs::metadata(auth_file).await?;
        let has_tokens = auth_data.get("tokens").is_some();
        let has_api_key = auth_data.get("OPENAI_API_KEY").is_some();
        
        let auth_mode = if has_tokens && !has_api_key {
            "ChatGPT".to_string()
        } else if has_api_key {
            "ApiKey".to_string()
        } else {
            "Unknown".to_string()
        };

        let mut system_info = HashMap::new();
        system_info.insert("hostname".to_string(), 
            gethostname::gethostname().to_string_lossy().to_string());
        system_info.insert("platform".to_string(), std::env::consts::OS.to_string());
        system_info.insert("arch".to_string(), std::env::consts::ARCH.to_string());

        Ok(BackupMetadata {
            original_file_size: file_metadata.len(),
            auth_mode,
            has_tokens,
            has_api_key,
            backup_version: "1.0".to_string(),
            system_info,
        })
    }

    /// Save backup handle to disk
    async fn save_backup_handle(&self, handle: &BackupHandle) -> MigrationResult<()> {
        let handle_path = self.backup_dir.join(format!("{}.handle", handle.id));
        let handle_json = serde_json::to_string_pretty(handle)?;
        tokio::fs::write(handle_path, handle_json).await?;
        Ok(())
    }

    /// Load backup handle from disk
    async fn load_backup_handle(&self, handle_path: &Path) -> MigrationResult<BackupHandle> {
        let handle_json = tokio::fs::read_to_string(handle_path).await?;
        let handle: BackupHandle = serde_json::from_str(&handle_json)?;
        Ok(handle)
    }

    /// Derive encryption key from system characteristics
    fn derive_encryption_key(codex_home: &Path) -> [u8; 32] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        
        // Use path and system info to derive key
        codex_home.hash(&mut hasher);
        std::env::consts::OS.hash(&mut hasher);
        gethostname::gethostname().hash(&mut hasher);
        
        let hash = hasher.finish();
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&hash.to_le_bytes());
        
        // Expand to full key using simple derivation
        for i in 1..4 {
            let derived_hash = hash.wrapping_mul(i as u64 + 1);
            let start = i * 8;
            key[start..start + 8].copy_from_slice(&derived_hash.to_le_bytes());
        }
        
        key
    }

    /// Encrypt content using XOR cipher (simple encryption for demo)
    fn encrypt_content(&self, content: &str) -> MigrationResult<Vec<u8>> {
        if let Some(key) = &self.encryption_key {
            let content_bytes = content.as_bytes();
            let mut encrypted = Vec::with_capacity(content_bytes.len());
            
            for (i, &byte) in content_bytes.iter().enumerate() {
                encrypted.push(byte ^ key[i % key.len()]);
            }
            
            Ok(encrypted)
        } else {
            Err(MigrationError::BackupFailed("No encryption key available".to_string()))
        }
    }

    /// Decrypt content using XOR cipher
    fn decrypt_content(&self, encrypted: &[u8]) -> MigrationResult<String> {
        if let Some(key) = &self.encryption_key {
            let mut decrypted = Vec::with_capacity(encrypted.len());
            
            for (i, &byte) in encrypted.iter().enumerate() {
                decrypted.push(byte ^ key[i % key.len()]);
            }
            
            String::from_utf8(decrypted)
                .map_err(|e| MigrationError::BackupFailed(format!("Decryption failed: {}", e)))
        } else {
            Err(MigrationError::BackupFailed("No encryption key available".to_string()))
        }
    }

    /// Calculate checksum for content verification
    fn calculate_checksum(&self, content: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Validate backup metadata
    fn validate_metadata(&self, metadata: &BackupMetadata) -> bool {
        !metadata.auth_mode.is_empty() &&
        metadata.original_file_size > 0 &&
        !metadata.backup_version.is_empty() &&
        !metadata.system_info.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_backup_creation_and_restoration() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let manager = BackupManager::new(temp_dir.path(), &config);

        // Create a test auth.json file
        let auth_file = temp_dir.path().join("auth.json");
        let test_content = r#"{"OPENAI_API_KEY": "test-key", "tokens": null}"#;
        tokio::fs::write(&auth_file, test_content).await.unwrap();

        // Create backup
        let backup_handle = manager.create_backup().await.unwrap();
        assert!(backup_handle.file_path.exists());
        assert!(!backup_handle.id.is_empty());

        // Verify backup
        let verification = manager.verify_backup(&backup_handle).await.unwrap();
        assert!(verification.is_valid);

        // Modify original file
        tokio::fs::write(&auth_file, r#"{"modified": true}"#).await.unwrap();

        // Restore from backup
        manager.restore_from_backup(&backup_handle).await.unwrap();

        // Verify restoration
        let restored_content = tokio::fs::read_to_string(&auth_file).await.unwrap();
        assert_eq!(restored_content, test_content);
    }

    #[tokio::test]
    async fn test_backup_listing_and_cleanup() {
        let temp_dir = tempdir().unwrap();
        let mut config = MigrationConfig::default();
        config.max_backups = 2;
        let manager = BackupManager::new(temp_dir.path(), &config);

        // Create test auth.json
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"test": "data"}"#).await.unwrap();

        // Create multiple backups
        let _backup1 = manager.create_backup().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let _backup2 = manager.create_backup().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let _backup3 = manager.create_backup().await.unwrap();

        // Check backup count before cleanup
        let backups_before = manager.list_backups().await.unwrap();
        assert_eq!(backups_before.len(), 3);

        // Cleanup should enforce max_backups limit
        manager.cleanup_old_backups().await.unwrap();
        
        let backups_after = manager.list_backups().await.unwrap();
        assert_eq!(backups_after.len(), 2);
    }

    #[tokio::test]
    async fn test_encrypted_backup() {
        let temp_dir = tempdir().unwrap();
        let mut config = MigrationConfig::default();
        config.encrypt_backups = true;
        let manager = BackupManager::new(temp_dir.path(), &config);

        // Create test auth.json
        let auth_file = temp_dir.path().join("auth.json");
        let test_content = r#"{"secret": "sensitive-data"}"#;
        tokio::fs::write(&auth_file, test_content).await.unwrap();

        // Create encrypted backup
        let backup_handle = manager.create_backup().await.unwrap();
        assert!(backup_handle.encrypted);

        // Verify the backup file is actually encrypted (not plain text)
        let backup_content = tokio::fs::read(&backup_handle.file_path).await.unwrap();
        let backup_str = String::from_utf8_lossy(&backup_content);
        assert!(!backup_str.contains("sensitive-data"));

        // Verify backup integrity
        let verification = manager.verify_backup(&backup_handle).await.unwrap();
        assert!(verification.is_valid);
        assert!(verification.can_decrypt);

        // Test restoration
        tokio::fs::write(&auth_file, "{}").await.unwrap();
        manager.restore_from_backup(&backup_handle).await.unwrap();
        
        let restored_content = tokio::fs::read_to_string(&auth_file).await.unwrap();
        assert_eq!(restored_content, test_content);
    }
}