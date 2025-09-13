// Claude Authentication Security Tests
// Comprehensive security validation for Claude authentication integration

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

use codex_core::auth::{ClaudeAuth, ClaudeOAuthFlow, SecureTokenStorage};
use codex_core::security::{SecurityEnforcer, AuthEvent};

/// Test token storage encryption and file permissions
#[test]
fn test_token_storage_encryption() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let storage_path = temp_dir.path().join("claude_tokens.json");
    
    let storage = SecureTokenStorage::new(&storage_path).expect("create secure storage");
    let test_tokens = create_test_claude_tokens();
    
    // Store tokens with encryption
    storage.store_encrypted(&test_tokens).expect("store encrypted tokens");
    
    // Validation: File should exist with correct permissions
    assert!(storage_path.exists(), "Token file should exist");
    
    let metadata = fs::metadata(&storage_path).expect("get file metadata");
    let permissions = metadata.permissions();
    assert_eq!(permissions.mode() & 0o777, 0o600, 
               "Token file should have 0o600 permissions (owner read/write only)");
    
    // Validation: Raw file content should be encrypted (not plaintext)
    let raw_content = fs::read_to_string(&storage_path).expect("read raw file");
    assert!(!raw_content.contains("claude_test_access_token"), 
            "Raw file should not contain plaintext tokens");
    assert!(!raw_content.contains("claude_test_refresh_token"), 
            "Raw file should not contain plaintext refresh tokens");
    
    // Validation: Decryption should recover original tokens
    let decrypted_tokens = storage.load_encrypted().expect("load and decrypt tokens");
    assert_eq!(decrypted_tokens.access_token, test_tokens.access_token, 
               "Decrypted tokens should match original");
    assert_eq!(decrypted_tokens.refresh_token, test_tokens.refresh_token, 
               "Decrypted refresh tokens should match original");
}

/// Test OAuth PKCE validation
#[test]
fn test_oauth_pkce_validation() {
    let security_enforcer = SecurityEnforcer::new();
    
    // Generate PKCE challenge and verifier
    let oauth_flow = ClaudeOAuthFlow::new();
    let auth_url = oauth_flow.generate_auth_url().expect("generate auth URL");
    
    // Extract PKCE challenge from URL
    let url = url::Url::parse(&auth_url).expect("parse auth URL");
    let challenge = url.query_pairs()
        .find(|(key, _)| key == "code_challenge")
        .map(|(_, value)| value.to_string())
        .expect("find code challenge");
    
    let challenge_method = url.query_pairs()
        .find(|(key, _)| key == "code_challenge_method")
        .map(|(_, value)| value.to_string())
        .expect("find challenge method");
    
    // Validation: Challenge method should be S256
    assert_eq!(challenge_method, "S256", "Should use SHA256 for PKCE challenge");
    
    // Validation: Challenge should be base64url encoded
    assert!(challenge.len() >= 43, "Challenge should be at least 43 characters");
    assert!(challenge.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'), 
            "Challenge should be base64url encoded");
    
    // Test verifier validation
    let verifier = oauth_flow.get_pkce_verifier().expect("get verifier");
    let is_valid = security_enforcer.verify_pkce_challenge(&verifier, &challenge)
        .expect("verify PKCE challenge");
    assert!(is_valid, "Verifier should validate against its challenge");
    
    // Test invalid verifier rejection
    let invalid_verifier = "invalid_verifier_string";
    let is_invalid = security_enforcer.verify_pkce_challenge(invalid_verifier, &challenge)
        .expect("verify invalid challenge");
    assert!(!is_invalid, "Invalid verifier should be rejected");
}

/// Test OAuth state parameter validation
#[test]
fn test_oauth_state_validation() {
    let security_enforcer = SecurityEnforcer::new();
    
    // Generate OAuth URL with state parameter
    let oauth_flow = ClaudeOAuthFlow::new();
    let auth_url = oauth_flow.generate_auth_url().expect("generate auth URL");
    
    // Extract state parameter
    let url = url::Url::parse(&auth_url).expect("parse auth URL");
    let state = url.query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
        .expect("find state parameter");
    
    // Validation: State should be cryptographically random
    assert!(state.len() >= 32, "State should be at least 32 characters");
    assert!(state.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'), 
            "State should be URL-safe");
    
    // Test state validation
    let is_valid = security_enforcer.validate_oauth_state(&state)
        .expect("validate OAuth state");
    assert!(is_valid, "Generated state should be valid");
    
    // Test invalid state rejection
    let invalid_state = "predictable_state_123";
    let is_invalid = security_enforcer.validate_oauth_state(invalid_state)
        .expect("validate invalid state");
    assert!(!is_invalid, "Predictable state should be rejected");
    
    // Test state reuse prevention
    security_enforcer.mark_state_used(&state).expect("mark state as used");
    let reuse_attempt = security_enforcer.validate_oauth_state(&state)
        .expect("validate reused state");
    assert!(!reuse_attempt, "Used state should be rejected on reuse");
}

/// Test session management security
#[tokio::test]
async fn test_session_management_security() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Create multiple sessions
    let session1 = auth_manager.create_session("user1").await.expect("create session 1");
    let session2 = auth_manager.create_session("user2").await.expect("create session 2");
    
    // Validation: Sessions should be isolated
    assert_ne!(session1.session_id, session2.session_id, 
               "Sessions should have unique IDs");
    
    let session1_data = auth_manager.get_session_data(&session1.session_id).await
        .expect("get session 1 data");
    let session2_data = auth_manager.get_session_data(&session2.session_id).await
        .expect("get session 2 data");
    
    assert_ne!(session1_data.user_id, session2_data.user_id, 
               "Sessions should be isolated by user");
    
    // Test session timeout
    let short_lived_session = auth_manager.create_session_with_timeout("temp_user", Duration::from_millis(100))
        .await.expect("create short-lived session");
    
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    let expired_session_result = auth_manager.get_session_data(&short_lived_session.session_id).await;
    assert!(expired_session_result.is_err(), "Expired session should be inaccessible");
    
    // Test concurrent session limits
    let mut sessions = Vec::new();
    for i in 0..20 {
        let session_result = auth_manager.create_session(&format!("user_{}", i)).await;
        if let Ok(session) = session_result {
            sessions.push(session);
        }
    }
    
    assert!(sessions.len() <= MAX_CONCURRENT_SESSIONS, 
            "Should enforce concurrent session limits");
    
    // Cleanup active sessions
    for session in sessions {
        auth_manager.destroy_session(&session.session_id).await
            .expect("cleanup session");
    }
}

/// Test API key protection
#[test]
fn test_api_key_protection() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_file = temp_dir.path().join("auth.json");
    
    // Create auth with API key
    let claude_auth = ClaudeAuth::from_api_key("sk-ant-test-key-123");
    
    // Store auth data
    claude_auth.save_to_file(&auth_file).expect("save auth to file");
    
    // Validation: API key should not appear in log files
    let log_content = std::fs::read_to_string("test.log").unwrap_or_default();
    assert!(!log_content.contains("sk-ant-test-key-123"), 
            "API key should not appear in logs");
    
    // Validation: API key should not appear in error messages
    let error_message = format!("Authentication failed for key: {}", 
        claude_auth.get_masked_api_key());
    assert!(!error_message.contains("sk-ant-test-key-123"), 
            "Full API key should not appear in error messages");
    assert!(error_message.contains("sk-ant-***"), 
            "API key should be masked in error messages");
    
    // Validation: Environment variables should be cleared after use
    std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-key-123");
    let claude_auth_from_env = ClaudeAuth::from_environment().expect("create from environment");
    
    // Simulate cleanup
    claude_auth_from_env.clear_sensitive_environment().expect("clear environment");
    
    let env_after_cleanup = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    assert!(env_after_cleanup.is_empty(), 
            "Sensitive environment variables should be cleared");
}

/// Test credential isolation between providers
#[tokio::test]
async fn test_credential_isolation() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Setup different credentials for each provider
    let claude_creds = ClaudeCredentials {
        api_key: "claude_key_123".to_string(),
        oauth_tokens: Some(create_test_claude_tokens()),
    };
    
    let openai_creds = OpenAICredentials {
        api_key: "openai_key_456".to_string(),
        organization: Some("org-test".to_string()),
    };
    
    auth_manager.set_claude_credentials(claude_creds).await.expect("set Claude credentials");
    auth_manager.set_openai_credentials(openai_creds).await.expect("set OpenAI credentials");
    
    // Validation: Claude provider should only access Claude credentials
    let claude_provider = auth_manager.get_claude_provider().await.expect("get Claude provider");
    let claude_env = claude_provider.get_environment_variables().await.expect("get Claude env");
    
    assert!(claude_env.contains_key("ANTHROPIC_API_KEY"), "Claude should have access to its API key");
    assert!(!claude_env.contains_key("OPENAI_API_KEY"), "Claude should not have access to OpenAI key");
    
    // Validation: OpenAI provider should only access OpenAI credentials
    let openai_provider = auth_manager.get_openai_provider().await.expect("get OpenAI provider");
    let openai_env = openai_provider.get_environment_variables().await.expect("get OpenAI env");
    
    assert!(openai_env.contains_key("OPENAI_API_KEY"), "OpenAI should have access to its API key");
    assert!(!openai_env.contains_key("ANTHROPIC_API_KEY"), "OpenAI should not have access to Claude key");
    
    // Validation: Cross-contamination should not occur
    let claude_token = claude_provider.get_access_token().await.expect("get Claude token");
    let openai_token = openai_provider.get_access_token().await.expect("get OpenAI token");
    
    assert_ne!(claude_token, openai_token, "Providers should use different tokens");
    assert!(claude_token.starts_with("claude_"), "Claude token should be properly identified");
    assert!(openai_token.starts_with("openai_"), "OpenAI token should be properly identified");
}

/// Test audit logging for authentication events
#[tokio::test]
async fn test_authentication_audit_logging() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let security_enforcer = SecurityEnforcer::new();
    let audit_log_path = temp_dir.path().join("auth_audit.log");
    
    security_enforcer.set_audit_log_path(&audit_log_path).expect("set audit log path");
    
    // Generate various authentication events
    let events = vec![
        AuthEvent::LoginAttempt { provider: "claude".to_string(), success: true },
        AuthEvent::TokenRefresh { provider: "claude".to_string(), success: true },
        AuthEvent::ProviderSwitch { from: "claude".to_string(), to: "openai".to_string() },
        AuthEvent::QuotaExceeded { provider: "claude".to_string(), user_id: "test_user".to_string() },
        AuthEvent::LoginAttempt { provider: "claude".to_string(), success: false },
    ];
    
    for event in events {
        security_enforcer.audit_auth_events(&event).expect("audit event");
    }
    
    // Validation: Audit log should contain all events
    let audit_content = std::fs::read_to_string(&audit_log_path).expect("read audit log");
    assert!(audit_content.contains("LoginAttempt"), "Should log login attempts");
    assert!(audit_content.contains("TokenRefresh"), "Should log token refreshes");
    assert!(audit_content.contains("ProviderSwitch"), "Should log provider switches");
    assert!(audit_content.contains("QuotaExceeded"), "Should log quota exceeded events");
    
    // Validation: Failed attempts should be logged
    assert!(audit_content.contains("success: false"), "Should log failed attempts");
    
    // Validation: Sensitive data should not appear in logs
    assert!(!audit_content.contains("sk-ant-"), "Should not log API keys");
    assert!(!audit_content.contains("access_token"), "Should not log access tokens");
}

// Helper functions

fn create_test_claude_tokens() -> ClaudeTokenData {
    ClaudeTokenData {
        access_token: "claude_test_access_token".to_string(),
        refresh_token: Some("claude_test_refresh_token".to_string()),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        subscription_tier: "max".to_string(),
    }
}

async fn create_test_auth_manager(codex_home: &Path) -> AuthManager {
    AuthManager::new(
        codex_home.to_path_buf(),
        AuthMode::ChatGPT,
        "security_test_client".to_string(),
    )
}

// Test constants
const MAX_CONCURRENT_SESSIONS: usize = 100;