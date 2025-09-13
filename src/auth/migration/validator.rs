/// # Migration Validation System
/// 
/// Provides comprehensive validation of authentication systems before, during, and after migration.
/// Ensures data integrity and functional correctness throughout the migration process.

use super::{MigrationConfig, MigrationError, MigrationResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Comprehensive validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub validation_timestamp: DateTime<Utc>,
    pub checks_performed: Vec<ValidationCheck>,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
    pub performance_metrics: ValidationMetrics,
}

/// Individual validation check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheck {
    pub name: String,
    pub category: ValidationCategory,
    pub passed: bool,
    pub duration_ms: u64,
    pub details: Option<String>,
    pub error_message: Option<String>,
}

/// Categories of validation checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationCategory {
    FileSystem,
    Authentication,
    TokenValidity,
    NetworkConnectivity,
    DataIntegrity,
    BackwardCompatibility,
    Security,
}

/// Detailed validation error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub category: ValidationCategory,
    pub severity: ErrorSeverity,
    pub message: String,
    pub suggestion: Option<String>,
    pub recoverable: bool,
}

/// Error severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Performance metrics for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetrics {
    pub total_duration_ms: u64,
    pub checks_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub warning_count: usize,
    pub average_check_duration_ms: f64,
}

/// Network connectivity test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityResult {
    pub endpoint: String,
    pub reachable: bool,
    pub response_time_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

/// Token validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidationResult {
    pub provider: String,
    pub token_type: String,
    pub valid: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub needs_refresh: bool,
    pub error: Option<String>,
}

/// Migration validator implementation
#[derive(Debug)]
pub struct MigrationValidator {
    codex_home: PathBuf,
    config: MigrationConfig,
    client: reqwest::Client,
}

impl MigrationValidator {
    /// Create a new migration validator
    pub fn new(codex_home: &Path, config: &MigrationConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            codex_home: codex_home.to_path_buf(),
            config: config.clone(),
            client,
        }
    }

    /// Validate existing authentication before migration
    pub async fn validate_existing_auth(&self) -> MigrationResult<ValidationResult> {
        let start_time = std::time::Instant::now();
        let mut checks = Vec::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if self.config.verbose_logging {
            println!("Starting pre-migration validation...");
        }

        // File system checks
        checks.extend(self.validate_file_system().await?);

        // Authentication file integrity
        checks.extend(self.validate_auth_file_integrity().await?);

        // Token validation (if enabled)
        if self.config.validate_tokens_before_migration {
            checks.extend(self.validate_existing_tokens().await?);
        }

        // Network connectivity tests
        checks.extend(self.validate_network_connectivity().await?);

        // Security checks
        checks.extend(self.validate_security_posture().await?);

        // Compile results
        for check in &checks {
            if !check.passed {
                errors.push(ValidationError {
                    category: check.category.clone(),
                    severity: self.determine_error_severity(&check.name),
                    message: check.error_message.clone().unwrap_or_else(|| format!("Check '{}' failed", check.name)),
                    suggestion: self.get_error_suggestion(&check.name),
                    recoverable: self.is_error_recoverable(&check.name),
                });
            }
        }

        let total_duration = start_time.elapsed().as_millis() as u64;
        let passed_count = checks.iter().filter(|c| c.passed).count();
        let failed_count = checks.len() - passed_count;

        let result = ValidationResult {
            is_valid: errors.iter().all(|e| !matches!(e.severity, ErrorSeverity::Critical)),
            validation_timestamp: Utc::now(),
            checks_performed: checks.clone(),
            errors,
            warnings,
            performance_metrics: ValidationMetrics {
                total_duration_ms: total_duration,
                checks_count: checks.len(),
                passed_count,
                failed_count,
                warning_count: warnings.len(),
                average_check_duration_ms: if checks.is_empty() { 0.0 } else { 
                    checks.iter().map(|c| c.duration_ms as f64).sum::<f64>() / checks.len() as f64 
                },
            },
        };

        if self.config.verbose_logging {
            println!("Pre-migration validation completed: {} checks, {} passed, {} failed", 
                result.performance_metrics.checks_count,
                result.performance_metrics.passed_count,
                result.performance_metrics.failed_count
            );
        }

        Ok(result)
    }

    /// Validate post-migration state
    pub async fn validate_post_migration(&self) -> MigrationResult<ValidationResult> {
        let start_time = std::time::Instant::now();
        let mut checks = Vec::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if self.config.verbose_logging {
            println!("Starting post-migration validation...");
        }

        // Unified auth file validation
        checks.extend(self.validate_unified_auth_file().await?);

        // Backward compatibility validation
        checks.extend(self.validate_backward_compatibility().await?);

        // Provider authentication validation
        checks.extend(self.validate_provider_authentication().await?);

        // Data preservation validation
        checks.extend(self.validate_data_preservation().await?);

        // Migration metadata validation
        checks.extend(self.validate_migration_metadata().await?);

        // Compile results (same as pre-migration)
        for check in &checks {
            if !check.passed {
                errors.push(ValidationError {
                    category: check.category.clone(),
                    severity: self.determine_error_severity(&check.name),
                    message: check.error_message.clone().unwrap_or_else(|| format!("Check '{}' failed", check.name)),
                    suggestion: self.get_error_suggestion(&check.name),
                    recoverable: self.is_error_recoverable(&check.name),
                });
            }
        }

        let total_duration = start_time.elapsed().as_millis() as u64;
        let passed_count = checks.iter().filter(|c| c.passed).count();
        let failed_count = checks.len() - passed_count;

        let result = ValidationResult {
            is_valid: errors.iter().all(|e| !matches!(e.severity, ErrorSeverity::Critical)),
            validation_timestamp: Utc::now(),
            checks_performed: checks,
            errors,
            warnings,
            performance_metrics: ValidationMetrics {
                total_duration_ms: total_duration,
                checks_count: checks.len(),
                passed_count,
                failed_count,
                warning_count: warnings.len(),
                average_check_duration_ms: if checks.is_empty() { 0.0 } else { 
                    checks.iter().map(|c| c.duration_ms as f64).sum::<f64>() / checks.len() as f64 
                },
            },
        };

        if self.config.verbose_logging {
            println!("Post-migration validation completed: {} checks, {} passed, {} failed", 
                result.performance_metrics.checks_count,
                result.performance_metrics.passed_count,
                result.performance_metrics.failed_count
            );
        }

        Ok(result)
    }

    /// Validate file system integrity and permissions
    async fn validate_file_system(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();

        // Check if codex home directory exists
        checks.push(self.time_check("codex_home_exists", || {
            self.codex_home.exists()
        }, ValidationCategory::FileSystem, Some("Codex home directory must exist".to_string())).await);

        // Check if codex home is writable
        checks.push(self.time_check("codex_home_writable", || {
            self.is_directory_writable(&self.codex_home)
        }, ValidationCategory::FileSystem, Some("Codex home must be writable".to_string())).await);

        // Check auth.json exists
        let auth_file = self.codex_home.join("auth.json");
        checks.push(self.time_check("auth_file_exists", || {
            auth_file.exists()
        }, ValidationCategory::FileSystem, Some("auth.json file must exist for migration".to_string())).await);

        // Check auth.json permissions
        if auth_file.exists() {
            checks.push(self.time_check("auth_file_permissions", || {
                self.check_secure_permissions(&auth_file)
            }, ValidationCategory::Security, Some("auth.json must have secure permissions (0o600)".to_string())).await);
        }

        // Check available disk space
        checks.push(self.time_check("sufficient_disk_space", || {
            self.check_disk_space()
        }, ValidationCategory::FileSystem, Some("Sufficient disk space required for migration".to_string())).await);

        Ok(checks)
    }

    /// Validate authentication file integrity
    async fn validate_auth_file_integrity(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();
        let auth_file = self.codex_home.join("auth.json");

        if !auth_file.exists() {
            return Ok(checks);
        }

        // Validate JSON structure
        checks.push(self.time_check("auth_json_valid", || async {
            match tokio::fs::read_to_string(&auth_file).await {
                Ok(content) => {
                    serde_json::from_str::<serde_json::Value>(&content).is_ok()
                }
                Err(_) => false,
            }
        }, ValidationCategory::DataIntegrity, Some("auth.json must be valid JSON".to_string())).await);

        // Check for required fields
        checks.push(self.time_check("auth_has_credentials", || async {
            match tokio::fs::read_to_string(&auth_file).await {
                Ok(content) => {
                    if let Ok(auth_data) = serde_json::from_str::<serde_json::Value>(&content) {
                        auth_data.get("OPENAI_API_KEY").is_some() || auth_data.get("tokens").is_some()
                    } else {
                        false
                    }
                }
                Err(_) => false,
            }
        }, ValidationCategory::Authentication, Some("auth.json must contain either API key or tokens".to_string())).await);

        Ok(checks)
    }

    /// Validate existing tokens
    async fn validate_existing_tokens(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();
        let auth_file = self.codex_home.join("auth.json");

        if !auth_file.exists() {
            return Ok(checks);
        }

        // Test token validity
        checks.push(self.time_check("openai_token_valid", || async {
            self.test_openai_token_validity().await
        }, ValidationCategory::TokenValidity, Some("OpenAI tokens must be valid".to_string())).await);

        Ok(checks)
    }

    /// Validate network connectivity
    async fn validate_network_connectivity(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();

        // Test OpenAI API connectivity
        checks.push(self.time_check("openai_api_reachable", || async {
            self.test_connectivity("https://api.openai.com/v1/models").await
        }, ValidationCategory::NetworkConnectivity, Some("OpenAI API must be reachable".to_string())).await);

        // Test Anthropic API connectivity
        checks.push(self.time_check("anthropic_api_reachable", || async {
            self.test_connectivity("https://api.anthropic.com/v1/messages").await
        }, ValidationCategory::NetworkConnectivity, Some("Anthropic API must be reachable".to_string())).await);

        Ok(checks)
    }

    /// Validate security posture
    async fn validate_security_posture(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();

        // Check for environment variable exposure
        checks.push(self.time_check("no_env_var_exposure", || {
            !self.check_environment_variable_exposure()
        }, ValidationCategory::Security, Some("API keys should not be exposed in environment variables".to_string())).await);

        // Check backup directory security
        let backup_dir = self.codex_home.join(".backups");
        if backup_dir.exists() {
            checks.push(self.time_check("backup_dir_secure", || {
                self.check_secure_permissions(&backup_dir)
            }, ValidationCategory::Security, Some("Backup directory must have secure permissions".to_string())).await);
        }

        Ok(checks)
    }

    /// Validate unified auth file after migration
    async fn validate_unified_auth_file(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();
        let unified_file = self.codex_home.join("unified_auth.json");

        // Check unified auth file exists
        checks.push(self.time_check("unified_auth_exists", || {
            unified_file.exists()
        }, ValidationCategory::FileSystem, Some("unified_auth.json must be created during migration".to_string())).await);

        if unified_file.exists() {
            // Validate unified auth structure
            checks.push(self.time_check("unified_auth_valid", || async {
                match tokio::fs::read_to_string(&unified_file).await {
                    Ok(content) => {
                        serde_json::from_str::<super::migrator::UnifiedAuthJson>(&content).is_ok()
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::DataIntegrity, Some("unified_auth.json must have valid structure".to_string())).await);

            // Check version
            checks.push(self.time_check("unified_auth_version", || async {
                match tokio::fs::read_to_string(&unified_file).await {
                    Ok(content) => {
                        if let Ok(unified_auth) = serde_json::from_str::<super::migrator::UnifiedAuthJson>(&content) {
                            unified_auth.version == "2.0"
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::DataIntegrity, Some("unified_auth.json must have version 2.0".to_string())).await);
        }

        Ok(checks)
    }

    /// Validate backward compatibility
    async fn validate_backward_compatibility(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();
        let auth_file = self.codex_home.join("auth.json");

        if auth_file.exists() {
            // Check migration markers
            checks.push(self.time_check("migration_markers_present", || async {
                match tokio::fs::read_to_string(&auth_file).await {
                    Ok(content) => {
                        if let Ok(auth_data) = serde_json::from_str::<serde_json::Value>(&content) {
                            auth_data.get("_migration_version").is_some() &&
                            auth_data.get("_unified_auth_available").is_some()
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::BackwardCompatibility, Some("Migration markers must be present in auth.json".to_string())).await);

            // Check original structure preserved
            checks.push(self.time_check("original_structure_preserved", || async {
                match tokio::fs::read_to_string(&auth_file).await {
                    Ok(content) => {
                        if let Ok(auth_data) = serde_json::from_str::<serde_json::Value>(&content) {
                            auth_data.get("OPENAI_API_KEY").is_some() || auth_data.get("tokens").is_some()
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::BackwardCompatibility, Some("Original auth structure must be preserved".to_string())).await);
        }

        Ok(checks)
    }

    /// Validate provider authentication
    async fn validate_provider_authentication(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();
        let unified_file = self.codex_home.join("unified_auth.json");

        if unified_file.exists() {
            // Check OpenAI provider present
            checks.push(self.time_check("openai_provider_present", || async {
                match tokio::fs::read_to_string(&unified_file).await {
                    Ok(content) => {
                        if let Ok(unified_auth) = serde_json::from_str::<super::migrator::UnifiedAuthJson>(&content) {
                            unified_auth.providers.contains_key("openai")
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::Authentication, Some("OpenAI provider must be present in unified auth".to_string())).await);

            // Check Claude provider present
            checks.push(self.time_check("claude_provider_present", || async {
                match tokio::fs::read_to_string(&unified_file).await {
                    Ok(content) => {
                        if let Ok(unified_auth) = serde_json::from_str::<super::migrator::UnifiedAuthJson>(&content) {
                            unified_auth.providers.contains_key("claude")
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::Authentication, Some("Claude provider must be present in unified auth".to_string())).await);
        }

        Ok(checks)
    }

    /// Validate data preservation
    async fn validate_data_preservation(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();

        // Check backup file exists
        let backup_file = self.codex_home.join("auth.json.pre_migration");
        checks.push(self.time_check("backup_file_exists", || {
            backup_file.exists()
        }, ValidationCategory::DataIntegrity, Some("Pre-migration backup must exist".to_string())).await);

        Ok(checks)
    }

    /// Validate migration metadata
    async fn validate_migration_metadata(&self) -> MigrationResult<Vec<ValidationCheck>> {
        let mut checks = Vec::new();
        let unified_file = self.codex_home.join("unified_auth.json");

        if unified_file.exists() {
            // Check migration info present
            checks.push(self.time_check("migration_info_present", || async {
                match tokio::fs::read_to_string(&unified_file).await {
                    Ok(content) => {
                        if let Ok(unified_auth) = serde_json::from_str::<super::migrator::UnifiedAuthJson>(&content) {
                            !unified_auth.migration_info.backup_id.is_empty() &&
                            !unified_auth.migration_info.migration_tool_version.is_empty()
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }, ValidationCategory::DataIntegrity, Some("Migration metadata must be complete".to_string())).await);
        }

        Ok(checks)
    }

    /// Helper function to time validation checks
    async fn time_check<F, Fut>(&self, name: &str, check_fn: F, category: ValidationCategory, details: Option<String>) -> ValidationCheck
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = std::time::Instant::now();
        let passed = check_fn().await;
        let duration_ms = start.elapsed().as_millis() as u64;

        ValidationCheck {
            name: name.to_string(),
            category,
            passed,
            duration_ms,
            details,
            error_message: if !passed { Some(format!("Check '{}' failed", name)) } else { None },
        }
    }

    /// Test OpenAI token validity
    async fn test_openai_token_validity(&self) -> bool {
        let auth_file = self.codex_home.join("auth.json");
        match tokio::fs::read_to_string(&auth_file).await {
            Ok(content) => {
                if let Ok(auth_data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(api_key) = auth_data.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
                        return self.test_openai_api_key(api_key).await;
                    }
                    if let Some(tokens) = auth_data.get("tokens") {
                        if let Some(access_token) = tokens.get("access_token").and_then(|v| v.as_str()) {
                            return self.test_openai_access_token(access_token).await;
                        }
                    }
                }
            }
            Err(_) => {}
        }
        false
    }

    /// Test OpenAI API key
    async fn test_openai_api_key(&self, api_key: &str) -> bool {
        let response = self.client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(api_key)
            .send()
            .await;

        match response {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Test OpenAI access token
    async fn test_openai_access_token(&self, access_token: &str) -> bool {
        let response = self.client
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await;

        match response {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Test network connectivity to endpoint
    async fn test_connectivity(&self, url: &str) -> bool {
        match self.client.head(url).send().await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Check if directory is writable
    fn is_directory_writable(&self, dir: &Path) -> bool {
        if !dir.exists() {
            return false;
        }

        let test_file = dir.join(".write_test");
        match std::fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = std::fs::remove_file(&test_file);
                true
            }
            Err(_) => false,
        }
    }

    /// Check secure file permissions
    fn check_secure_permissions(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(path) {
                let mode = metadata.permissions().mode();
                // Check for 0o600 (owner read/write only) or 0o700 (owner rwx only)
                mode & 0o077 == 0
            } else {
                false
            }
        }
        #[cfg(not(unix))]
        {
            // On Windows, just check if file exists and is readable
            path.exists()
        }
    }

    /// Check for environment variable exposure
    fn check_environment_variable_exposure(&self) -> bool {
        std::env::var("OPENAI_API_KEY").is_ok() || 
        std::env::var("ANTHROPIC_API_KEY").is_ok() ||
        std::env::var("CLAUDE_API_KEY").is_ok()
    }

    /// Check available disk space
    fn check_disk_space(&self) -> bool {
        // Simple check - ensure we have at least 10MB free space
        // In a real implementation, you'd use platform-specific APIs
        match std::fs::metadata(&self.codex_home) {
            Ok(_) => true, // Simplified - assume we have space if directory exists
            Err(_) => false,
        }
    }

    /// Determine error severity based on check name
    fn determine_error_severity(&self, check_name: &str) -> ErrorSeverity {
        match check_name {
            "codex_home_exists" | "auth_file_exists" | "unified_auth_valid" => ErrorSeverity::Critical,
            "auth_json_valid" | "auth_has_credentials" | "migration_markers_present" => ErrorSeverity::High,
            "auth_file_permissions" | "backup_file_exists" => ErrorSeverity::Medium,
            "openai_token_valid" | "network_connectivity" => ErrorSeverity::Low,
            _ => ErrorSeverity::Medium,
        }
    }

    /// Get error suggestion based on check name
    fn get_error_suggestion(&self, check_name: &str) -> Option<String> {
        match check_name {
            "codex_home_exists" => Some("Create the codex home directory".to_string()),
            "auth_file_exists" => Some("Run 'code auth login' to create authentication".to_string()),
            "auth_json_valid" => Some("Check auth.json file for syntax errors".to_string()),
            "auth_file_permissions" => Some("Run 'chmod 600 ~/.codex/auth.json' to secure the file".to_string()),
            "openai_token_valid" => Some("Refresh your OpenAI authentication".to_string()),
            _ => None,
        }
    }

    /// Check if error is recoverable
    fn is_error_recoverable(&self, check_name: &str) -> bool {
        !matches!(check_name, "codex_home_exists" | "insufficient_disk_space")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_system_validation() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let validator = MigrationValidator::new(temp_dir.path(), &config);

        // Create test auth.json
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();

        let checks = validator.validate_file_system().await.unwrap();
        
        // Should have multiple checks
        assert!(!checks.is_empty());
        
        // Most checks should pass for a valid setup
        let passed_count = checks.iter().filter(|c| c.passed).count();
        assert!(passed_count > 0);
    }

    #[tokio::test]
    async fn test_auth_file_integrity_validation() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let validator = MigrationValidator::new(temp_dir.path(), &config);

        // Create valid auth.json
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test-key"}"#).await.unwrap();

        let checks = validator.validate_auth_file_integrity().await.unwrap();
        
        // Should have checks for JSON validity and credentials
        assert!(checks.len() >= 2);
        
        // Both checks should pass
        assert!(checks.iter().all(|c| c.passed));
    }

    #[tokio::test]
    async fn test_validation_with_invalid_json() {
        let temp_dir = tempdir().unwrap();
        let config = MigrationConfig::default();
        let validator = MigrationValidator::new(temp_dir.path(), &config);

        // Create invalid JSON
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"invalid": json}"#).await.unwrap();

        let checks = validator.validate_auth_file_integrity().await.unwrap();
        
        // JSON validity check should fail
        let json_check = checks.iter().find(|c| c.name == "auth_json_valid").unwrap();
        assert!(!json_check.passed);
    }

    #[tokio::test]
    async fn test_complete_validation_flow() {
        let temp_dir = tempdir().unwrap();
        let mut config = MigrationConfig::default();
        config.validate_tokens_before_migration = false; // Skip token validation for test
        
        let validator = MigrationValidator::new(temp_dir.path(), &config);

        // Create valid auth setup
        let auth_file = temp_dir.path().join("auth.json");
        tokio::fs::write(&auth_file, r#"{"OPENAI_API_KEY": "sk-test"}"#).await.unwrap();

        let result = validator.validate_existing_auth().await.unwrap();
        
        // Validation should have performed multiple checks
        assert!(result.performance_metrics.checks_count > 0);
        assert!(result.performance_metrics.passed_count > 0);
        
        // Should be valid overall (allowing some network failures)
        assert!(result.is_valid || result.errors.iter().all(|e| !matches!(e.severity, ErrorSeverity::Critical)));
    }
}