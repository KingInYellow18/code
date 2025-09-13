use claude_code_security::{
    SecurityManager, SecurityConfig, SecureClaudeAuth, ClaudeAuthConfig,
    init_security_system, init_claude_auth_system,
};
use tempfile::tempdir;
use tokio_test;

#[tokio::test]
async fn test_security_system_initialization() {
    let temp_dir = tempdir().unwrap();
    
    let config = SecurityConfig {
        token_storage_path: temp_dir.path().join("tokens.json"),
        audit_log_path: temp_dir.path().join("audit.log"),
        enable_encryption: true,
        enable_audit_logging: true,
        require_pkce: true,
        token_rotation_enabled: true,
        max_concurrent_oauth_flows: 3,
        session_timeout_minutes: 60,
        require_secure_transport: false, // Disabled for tests
    };

    let security_manager = SecurityManager::new(config).unwrap();
    
    // Test that all security components are properly initialized
    assert!(security_manager.token_storage().is_some());
    assert!(security_manager.session_manager().is_some());
    
    // Test security health check
    let health_report = security_manager.security_health_check();
    assert!(health_report.oauth_security_enabled);
    assert!(health_report.session_security_enabled);
    assert!(health_report.audit_logging_enabled);
}

#[tokio::test]
async fn test_claude_auth_initialization() {
    let temp_dir = tempdir().unwrap();
    
    let config = ClaudeAuthConfig {
        client_id: "test_client_id".to_string(),
        auth_endpoint: "https://auth.anthropic.com/oauth/authorize".to_string(),
        token_endpoint: "https://auth.anthropic.com/oauth/token".to_string(),
        subscription_endpoint: "https://api.anthropic.com/v1/subscription".to_string(),
        redirect_uri: "http://localhost:1456/callback".to_string(),
        scopes: vec!["api".to_string(), "subscription".to_string()],
        require_max_subscription: false,
        enable_subscription_check: false, // Disabled for tests
    };

    let storage_path = temp_dir.path().join("claude_tokens.json");
    let claude_auth = SecureClaudeAuth::new(config, storage_path).unwrap();
    
    assert!(!claude_auth.is_authenticated()); // No tokens initially
}

#[tokio::test]
async fn test_oauth_flow_security() {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path().join("claude_tokens.json");
    
    let config = ClaudeAuthConfig {
        client_id: "test_client_id".to_string(),
        redirect_uri: "http://localhost:1456/callback".to_string(),
        ..Default::default()
    };

    let mut claude_auth = SecureClaudeAuth::new(config, storage_path).unwrap();
    
    // Test OAuth flow start
    let auth_url = claude_auth.start_oauth_flow().unwrap();
    
    // Verify security parameters are present
    assert!(auth_url.contains("code_challenge="));
    assert!(auth_url.contains("code_challenge_method=S256"));
    assert!(auth_url.contains("state="));
    assert!(auth_url.contains("response_type=code"));
}

#[tokio::test]
async fn test_token_storage_security() {
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path().join("secure_tokens.json");
    
    let storage = claude_code_security::SecureTokenStorage::new(storage_path.clone()).unwrap();
    
    let test_tokens = claude_code_security::security::secure_token_storage::TokenData {
        access_token: "test_access_token".to_string(),
        refresh_token: "test_refresh_token".to_string(),
        id_token: "test_id_token".to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        account_id: Some("test_account".to_string()),
        provider: "claude".to_string(),
    };
    
    // Store tokens
    storage.store_tokens(&test_tokens).unwrap();
    assert!(storage.tokens_exist());
    
    // Retrieve tokens
    let retrieved_tokens = storage.retrieve_tokens().unwrap().unwrap();
    assert_eq!(test_tokens.access_token, retrieved_tokens.access_token);
    assert_eq!(test_tokens.provider, retrieved_tokens.provider);
    
    // Test file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&storage_path).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
    
    // Delete tokens
    storage.delete_tokens().unwrap();
    assert!(!storage.tokens_exist());
}

#[tokio::test]
async fn test_audit_logging() {
    let temp_dir = tempdir().unwrap();
    let log_path = temp_dir.path().join("test_audit.log");
    
    let mut logger = claude_code_security::SecurityAuditLogger::new(log_path.clone()).unwrap();
    
    // Log some test events
    logger.log_login_success(
        Some("test_user".to_string()),
        Some("test_session".to_string()),
        Some("test_client".to_string()),
        Some("127.0.0.1".to_string()),
    ).unwrap();
    
    logger.log_security_violation(
        "test_violation",
        Some("test_user".to_string()),
        Some("test_session".to_string()),
        "Test security violation for testing",
    ).unwrap();
    
    logger.flush_buffer().unwrap();
    
    // Verify log file exists and contains expected content
    assert!(log_path.exists());
    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(log_content.contains("test_user"));
    assert!(log_content.contains("Login"));
    assert!(log_content.contains("SecurityViolation"));
    
    // Test metrics generation
    let start_time = chrono::Utc::now() - chrono::Duration::hours(1);
    let end_time = chrono::Utc::now() + chrono::Duration::hours(1);
    let metrics = logger.generate_metrics(start_time, end_time).unwrap();
    
    assert_eq!(metrics.successful_logins, 1);
    assert_eq!(metrics.security_violations, 1);
}

#[tokio::test]
async fn test_session_security() {
    let config = claude_code_security::security::session_security::SessionConfig::default();
    let session_manager = claude_code_security::SessionSecurityManager::new(config);
    
    let context = claude_code_security::security::session_security::SessionValidationContext {
        ip_address: Some("192.168.1.1".to_string()),
        user_agent: Some("TestAgent/1.0".to_string()),
        requested_scopes: vec!["api".to_string()],
        current_time: chrono::Utc::now(),
    };
    
    // Create session
    let session = session_manager.create_session(
        "test_user".to_string(),
        "test_client".to_string(),
        vec!["api".to_string()],
        &context,
    ).unwrap();
    
    // Validate session
    let validation_result = session_manager.validate_session(
        &session.session_id,
        &session.access_token,
        &context,
    );
    assert!(validation_result.is_ok());
    
    // Test token rotation
    let rotation_result = session_manager.rotate_tokens(
        &session.session_id,
        &session.refresh_token,
        &context,
    ).unwrap();
    
    assert_ne!(rotation_result.new_access_token, session.access_token);
    assert_eq!(rotation_result.rotation_count, 1);
    
    // Destroy session
    session_manager.destroy_session(&session.session_id).unwrap();
}

#[tokio::test]
async fn test_pkce_security() {
    use claude_code_security::security::oauth_security::{SecureOAuthFlow, OAuthSecurityManager};
    
    let mut oauth_manager = OAuthSecurityManager::new(3);
    
    // Start OAuth flow
    let session_id = oauth_manager.start_flow(
        "test_client".to_string(),
        "http://localhost:1456/callback".to_string(),
    ).unwrap();
    
    let flow = oauth_manager.get_flow(&session_id).unwrap();
    
    // Generate authorization URL
    let auth_request = flow.generate_authorization_url(
        "https://auth.anthropic.com/oauth/authorize",
        &["api", "subscription"],
    ).unwrap();
    
    // Verify PKCE parameters
    assert!(auth_request.authorization_url.contains("code_challenge="));
    assert!(auth_request.authorization_url.contains("code_challenge_method=S256"));
    assert!(!auth_request.pkce_challenge.is_empty());
    
    // Test PKCE verification
    let security_state = flow.get_security_state();
    assert!(flow.verify_pkce(&security_state.pkce_verifier).is_ok());
    assert!(flow.verify_pkce("invalid_verifier").is_err());
    
    // Test state validation
    let valid_callback = flow.validate_callback("test_code", &auth_request.state, None);
    assert!(valid_callback.is_ok());
    
    let invalid_callback = flow.validate_callback("test_code", "invalid_state", None);
    assert!(invalid_callback.is_err());
}

#[tokio::test]
async fn test_environment_security_validation() {
    let temp_dir = tempdir().unwrap();
    
    let config = SecurityConfig {
        token_storage_path: temp_dir.path().join("tokens.json"),
        audit_log_path: temp_dir.path().join("audit.log"),
        require_secure_transport: false, // Disabled for tests
        ..Default::default()
    };

    let security_manager = SecurityManager::new(config).unwrap();
    
    // Test environment validation (should not fail even with env vars set)
    let validation_result = security_manager.validate_environment();
    assert!(validation_result.is_ok());
}

#[test]
fn test_default_configurations() {
    // Test default security configuration
    let security_config = SecurityConfig::default();
    assert!(security_config.enable_encryption);
    assert!(security_config.enable_audit_logging);
    assert!(security_config.require_pkce);
    assert!(security_config.token_rotation_enabled);
    
    // Test default Claude auth configuration
    let claude_config = ClaudeAuthConfig::default();
    assert_eq!(claude_config.client_id, "claude_code_client");
    assert!(claude_config.scopes.contains(&"api".to_string()));
    assert!(claude_config.enable_subscription_check);
}