use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Security audit logging for authentication events
#[derive(Debug)]
pub struct SecurityAuditLogger {
    log_file: PathBuf,
    max_log_size: u64,
    max_log_files: usize,
    buffer: Vec<AuditEvent>,
}

#[derive(Debug, Error)]
pub enum AuditLogError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Log rotation failed: {0}")]
    LogRotation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: AuthEventType,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub client_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthEventType {
    Login,
    Logout,
    TokenRefresh,
    TokenExpired,
    OAuthStart,
    OAuthCallback,
    OAuthError,
    ApiKeyAuth,
    PermissionDenied,
    SecurityViolation,
    SessionCreated,
    SessionDestroyed,
    PasswordReset,
    AccountLocked,
    TwoFactorAuth,
    SuspiciousActivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMetrics {
    pub total_events: u64,
    pub failed_logins: u64,
    pub successful_logins: u64,
    pub token_refreshes: u64,
    pub security_violations: u64,
    pub time_range_start: DateTime<Utc>,
    pub time_range_end: DateTime<Utc>,
}

impl SecurityAuditLogger {
    /// Create new security audit logger
    pub fn new(log_file: PathBuf) -> Result<Self, AuditLogError> {
        // Ensure log directory exists
        if let Some(parent) = log_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            log_file,
            max_log_size: 10 * 1024 * 1024, // 10MB
            max_log_files: 5,
            buffer: Vec::new(),
        })
    }

    /// Log authentication event
    pub fn log_auth_event(&mut self, mut event: AuditEvent) -> Result<(), AuditLogError> {
        // Ensure timestamp is set
        if event.timestamp.timestamp() == 0 {
            event.timestamp = Utc::now();
        }

        // Add to buffer
        self.buffer.push(event.clone());

        // Write to file immediately for critical events
        if matches!(event.severity, Severity::Critical | Severity::Error) {
            self.flush_buffer()?;
        }

        // Check if log rotation is needed
        self.check_log_rotation()?;

        Ok(())
    }

    /// Log successful login
    pub fn log_login_success(
        &mut self,
        user_id: Option<String>,
        session_id: Option<String>,
        client_id: Option<String>,
        ip_address: Option<String>,
    ) -> Result<(), AuditLogError> {
        let event = AuditEvent {
            timestamp: Utc::now(),
            event_type: AuthEventType::Login,
            user_id,
            session_id,
            client_id,
            ip_address,
            user_agent: None,
            success: true,
            error_message: None,
            metadata: serde_json::json!({}),
            severity: Severity::Info,
        };

        self.log_auth_event(event)
    }

    /// Log failed login
    pub fn log_login_failure(
        &mut self,
        user_id: Option<String>,
        error: &str,
        ip_address: Option<String>,
        client_id: Option<String>,
    ) -> Result<(), AuditLogError> {
        let event = AuditEvent {
            timestamp: Utc::now(),
            event_type: AuthEventType::Login,
            user_id,
            session_id: None,
            client_id,
            ip_address,
            user_agent: None,
            success: false,
            error_message: Some(error.to_string()),
            metadata: serde_json::json!({}),
            severity: Severity::Warning,
        };

        self.log_auth_event(event)
    }

    /// Log OAuth event
    pub fn log_oauth_event(
        &mut self,
        event_type: AuthEventType,
        session_id: Option<String>,
        client_id: Option<String>,
        success: bool,
        error: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), AuditLogError> {
        let severity = if success {
            Severity::Info
        } else {
            match event_type {
                AuthEventType::OAuthError => Severity::Error,
                AuthEventType::SecurityViolation => Severity::Critical,
                _ => Severity::Warning,
            }
        };

        let event = AuditEvent {
            timestamp: Utc::now(),
            event_type,
            user_id: None,
            session_id,
            client_id,
            ip_address: None,
            user_agent: None,
            success,
            error_message: error,
            metadata: metadata.unwrap_or(serde_json::json!({})),
            severity,
        };

        self.log_auth_event(event)
    }

    /// Log security violation
    pub fn log_security_violation(
        &mut self,
        violation_type: &str,
        user_id: Option<String>,
        session_id: Option<String>,
        details: &str,
    ) -> Result<(), AuditLogError> {
        let event = AuditEvent {
            timestamp: Utc::now(),
            event_type: AuthEventType::SecurityViolation,
            user_id,
            session_id,
            client_id: None,
            ip_address: None,
            user_agent: None,
            success: false,
            error_message: Some(details.to_string()),
            metadata: serde_json::json!({
                "violation_type": violation_type
            }),
            severity: Severity::Critical,
        };

        self.log_auth_event(event)
    }

    /// Log token refresh event
    pub fn log_token_refresh(
        &mut self,
        user_id: Option<String>,
        session_id: Option<String>,
        success: bool,
        error: Option<String>,
    ) -> Result<(), AuditLogError> {
        let event = AuditEvent {
            timestamp: Utc::now(),
            event_type: AuthEventType::TokenRefresh,
            user_id,
            session_id,
            client_id: None,
            ip_address: None,
            user_agent: None,
            success,
            error_message: error,
            metadata: serde_json::json!({}),
            severity: if success { Severity::Info } else { Severity::Warning },
        };

        self.log_auth_event(event)
    }

    /// Flush buffered events to disk
    pub fn flush_buffer(&mut self) -> Result<(), AuditLogError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut file = self.open_log_file()?;
        
        for event in &self.buffer {
            let log_line = serde_json::to_string(event)?;
            writeln!(file, "{}", log_line)?;
        }
        
        file.flush()?;
        self.buffer.clear();
        
        Ok(())
    }

    /// Generate security metrics from log file
    pub fn generate_metrics(&self, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> Result<SecurityMetrics, AuditLogError> {
        let mut metrics = SecurityMetrics {
            total_events: 0,
            failed_logins: 0,
            successful_logins: 0,
            token_refreshes: 0,
            security_violations: 0,
            time_range_start: start_time,
            time_range_end: end_time,
        };

        if !self.log_file.exists() {
            return Ok(metrics);
        }

        let content = std::fs::read_to_string(&self.log_file)?;
        
        for line in content.lines() {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                if event.timestamp >= start_time && event.timestamp <= end_time {
                    metrics.total_events += 1;
                    
                    match event.event_type {
                        AuthEventType::Login if event.success => metrics.successful_logins += 1,
                        AuthEventType::Login if !event.success => metrics.failed_logins += 1,
                        AuthEventType::TokenRefresh => metrics.token_refreshes += 1,
                        AuthEventType::SecurityViolation => metrics.security_violations += 1,
                        _ => {}
                    }
                }
            }
        }

        Ok(metrics)
    }

    /// Get recent security events
    pub fn get_recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, AuditLogError> {
        let mut events = Vec::new();
        
        if !self.log_file.exists() {
            return Ok(events);
        }

        let content = std::fs::read_to_string(&self.log_file)?;
        let lines: Vec<&str> = content.lines().rev().take(limit).collect();
        
        for line in lines.iter().rev() {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Check if log rotation is needed
    fn check_log_rotation(&self) -> Result<(), AuditLogError> {
        if !self.log_file.exists() {
            return Ok(());
        }

        let metadata = std::fs::metadata(&self.log_file)?;
        if metadata.len() > self.max_log_size {
            self.rotate_logs()?;
        }

        Ok(())
    }

    /// Rotate log files
    fn rotate_logs(&self) -> Result<(), AuditLogError> {
        let log_dir = self.log_file.parent()
            .ok_or_else(|| AuditLogError::LogRotation("Invalid log file path".to_string()))?;
        
        let log_name = self.log_file.file_stem()
            .ok_or_else(|| AuditLogError::LogRotation("Invalid log file name".to_string()))?
            .to_string_lossy();
        
        let log_ext = self.log_file.extension()
            .unwrap_or_default()
            .to_string_lossy();

        // Rotate existing log files
        for i in (1..self.max_log_files).rev() {
            let old_file = log_dir.join(format!("{}.{}.{}", log_name, i, log_ext));
            let new_file = log_dir.join(format!("{}.{}.{}", log_name, i + 1, log_ext));
            
            if old_file.exists() {
                std::fs::rename(&old_file, &new_file)?;
            }
        }

        // Move current log to .1
        let first_rotated = log_dir.join(format!("{}.1.{}", log_name, log_ext));
        std::fs::rename(&self.log_file, &first_rotated)?;

        Ok(())
    }

    /// Open log file with secure permissions
    fn open_log_file(&self) -> Result<File, AuditLogError> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);

        #[cfg(unix)]
        {
            options.mode(0o600); // Read/write for owner only
        }

        Ok(options.open(&self.log_file)?)
    }
}

/// Global security audit logger instance
lazy_static::lazy_static! {
    static ref GLOBAL_AUDIT_LOGGER: std::sync::Mutex<Option<SecurityAuditLogger>> = 
        std::sync::Mutex::new(None);
}

/// Initialize global audit logger
pub fn init_audit_logger(log_file: PathBuf) -> Result<(), AuditLogError> {
    let logger = SecurityAuditLogger::new(log_file)?;
    let mut global_logger = GLOBAL_AUDIT_LOGGER.lock().unwrap();
    *global_logger = Some(logger);
    Ok(())
}

/// Log event using global logger
pub fn log_audit_event(event: AuditEvent) -> Result<(), AuditLogError> {
    let mut global_logger = GLOBAL_AUDIT_LOGGER.lock().unwrap();
    if let Some(ref mut logger) = *global_logger {
        logger.log_auth_event(event)?;
    }
    Ok(())
}

/// Convenience function to log login success
pub fn log_login_success(
    user_id: Option<String>,
    session_id: Option<String>,
    client_id: Option<String>,
    ip_address: Option<String>,
) -> Result<(), AuditLogError> {
    let mut global_logger = GLOBAL_AUDIT_LOGGER.lock().unwrap();
    if let Some(ref mut logger) = *global_logger {
        logger.log_login_success(user_id, session_id, client_id, ip_address)?;
    }
    Ok(())
}

/// Convenience function to log security violation
pub fn log_security_violation(
    violation_type: &str,
    user_id: Option<String>,
    session_id: Option<String>,
    details: &str,
) -> Result<(), AuditLogError> {
    let mut global_logger = GLOBAL_AUDIT_LOGGER.lock().unwrap();
    if let Some(ref mut logger) = *global_logger {
        logger.log_security_violation(violation_type, user_id, session_id, details)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_audit_logger_creation() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("audit.log");
        
        let logger = SecurityAuditLogger::new(log_file).unwrap();
        assert!(logger.buffer.is_empty());
    }

    #[test]
    fn test_logging_events() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("audit.log");
        
        let mut logger = SecurityAuditLogger::new(log_file.clone()).unwrap();
        
        logger.log_login_success(
            Some("user123".to_string()),
            Some("session456".to_string()),
            Some("client789".to_string()),
            Some("192.168.1.1".to_string()),
        ).unwrap();
        
        logger.flush_buffer().unwrap();
        
        assert!(log_file.exists());
        let content = std::fs::read_to_string(&log_file).unwrap();
        assert!(content.contains("user123"));
        assert!(content.contains("Login"));
    }

    #[test]
    fn test_security_metrics() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("audit.log");
        
        let mut logger = SecurityAuditLogger::new(log_file).unwrap();
        
        let start_time = Utc::now() - chrono::Duration::hours(1);
        let end_time = Utc::now() + chrono::Duration::hours(1);
        
        // Log some events
        logger.log_login_success(Some("user1".to_string()), None, None, None).unwrap();
        logger.log_login_failure(Some("user2".to_string()), "Invalid password", None, None).unwrap();
        logger.log_security_violation("PKCE violation", Some("user3".to_string()), None, "Invalid PKCE verifier").unwrap();
        
        logger.flush_buffer().unwrap();
        
        let metrics = logger.generate_metrics(start_time, end_time).unwrap();
        assert_eq!(metrics.total_events, 3);
        assert_eq!(metrics.successful_logins, 1);
        assert_eq!(metrics.failed_logins, 1);
        assert_eq!(metrics.security_violations, 1);
    }
}