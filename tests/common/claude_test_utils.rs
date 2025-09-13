// Claude Authentication Test Utilities
// Shared helpers and fixtures for Claude authentication testing

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header, body_json_schema};

use codex_core::auth::{ClaudeAuth, ClaudeAuthMode, ClaudeTokenData, AuthManager};
use codex_core::config::Config;
use codex_protocol::mcp_protocol::AuthMode;

/// Mock Claude API server for testing
pub struct MockClaudeServer {
    pub server: MockServer,
    pub subscription_tier: String,
    pub quota_limit: u64,
    pub quota_used: u64,
    pub rate_limited: bool,
}

impl MockClaudeServer {
    /// Create a mock Claude server with Max subscription
    pub async fn with_max_subscription() -> Self {
        let server = MockServer::start().await;
        Self::setup_max_subscription_mocks(&server).await;
        
        Self {
            server,
            subscription_tier: "max".to_string(),
            quota_limit: 1000000,
            quota_used: 50000,
            rate_limited: false,
        }
    }
    
    /// Create a mock Claude server with quota exceeded
    pub async fn with_quota_exceeded() -> Self {
        let server = MockServer::start().await;
        Self::setup_quota_exceeded_mocks(&server).await;
        
        Self {
            server,
            subscription_tier: "max".to_string(),
            quota_limit: 100000,
            quota_used: 100000,
            rate_limited: true,
        }
    }
    
    /// Create a mock Claude server with OAuth errors
    pub async fn with_oauth_error() -> Self {
        let server = MockServer::start().await;
        Self::setup_oauth_error_mocks(&server).await;
        
        Self {
            server,
            subscription_tier: "unknown".to_string(),
            quota_limit: 0,
            quota_used: 0,
            rate_limited: false,
        }
    }
    
    /// Create a mock Claude server with Pro subscription (limited quota)
    pub async fn with_pro_subscription() -> Self {
        let server = MockServer::start().await;
        Self::setup_pro_subscription_mocks(&server).await;
        
        Self {
            server,
            subscription_tier: "pro".to_string(),
            quota_limit: 50000,
            quota_used: 10000,
            rate_limited: false,
        }
    }
    
    pub fn uri(&self) -> String {
        self.server.uri()
    }
    
    async fn setup_max_subscription_mocks(server: &MockServer) {
        // Subscription endpoint
        Mock::given(method("GET"))
            .and(path("/v1/subscription"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tier": "max",
                "usage_limit": 1000000,
                "usage_current": 50000,
                "reset_date": "2024-01-01T00:00:00Z",
                "features": ["api_access", "high_quota", "priority_support"]
            })))
            .mount(server)
            .await;
            
        // Messages endpoint
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_test_claude",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "Claude response"}],
                "model": "claude-3-sonnet-20240229",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 15}
            })))
            .mount(server)
            .await;
            
        // OAuth token endpoint
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "claude_test_access_token",
                "refresh_token": "claude_test_refresh_token",
                "expires_in": 3600,
                "token_type": "Bearer",
                "scope": "api subscription"
            })))
            .mount(server)
            .await;
    }
    
    async fn setup_quota_exceeded_mocks(server: &MockServer) {
        // Subscription endpoint shows quota exceeded
        Mock::given(method("GET"))
            .and(path("/v1/subscription"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tier": "max",
                "usage_limit": 100000,
                "usage_current": 100000,
                "reset_date": "2024-01-01T00:00:00Z",
                "features": ["api_access", "high_quota", "priority_support"]
            })))
            .mount(server)
            .await;
            
        // Messages endpoint returns rate limit error
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "type": "error",
                "error": {
                    "type": "rate_limit_error",
                    "message": "Request rate limit exceeded. Please slow down your requests."
                }
            })))
            .mount(server)
            .await;
    }
    
    async fn setup_oauth_error_mocks(server: &MockServer) {
        // OAuth token endpoint returns error
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "The provided authorization grant is invalid, expired, revoked, or does not match the redirection URI."
            })))
            .mount(server)
            .await;
            
        // Subscription endpoint requires valid auth
        Mock::given(method("GET"))
            .and(path("/v1/subscription"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "type": "error",
                "error": {
                    "type": "authentication_error",
                    "message": "Invalid API key provided."
                }
            })))
            .mount(server)
            .await;
    }
    
    async fn setup_pro_subscription_mocks(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/v1/subscription"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tier": "pro",
                "usage_limit": 50000,
                "usage_current": 10000,
                "reset_date": "2024-01-01T00:00:00Z",
                "features": ["api_access", "medium_quota"]
            })))
            .mount(server)
            .await;
            
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg_test_claude_pro",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "Claude Pro response"}],
                "model": "claude-3-haiku-20240307",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 8, "output_tokens": 12}
            })))
            .mount(server)
            .await;
    }
}

/// Test utilities for creating Claude authentication objects
pub struct ClaudeTestUtils;

impl ClaudeTestUtils {
    /// Create a Claude auth with Max subscription tokens
    pub fn create_max_subscription_auth() -> ClaudeAuth {
        ClaudeAuth {
            mode: ClaudeAuthMode::MaxSubscription,
            subscription_tier: Some("max".to_string()),
            api_key: None,
            oauth_tokens: Some(Self::create_max_tokens()),
            client: reqwest::Client::new(),
        }
    }
    
    /// Create a Claude auth with Pro subscription tokens
    pub fn create_pro_subscription_auth() -> ClaudeAuth {
        ClaudeAuth {
            mode: ClaudeAuthMode::ProSubscription,
            subscription_tier: Some("pro".to_string()),
            api_key: None,
            oauth_tokens: Some(Self::create_pro_tokens()),
            client: reqwest::Client::new(),
        }
    }
    
    /// Create a Claude auth with API key only
    pub fn create_api_key_auth(api_key: &str) -> ClaudeAuth {
        ClaudeAuth {
            mode: ClaudeAuthMode::ApiKey,
            subscription_tier: None,
            api_key: Some(api_key.to_string()),
            oauth_tokens: None,
            client: reqwest::Client::new(),
        }
    }
    
    /// Create test tokens for Max subscription
    pub fn create_max_tokens() -> ClaudeTokenData {
        ClaudeTokenData {
            access_token: "claude_max_access_token".to_string(),
            refresh_token: Some("claude_max_refresh_token".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
            subscription_tier: "max".to_string(),
        }
    }
    
    /// Create test tokens for Pro subscription
    pub fn create_pro_tokens() -> ClaudeTokenData {
        ClaudeTokenData {
            access_token: "claude_pro_access_token".to_string(),
            refresh_token: Some("claude_pro_refresh_token".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(12),
            subscription_tier: "pro".to_string(),
        }
    }
    
    /// Create expiring tokens (for refresh testing)
    pub fn create_expiring_tokens() -> ClaudeTokenData {
        ClaudeTokenData {
            access_token: "claude_expiring_access_token".to_string(),
            refresh_token: Some("claude_expiring_refresh_token".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            subscription_tier: "max".to_string(),
        }
    }
    
    /// Create expired tokens (for refresh testing)
    pub fn create_expired_tokens() -> ClaudeTokenData {
        ClaudeTokenData {
            access_token: "claude_expired_access_token".to_string(),
            refresh_token: Some("claude_expired_refresh_token".to_string()),
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
            subscription_tier: "max".to_string(),
        }
    }
}

/// Test environment setup utilities
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub auth_manager: AuthManager,
    pub claude_server: Option<MockClaudeServer>,
    pub openai_server: Option<MockServer>,
}

impl TestEnvironment {
    /// Create a new test environment with both Claude and OpenAI mocks
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("create temp dir");
        let auth_manager = AuthManager::new(
            temp_dir.path().to_path_buf(),
            AuthMode::ChatGPT,
            "test_client".to_string(),
        );
        
        Self {
            temp_dir,
            auth_manager,
            claude_server: None,
            openai_server: None,
        }
    }
    
    /// Add Claude Max subscription to the environment
    pub async fn with_claude_max(&mut self) -> &mut Self {
        let claude_server = MockClaudeServer::with_max_subscription().await;
        let claude_auth = ClaudeTestUtils::create_max_subscription_auth();
        
        self.auth_manager.add_claude_provider(claude_auth)
            .await
            .expect("add Claude provider");
        self.claude_server = Some(claude_server);
        self
    }
    
    /// Add Claude with quota exceeded to the environment
    pub async fn with_claude_quota_exceeded(&mut self) -> &mut Self {
        let claude_server = MockClaudeServer::with_quota_exceeded().await;
        let claude_auth = ClaudeTestUtils::create_max_subscription_auth();
        
        self.auth_manager.add_claude_provider(claude_auth)
            .await
            .expect("add Claude provider");
        self.claude_server = Some(claude_server);
        self
    }
    
    /// Add OpenAI to the environment
    pub async fn with_openai(&mut self) -> &mut Self {
        let openai_server = MockServer::start().await;
        
        // Setup OpenAI mock responses
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "gpt-4",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "OpenAI response"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 15,
                    "total_tokens": 25
                }
            })))
            .mount(&openai_server)
            .await;
            
        self.openai_server = Some(openai_server);
        self
    }
    
    pub fn codex_home(&self) -> &Path {
        self.temp_dir.path()
    }
    
    pub fn claude_uri(&self) -> Option<String> {
        self.claude_server.as_ref().map(|s| s.uri())
    }
    
    pub fn openai_uri(&self) -> Option<String> {
        self.openai_server.as_ref().map(|s| s.uri())
    }
}

/// Assertion helpers for Claude authentication
pub struct ClaudeTestAssertions;

impl ClaudeTestAssertions {
    /// Assert that two Claude auth objects are equivalent
    pub fn assert_auth_equivalent(a: &ClaudeAuth, b: &ClaudeAuth) {
        assert_eq!(a.mode, b.mode, "Auth modes should be equal");
        assert_eq!(a.subscription_tier, b.subscription_tier, "Subscription tiers should be equal");
        assert_eq!(a.api_key, b.api_key, "API keys should be equal");
        
        match (&a.oauth_tokens, &b.oauth_tokens) {
            (Some(tokens_a), Some(tokens_b)) => {
                assert_eq!(tokens_a.access_token, tokens_b.access_token, "Access tokens should be equal");
                assert_eq!(tokens_a.refresh_token, tokens_b.refresh_token, "Refresh tokens should be equal");
                assert_eq!(tokens_a.subscription_tier, tokens_b.subscription_tier, "Token subscription tiers should be equal");
            }
            (None, None) => {},
            _ => panic!("OAuth tokens presence should match"),
        }
    }
    
    /// Assert that environment variables are correctly set for Claude
    pub fn assert_claude_environment_variables(env: &HashMap<String, String>) {
        assert!(env.contains_key("ANTHROPIC_API_KEY"), "Should have ANTHROPIC_API_KEY");
        assert!(env.contains_key("CLAUDE_API_KEY"), "Should have CLAUDE_API_KEY");
        
        let anthropic_key = env.get("ANTHROPIC_API_KEY").unwrap();
        let claude_key = env.get("CLAUDE_API_KEY").unwrap();
        assert_eq!(anthropic_key, claude_key, "ANTHROPIC_API_KEY and CLAUDE_API_KEY should be equal");
        
        // Check for Claude-specific environment variables
        if let Some(subscription) = env.get("CLAUDE_SUBSCRIPTION") {
            assert!(["max", "pro", "api"].contains(&subscription.as_str()), 
                   "CLAUDE_SUBSCRIPTION should be a valid tier");
        }
    }
    
    /// Assert that OpenAI environment variables are correctly set
    pub fn assert_openai_environment_variables(env: &HashMap<String, String>) {
        assert!(env.contains_key("OPENAI_API_KEY"), "Should have OPENAI_API_KEY");
        
        // Should not have Claude variables when using OpenAI
        assert!(!env.contains_key("ANTHROPIC_API_KEY"), "Should not have ANTHROPIC_API_KEY when using OpenAI");
        assert!(!env.contains_key("CLAUDE_API_KEY"), "Should not have CLAUDE_API_KEY when using OpenAI");
    }
    
    /// Assert that quota allocation is within expected bounds
    pub fn assert_quota_allocation_valid(allocated: u64, requested: u64, available: u64) {
        assert!(allocated <= requested, "Allocated quota should not exceed requested");
        assert!(allocated <= available, "Allocated quota should not exceed available");
        
        if available >= requested {
            assert_eq!(allocated, requested, "Should allocate full requested amount when available");
        } else {
            assert_eq!(allocated, available, "Should allocate all available when insufficient");
        }
    }
    
    /// Assert that token data is valid
    pub fn assert_valid_token_data(tokens: &ClaudeTokenData) {
        assert!(!tokens.access_token.is_empty(), "Access token should not be empty");
        assert!(tokens.refresh_token.is_some(), "Refresh token should be present");
        assert!(tokens.expires_at > chrono::Utc::now(), "Token should not be expired");
        assert!(["max", "pro", "free"].contains(&tokens.subscription_tier.as_str()), 
               "Subscription tier should be valid");
    }
    
    /// Assert that performance metrics meet requirements
    pub fn assert_performance_requirements(duration: std::time::Duration, operation: &str) {
        match operation {
            "cached_auth" => {
                assert!(duration < std::time::Duration::from_millis(100), 
                       "Cached authentication should be < 100ms, was {:?}", duration);
            }
            "token_refresh" => {
                assert!(duration < std::time::Duration::from_secs(5), 
                       "Token refresh should be < 5s, was {:?}", duration);
            }
            "provider_selection" => {
                assert!(duration < std::time::Duration::from_millis(50), 
                       "Provider selection should be < 50ms, was {:?}", duration);
            }
            "quota_allocation" => {
                assert!(duration < std::time::Duration::from_millis(100), 
                       "Quota allocation should be < 100ms, was {:?}", duration);
            }
            _ => {
                // General performance requirement
                assert!(duration < std::time::Duration::from_secs(1), 
                       "{} should be < 1s, was {:?}", operation, duration);
            }
        }
    }
}

/// Test fixtures and data
pub struct TestFixtures;

impl TestFixtures {
    /// Load test subscription response JSON
    pub fn claude_max_subscription_response() -> serde_json::Value {
        serde_json::json!({
            "tier": "max",
            "usage_limit": 1000000,
            "usage_current": 50000,
            "reset_date": "2024-01-01T00:00:00Z",
            "features": ["api_access", "high_quota", "priority_support"]
        })
    }
    
    /// Load test OAuth token response JSON
    pub fn claude_oauth_token_response() -> serde_json::Value {
        serde_json::json!({
            "access_token": "claude_test_access_token",
            "refresh_token": "claude_test_refresh_token",
            "expires_in": 3600,
            "token_type": "Bearer",
            "scope": "api subscription"
        })
    }
    
    /// Load test quota exceeded response JSON
    pub fn claude_quota_exceeded_response() -> serde_json::Value {
        serde_json::json!({
            "type": "error",
            "error": {
                "type": "rate_limit_error",
                "message": "Request rate limit exceeded. Please slow down your requests."
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mock_claude_server_max_subscription() {
        let mock_server = MockClaudeServer::with_max_subscription().await;
        
        // Test subscription endpoint
        let client = reqwest::Client::new();
        let response = client
            .get(&format!("{}/v1/subscription", mock_server.uri()))
            .send()
            .await
            .expect("send request");
            
        assert_eq!(response.status(), 200);
        
        let subscription: serde_json::Value = response.json().await.expect("parse JSON");
        assert_eq!(subscription["tier"], "max");
        assert_eq!(subscription["usage_limit"], 1000000);
    }
    
    #[test]
    fn test_claude_test_utils() {
        let max_auth = ClaudeTestUtils::create_max_subscription_auth();
        assert_eq!(max_auth.mode, ClaudeAuthMode::MaxSubscription);
        assert_eq!(max_auth.subscription_tier, Some("max".to_string()));
        
        let api_auth = ClaudeTestUtils::create_api_key_auth("sk-ant-test");
        assert_eq!(api_auth.mode, ClaudeAuthMode::ApiKey);
        assert_eq!(api_auth.api_key, Some("sk-ant-test".to_string()));
    }
    
    #[tokio::test]
    async fn test_environment_setup() {
        let mut env = TestEnvironment::new().await;
        env.with_claude_max().await;
        env.with_openai().await;
        
        assert!(env.claude_uri().is_some());
        assert!(env.openai_uri().is_some());
    }
}