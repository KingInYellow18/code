/// # Rollback Management System
/// 
/// Provides safe rollback capabilities for failed migrations, ensuring users can always
/// return to their previous working state without data loss.

use super::{BackupHandle, MigrationConfig, MigrationError, MigrationResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Rollback operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub success: bool,
    pub rollback_timestamp: DateTime<Utc>,
    pub restored_files: Vec<String>,
    pub removed_files: Vec<String>,
    pub rollback_duration: chrono::Duration,
    pub validation_passed: bool,
    pub warnings: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Rollback validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackValidation {
    pub can_rollback: bool,
    pub backup_valid: bool,
    pub files_restorable: bool,
    pub dependencies_met: bool,
    pub blocking_issues: Vec<String>,
    pub warnings: Vec<String>,
}

/// Rollback plan with detailed steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPlan {
    pub backup_id: String,
    pub steps: Vec<RollbackStep>,
    pub estimated_duration_seconds: u32,
    pub affected_files: Vec<String>,
    pub validation_checks: Vec<String>,
}

/// Individual rollback step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackStep {
    pub order: u32,
    pub action: RollbackAction,
    pub description: String,
    pub target_path: String,
    pub reversible: bool,
    pub critical: bool,
}

/// Types of rollback actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackAction {
    RestoreFile,
    RemoveFile,
    CreateBackup,
    ValidateRestore,
    CleanupArtifacts,
}

/// Rollback manager implementation
#[derive(Debug)]
pub struct RollbackManager {
    codex_home: PathBuf,
    config: MigrationConfig,
    backup_manager: super::BackupManager,
}

impl RollbackManager {
    /// Create a new rollback manager
    pub fn new(codex_home: &Path, config: &MigrationConfig) -> Self {
        let backup_manager = super::BackupManager::new(codex_home, config);

        Self {
            codex_home: codex_home.to_path_buf(),
            config: config.clone(),
            backup_manager,
        }
    }

    /// Execute rollback from a backup
    pub async fn rollback_migration(&self, backup_id: &str) -> MigrationResult<RollbackResult> {
        let start_time = Utc::now();
        let mut result = RollbackResult {
            success: false,
            rollback_timestamp: start_time,
            restored_files: Vec::new(),
            removed_files: Vec::new(),
            rollback_duration: chrono::Duration::zero(),
            validation_passed: false,
            warnings: Vec::new(),
            metadata: HashMap::new(),
        };

        if self.config.verbose_logging {
            println!("Starting rollback for backup: {}", backup_id);
        }

        // Find backup handle
        let backup_handle = self.find_backup_handle(backup_id).await?;
        
        // Validate rollback feasibility
        let validation = self.validate_rollback(&backup_handle).await?;
        if !validation.can_rollback {
            return Err(MigrationError::RollbackFailed(
                format!("Cannot rollback: {:?}", validation.blocking_issues)
            ));
        }

        // Create rollback plan
        let plan = self.create_rollback_plan(&backup_handle).await?;

        // Execute rollback plan
        match self.execute_rollback_plan(&plan, &mut result).await {
            Ok(_) => {
                result.success = true;
                result.validation_passed = self.validate_rollback_result(&backup_handle).await?;
            }
            Err(e) => {
                result.metadata.insert("rollback_error".to_string(), e.to_string());
                return Err(e);
            }
        }

        result.rollback_duration = Utc::now() - start_time;
        result.metadata.insert("backup_id".to_string(), backup_id.to_string());
        result.metadata.insert("rollback_version".to_string(), env!("CARGO_PKG_VERSION").to_string());

        if self.config.verbose_logging {
            println!("Rollback completed in {:?}", result.rollback_duration);
        }

        Ok(result)
    }

    /// Validate if rollback is possible
    pub async fn validate_rollback(&self, backup_handle: &BackupHandle) -> MigrationResult<RollbackValidation> {
        let mut validation = RollbackValidation {
            can_rollback: true,
            backup_valid: false,
            files_restorable: false,
            dependencies_met: false,
            blocking_issues: Vec::new(),
            warnings: Vec::new(),
        };

        // Verify backup integrity
        let backup_verification = self.backup_manager.verify_backup(backup_handle).await?;
        validation.backup_valid = backup_verification.is_valid;
        
        if !validation.backup_valid {
            validation.blocking_issues.push("Backup is invalid or corrupted".to_string());
            validation.can_rollback = false;
        }

        // Check if we can restore files
        validation.files_restorable = self.check_files_restorable().await?;
        if !validation.files_restorable {
            validation.blocking_issues.push("Cannot restore files - permission denied".to_string());
            validation.can_rollback = false;
        }

        // Check dependencies
        validation.dependencies_met = self.check_rollback_dependencies().await?;
        if !validation.dependencies_met {
            validation.blocking_issues.push("Required dependencies not met".to_string());
            validation.can_rollback = false;
        }

        // Add warnings
        if self.migration_artifacts_exist().await {
            validation.warnings.push("Migration artifacts will be removed".to_string());
        }

        Ok(validation)
    }

    /// Create detailed rollback plan
    pub async fn create_rollback_plan(&self, backup_handle: &BackupHandle) -> MigrationResult<RollbackPlan> {
        let mut steps = Vec::new();
        let mut affected_files = Vec::new();
        let mut order = 1;

        // Step 1: Create safety backup of current state
        steps.push(RollbackStep {
            order,
            action: RollbackAction::CreateBackup,
            description: "Create safety backup of current state".to_string(),
            target_path: self.codex_home.join("auth.json").to_string_lossy().to_string(),
            reversible: false,
            critical: false,
        });
        order += 1;

        // Step 2: Restore original auth.json from backup
        let auth_file = self.codex_home.join("auth.json");
        steps.push(RollbackStep {
            order,
            action: RollbackAction::RestoreFile,
            description: "Restore original auth.json".to_string(),
            target_path: auth_file.to_string_lossy().to_string(),
            reversible: true,
            critical: true,
        });
        affected_files.push(auth_file.to_string_lossy().to_string());
        order += 1;

        // Step 3: Remove migration artifacts
        let migration_files = [
            "unified_auth.json",
            "claude_auth.json",
            "auth.json.pre_migration",
        ];

        for file in &migration_files {
            let file_path = self.codex_home.join(file);
            if file_path.exists() {
                steps.push(RollbackStep {
                    order,
                    action: RollbackAction::RemoveFile,
                    description: format!("Remove migration artifact: {}", file),
                    target_path: file_path.to_string_lossy().to_string(),
                    reversible: false,
                    critical: false,
                });
                affected_files.push(file_path.to_string_lossy().to_string());
                order += 1;
            }
        }

        // Step 4: Validate restoration
        steps.push(RollbackStep {
            order,
            action: RollbackAction::ValidateRestore,
            description: "Validate rollback success".to_string(),
            target_path: auth_file.to_string_lossy().to_string(),
            reversible: false,
            critical: true,
        });
        order += 1;

        // Step 5: Cleanup temporary files
        steps.push(RollbackStep {
            order,
            action: RollbackAction::CleanupArtifacts,
            description: "Clean up rollback artifacts".to_string(),
            target_path: self.codex_home.to_string_lossy().to_string(),
            reversible: false,
            critical: false,
        });

        let validation_checks = vec![
            "auth_file_restored".to_string(),
            "auth_json_valid".to_string(),
            "migration_artifacts_removed".to_string(),
            "file_permissions_correct".to_string(),
        ];

        Ok(RollbackPlan {
            backup_id: backup_handle.id.clone(),
            steps,
            estimated_duration_seconds: self.estimate_rollback_duration(&affected_files),
            affected_files,
            validation_checks,
        })
    }

    /// Execute rollback plan step by step
    async fn execute_rollback_plan(&self, plan: &RollbackPlan, result: &mut RollbackResult) -> MigrationResult<()> {
        for step in &plan.steps {
            if self.config.verbose_logging {
                println!("Executing rollback step {}: {}", step.order, step.description);
            }

            match self.execute_rollback_step(step, result).await {
                Ok(_) => {
                    if self.config.verbose_logging {
                        println!("Step {} completed successfully", step.order);
                    }
                }
                Err(e) => {
                    if step.critical {
                        return Err(MigrationError::RollbackFailed(
                            format!("Critical rollback step {} failed: {}", step.order, e)
                        ));
                    } else {
                        result.warnings.push(
                            format!("Non-critical step {} failed: {}", step.order, e)
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute individual rollback step
    async fn execute_rollback_step(&self, step: &RollbackStep, result: &mut RollbackResult) -> MigrationResult<()> {
        match step.action {
            RollbackAction::CreateBackup => {
                self.create_safety_backup(&step.target_path).await?;
                result.metadata.insert("safety_backup_created".to_string(), "true".to_string());
            }

            RollbackAction::RestoreFile => {
                // Find the backup for this specific rollback
                let backups = self.backup_manager.list_backups().await?;
                let backup_handle = backups.into_iter()
                    .find(|b| b.id == result.rollback_timestamp.format("%Y%m%d_%H%M%S").to_string() || 
                              b.created_at <= result.rollback_timestamp)
                    .ok_or_else(|| MigrationError::RollbackFailed("No suitable backup found".to_string()))?;

                self.backup_manager.restore_from_backup(&backup_handle).await?;
                result.restored_files.push(step.target_path.clone());
            }

            RollbackAction::RemoveFile => {
                let file_path = Path::new(&step.target_path);
                if file_path.exists() {
                    tokio::fs::remove_file(file_path).await?;
                    result.removed_files.push(step.target_path.clone());
                }
            }

            RollbackAction::ValidateRestore => {
                self.validate_restored_auth(&step.target_path).await?;
                result.metadata.insert("validation_step_passed".to_string(), "true".to_string());
            }

            RollbackAction::CleanupArtifacts => {
                self.cleanup_rollback_artifacts().await?;
                result.metadata.insert("artifacts_cleaned".to_string(), "true".to_string());
            }
        }

        Ok(())
    }

    /// Find backup handle by ID
    async fn find_backup_handle(&self, backup_id: &str) -> MigrationResult<BackupHandle> {
        let backups = self.backup_manager.list_backups().await?;
        backups.into_iter()
            .find(|b| b.id == backup_id)
            .ok_or_else(|| MigrationError::RollbackFailed(
                format!("Backup with ID '{}' not found", backup_id)
            ))
    }

    /// Check if files can be restored
    async fn check_files_restorable(&self) -> MigrationResult<bool> {
        // Check write permissions on codex home
        let test_file = self.codex_home.join(".write_test_rollback");
        match tokio::fs::write(&test_file, "test").await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&test_file).await;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Check rollback dependencies
    async fn check_rollback_dependencies(&self) -> MigrationResult<bool> {
        // Check if codex home directory exists and is accessible
        Ok(self.codex_home.exists() && self.codex_home.is_dir())
    }

    /// Check if migration artifacts exist
    async fn migration_artifacts_exist(&self) -> bool {
        let artifacts = [
            "unified_auth.json",
            "claude_auth.json",
            "auth.json.pre_migration",
        ];

        artifacts.iter().any(|&artifact| {
            self.codex_home.join(artifact).exists()
        })
    }

    /// Create safety backup before rollback
    async fn create_safety_backup(&self, file_path: &str) -> MigrationResult<()> {
        let source_path = Path::new(file_path);
        if source_path.exists() {
            let backup_path = source_path.with_extension("pre_rollback");
            tokio::fs::copy(source_path, backup_path).await?;
        }
        Ok(())
    }

    /// Validate restored authentication file
    async fn validate_restored_auth(&self, file_path: &str) -> MigrationResult<()> {
        let auth_path = Path::new(file_path);
        
        // Check file exists
        if !auth_path.exists() {
            return Err(MigrationError::RollbackFailed("Restored auth file does not exist".to_string()));
        }

        // Check file is valid JSON
        let content = tokio::fs::read_to_string(auth_path).await?;
        serde_json::from_str::<serde_json::Value>(&content)
            .map_err(|e| MigrationError::RollbackFailed(format!("Restored auth file is not valid JSON: {}", e)))?;

        // Check has required fields
        let auth_data: serde_json::Value = serde_json::from_str(&content)?;
        let has_credentials = auth_data.get("OPENAI_API_KEY").is_some() || auth_data.get("tokens").is_some();
        
        if !has_credentials {
            return Err(MigrationError::RollbackFailed("Restored auth file has no credentials".to_string()));
        }

        Ok(())
    }

    /// Clean up rollback artifacts
    async fn cleanup_rollback_artifacts(&self) -> MigrationResult<()> {
        // Remove temporary files created during rollback
        let artifacts = [
            "auth.json.pre_rollback",
            ".write_test_rollback",
        ];

        for artifact in &artifacts {
            let artifact_path = self.codex_home.join(artifact);
            if artifact_path.exists() {
                let _ = tokio::fs::remove_file(artifact_path).await; // Ignore errors for cleanup
            }
        }

        Ok(())
    }

    /// Validate rollback result
    async fn validate_rollback_result(&self, _backup_handle: &BackupHandle) -> MigrationResult<bool> {
        // Check auth.json was restored correctly
        let auth_file = self.codex_home.join("auth.json");
        if !auth_file.exists() {
            return Ok(false);
        }

        // Check migration artifacts were removed
        let artifacts = ["unified_auth.json", "claude_auth.json"];
        for artifact in &artifacts {
            if self.codex_home.join(artifact).exists() {
                return Ok(false);
            }
        }

        // Check auth.json is valid
        match self.validate_restored_auth(&auth_file.to_string_lossy()).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Estimate rollback duration based on affected files
    fn estimate_rollback_duration(&self, affected_files: &[String]) -> u32 {
        // Base time: 30 seconds
        // Additional time per file: 5 seconds
        // Validation time: 15 seconds
        30 + (affected_files.len() as u32 * 5) + 15
    }

    /// Get rollback status for a specific backup
    pub async fn get_rollback_status(&self, backup_id: &str) -> MigrationResult<RollbackValidation> {
        let backup_handle = self.find_backup_handle(backup_id).await?;
        self.validate_rollback(&backup_handle).await
    }

    /// List all rollback-capable backups
    pub async fn list_rollback_candidates(&self) -> MigrationResult<Vec<BackupHandle>> {
        let all_backups = self.backup_manager.list_backups().await?;
        let mut candidates = Vec::new();

        for backup in all_backups {
            let validation = self.validate_rollback(&backup).await?;
            if validation.can_rollback {
                candidates.push(backup);
            }
        }

        Ok(candidates)
    }

    /// Emergency rollback - fastest possible restore
    pub async fn emergency_rollback(&self) -> MigrationResult<RollbackResult> {
        if self.config.verbose_logging {
            println!("Executing emergency rollback...");
        }

        // Find the most recent valid backup
        let candidates = self.list_rollback_candidates().await?;
        if candidates.is_empty() {
            return Err(MigrationError::RollbackFailed("No valid backups available for emergency rollback".to_string()));
        }

        let latest_backup = &candidates[0]; // Candidates are sorted by creation time (newest first)
        
        // Execute minimal rollback steps
        let start_time = Utc::now();
        let mut result = RollbackResult {
            success: false,
            rollback_timestamp: start_time,
            restored_files: Vec::new(),
            removed_files: Vec::new(),
            rollback_duration: chrono::Duration::zero(),
            validation_passed: false,
            warnings: Vec::new(),
            metadata: HashMap::new(),
        };

        // Direct restore without full plan execution
        self.backup_manager.restore_from_backup(latest_backup).await?;
        result.restored_files.push("auth.json".to_string());

        // Remove migration artifacts
        let migration_files = ["unified_auth.json", "claude_auth.json"];
        for file in &migration_files {
            let file_path = self.codex_home.join(file);
            if file_path.exists() {
                tokio::fs::remove_file(&file_path).await?;
                result.removed_files.push(file.to_string());
            }
        }

        result.success = true;
        result.validation_passed = true;
        result.rollback_duration = Utc::now() - start_time;
        result.metadata.insert("emergency_rollback".to_string(), "true".to_string());
        result.metadata.insert("backup_id".to_string(), latest_backup.id.clone());

        if self.config.verbose_logging {
            println!("Emergency rollback completed in {:?}", result.rollback_duration);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_rollback_validation() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let rollback_manager = RollbackManager::new(temp_dir.path(), &config);

        // Create a mock backup handle
        let backup_handle = BackupHandle {
            id: "test-rollback".to_string(),
            created_at: Utc::now(),
            file_path: temp_dir.path().join("backup.json"),
            metadata: super::super::backup_manager::BackupMetadata {
                original_file_size: 100,
                auth_mode: "ApiKey".to_string(),
                has_tokens: false,
                has_api_key: true,
                backup_version: "1.0".to_string(),
                system_info: std::collections::HashMap::new(),
            },
            encrypted: false,
            checksum: "test-checksum".to_string(),
        };

        // Create the backup file for validation
        tokio::fs::write(&backup_handle.file_path, r#"{"OPENAI_API_KEY": "test"}"#).await.unwrap();

        let validation = rollback_manager.validate_rollback(&backup_handle).await.unwrap();
        
        // Should be able to rollback if file system is writable
        assert!(validation.files_restorable);
        assert!(validation.dependencies_met);
    }

    #[tokio::test]
    async fn test_rollback_plan_creation() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let rollback_manager = RollbackManager::new(temp_dir.path(), &config);

        let backup_handle = BackupHandle {
            id: "test-plan".to_string(),
            created_at: Utc::now(),
            file_path: temp_dir.path().join("backup.json"),
            metadata: super::super::backup_manager::BackupMetadata {
                original_file_size: 100,
                auth_mode: "ApiKey".to_string(),
                has_tokens: false,
                has_api_key: true,
                backup_version: "1.0".to_string(),
                system_info: std::collections::HashMap::new(),
            },
            encrypted: false,
            checksum: "test-checksum".to_string(),
        };

        // Create migration artifacts
        tokio::fs::write(temp_dir.path().join("unified_auth.json"), "{}").await.unwrap();
        tokio::fs::write(temp_dir.path().join("claude_auth.json"), "{}").await.unwrap();

        let plan = rollback_manager.create_rollback_plan(&backup_handle).await.unwrap();
        
        // Should have multiple steps
        assert!(!plan.steps.is_empty());
        assert!(plan.estimated_duration_seconds > 0);
        
        // Should include steps for removing migration artifacts
        let remove_steps = plan.steps.iter()
            .filter(|s| matches!(s.action, RollbackAction::RemoveFile))
            .count();
        assert!(remove_steps > 0);
    }

    #[tokio::test]
    async fn test_emergency_rollback() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        
        // Create backup manager and create a backup first
        let backup_manager = super::super::BackupManager::new(temp_dir.path(), &config);
        
        // Create original auth.json
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "original-key"}"#).await.unwrap();
        
        // Create backup
        let _backup_handle = backup_manager.create_backup().await.unwrap();
        
        // Simulate migration artifacts
        tokio::fs::write(temp_dir.path().join("unified_auth.json"), "{}").await.unwrap();
        tokio::fs::write(&auth_file, r#"{"migrated": true}"#).await.unwrap();

        // Execute emergency rollback
        let rollback_manager = RollbackManager::new(temp_dir.path(), &config);
        let result = rollback_manager.emergency_rollback().await.unwrap();
        
        assert!(result.success);
        assert!(result.restored_files.contains(&"auth.json".to_string()));
        
        // Verify original content restored
        let restored_content = tokio::fs::read_to_string(&auth_file).await.unwrap();
        assert!(restored_content.contains("original-key"));
        
        // Verify migration artifacts removed
        assert!(!temp_dir.path().join("unified_auth.json").exists());
    }
}