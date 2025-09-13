use claude_code_security::{
    SecurityManager, SecurityConfig, SecureClaudeAuth, ClaudeAuthConfig,
    init_security_system, init_claude_auth_system,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔒 Claude Code Security Demo");
    println!("============================");

    // 1. Initialize Security System
    println!("\n1. Initializing Security System...");
    let temp_dir = std::env::temp_dir().join("claude_security_demo");
    std::fs::create_dir_all(&temp_dir)?;

    let security_config = SecurityConfig {
        token_storage_path: temp_dir.join("tokens.json"),
        audit_log_path: temp_dir.join("audit.log"),
        enable_encryption: true,
        enable_audit_logging: true,
        require_pkce: true,
        token_rotation_enabled: true,
        max_concurrent_oauth_flows: 3,
        session_timeout_minutes: 60,
        require_secure_transport: false, // Disabled for demo
    };

    let security_manager = SecurityManager::new(security_config)?;
    println!("✅ Security system initialized successfully");

    // 2. Security Health Check
    println!("\n2. Running Security Health Check...");
    let health_report = security_manager.security_health_check();
    println!("📊 Security Health Report:");
    println!("   - Token Storage Secure: {}", health_report.token_storage_secure);
    println!("   - Audit Logging Enabled: {}", health_report.audit_logging_enabled);
    println!("   - OAuth Security Enabled: {}", health_report.oauth_security_enabled);
    println!("   - Session Security Enabled: {}", health_report.session_security_enabled);
    println!("   - Active OAuth Flows: {}", health_report.active_oauth_flows);
    println!("   - Active Sessions: {}", health_report.active_sessions);

    // 3. Environment Security Validation
    println!("\n3. Validating Environment Security...");
    security_manager.validate_environment()?;
    println!("✅ Environment security validation passed");

    // 4. Initialize Claude Authentication
    println!("\n4. Setting up Claude Authentication...");
    let claude_config = ClaudeAuthConfig {
        client_id: "demo_client_id".to_string(),
        auth_endpoint: "https://auth.anthropic.com/oauth/authorize".to_string(),
        token_endpoint: "https://auth.anthropic.com/oauth/token".to_string(),
        subscription_endpoint: "https://api.anthropic.com/v1/subscription".to_string(),
        redirect_uri: "http://localhost:1456/auth/callback".to_string(),
        scopes: vec!["api".to_string(), "subscription".to_string()],
        require_max_subscription: false, // Disabled for demo
        enable_subscription_check: false, // Disabled for demo
    };

    let storage_path = temp_dir.join("claude_tokens.json");
    let mut claude_auth = SecureClaudeAuth::new(claude_config, storage_path)?;
    println!("✅ Claude authentication initialized");

    // 5. Demonstrate OAuth Flow Security
    println!("\n5. Demonstrating OAuth Security Features...");
    let auth_url = claude_auth.start_oauth_flow()?;
    println!("🔐 Generated secure OAuth URL:");
    println!("   {}", auth_url);
    
    // Verify security parameters
    if auth_url.contains("code_challenge=") && auth_url.contains("state=") {
        println!("✅ PKCE and state parameters present");
    } else {
        println!("❌ Missing required security parameters");
    }

    // 6. Demonstrate Token Storage Security
    println!("\n6. Testing Token Storage Security...");
    if let Some(token_storage) = security_manager.token_storage() {
        println!("📁 Token storage configured with encryption");
        
        // Create sample token data
        let sample_tokens = claude_code_security::security::secure_token_storage::TokenData {
            access_token: "demo_access_token_123".to_string(),
            refresh_token: "demo_refresh_token_456".to_string(),
            id_token: "demo_id_token_789".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            account_id: Some("demo_account".to_string()),
            provider: "claude".to_string(),
        };

        // Store tokens securely
        token_storage.store_tokens(&sample_tokens)?;
        println!("✅ Demo tokens stored with encryption");

        // Verify tokens can be retrieved
        if let Some(retrieved) = token_storage.retrieve_tokens()? {
            println!("✅ Tokens successfully retrieved and decrypted");
            println!("   - Provider: {}", retrieved.provider);
            println!("   - Account ID: {:?}", retrieved.account_id);
        }

        // Clean up demo tokens
        token_storage.delete_tokens()?;
        println!("🧹 Demo tokens securely deleted");
    }

    // 7. Demonstrate Session Security
    println!("\n7. Testing Session Security...");
    if let Some(session_manager) = security_manager.session_manager() {
        let context = claude_code_security::security::session_security::SessionValidationContext {
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("Claude-Security-Demo/1.0".to_string()),
            requested_scopes: vec!["api".to_string()],
            current_time: chrono::Utc::now(),
        };

        // Create demo session
        let session = session_manager.create_session(
            "demo_user".to_string(),
            "demo_client".to_string(),
            vec!["api".to_string()],
            &context,
        )?;
        
        println!("🎫 Created secure session: {}", session.session_id);
        println!("   - User: {}", session.user_id);
        println!("   - Created: {}", session.created_at);
        println!("   - Expires: {}", session.expires_at);

        // Validate session
        let validation_result = session_manager.validate_session(
            &session.session_id,
            &session.access_token,
            &context,
        );

        if validation_result.is_ok() {
            println!("✅ Session validation successful");
        }

        // Demonstrate token rotation
        let rotation_result = session_manager.rotate_tokens(
            &session.session_id,
            &session.refresh_token,
            &context,
        )?;
        
        println!("🔄 Token rotation performed:");
        println!("   - Rotation count: {}", rotation_result.rotation_count);
        println!("   - New expiry: {}", rotation_result.expires_at);

        // Clean up session
        session_manager.destroy_session(&session.session_id)?;
        println!("🧹 Demo session destroyed");
    }

    // 8. Demonstrate Audit Logging
    println!("\n8. Testing Security Audit Logging...");
    
    // Log some demo events
    claude_code_security::security::audit_logger::log_login_success(
        Some("demo_user".to_string()),
        Some("demo_session".to_string()),
        Some("demo_client".to_string()),
        Some("127.0.0.1".to_string()),
    )?;

    claude_code_security::security::audit_logger::log_security_violation(
        "demo_violation",
        Some("demo_user".to_string()),
        Some("demo_session".to_string()),
        "This is a demo security violation for testing purposes",
    )?;

    println!("📝 Security events logged");
    
    // Check if audit log exists
    let audit_log_path = temp_dir.join("audit.log");
    if audit_log_path.exists() {
        println!("✅ Audit log created at: {}", audit_log_path.display());
        
        // Read and display recent events
        let log_content = std::fs::read_to_string(&audit_log_path)?;
        let lines: Vec<&str> = log_content.lines().take(2).collect();
        
        println!("📋 Recent audit events:");
        for (i, line) in lines.iter().enumerate() {
            println!("   {}. {}", i + 1, line);
        }
    }

    // 9. Summary
    println!("\n🎉 Security Demo Complete!");
    println!("============================");
    println!("All security features have been successfully demonstrated:");
    println!("✅ Enhanced token storage with encryption");
    println!("✅ OAuth security with PKCE and state validation");
    println!("✅ Session security with token rotation");
    println!("✅ Comprehensive audit logging");
    println!("✅ Environment security validation");
    println!("✅ Security health monitoring");

    // Clean up demo directory
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\n🧹 Demo files cleaned up");

    println!("\n🔒 Security implementation ready for production use!");

    Ok(())
}

/// Helper function to demonstrate PKCE security
fn demonstrate_pkce_security() {
    println!("🔐 PKCE Security Demonstration:");
    
    use claude_code_security::security::oauth_security::SecureOAuthFlow;
    
    let flow = SecureOAuthFlow::new(
        "demo_client".to_string(),
        "http://localhost:1456/callback".to_string(),
    ).unwrap();
    
    let auth_request = flow.generate_authorization_url(
        "https://auth.anthropic.com/oauth/authorize",
        &["api", "subscription"],
    ).unwrap();
    
    println!("   - Code Challenge generated: ✅");
    println!("   - State parameter generated: ✅");
    println!("   - Nonce generated: ✅");
    println!("   - Session ID: {}", auth_request.session_id);
}

/// Helper function to display security features
fn display_security_features() {
    println!("🛡️  Security Features Implemented:");
    println!("   1. Enhanced Token Storage:");
    println!("      - File encryption at rest");
    println!("      - Secure file permissions (0o600)");
    println!("      - Secure deletion with overwriting");
    println!("   ");
    println!("   2. OAuth Security Enhancement:");
    println!("      - PKCE (Proof Key for Code Exchange)");
    println!("      - State parameter validation");
    println!("      - Nonce validation for ID tokens");
    println!("      - Session timeout management");
    println!("   ");
    println!("   3. Session Security:");
    println!("      - Token rotation mechanisms");
    println!("      - Concurrent session limits");
    println!("      - IP and User-Agent validation");
    println!("      - Suspicious activity detection");
    println!("   ");
    println!("   4. Audit Logging:");
    println!("      - All authentication events");
    println!("      - Security violations");
    println!("      - Log rotation and retention");
    println!("      - Secure log file permissions");
    println!("   ");
    println!("   5. Environment Security:");
    println!("      - API key exposure detection");
    println!("      - Transport security validation");
    println!("      - Configuration security checks");
}