use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration};
use thiserror::Error;
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

/// Enhanced session security with token rotation and secure session management
#[derive(Debug)]
pub struct SessionSecurityManager {
    sessions: Arc<RwLock<HashMap<String, SecureSession>>>,
    config: SessionConfig,
}

#[derive(Debug, Error)]
pub enum SessionSecurityError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Session expired: {0}")]
    SessionExpired(String),
    #[error("Invalid session token")]
    InvalidToken,
    #[error("Session rotation required")]
    RotationRequired,
    #[error("Concurrent session limit exceeded")]
    ConcurrentLimitExceeded,
    #[error("Session security violation: {0}")]
    SecurityViolation(String),
    #[error("Token validation failed: {0}")]
    TokenValidationFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureSession {
    pub session_id: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub refresh_expires_at: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub rotation_count: u32,
    pub security_flags: SessionSecurityFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSecurityFlags {
    pub requires_mfa: bool,
    pub is_suspicious: bool,
    pub force_rotation: bool,
    pub restricted_access: bool,
    pub high_privilege: bool,
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub access_token_lifetime: Duration,
    pub refresh_token_lifetime: Duration,
    pub rotation_threshold: Duration,
    pub max_concurrent_sessions: usize,
    pub require_ip_consistency: bool,
    pub require_user_agent_consistency: bool,
    pub max_rotation_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRotationResult {
    pub new_access_token: String,
    pub new_refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub rotation_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionValidationContext {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub requested_scopes: Vec<String>,
    pub current_time: DateTime<Utc>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            access_token_lifetime: Duration::hours(1),
            refresh_token_lifetime: Duration::days(30),
            rotation_threshold: Duration::minutes(30),
            max_concurrent_sessions: 5,
            require_ip_consistency: false, // Disabled by default for dev environments
            require_user_agent_consistency: false,
            max_rotation_count: 100,
        }
    }
}

impl Default for SessionSecurityFlags {
    fn default() -> Self {
        Self {
            requires_mfa: false,
            is_suspicious: false,
            force_rotation: false,
            restricted_access: false,
            high_privilege: false,
        }
    }
}

impl SessionSecurityManager {
    /// Create new session security manager
    pub fn new(config: SessionConfig) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Create a new secure session
    pub fn create_session(
        &self,
        user_id: String,
        client_id: String,
        scopes: Vec<String>,
        context: &SessionValidationContext,
    ) -> Result<SecureSession, SessionSecurityError> {
        // Check concurrent session limit
        self.cleanup_expired_sessions();
        {
            let sessions = self.sessions.read().unwrap();
            let user_sessions: Vec<_> = sessions
                .values()
                .filter(|s| s.user_id == user_id)
                .collect();
            
            if user_sessions.len() >= self.config.max_concurrent_sessions {
                return Err(SessionSecurityError::ConcurrentLimitExceeded);
            }
        }

        let now = context.current_time;
        let session_id = Self::generate_session_id();
        let access_token = Self::generate_token();
        let refresh_token = Self::generate_token();

        let session = SecureSession {
            session_id: session_id.clone(),
            user_id,
            access_token,
            refresh_token,
            created_at: now,
            last_accessed: now,
            expires_at: now + self.config.access_token_lifetime,
            refresh_expires_at: now + self.config.refresh_token_lifetime,
            ip_address: context.ip_address.clone(),
            user_agent: context.user_agent.clone(),
            client_id,
            scopes,
            rotation_count: 0,
            security_flags: SessionSecurityFlags::default(),
        };

        // Store session
        {
            let mut sessions = self.sessions.write().unwrap();
            sessions.insert(session_id, session.clone());
        }

        Ok(session)
    }

    /// Validate session and return updated session if valid
    pub fn validate_session(
        &self,
        session_id: &str,
        access_token: &str,
        context: &SessionValidationContext,
    ) -> Result<SecureSession, SessionSecurityError> {
        let mut sessions = self.sessions.write().unwrap();
        
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionSecurityError::SessionNotFound(session_id.to_string()))?;

        // Check if session is expired
        if context.current_time > session.expires_at {
            return Err(SessionSecurityError::SessionExpired(session_id.to_string()));
        }

        // Validate access token
        if session.access_token != access_token {
            return Err(SessionSecurityError::InvalidToken);
        }

        // Check for security violations
        self.check_security_violations(session, context)?;

        // Check if rotation is required
        let needs_rotation = self.should_rotate_tokens(session, context);
        if needs_rotation {
            session.security_flags.force_rotation = true;
            return Err(SessionSecurityError::RotationRequired);
        }

        // Update last accessed time
        session.last_accessed = context.current_time;

        Ok(session.clone())
    }

    /// Rotate session tokens
    pub fn rotate_tokens(
        &self,
        session_id: &str,
        refresh_token: &str,
        context: &SessionValidationContext,
    ) -> Result<TokenRotationResult, SessionSecurityError> {
        let mut sessions = self.sessions.write().unwrap();
        
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionSecurityError::SessionNotFound(session_id.to_string()))?;

        // Check refresh token validity
        if session.refresh_token != refresh_token {
            return Err(SessionSecurityError::InvalidToken);
        }

        // Check if refresh token is expired
        if context.current_time > session.refresh_expires_at {
            return Err(SessionSecurityError::SessionExpired(session_id.to_string()));
        }

        // Check rotation limit
        if session.rotation_count >= self.config.max_rotation_count {
            return Err(SessionSecurityError::SecurityViolation(
                "Maximum token rotations exceeded".to_string()
            ));
        }

        // Generate new tokens
        let new_access_token = Self::generate_token();
        let new_refresh_token = Self::generate_token();
        let now = context.current_time;

        // Update session
        session.access_token = new_access_token.clone();
        session.refresh_token = new_refresh_token.clone();
        session.expires_at = now + self.config.access_token_lifetime;
        session.last_accessed = now;
        session.rotation_count += 1;
        session.security_flags.force_rotation = false;

        Ok(TokenRotationResult {
            new_access_token,
            new_refresh_token,
            expires_at: session.expires_at,
            rotation_count: session.rotation_count,
        })
    }

    /// Destroy session
    pub fn destroy_session(&self, session_id: &str) -> Result<(), SessionSecurityError> {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(session_id)
            .ok_or_else(|| SessionSecurityError::SessionNotFound(session_id.to_string()))?;
        Ok(())
    }

    /// Destroy all sessions for a user
    pub fn destroy_user_sessions(&self, user_id: &str) -> usize {
        let mut sessions = self.sessions.write().unwrap();
        let mut to_remove = Vec::new();
        
        for (session_id, session) in sessions.iter() {
            if session.user_id == user_id {
                to_remove.push(session_id.clone());
            }
        }
        
        let count = to_remove.len();
        for session_id in to_remove {
            sessions.remove(&session_id);
        }
        
        count
    }

    /// Get session information
    pub fn get_session(&self, session_id: &str) -> Option<SecureSession> {
        let sessions = self.sessions.read().unwrap();
        sessions.get(session_id).cloned()
    }

    /// List active sessions for a user
    pub fn list_user_sessions(&self, user_id: &str) -> Vec<SecureSession> {
        let sessions = self.sessions.read().unwrap();
        sessions
            .values()
            .filter(|s| s.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Mark session as suspicious
    pub fn mark_suspicious(&self, session_id: &str, reason: &str) -> Result<(), SessionSecurityError> {
        let mut sessions = self.sessions.write().unwrap();
        
        if let Some(session) = sessions.get_mut(session_id) {
            session.security_flags.is_suspicious = true;
            session.security_flags.force_rotation = true;
            
            // Log security event
            crate::security::audit_logger::log_security_violation(
                "suspicious_session",
                Some(session.user_id.clone()),
                Some(session_id.to_string()),
                reason,
            ).ok();
            
            Ok(())
        } else {
            Err(SessionSecurityError::SessionNotFound(session_id.to_string()))
        }
    }

    /// Cleanup expired sessions
    pub fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.write().unwrap();
        let now = Utc::now();
        
        sessions.retain(|_, session| {
            now <= session.refresh_expires_at
        });
    }

    /// Get session statistics
    pub fn get_session_stats(&self) -> SessionStats {
        let sessions = self.sessions.read().unwrap();
        let now = Utc::now();
        
        let total_sessions = sessions.len();
        let active_sessions = sessions
            .values()
            .filter(|s| now <= s.expires_at)
            .count();
        let suspicious_sessions = sessions
            .values()
            .filter(|s| s.security_flags.is_suspicious)
            .count();
        
        SessionStats {
            total_sessions,
            active_sessions,
            suspicious_sessions,
            expired_sessions: total_sessions - active_sessions,
        }
    }

    /// Check for security violations
    fn check_security_violations(
        &self,
        session: &SecureSession,
        context: &SessionValidationContext,
    ) -> Result<(), SessionSecurityError> {
        // Check IP address consistency
        if self.config.require_ip_consistency {
            if let (Some(session_ip), Some(context_ip)) = (&session.ip_address, &context.ip_address) {
                if session_ip != context_ip {
                    return Err(SessionSecurityError::SecurityViolation(
                        format!("IP address mismatch: expected {}, got {}", session_ip, context_ip)
                    ));
                }
            }
        }

        // Check User-Agent consistency
        if self.config.require_user_agent_consistency {
            if let (Some(session_ua), Some(context_ua)) = (&session.user_agent, &context.user_agent) {
                if session_ua != context_ua {
                    return Err(SessionSecurityError::SecurityViolation(
                        "User-Agent mismatch".to_string()
                    ));
                }
            }
        }

        // Check if session is marked as suspicious
        if session.security_flags.is_suspicious {
            return Err(SessionSecurityError::SecurityViolation(
                "Session marked as suspicious".to_string()
            ));
        }

        Ok(())
    }

    /// Check if tokens should be rotated
    fn should_rotate_tokens(&self, session: &SecureSession, context: &SessionValidationContext) -> bool {
        // Force rotation if flagged
        if session.security_flags.force_rotation {
            return true;
        }

        // Rotate based on time threshold
        let time_since_last_access = context.current_time - session.last_accessed;
        if time_since_last_access > self.config.rotation_threshold {
            return true;
        }

        // Rotate for high-privilege sessions more frequently
        if session.security_flags.high_privilege && time_since_last_access > Duration::minutes(15) {
            return true;
        }

        false
    }

    /// Generate cryptographically secure session ID
    fn generate_session_id() -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        format!("sess_{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Generate cryptographically secure token
    fn generate_token() -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

#[derive(Debug, Clone)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub suspicious_sessions: usize,
    pub expired_sessions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_context() -> SessionValidationContext {
        SessionValidationContext {
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
            requested_scopes: vec!["read".to_string(), "write".to_string()],
            current_time: Utc::now(),
        }
    }

    #[test]
    fn test_session_creation() {
        let config = SessionConfig::default();
        let manager = SessionSecurityManager::new(config);
        let context = create_test_context();

        let session = manager.create_session(
            "user123".to_string(),
            "client456".to_string(),
            vec!["read".to_string()],
            &context,
        ).unwrap();

        assert_eq!(session.user_id, "user123");
        assert_eq!(session.client_id, "client456");
        assert!(!session.access_token.is_empty());
        assert!(!session.refresh_token.is_empty());
    }

    #[test]
    fn test_session_validation() {
        let config = SessionConfig::default();
        let manager = SessionSecurityManager::new(config);
        let context = create_test_context();

        let session = manager.create_session(
            "user123".to_string(),
            "client456".to_string(),
            vec!["read".to_string()],
            &context,
        ).unwrap();

        // Valid session
        let result = manager.validate_session(
            &session.session_id,
            &session.access_token,
            &context,
        );
        assert!(result.is_ok());

        // Invalid token
        let result = manager.validate_session(
            &session.session_id,
            "invalid_token",
            &context,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_token_rotation() {
        let config = SessionConfig::default();
        let manager = SessionSecurityManager::new(config);
        let context = create_test_context();

        let session = manager.create_session(
            "user123".to_string(),
            "client456".to_string(),
            vec!["read".to_string()],
            &context,
        ).unwrap();

        let rotation_result = manager.rotate_tokens(
            &session.session_id,
            &session.refresh_token,
            &context,
        ).unwrap();

        assert_ne!(rotation_result.new_access_token, session.access_token);
        assert_ne!(rotation_result.new_refresh_token, session.refresh_token);
        assert_eq!(rotation_result.rotation_count, 1);
    }

    #[test]
    fn test_concurrent_session_limit() {
        let mut config = SessionConfig::default();
        config.max_concurrent_sessions = 2;
        let manager = SessionSecurityManager::new(config);
        let context = create_test_context();

        // Create maximum allowed sessions
        for i in 0..2 {
            let _session = manager.create_session(
                "user123".to_string(),
                format!("client{}", i),
                vec!["read".to_string()],
                &context,
            ).unwrap();
        }

        // Should fail to create another session
        let result = manager.create_session(
            "user123".to_string(),
            "client3".to_string(),
            vec!["read".to_string()],
            &context,
        );
        assert!(matches!(result, Err(SessionSecurityError::ConcurrentLimitExceeded)));
    }

    #[test]
    fn test_suspicious_session_marking() {
        let config = SessionConfig::default();
        let manager = SessionSecurityManager::new(config);
        let context = create_test_context();

        let session = manager.create_session(
            "user123".to_string(),
            "client456".to_string(),
            vec!["read".to_string()],
            &context,
        ).unwrap();

        // Mark session as suspicious
        manager.mark_suspicious(&session.session_id, "Multiple failed attempts").unwrap();

        // Session validation should fail
        let result = manager.validate_session(
            &session.session_id,
            &session.access_token,
            &context,
        );
        assert!(matches!(result, Err(SessionSecurityError::SecurityViolation(_))));
    }
}