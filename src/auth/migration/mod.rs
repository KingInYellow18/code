/// # Authentication Migration System
/// 
/// Provides zero-downtime migration from OpenAI-only authentication to unified Claude+OpenAI system.
/// 
/// ## Core Principles
/// - **Zero Downtime**: Preserve all existing sessions and credentials
/// - **Backward Compatibility**: Existing workflows continue unchanged
/// - **Data Preservation**: No loss of OpenAI tokens or configuration
/// - **Rollback Support**: Ability to revert if issues arise
/// 
/// ## Migration Strategy
/// 1. **Backup Phase**: Create secure backup of existing auth.json
/// 2. **Validation Phase**: Verify existing authentication still works
/// 3. **Extension Phase**: Add Claude auth support alongside existing
/// 4. **Testing Phase**: Verify both providers work correctly
/// 5. **Cleanup Phase**: Archive old backup files after confirmation

pub mod backup_manager;
pub mod migrator;
pub mod validator;
pub mod rollback;
pub mod testing;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

pub use backup_manager::BackupManager;
pub use migrator::AuthMigrator;
pub use validator::MigrationValidator;
pub use rollback::RollbackManager;
pub use testing::MigrationTester;

/// Migration progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    pub phase: MigrationPhase,
    pub started_at: DateTime<Utc>,
    pub completed_phases: Vec<MigrationPhase>,
    pub failed_phases: Vec<(MigrationPhase, String)>,
    pub backup_handle: Option<String>,
    pub rollback_available: bool,
    pub metadata: HashMap<String, String>,
}

/// Migration phases in order
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MigrationPhase {
    /// Initial state analysis and backup creation
    Backup,
    /// Validate existing authentication functionality
    Validation,
    /// Extend auth system to support Claude
    Extension,
    /// Test both providers work correctly
    Testing,
    /// Archive backup files and complete migration
    Cleanup,
    /// Migration completed successfully
    Completed,
    /// Migration failed and rolled back
    RolledBack,
}

impl MigrationPhase {
    /// Get the next phase in the migration sequence
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Backup => Some(Self::Validation),
            Self::Validation => Some(Self::Extension),
            Self::Extension => Some(Self::Testing),
            Self::Testing => Some(Self::Cleanup),
            Self::Cleanup => Some(Self::Completed),
            Self::Completed | Self::RolledBack => None,
        }
    }

    /// Check if this phase is terminal (no next phase)
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::RolledBack)
    }
}

/// Migration configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Maximum number of backup files to keep
    pub max_backups: usize,
    /// Enable automatic rollback on failure
    pub auto_rollback_on_failure: bool,
    /// Validate tokens before migration
    pub validate_tokens_before_migration: bool,
    /// Create encrypted backups
    pub encrypt_backups: bool,
    /// Backup retention period in days
    pub backup_retention_days: u32,
    /// Enable verbose logging
    pub verbose_logging: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            max_backups: 10,
            auto_rollback_on_failure: true,
            validate_tokens_before_migration: true,
            encrypt_backups: true,
            backup_retention_days: 30,
            verbose_logging: false,
        }
    }
}

/// Result type for migration operations
pub type MigrationResult<T> = Result<T, MigrationError>;

/// Migration errors with detailed context
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("Backup creation failed: {0}")]
    BackupFailed(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Migration extension failed: {0}")]
    ExtensionFailed(String),
    
    #[error("Testing failed: {0}")]
    TestingFailed(String),
    
    #[error("Rollback failed: {0}")]
    RollbackFailed(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
}

/// Main migration coordinator
#[derive(Debug)]
pub struct MigrationCoordinator {
    config: MigrationConfig,
    codex_home: PathBuf,
    backup_manager: BackupManager,
    migrator: AuthMigrator,
    validator: MigrationValidator,
    rollback_manager: RollbackManager,
    tester: MigrationTester,
}

impl MigrationCoordinator {
    /// Create a new migration coordinator
    pub fn new(codex_home: PathBuf, config: MigrationConfig) -> Self {
        let backup_manager = BackupManager::new(&codex_home, &config);
        let migrator = AuthMigrator::new(&codex_home, &config);
        let validator = MigrationValidator::new(&codex_home, &config);
        let rollback_manager = RollbackManager::new(&codex_home, &config);
        let tester = MigrationTester::new(&codex_home, &config);

        Self {
            config,
            codex_home,
            backup_manager,
            migrator,
            validator,
            rollback_manager,
            tester,
        }
    }

    /// Execute the complete migration process
    pub async fn execute_migration(&mut self) -> MigrationResult<MigrationProgress> {
        let mut progress = MigrationProgress {
            phase: MigrationPhase::Backup,
            started_at: Utc::now(),
            completed_phases: Vec::new(),
            failed_phases: Vec::new(),
            backup_handle: None,
            rollback_available: false,
            metadata: HashMap::new(),
        };

        // Store initial progress
        self.store_progress(&progress).await?;

        // Execute each phase with automatic rollback on failure
        if let Err(e) = self.execute_phases(&mut progress).await {
            if self.config.auto_rollback_on_failure && progress.rollback_available {
                match self.execute_rollback(&mut progress).await {
                    Ok(_) => {
                        progress.phase = MigrationPhase::RolledBack;
                        self.store_progress(&progress).await?;
                        return Err(e);
                    }
                    Err(rollback_err) => {
                        return Err(MigrationError::RollbackFailed(format!(
                            "Original error: {}. Rollback error: {}", e, rollback_err
                        )));
                    }
                }
            }
            return Err(e);
        }

        progress.phase = MigrationPhase::Completed;
        self.store_progress(&progress).await?;
        Ok(progress)
    }

    /// Execute all migration phases sequentially
    async fn execute_phases(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        while !progress.phase.is_terminal() {
            if self.config.verbose_logging {
                println!("Executing phase: {:?}", progress.phase);
            }

            let result = match progress.phase {
                MigrationPhase::Backup => self.execute_backup_phase(progress).await,
                MigrationPhase::Validation => self.execute_validation_phase(progress).await,
                MigrationPhase::Extension => self.execute_extension_phase(progress).await,
                MigrationPhase::Testing => self.execute_testing_phase(progress).await,
                MigrationPhase::Cleanup => self.execute_cleanup_phase(progress).await,
                _ => unreachable!("Terminal phases should not be executed"),
            };

            match result {
                Ok(_) => {
                    progress.completed_phases.push(progress.phase.clone());
                    if let Some(next_phase) = progress.phase.next() {
                        progress.phase = next_phase;
                    }
                    self.store_progress(progress).await?;
                }
                Err(e) => {
                    progress.failed_phases.push((progress.phase.clone(), e.to_string()));
                    self.store_progress(progress).await?;
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Execute backup phase
    async fn execute_backup_phase(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        let backup_handle = self.backup_manager.create_backup().await?;
        progress.backup_handle = Some(backup_handle.id.clone());
        progress.rollback_available = true;
        progress.metadata.insert("backup_created_at".to_string(), Utc::now().to_rfc3339());
        Ok(())
    }

    /// Execute validation phase
    async fn execute_validation_phase(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        let validation_result = self.validator.validate_existing_auth().await?;
        
        if !validation_result.is_valid {
            return Err(MigrationError::ValidationFailed(
                format!("Existing authentication is invalid: {:?}", validation_result.errors)
            ));
        }

        progress.metadata.insert("validation_passed".to_string(), "true".to_string());
        progress.metadata.insert("validated_at".to_string(), Utc::now().to_rfc3339());
        Ok(())
    }

    /// Execute extension phase
    async fn execute_extension_phase(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        let backup_handle = progress.backup_handle.as_ref()
            .ok_or_else(|| MigrationError::InvalidState("No backup handle available".to_string()))?;

        let migration_result = self.migrator.migrate_to_unified_format(backup_handle).await?;
        
        progress.metadata.insert("migration_completed_at".to_string(), Utc::now().to_rfc3339());
        progress.metadata.insert("migrated_providers".to_string(), 
            migration_result.migrated_providers.join(","));
        Ok(())
    }

    /// Execute testing phase
    async fn execute_testing_phase(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        let test_results = self.tester.run_comprehensive_tests().await?;
        
        if !test_results.all_passed() {
            return Err(MigrationError::TestingFailed(
                format!("Migration tests failed: {:?}", test_results.failed_tests())
            ));
        }

        progress.metadata.insert("tests_passed".to_string(), test_results.passed_count().to_string());
        progress.metadata.insert("tested_at".to_string(), Utc::now().to_rfc3339());
        Ok(())
    }

    /// Execute cleanup phase
    async fn execute_cleanup_phase(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        if let Some(backup_handle) = &progress.backup_handle {
            self.backup_manager.archive_backup(backup_handle).await?;
        }

        // Clean up old backups
        self.backup_manager.cleanup_old_backups().await?;
        
        progress.metadata.insert("cleanup_completed_at".to_string(), Utc::now().to_rfc3339());
        progress.rollback_available = false; // Cleanup removes rollback capability
        Ok(())
    }

    /// Execute rollback to previous state
    async fn execute_rollback(&mut self, progress: &mut MigrationProgress) -> MigrationResult<()> {
        if let Some(backup_handle) = &progress.backup_handle {
            self.rollback_manager.rollback_migration(backup_handle).await?;
            progress.metadata.insert("rolled_back_at".to_string(), Utc::now().to_rfc3339());
            Ok(())
        } else {
            Err(MigrationError::RollbackFailed("No backup handle available".to_string()))
        }
    }

    /// Store migration progress to memory
    async fn store_progress(&self, progress: &MigrationProgress) -> MigrationResult<()> {
        // Store in claude-flow memory for coordination
        let progress_json = serde_json::to_string(progress)?;
        
        // In a real implementation, this would use the memory management system
        // For now, we'll simulate it with a local file
        let progress_file = self.codex_home.join(".migration_progress.json");
        tokio::fs::write(progress_file, progress_json).await?;
        
        Ok(())
    }

    /// Get current migration progress
    pub async fn get_progress(&self) -> MigrationResult<Option<MigrationProgress>> {
        let progress_file = self.codex_home.join(".migration_progress.json");
        
        if !progress_file.exists() {
            return Ok(None);
        }

        let progress_json = tokio::fs::read_to_string(progress_file).await?;
        let progress: MigrationProgress = serde_json::from_str(&progress_json)?;
        Ok(Some(progress))
    }

    /// Check if migration is needed
    pub async fn is_migration_needed(&self) -> MigrationResult<bool> {
        // Check if there's an existing OpenAI-only auth.json that needs migration
        let auth_file = self.codex_home.join("auth.json");
        if !auth_file.exists() {
            return Ok(false);
        }

        // Check if unified auth format is already in place
        let unified_auth_file = self.codex_home.join("unified_auth.json");
        if unified_auth_file.exists() {
            return Ok(false);
        }

        // Check current migration progress
        if let Some(progress) = self.get_progress().await? {
            return Ok(!matches!(progress.phase, MigrationPhase::Completed));
        }

        Ok(true)
    }

    /// Get migration status summary
    pub async fn get_status_summary(&self) -> MigrationResult<MigrationStatusSummary> {
        let is_needed = self.is_migration_needed().await?;
        let progress = self.get_progress().await?;
        
        Ok(MigrationStatusSummary {
            migration_needed: is_needed,
            current_progress: progress,
            backup_count: self.backup_manager.get_backup_count().await?,
            estimated_duration_minutes: self.estimate_migration_duration().await?,
        })
    }

    /// Estimate migration duration based on system state
    async fn estimate_migration_duration(&self) -> MigrationResult<u32> {
        // Base duration estimates per phase (in minutes)
        let base_durations = [
            (MigrationPhase::Backup, 2),
            (MigrationPhase::Validation, 1),
            (MigrationPhase::Extension, 3),
            (MigrationPhase::Testing, 5),
            (MigrationPhase::Cleanup, 1),
        ];

        // Additional time based on system complexity
        let auth_file_size = if let Ok(metadata) = tokio::fs::metadata(self.codex_home.join("auth.json")).await {
            metadata.len()
        } else {
            0
        };

        let complexity_factor = if auth_file_size > 10_000 { 1.5 } else { 1.0 };
        
        let total_minutes: f32 = base_durations.iter()
            .map(|(_, duration)| *duration as f32)
            .sum::<f32>() * complexity_factor;

        Ok(total_minutes.ceil() as u32)
    }
}

/// Summary of migration status
#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationStatusSummary {
    pub migration_needed: bool,
    pub current_progress: Option<MigrationProgress>,
    pub backup_count: usize,
    pub estimated_duration_minutes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_migration_phase_sequence() {
        assert_eq!(MigrationPhase::Backup.next(), Some(MigrationPhase::Validation));
        assert_eq!(MigrationPhase::Validation.next(), Some(MigrationPhase::Extension));
        assert_eq!(MigrationPhase::Extension.next(), Some(MigrationPhase::Testing));
        assert_eq!(MigrationPhase::Testing.next(), Some(MigrationPhase::Cleanup));
        assert_eq!(MigrationPhase::Cleanup.next(), Some(MigrationPhase::Completed));
        assert_eq!(MigrationPhase::Completed.next(), None);
        assert_eq!(MigrationPhase::RolledBack.next(), None);
    }

    #[tokio::test]
    async fn test_migration_coordinator_creation() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let coordinator = MigrationCoordinator::new(temp_dir.path().to_path_buf(), config);
        
        assert_eq!(coordinator.codex_home, temp_dir.path());
    }

    #[tokio::test]
    async fn test_migration_needed_detection() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let coordinator = MigrationCoordinator::new(temp_dir.path().to_path_buf(), config);
        
        // No auth file - no migration needed
        assert!(!coordinator.is_migration_needed().await.unwrap());
        
        // Create auth file - migration needed
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "test"}"#).await.unwrap();
        assert!(coordinator.is_migration_needed().await.unwrap());
        
        // Create unified auth file - no migration needed
        let unified_auth_file = temp_dir.path().join("unified_auth.json");
        tokio::fs::write(&unified_auth_file, r#"{"version": "2.0"}"#).await.unwrap();
        assert!(!coordinator.is_migration_needed().await.unwrap());
    }
}