//! Security module for Claude authentication integration
//! 
//! This module provides comprehensive security features including:
//! - Enhanced token storage with encryption
//! - OAuth security with PKCE and state validation
//! - Security audit logging
//! - Session security with token rotation
//! - Environment variable security

pub mod secure_token_storage;
pub mod oauth_security;
pub mod audit_logger;
pub mod session_security;

pub use secure_token_storage::{SecureTokenStorage, SecureStorageError};
pub use oauth_security::{SecureOAuthFlow, OAuthSecurityManager, OAuthSecurityError};
pub use audit_logger::{SecurityAuditLogger, AuditEvent, AuthEventType, Severity};
pub use session_security::{SessionSecurityManager, SecureSession, SessionSecurityError};

use std::path::PathBuf;
use thiserror::Error;

/// Comprehensive security error type
#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Storage security error: {0}")]
    Storage(#[from] SecureStorageError),
    #[error("OAuth security error: {0}")]
    OAuth(#[from] OAuthSecurityError),
    #[error("Audit logging error: {0}")]
    Audit(#[from] audit_logger::AuditLogError),
    #[error("Session security error: {0}")]
    Session(#[from] SessionSecurityError),
    #[error("Environment security error: {0}")]
    Environment(String),
}

/// Security configuration for the authentication system
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub token_storage_path: PathBuf,
    pub audit_log_path: PathBuf,
    pub enable_encryption: bool,
    pub enable_audit_logging: bool,
    pub require_pkce: bool,
    pub token_rotation_enabled: bool,
    pub max_concurrent_oauth_flows: usize,
    pub session_timeout_minutes: i64,
    pub require_secure_transport: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            token_storage_path: dirs::home_dir()
                .unwrap_or_default()
                .join(".codex")
                .join("secure_tokens.json"),
            audit_log_path: dirs::home_dir()
                .unwrap_or_default()
                .join(".codex")
                .join("security_audit.log"),
            enable_encryption: true,
            enable_audit_logging: true,
            require_pkce: true,
            token_rotation_enabled: true,
            max_concurrent_oauth_flows: 3,
            session_timeout_minutes: 60,
            require_secure_transport: true,
        }
    }
}

/// Unified security manager that coordinates all security components
pub struct SecurityManager {
    config: SecurityConfig,
    token_storage: Option<SecureTokenStorage>,
    oauth_manager: Option<OAuthSecurityManager>,
    session_manager: Option<SessionSecurityManager>,
}

impl SecurityManager {
    /// Create new security manager with configuration
    pub fn new(config: SecurityConfig) -> Result<Self, SecurityError> {
        let mut manager = Self {
            config: config.clone(),
            token_storage: None,
            oauth_manager: None,
            session_manager: None,
        };

        // Initialize components based on configuration
        if config.enable_encryption {
            manager.token_storage = Some(SecureTokenStorage::new(config.token_storage_path.clone())?);
        }

        if config.require_pkce {
            manager.oauth_manager = Some(OAuthSecurityManager::new(config.max_concurrent_oauth_flows));
        }

        if config.token_rotation_enabled {
            let session_config = session_security::SessionConfig {
                access_token_lifetime: chrono::Duration::minutes(config.session_timeout_minutes),
                ..Default::default()
            };
            manager.session_manager = Some(SessionSecurityManager::new(session_config));
        }

        // Initialize audit logging if enabled
        if config.enable_audit_logging {
            audit_logger::init_audit_logger(config.audit_log_path.clone())?;
        }

        Ok(manager)
    }

    /// Get token storage instance
    pub fn token_storage(&self) -> Option<&SecureTokenStorage> {
        self.token_storage.as_ref()
    }

    /// Get OAuth security manager
    pub fn oauth_manager(&mut self) -> Option<&mut OAuthSecurityManager> {
        self.oauth_manager.as_mut()
    }

    /// Get session security manager
    pub fn session_manager(&self) -> Option<&SessionSecurityManager> {
        self.session_manager.as_ref()
    }

    /// Validate environment security
    pub fn validate_environment(&self) -> Result<(), SecurityError> {
        // Check for insecure environment variables
        let insecure_vars = [
            "ANTHROPIC_API_KEY",
            "CLAUDE_API_KEY", 
            "OPENAI_API_KEY",
        ];

        for var in &insecure_vars {
            if let Ok(value) = std::env::var(var) {
                if !value.is_empty() {
                    // Log warning about environment variable usage
                    let event = AuditEvent {
                        timestamp: chrono::Utc::now(),
                        event_type: AuthEventType::SecurityViolation,
                        user_id: None,
                        session_id: None,
                        client_id: None,
                        ip_address: None,
                        user_agent: None,
                        success: false,
                        error_message: Some(format!("Insecure environment variable detected: {}", var)),
                        metadata: serde_json::json!({
                            "variable": var,
                            "recommendation": "Use secure token storage instead"
                        }),
                        severity: Severity::Warning,
                    };
                    
                    audit_logger::log_audit_event(event).ok();
                }
            }
        }

        // Validate transport security in production
        if self.config.require_secure_transport {
            // This would typically check for HTTPS enforcement
            // For now, we'll just log a warning if running in insecure mode
            if std::env::var("CODEX_INSECURE_MODE").is_ok() {
                let event = AuditEvent {
                    timestamp: chrono::Utc::now(),
                    event_type: AuthEventType::SecurityViolation,
                    user_id: None,
                    session_id: None,
                    client_id: None,
                    ip_address: None,
                    user_agent: None,
                    success: false,
                    error_message: Some("Running in insecure mode".to_string()),
                    metadata: serde_json::json!({
                        "recommendation": "Remove CODEX_INSECURE_MODE in production"
                    }),
                    severity: Severity::Warning,
                };
                
                audit_logger::log_audit_event(event).ok();
            }
        }

        Ok(())
    }

    /// Perform security health check
    pub fn security_health_check(&self) -> SecurityHealthReport {
        let mut report = SecurityHealthReport::default();

        // Check token storage
        if let Some(storage) = &self.token_storage {
            report.token_storage_secure = storage.tokens_exist();
        }

        // Check audit logging
        if self.config.enable_audit_logging {
            report.audit_logging_enabled = true;
        }

        // Check OAuth security
        if let Some(oauth_manager) = &self.oauth_manager {
            report.oauth_security_enabled = true;
            report.active_oauth_flows = oauth_manager.active_flow_count();
        }

        // Check session security
        if let Some(session_manager) = &self.session_manager {
            let stats = session_manager.get_session_stats();
            report.session_security_enabled = true;
            report.active_sessions = stats.active_sessions;
            report.suspicious_sessions = stats.suspicious_sessions;
        }

        report
    }
}

/// Security health report
#[derive(Debug, Default)]
pub struct SecurityHealthReport {
    pub token_storage_secure: bool,
    pub audit_logging_enabled: bool,
    pub oauth_security_enabled: bool,
    pub session_security_enabled: bool,
    pub active_oauth_flows: usize,
    pub active_sessions: usize,
    pub suspicious_sessions: usize,
    pub security_violations_24h: u64,
}

/// Initialize security subsystem with default configuration
pub fn init_security() -> Result<SecurityManager, SecurityError> {
    let config = SecurityConfig::default();
    SecurityManager::new(config)
}

/// Initialize security subsystem with custom configuration
pub fn init_security_with_config(config: SecurityConfig) -> Result<SecurityManager, SecurityError> {
    SecurityManager::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_security_manager_creation() {
        let temp_dir = tempdir().unwrap();
        
        let config = SecurityConfig {
            token_storage_path: temp_dir.path().join("tokens.json"),
            audit_log_path: temp_dir.path().join("audit.log"),
            ..Default::default()
        };

        let manager = SecurityManager::new(config).unwrap();
        assert!(manager.token_storage.is_some());
        assert!(manager.oauth_manager.is_some());
        assert!(manager.session_manager.is_some());
    }

    #[test]
    fn test_security_health_check() {
        let temp_dir = tempdir().unwrap();
        
        let config = SecurityConfig {
            token_storage_path: temp_dir.path().join("tokens.json"),
            audit_log_path: temp_dir.path().join("audit.log"),
            ..Default::default()
        };

        let manager = SecurityManager::new(config).unwrap();
        let report = manager.security_health_check();
        
        assert!(report.audit_logging_enabled);
        assert!(report.oauth_security_enabled);
        assert!(report.session_security_enabled);
    }

    #[test]
    fn test_environment_validation() {
        let temp_dir = tempdir().unwrap();
        
        let config = SecurityConfig {
            token_storage_path: temp_dir.path().join("tokens.json"),
            audit_log_path: temp_dir.path().join("audit.log"),
            ..Default::default()
        };

        let manager = SecurityManager::new(config).unwrap();
        
        // This should not fail even if environment variables are set
        assert!(manager.validate_environment().is_ok());
    }
}