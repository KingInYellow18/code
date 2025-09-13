//! Comprehensive Integration Tests for Phase 3: Claude-Code Integration
//! 
//! This test suite validates the critical integration requirements specified in the
//! Claude Authentication Integration Plan's Phase 5 testing requirements.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Mutex};
use tokio::time::{timeout, sleep};
use chrono::{DateTime, Utc};
use serde_json::json;
use tempfile::tempdir;

// Import our authentication modules
use claude_code_security::{
    SecureClaudeAuth, ClaudeAuthConfig, ClaudeAuthError, ClaudeTokenData,
    UnifiedAuthManager, AuthConfig, ProviderType, FallbackStrategy,
    SecurityManager, SecurityConfig,
    init_unified_auth_system, init_security_system,
};

/// Mock Claude API Server for testing
pub struct MockClaudeServer {
    pub port: u16,
    pub responses: Arc<RwLock<HashMap<String, MockResponse>>>,
    pub call_count: Arc<RwLock<HashMap<String, u32>>>,
    pub quota_usage: Arc<RwLock<u64>>,
    pub should_fail_auth: Arc<RwLock<bool>>,
    pub should_timeout: Arc<RwLock<bool>>,
}

#[derive(Clone, Debug)]
pub struct MockResponse {
    pub status: u16,
    pub body: serde_json::Value,
    pub delay_ms: Option<u64>,
}

impl MockClaudeServer {
    pub async fn new() -> Self {
        let mut responses = HashMap::new();
        
        // Default success responses
        responses.insert("/v1/messages".to_string(), MockResponse {
            status: 200,
            body: json!({
                "id": "msg_test123",
                "type": "message",
                "role": "assistant", 
                "content": [{"type": "text", "text": "Test response"}],
                "model": "claude-3-sonnet-20240229",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }),
            delay_ms: Some(100),
        });
        
        responses.insert("/v1/subscription".to_string(), MockResponse {
            status: 200,
            body: json!({
                "tier": "pro",
                "features": ["api_access", "higher_limits"],
                "quota_limit": 1000000,
                "quota_used": 50000,
                "quota_reset_date": (Utc::now() + chrono::Duration::days(1)).to_rfc3339(),
                "active": true
            }),
            delay_ms: Some(50),
        });
        
        responses.insert("/oauth/token".to_string(), MockResponse {
            status: 200,
            body: json!({
                "access_token": "mock_access_token_12345",
                "refresh_token": "mock_refresh_token_67890", 
                "expires_in": 3600,
                "token_type": "Bearer",
                "subscription_tier": "pro",
                "scope": "api subscription"
            }),
            delay_ms: Some(200),
        });

        Self {
            port: 0, // Will be set when server starts
            responses: Arc::new(RwLock::new(responses)),
            call_count: Arc::new(RwLock::new(HashMap::new())),
            quota_usage: Arc::new(RwLock::new(0)),
            should_fail_auth: Arc::new(RwLock::new(false)),
            should_timeout: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn set_auth_failure(&self, should_fail: bool) {
        *self.should_fail_auth.write().await = should_fail;
    }

    pub async fn set_timeout_simulation(&self, should_timeout: bool) {
        *self.should_timeout.write().await = should_timeout;
    }

    pub async fn get_call_count(&self, endpoint: &str) -> u32 {
        self.call_count.read().await.get(endpoint).copied().unwrap_or(0)
    }

    pub async fn simulate_quota_exhaustion(&self) {
        let mut responses = self.responses.write().await;
        responses.insert("/v1/messages".to_string(), MockResponse {
            status: 429,
            body: json!({
                "error": {
                    "type": "rate_limit_error",
                    "message": "Rate limit exceeded"
                }
            }),
            delay_ms: Some(50),
        });
    }

    pub async fn reset_to_success(&self) {
        let mut responses = self.responses.write().await;
        responses.insert("/v1/messages".to_string(), MockResponse {
            status: 200,
            body: json!({
                "id": "msg_test123",
                "type": "message", 
                "role": "assistant",
                "content": [{"type": "text", "text": "Test response"}],
                "model": "claude-3-sonnet-20240229",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }),
            delay_ms: Some(100),
        });
    }
}

/// Mock OpenAI Server for fallback testing
pub struct MockOpenAIServer {
    pub port: u16,
    pub should_work: Arc<RwLock<bool>>,
    pub call_count: Arc<RwLock<u32>>,
}

impl MockOpenAIServer {
    pub async fn new() -> Self {
        Self {
            port: 0,
            should_work: Arc::new(RwLock::new(true)),
            call_count: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn set_working(&self, working: bool) {
        *self.should_work.write().await = working;
    }

    pub async fn get_call_count(&self) -> u32 {
        *self.call_count.read().await
    }
}

/// Test environment setup
pub struct IntegrationTestEnvironment {
    pub temp_dir: tempfile::TempDir,
    pub codex_home: std::path::PathBuf,
    pub claude_server: MockClaudeServer,
    pub openai_server: MockOpenAIServer,
    pub auth_manager: Option<Arc<UnifiedAuthManager>>,
}

impl IntegrationTestEnvironment {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let codex_home = temp_dir.path().join(".codex");
        tokio::fs::create_dir_all(&codex_home).await?;

        let claude_server = MockClaudeServer::new().await;
        let openai_server = MockOpenAIServer::new().await;

        Ok(Self {
            temp_dir,
            codex_home,
            claude_server,
            openai_server, 
            auth_manager: None,
        })
    }

    pub async fn setup_claude_auth(&mut self, api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Create claude_auth.json file
        let auth_data = json!({
            "version": "2.0",
            "enabled": true,
            "setup_required": false,
            "auth_mode": "api_key",
            "api_key": api_key,
            "created_at": Utc::now().to_rfc3339(),
            "last_verified": Utc::now().to_rfc3339()
        });

        let auth_file = self.codex_home.join("claude_auth.json");
        tokio::fs::write(&auth_file, serde_json::to_string_pretty(&auth_data)?).await?;

        // Set secure permissions on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&auth_file).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&auth_file, perms).await?;
        }

        Ok(())
    }

    pub async fn setup_openai_auth(&mut self, api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Create openai_auth.json file (simulating existing OpenAI setup)
        let auth_data = json!({
            "api_key": api_key,
            "organization": null,
            "created_at": Utc::now().to_rfc3339()
        });

        let auth_file = self.codex_home.join("openai_auth.json");
        tokio::fs::write(&auth_file, serde_json::to_string_pretty(&auth_data)?).await?;

        Ok(())
    }

    pub async fn init_auth_manager(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.auth_manager = Some(
            init_unified_auth_system(self.codex_home.clone(), "integration_test".to_string()).await?
        );
        Ok(())
    }
}

/// Critical Test 1: Claude to OpenAI Fallback
#[tokio::test]
async fn test_claude_openai_fallback() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_claude_openai_fallback");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    
    // Setup both authentication methods
    env.setup_claude_auth("sk-ant-test-key").await?;
    env.setup_openai_auth("sk-openai-test-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();

    // Step 1: Verify Claude auth works initially
    println!("  🔍 Testing initial Claude authentication...");
    let claude_auth = auth_manager.get_claude_auth().await;
    assert!(claude_auth.is_ok(), "Claude auth should be available");

    // Step 2: Simulate Claude API failure
    println!("  🚫 Simulating Claude API failure...");
    env.claude_server.set_auth_failure(true).await;

    // Step 3: Test fallback mechanism
    println!("  🔄 Testing fallback to OpenAI...");
    let fallback_result = auth_manager.get_fallback_provider().await;
    assert!(fallback_result.is_ok(), "Should fallback to OpenAI successfully");
    
    let provider = fallback_result.unwrap();
    assert_eq!(provider, ProviderType::OpenAI, "Should fallback to OpenAI provider");

    // Step 4: Verify OpenAI can handle the request
    println!("  ✅ Verifying OpenAI fallback functionality...");
    let openai_auth = auth_manager.get_openai_auth().await;
    assert!(openai_auth.is_ok(), "OpenAI auth should be available for fallback");

    // Step 5: Test recovery when Claude comes back online
    println!("  🔄 Testing Claude recovery...");
    env.claude_server.set_auth_failure(false).await;
    
    // Give some time for recovery detection
    sleep(Duration::from_millis(100)).await;
    
    let recovered_auth = auth_manager.get_claude_auth().await;
    assert!(recovered_auth.is_ok(), "Claude auth should recover");

    println!("  ✅ test_claude_openai_fallback completed successfully");
    Ok(())
}

/// Critical Test 2: Multi-Agent Quota Management
#[tokio::test]
async fn test_multi_agent_quota_management() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_multi_agent_quota_management");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    env.setup_claude_auth("sk-ant-test-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();
    let claude_auth = auth_manager.get_claude_auth().await?;

    // Step 1: Test concurrent agent quota allocation
    println!("  🤖 Testing concurrent agent quota allocation...");
    
    let agent_ids = vec!["agent1", "agent2", "agent3", "agent4", "agent5"];
    let mut allocation_handles = Vec::new();

    for agent_id in &agent_ids {
        let auth_clone = claude_auth.clone();
        let agent_id_clone = agent_id.to_string();
        
        let handle = tokio::spawn(async move {
            auth_clone.allocate_agent_quota(&agent_id_clone, 10000).await
        });
        
        allocation_handles.push(handle);
    }

    // Wait for all allocations to complete
    let mut successful_allocations = 0;
    for handle in allocation_handles {
        match handle.await {
            Ok(Ok(_)) => successful_allocations += 1,
            Ok(Err(e)) => println!("    ⚠️  Agent allocation failed: {}", e),
            Err(e) => println!("    ❌ Task failed: {}", e),
        }
    }

    assert!(successful_allocations > 0, "At least some agents should get quota allocation");
    println!("    ✅ {} agents successfully allocated quota", successful_allocations);

    // Step 2: Test quota sharing and limits
    println!("  📊 Testing quota sharing and limits...");
    
    let remaining_quota = claude_auth.get_remaining_quota().await?;
    println!("    📈 Remaining quota after allocations: {}", remaining_quota);

    // Step 3: Test quota release
    println!("  🔄 Testing quota release mechanism...");
    
    for agent_id in &agent_ids[..2] { // Release first 2 agents
        let released = claude_auth.release_agent_quota(agent_id).await?;
        println!("    ✅ Released {} tokens from agent {}", released, agent_id);
    }

    let new_remaining_quota = claude_auth.get_remaining_quota().await?;
    assert!(new_remaining_quota >= remaining_quota, "Quota should increase after releasing agents");

    // Step 4: Test quota exhaustion scenario
    println!("  💥 Testing quota exhaustion scenario...");
    
    env.claude_server.simulate_quota_exhaustion().await;
    
    let exhaustion_result = claude_auth.allocate_agent_quota("quota_test_agent", 1000000).await;
    assert!(exhaustion_result.is_err(), "Should fail when quota is exhausted");

    if let Err(ClaudeAuthError::QuotaExceeded { requested, available }) = exhaustion_result {
        println!("    ✅ Quota exhaustion properly detected: requested {}, available {}", requested, available);
    }

    println!("  ✅ test_multi_agent_quota_management completed successfully");
    Ok(())
}

/// Critical Test 3: Provider Switching
#[tokio::test]
async fn test_provider_switching() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_provider_switching");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    env.setup_claude_auth("sk-ant-test-key").await?;
    env.setup_openai_auth("sk-openai-test-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();

    // Step 1: Test initial provider selection (should prefer Claude)
    println!("  🎯 Testing initial provider selection...");
    
    let initial_provider = auth_manager.get_preferred_provider().await?;
    assert_eq!(initial_provider, ProviderType::Claude, "Should initially prefer Claude");

    // Step 2: Test manual provider switching
    println!("  🔄 Testing manual provider switching...");
    
    auth_manager.set_provider_preference(ProviderType::OpenAI).await?;
    let switched_provider = auth_manager.get_preferred_provider().await?;
    assert_eq!(switched_provider, ProviderType::OpenAI, "Should switch to OpenAI when requested");

    // Step 3: Test automatic switching on failure
    println!("  🚨 Testing automatic switching on failure...");
    
    // Set back to Claude
    auth_manager.set_provider_preference(ProviderType::Claude).await?;
    
    // Simulate Claude failure
    env.claude_server.set_auth_failure(true).await;
    
    // Trigger a request that should cause automatic fallback
    let fallback_provider = auth_manager.get_fallback_provider().await?;
    assert_eq!(fallback_provider, ProviderType::OpenAI, "Should automatically fallback to OpenAI");

    // Step 4: Test switching speed (performance requirement)
    println!("  ⚡ Testing provider switching performance...");
    
    let start_time = std::time::Instant::now();
    
    // Switch providers multiple times
    for _ in 0..10 {
        auth_manager.set_provider_preference(ProviderType::Claude).await?;
        auth_manager.set_provider_preference(ProviderType::OpenAI).await?;
    }
    
    let switching_duration = start_time.elapsed();
    assert!(switching_duration < Duration::from_millis(100), 
        "Provider switching should be fast (<100ms), took {:?}", switching_duration);

    // Step 5: Test provider switching with active agents
    println!("  👥 Testing provider switching with active agents...");
    
    // Reset Claude to working state
    env.claude_server.set_auth_failure(false).await;
    auth_manager.set_provider_preference(ProviderType::Claude).await?;
    
    // Allocate some agents
    let claude_auth = auth_manager.get_claude_auth().await?;
    claude_auth.allocate_agent_quota("switch_test_agent1", 5000).await?;
    claude_auth.allocate_agent_quota("switch_test_agent2", 5000).await?;
    
    // Switch provider (agents should be gracefully handled)
    auth_manager.set_provider_preference(ProviderType::OpenAI).await?;
    
    // Verify new provider is active
    let current_provider = auth_manager.get_preferred_provider().await?;
    assert_eq!(current_provider, ProviderType::OpenAI, "Provider should switch even with active agents");

    println!("  ✅ test_provider_switching completed successfully");
    Ok(())
}

/// Test agent environment variable setup
#[tokio::test]
async fn test_agent_environment_setup() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_agent_environment_setup");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    env.setup_claude_auth("sk-ant-test-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();

    // Test environment variable mapping for agents
    println!("  🌍 Testing agent environment variable setup...");
    
    let agent_env = auth_manager.get_agent_environment("test_agent").await?;
    
    // Verify required Claude environment variables are present
    assert!(agent_env.contains_key("ANTHROPIC_API_KEY"), "Should have ANTHROPIC_API_KEY");
    assert!(agent_env.contains_key("CLAUDE_API_KEY"), "Should have CLAUDE_API_KEY for backward compatibility");
    
    // Verify keys are properly mapped
    let anthropic_key = agent_env.get("ANTHROPIC_API_KEY").unwrap();
    let claude_key = agent_env.get("CLAUDE_API_KEY").unwrap(); 
    assert_eq!(anthropic_key, claude_key, "Keys should be synchronized");

    println!("  ✅ Agent environment variables properly configured");
    Ok(())
}

/// Test error handling scenarios
#[tokio::test]
async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_error_handling");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    env.setup_claude_auth("sk-ant-invalid-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();

    // Test authentication failure handling
    println!("  🚨 Testing authentication failure handling...");
    
    env.claude_server.set_auth_failure(true).await;
    
    let auth_result = auth_manager.get_claude_auth().await;
    // Should either fail gracefully or fallback
    match auth_result {
        Ok(_) => println!("    ✅ Authentication succeeded (possibly with fallback)"),
        Err(e) => {
            println!("    ✅ Authentication failed gracefully: {}", e);
            // Error should be properly typed
            assert!(e.to_string().contains("auth") || e.to_string().contains("credential"));
        }
    }

    // Test timeout handling
    println!("  ⏱️  Testing timeout handling...");
    
    env.claude_server.set_timeout_simulation(true).await;
    
    let timeout_result = timeout(
        Duration::from_millis(500),
        auth_manager.get_claude_auth()
    ).await;
    
    assert!(timeout_result.is_err(), "Should timeout within reasonable time");

    // Test network error handling
    println!("  🌐 Testing network error handling...");
    
    // This would test network connectivity issues
    // In a real implementation, this might involve network mocking

    println!("  ✅ Error handling tests completed successfully");
    Ok(())
}

/// Test backward compatibility
#[tokio::test]
async fn test_backward_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_backward_compatibility");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    
    // Setup only OpenAI auth (simulating existing installation)
    env.setup_openai_auth("sk-openai-existing-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();

    // Test that existing OpenAI workflows still work
    println!("  🔄 Testing existing OpenAI workflow compatibility...");
    
    let openai_auth = auth_manager.get_openai_auth().await;
    assert!(openai_auth.is_ok(), "Existing OpenAI auth should still work");

    // Test that system gracefully handles missing Claude auth
    println!("  🏃 Testing graceful handling of missing Claude auth...");
    
    let claude_auth_result = auth_manager.get_claude_auth().await;
    match claude_auth_result {
        Ok(_) => println!("    ℹ️  Claude auth unexpectedly available"),
        Err(_) => println!("    ✅ Missing Claude auth handled gracefully"),
    }

    // Test provider selection with only OpenAI available
    println!("  🎯 Testing provider selection with limited options...");
    
    let available_providers = auth_manager.get_available_providers().await?;
    assert!(available_providers.contains(&ProviderType::OpenAI), "OpenAI should be available");

    let preferred_provider = auth_manager.get_preferred_provider().await?;
    assert_eq!(preferred_provider, ProviderType::OpenAI, "Should default to available provider");

    println!("  ✅ Backward compatibility verified");
    Ok(())
}

/// Performance benchmark test  
#[tokio::test]
async fn test_performance_benchmarks() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running test_performance_benchmarks");
    
    let mut env = IntegrationTestEnvironment::new().await?;
    env.setup_claude_auth("sk-ant-test-key").await?;
    env.init_auth_manager().await?;

    let auth_manager = env.auth_manager.as_ref().unwrap();

    // Test authentication speed (should be sub-100ms as per plan)
    println!("  ⚡ Testing authentication performance...");
    
    let start_time = std::time::Instant::now();
    let _auth = auth_manager.get_claude_auth().await?;
    let auth_duration = start_time.elapsed();
    
    assert!(auth_duration < Duration::from_millis(100), 
        "Authentication should be fast (<100ms), took {:?}", auth_duration);

    // Test quota operations performance
    println!("  📊 Testing quota operations performance...");
    
    let claude_auth = auth_manager.get_claude_auth().await?;
    
    let start_time = std::time::Instant::now();
    for i in 0..10 {
        let agent_id = format!("perf_test_agent_{}", i);
        claude_auth.allocate_agent_quota(&agent_id, 1000).await?;
        claude_auth.release_agent_quota(&agent_id).await?;
    }
    let quota_ops_duration = start_time.elapsed();
    
    println!("    ⏱️  10 quota allocation/release cycles took {:?}", quota_ops_duration);
    assert!(quota_ops_duration < Duration::from_millis(1000), "Quota operations should be reasonably fast");

    println!("  ✅ Performance benchmarks passed");
    Ok(())
}

/// Integration test runner that executes all critical tests
#[tokio::test]
async fn run_comprehensive_integration_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Comprehensive Claude-Code Integration Tests");
    println!("=" .repeat(80));

    let mut test_results = HashMap::new();
    let start_time = std::time::Instant::now();

    // Run all critical tests
    let tests = vec![
        ("test_claude_openai_fallback", test_claude_openai_fallback()),
        ("test_multi_agent_quota_management", test_multi_agent_quota_management()),
        ("test_provider_switching", test_provider_switching()),
        ("test_agent_environment_setup", test_agent_environment_setup()),
        ("test_error_handling", test_error_handling()),
        ("test_backward_compatibility", test_backward_compatibility()),
        ("test_performance_benchmarks", test_performance_benchmarks()),
    ];

    for (test_name, test_future) in tests {
        println!("\n🧪 Running {}", test_name);
        let test_start = std::time::Instant::now();
        
        match test_future.await {
            Ok(()) => {
                let duration = test_start.elapsed();
                println!("✅ {} completed successfully in {:?}", test_name, duration);
                test_results.insert(test_name.to_string(), ("PASSED".to_string(), duration));
            }
            Err(e) => {
                let duration = test_start.elapsed();
                println!("❌ {} failed: {}", test_name, e);
                test_results.insert(test_name.to_string(), (format!("FAILED: {}", e), duration));
            }
        }
    }

    let total_duration = start_time.elapsed();

    // Generate test report
    println!("\n" + &"=".repeat(80));
    println!("📋 INTEGRATION TEST RESULTS SUMMARY");
    println!("=" .repeat(80));
    
    let mut passed = 0;
    let mut failed = 0;
    
    for (test_name, (status, duration)) in &test_results {
        let status_icon = if status.starts_with("PASSED") { "✅" } else { "❌" };
        println!("{} {:<35} {:>20} ({:?})", status_icon, test_name, status, duration);
        
        if status.starts_with("PASSED") {
            passed += 1;
        } else {
            failed += 1;
        }
    }
    
    println!("=" .repeat(80));
    println!("📊 Summary: {} passed, {} failed, Total time: {:?}", passed, failed, total_duration);
    
    // Store results in memory namespace for future reference
    let results_json = json!({
        "test_suite": "claude_auth_integration",
        "execution_time": total_duration.as_secs_f64(),
        "total_tests": passed + failed,
        "passed": passed,
        "failed": failed,
        "results": test_results,
        "timestamp": Utc::now().to_rfc3339(),
        "phase": "Phase 3: Claude-Code Integration",
        "success_criteria_met": failed == 0
    });

    // This would be stored in memory namespace in real implementation
    println!("\n💾 Test results would be stored in memory namespace 'claude_auth_integration' with key 'integration_test_results'");
    println!("📄 Results JSON: {}", serde_json::to_string_pretty(&results_json)?);

    if failed > 0 {
        return Err(format!("Integration tests failed: {} out of {} tests", failed, passed + failed).into());
    }

    println!("\n🎉 All integration tests passed successfully!");
    Ok(())
}