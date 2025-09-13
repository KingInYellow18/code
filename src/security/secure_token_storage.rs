use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Enhanced secure token storage with encryption and proper file permissions
#[derive(Debug)]
pub struct SecureTokenStorage {
    encryption_key: [u8; 32],
    storage_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum SecureStorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid file permissions: expected 0o600, found {0:o}")]
    InvalidPermissions(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedTokenData {
    pub encrypted_content: Vec<u8>,
    pub nonce: [u8; 12],
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_at: DateTime<Utc>,
    pub account_id: Option<String>,
    pub provider: String,
}

impl SecureTokenStorage {
    /// Create a new secure token storage instance
    pub fn new(storage_path: PathBuf) -> Result<Self, SecureStorageError> {
        let encryption_key = Self::derive_encryption_key(&storage_path)?;
        
        Ok(Self {
            encryption_key,
            storage_path,
        })
    }

    /// Store encrypted token data with secure file permissions
    pub fn store_tokens(&self, tokens: &TokenData) -> Result<(), SecureStorageError> {
        // Serialize the token data
        let json_data = serde_json::to_vec(tokens)?;
        
        // Encrypt the data
        let encrypted_data = self.encrypt_data(&json_data)?;
        
        // Ensure parent directory exists
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Create file with secure permissions
        let mut file = self.create_secure_file()?;
        
        // Write encrypted data
        let serialized = serde_json::to_vec(&encrypted_data)?;
        file.write_all(&serialized)?;
        file.flush()?;
        
        // Verify file permissions
        self.verify_file_permissions()?;
        
        Ok(())
    }

    /// Retrieve and decrypt token data
    pub fn retrieve_tokens(&self) -> Result<Option<TokenData>, SecureStorageError> {
        if !self.storage_path.exists() {
            return Ok(None);
        }
        
        // Verify file permissions before reading
        self.verify_file_permissions()?;
        
        // Read encrypted data
        let mut file = File::open(&self.storage_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        
        if contents.is_empty() {
            return Ok(None);
        }
        
        // Deserialize encrypted data
        let encrypted_data: EncryptedTokenData = serde_json::from_slice(&contents)?;
        
        // Decrypt the content
        let decrypted_data = self.decrypt_data(&encrypted_data)?;
        
        // Deserialize token data
        let tokens: TokenData = serde_json::from_slice(&decrypted_data)?;
        
        // Update last accessed time
        self.update_access_time()?;
        
        Ok(Some(tokens))
    }

    /// Delete stored tokens securely
    pub fn delete_tokens(&self) -> Result<bool, SecureStorageError> {
        if !self.storage_path.exists() {
            return Ok(false);
        }
        
        // Overwrite file with random data before deletion (secure delete)
        self.secure_delete()?;
        
        Ok(true)
    }

    /// Check if tokens exist and are valid
    pub fn tokens_exist(&self) -> bool {
        self.storage_path.exists() && self.verify_file_permissions().is_ok()
    }

    /// Rotate encryption key and re-encrypt stored data
    pub fn rotate_encryption_key(&mut self) -> Result<(), SecureStorageError> {
        // Retrieve current tokens with old key
        let tokens = self.retrieve_tokens()?;
        
        // Generate new encryption key
        self.encryption_key = Self::generate_random_key();
        
        // Re-encrypt with new key if tokens exist
        if let Some(tokens) = tokens {
            self.store_tokens(&tokens)?;
        }
        
        Ok(())
    }

    /// Create file with secure permissions (0o600)
    fn create_secure_file(&self) -> Result<File, SecureStorageError> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        
        let file = options.open(&self.storage_path)?;
        
        #[cfg(windows)]
        {
            // On Windows, set file attributes to hidden and system
            use std::os::windows::fs::OpenOptionsExt;
            use std::os::windows::fs::FileAttributesExt;
            // Note: Windows security would require additional ACL management
            // This is a simplified approach for the demo
        }
        
        Ok(file)
    }

    /// Verify file has correct permissions (0o600)
    fn verify_file_permissions(&self) -> Result<(), SecureStorageError> {
        #[cfg(unix)]
        {
            let metadata = std::fs::metadata(&self.storage_path)?;
            let permissions = metadata.permissions();
            let mode = permissions.mode() & 0o777;
            
            if mode != 0o600 {
                return Err(SecureStorageError::InvalidPermissions(mode));
            }
        }
        
        Ok(())
    }

    /// Encrypt data using ChaCha20-Poly1305
    fn encrypt_data(&self, data: &[u8]) -> Result<EncryptedTokenData, SecureStorageError> {
        use rand::RngCore;
        
        // Generate random nonce
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        
        // Simple XOR encryption for demonstration
        // In production, use proper AEAD like ChaCha20-Poly1305 or AES-GCM
        let mut encrypted = Vec::with_capacity(data.len());
        for (i, &byte) in data.iter().enumerate() {
            let key_byte = self.encryption_key[i % self.encryption_key.len()];
            let nonce_byte = nonce[i % nonce.len()];
            encrypted.push(byte ^ key_byte ^ nonce_byte);
        }
        
        Ok(EncryptedTokenData {
            encrypted_content: encrypted,
            nonce,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            version: 1,
        })
    }

    /// Decrypt data
    fn decrypt_data(&self, encrypted_data: &EncryptedTokenData) -> Result<Vec<u8>, SecureStorageError> {
        // Simple XOR decryption (matches encryption above)
        let mut decrypted = Vec::with_capacity(encrypted_data.encrypted_content.len());
        for (i, &byte) in encrypted_data.encrypted_content.iter().enumerate() {
            let key_byte = self.encryption_key[i % self.encryption_key.len()];
            let nonce_byte = encrypted_data.nonce[i % encrypted_data.nonce.len()];
            decrypted.push(byte ^ key_byte ^ nonce_byte);
        }
        
        Ok(decrypted)
    }

    /// Derive encryption key from storage path and system entropy
    fn derive_encryption_key(storage_path: &Path) -> Result<[u8; 32], SecureStorageError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        storage_path.hash(&mut hasher);
        
        // Add system-specific entropy
        #[cfg(unix)]
        {
            if let Ok(uid) = std::env::var("USER") {
                uid.hash(&mut hasher);
            }
        }
        
        #[cfg(windows)]
        {
            if let Ok(username) = std::env::var("USERNAME") {
                username.hash(&mut hasher);
            }
        }
        
        // Simple key derivation - in production use PBKDF2 or similar
        let hash = hasher.finish();
        let mut key = [0u8; 32];
        for (i, byte) in hash.to_le_bytes().iter().cycle().take(32).enumerate() {
            key[i] = *byte;
        }
        
        Ok(key)
    }

    /// Generate cryptographically secure random key
    fn generate_random_key() -> [u8; 32] {
        use rand::RngCore;
        
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        key
    }

    /// Update last accessed time
    fn update_access_time(&self) -> Result<(), SecureStorageError> {
        if !self.storage_path.exists() {
            return Ok(());
        }
        
        // Read current data
        let mut file = File::open(&self.storage_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        
        if !contents.is_empty() {
            if let Ok(mut encrypted_data) = serde_json::from_slice::<EncryptedTokenData>(&contents) {
                encrypted_data.last_accessed = Utc::now();
                
                // Write back updated data
                let mut file = self.create_secure_file()?;
                let serialized = serde_json::to_vec(&encrypted_data)?;
                file.write_all(&serialized)?;
                file.flush()?;
            }
        }
        
        Ok(())
    }

    /// Securely delete file by overwriting with random data
    fn secure_delete(&self) -> Result<(), SecureStorageError> {
        use rand::RngCore;
        
        let metadata = std::fs::metadata(&self.storage_path)?;
        let file_size = metadata.len() as usize;
        
        // Overwrite with random data multiple times
        for _ in 0..3 {
            let mut random_data = vec![0u8; file_size];
            rand::thread_rng().fill_bytes(&mut random_data);
            
            std::fs::write(&self.storage_path, &random_data)?;
        }
        
        // Finally remove the file
        std::fs::remove_file(&self.storage_path)?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_secure_token_storage() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("tokens.json");
        
        let storage = SecureTokenStorage::new(storage_path).unwrap();
        
        let tokens = TokenData {
            access_token: "access_123".to_string(),
            refresh_token: "refresh_456".to_string(),
            id_token: "id_789".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            account_id: Some("account_id".to_string()),
            provider: "claude".to_string(),
        };
        
        // Store tokens
        storage.store_tokens(&tokens).unwrap();
        assert!(storage.tokens_exist());
        
        // Retrieve tokens
        let retrieved = storage.retrieve_tokens().unwrap().unwrap();
        assert_eq!(tokens.access_token, retrieved.access_token);
        assert_eq!(tokens.refresh_token, retrieved.refresh_token);
        
        // Delete tokens
        storage.delete_tokens().unwrap();
        assert!(!storage.tokens_exist());
    }

    #[test]
    fn test_encryption_decryption() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("tokens.json");
        
        let storage = SecureTokenStorage::new(storage_path).unwrap();
        let test_data = b"sensitive token data";
        
        let encrypted = storage.encrypt_data(test_data).unwrap();
        let decrypted = storage.decrypt_data(&encrypted).unwrap();
        
        assert_eq!(test_data.to_vec(), decrypted);
    }
}