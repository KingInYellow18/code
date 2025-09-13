// Claude Authentication Performance Tests
// Validation of performance requirements from the integration plan

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tempfile::TempDir;

use codex_core::auth::{AuthManager, ClaudeAuth, ClaudeQuotaManager};
use codex_core::performance::{PerformanceMetrics, AuthCacheMetrics};
use codex_protocol::mcp_protocol::AuthMode;

/// Test authentication caching performance - should be sub-100ms for cached tokens
#[tokio::test]
async fn test_authentication_caching() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Setup: Add Claude auth with valid tokens
    let claude_auth = ClaudeAuth::with_cached_tokens(create_test_claude_tokens());
    auth_manager.add_claude_provider(claude_auth).await.expect("add Claude provider");
    
    // Warm up cache with initial request
    let _ = auth_manager.get_access_token("claude").await.expect("warm up cache");
    
    // Test: Measure cached authentication performance
    let start_time = Instant::now();
    let cached_token = auth_manager.get_access_token("claude").await.expect("get cached token");
    let cache_duration = start_time.elapsed();
    
    // Validation: Cached authentication should be sub-100ms
    assert!(cache_duration < Duration::from_millis(100), 
            "Cached authentication should be < 100ms, was {:?}", cache_duration);
    assert!(!cached_token.is_empty(), "Should return valid cached token");
    
    // Test: Multiple rapid requests should hit cache
    let mut durations = Vec::new();
    for _ in 0..10 {
        let start = Instant::now();
        let _ = auth_manager.get_access_token("claude").await.expect("get token");
        durations.push(start.elapsed());
    }
    
    let avg_duration = durations.iter().sum::<Duration>() / durations.len() as u32;
    assert!(avg_duration < Duration::from_millis(50), 
            "Average cached request should be < 50ms, was {:?}", avg_duration);
    
    // Test: Cache hit rate should be high
    let cache_metrics = auth_manager.get_cache_metrics().await.expect("get cache metrics");
    assert!(cache_metrics.hit_rate > 0.9, 
            "Cache hit rate should be > 90%, was {:.2}", cache_metrics.hit_rate);
}

/// Test token refresh optimization
#[tokio::test]
async fn test_token_refresh_optimization() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Setup: Add Claude auth with expiring tokens
    let mut claude_tokens = create_test_claude_tokens();
    claude_tokens.expires_at = chrono::Utc::now() + chrono::Duration::seconds(5); // Expires soon
    
    let claude_auth = ClaudeAuth::with_tokens(claude_tokens);
    auth_manager.add_claude_provider(claude_auth).await.expect("add Claude provider");
    
    // Test: Measure token refresh performance
    let start_time = Instant::now();
    let refreshed_token = auth_manager.refresh_token("claude").await.expect("refresh token");
    let refresh_duration = start_time.elapsed();
    
    // Validation: Token refresh should complete in reasonable time
    assert!(refresh_duration < Duration::from_secs(5), 
            "Token refresh should be < 5s, was {:?}", refresh_duration);
    assert!(!refreshed_token.is_empty(), "Should return valid refreshed token");
    
    // Test: Background refresh should not block operations
    auth_manager.enable_background_refresh("claude", Duration::from_secs(30))
        .await.expect("enable background refresh");
    
    let operation_start = Instant::now();
    let _ = auth_manager.get_access_token("claude").await.expect("get token during refresh");
    let operation_duration = operation_start.elapsed();
    
    assert!(operation_duration < Duration::from_millis(100), 
            "Operations should not be blocked by background refresh");
    
    // Test: Concurrent refresh handling
    let refresh_tasks: Vec<_> = (0..5).map(|_| {
        let auth_manager = auth_manager.clone();
        tokio::spawn(async move {
            auth_manager.refresh_token("claude").await
        })
    }).collect();
    
    let results: Vec<_> = futures::future::join_all(refresh_tasks).await;
    let successful_refreshes = results.iter().filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok()).count();
    
    // Validation: Only one refresh should succeed, others should reuse result
    assert_eq!(successful_refreshes, 1, "Only one concurrent refresh should execute");
    
    // All should get the same refreshed token
    let tokens: Vec<_> = results.iter()
        .filter_map(|r| r.as_ref().ok()?.as_ref().ok())
        .collect();
    assert!(tokens.iter().all(|&t| t == tokens[0]), "All concurrent requests should get same token");
}

/// Test multi-agent coordination performance
#[tokio::test]
async fn test_multi_agent_coordination_performance() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let quota_manager = ClaudeQuotaManager::new(100000, 0).await; // Large quota
    
    // Test: Measure quota allocation performance for many agents
    let start_time = Instant::now();
    let allocation_tasks: Vec<_> = (0..100).map(|i| {
        let quota_manager = quota_manager.clone();
        tokio::spawn(async move {
            quota_manager.allocate_quota(&format!("agent_{}", i), 1000).await
        })
    }).collect();
    
    let allocation_results: Vec<_> = futures::future::join_all(allocation_tasks).await;
    let allocation_duration = start_time.elapsed();
    
    // Validation: Quota allocation should be fast even with many agents
    assert!(allocation_duration < Duration::from_secs(1), 
            "100 quota allocations should complete in < 1s, was {:?}", allocation_duration);
    
    let successful_allocations = allocation_results.iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();
    assert!(successful_allocations >= 95, "At least 95% of allocations should succeed");
    
    // Test: Measure agent environment setup performance
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    let claude_auth = ClaudeAuth::with_cached_tokens(create_test_claude_tokens());
    auth_manager.add_claude_provider(claude_auth).await.expect("add Claude provider");
    
    let env_setup_start = Instant::now();
    let env_setup_tasks: Vec<_> = (0..50).map(|i| {
        let auth_manager = auth_manager.clone();
        tokio::spawn(async move {
            auth_manager.setup_agent_environment(&format!("agent_{}", i)).await
        })
    }).collect();
    
    let env_results: Vec<_> = futures::future::join_all(env_setup_tasks).await;
    let env_setup_duration = env_setup_start.elapsed();
    
    // Validation: Agent environment setup should be fast
    assert!(env_setup_duration < Duration::from_millis(500), 
            "50 agent environments should setup in < 500ms, was {:?}", env_setup_duration);
    
    let successful_setups = env_results.iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();
    assert_eq!(successful_setups, 50, "All agent environment setups should succeed");
}

/// Test memory usage validation
#[tokio::test]
async fn test_memory_usage_validation() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let initial_memory = get_memory_usage();
    
    // Test: Create auth manager with multiple providers
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Add multiple Claude authentications
    for i in 0..10 {
        let claude_auth = ClaudeAuth::with_tokens(create_test_claude_tokens());
        auth_manager.add_claude_provider_with_id(&format!("claude_{}", i), claude_auth)
            .await.expect("add Claude provider");
    }
    
    let after_setup_memory = get_memory_usage();
    let setup_memory_increase = after_setup_memory - initial_memory;
    
    // Validation: Memory increase should be reasonable
    assert!(setup_memory_increase < 50 * 1024 * 1024, // 50MB
            "Auth setup should use < 50MB, used {}MB", 
            setup_memory_increase / (1024 * 1024));
    
    // Test: Simulate long-running session with many operations
    for round in 0..100 {
        // Simulate authentication operations
        for i in 0..10 {
            let _ = auth_manager.get_access_token(&format!("claude_{}", i % 10)).await;
            let _ = auth_manager.setup_agent_environment(&format!("agent_{}_{}", round, i)).await;
        }
        
        // Force garbage collection periodically
        if round % 10 == 0 {
            // Rust doesn't have explicit GC, but we can simulate cleanup
            auth_manager.cleanup_expired_sessions().await.expect("cleanup sessions");
        }
    }
    
    let final_memory = get_memory_usage();
    let total_memory_increase = final_memory - initial_memory;
    
    // Validation: Memory should not grow unbounded
    assert!(total_memory_increase < 100 * 1024 * 1024, // 100MB
            "Total memory increase should be < 100MB, was {}MB", 
            total_memory_increase / (1024 * 1024));
    
    // Test: Memory should be reclaimed after cleanup
    auth_manager.cleanup_all_providers().await.expect("cleanup all providers");
    tokio::time::sleep(Duration::from_millis(100)).await; // Allow cleanup to complete
    
    let after_cleanup_memory = get_memory_usage();
    let memory_reclaimed = final_memory - after_cleanup_memory;
    
    assert!(memory_reclaimed > setup_memory_increase / 2, 
            "At least 50% of allocated memory should be reclaimed");
}

/// Test provider selection speed
#[tokio::test]
async fn test_provider_selection_performance() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Setup: Add multiple providers with different characteristics
    let claude_auth = ClaudeAuth::with_subscription("max");
    let openai_auth = OpenAIAuth::with_api_key("sk-test-key");
    
    auth_manager.add_claude_provider(claude_auth).await.expect("add Claude");
    auth_manager.add_openai_provider(openai_auth).await.expect("add OpenAI");
    
    // Test: Measure provider selection performance
    let selection_times = Arc::new(std::sync::Mutex::new(Vec::new()));
    
    let selection_tasks: Vec<_> = (0..1000).map(|i| {
        let auth_manager = auth_manager.clone();
        let times = selection_times.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            let _provider = auth_manager.select_optimal_provider(&TaskContext::new())
                .await.expect("select provider");
            let duration = start.elapsed();
            
            times.lock().unwrap().push(duration);
        })
    }).collect();
    
    futures::future::join_all(selection_tasks).await;
    
    let times = selection_times.lock().unwrap();
    let avg_selection_time = times.iter().sum::<Duration>() / times.len() as u32;
    let max_selection_time = *times.iter().max().unwrap();
    
    // Validation: Provider selection should be very fast
    assert!(avg_selection_time < Duration::from_millis(10), 
            "Average provider selection should be < 10ms, was {:?}", avg_selection_time);
    assert!(max_selection_time < Duration::from_millis(50), 
            "Max provider selection should be < 50ms, was {:?}", max_selection_time);
    
    // Test: Selection under load
    let load_test_start = Instant::now();
    let semaphore = Arc::new(Semaphore::new(100)); // Limit concurrency
    
    let load_tasks: Vec<_> = (0..10000).map(|_| {
        let auth_manager = auth_manager.clone();
        let semaphore = semaphore.clone();
        tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            auth_manager.select_optimal_provider(&TaskContext::new()).await
        })
    }).collect();
    
    let load_results: Vec<_> = futures::future::join_all(load_tasks).await;
    let load_test_duration = load_test_start.elapsed();
    
    // Validation: High load should not significantly degrade performance
    assert!(load_test_duration < Duration::from_secs(10), 
            "10k provider selections should complete in < 10s");
    
    let successful_selections = load_results.iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();
    assert!(successful_selections > 9950, "At least 99.5% of selections should succeed");
}

/// Test authentication under concurrent load
#[tokio::test]
async fn test_authentication_under_load() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let auth_manager = create_test_auth_manager(temp_dir.path()).await;
    
    // Setup: Add Claude auth
    let claude_auth = ClaudeAuth::with_cached_tokens(create_test_claude_tokens());
    auth_manager.add_claude_provider(claude_auth).await.expect("add Claude provider");
    
    // Test: Concurrent authentication requests
    let load_start = Instant::now();
    let auth_tasks: Vec<_> = (0..1000).map(|_| {
        let auth_manager = auth_manager.clone();
        tokio::spawn(async move {
            timeout(Duration::from_secs(5), 
                   auth_manager.get_access_token("claude")).await
        })
    }).collect();
    
    let auth_results: Vec<_> = futures::future::join_all(auth_tasks).await;
    let load_duration = load_start.elapsed();
    
    // Validation: High concurrency should not cause failures
    let successful_auths = auth_results.iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();
    
    assert!(successful_auths > 990, "At least 99% of concurrent auths should succeed");
    assert!(load_duration < Duration::from_secs(30), 
            "1000 concurrent auths should complete in < 30s");
    
    // Test: Performance degradation should be minimal
    let single_auth_start = Instant::now();
    let _token = auth_manager.get_access_token("claude").await.expect("single auth");
    let single_auth_duration = single_auth_start.elapsed();
    
    assert!(single_auth_duration < Duration::from_millis(200), 
            "Single auth after load should still be fast (< 200ms)");
}

// Helper functions

async fn create_test_auth_manager(codex_home: &std::path::Path) -> AuthManager {
    AuthManager::new(
        codex_home.to_path_buf(),
        AuthMode::ChatGPT,
        "performance_test_client".to_string(),
    )
}

fn create_test_claude_tokens() -> ClaudeTokenData {
    ClaudeTokenData {
        access_token: "claude_test_access_token".to_string(),
        refresh_token: Some("claude_test_refresh_token".to_string()),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        subscription_tier: "max".to_string(),
    }
}

fn get_memory_usage() -> usize {
    // Platform-specific memory usage measurement
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("ps")
            .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
            .output() 
        {
            if let Ok(rss_str) = String::from_utf8(output.stdout) {
                if let Ok(kb) = rss_str.trim().parse::<usize>() {
                    return kb * 1024; // Convert KB to bytes
                }
            }
        }
    }
    
    // Fallback for other platforms or if measurement fails
    0
}

struct TaskContext {
    agent_id: String,
    estimated_tokens: u64,
    priority: TaskPriority,
}

impl TaskContext {
    fn new() -> Self {
        Self {
            agent_id: "test_agent".to_string(),
            estimated_tokens: 1000,
            priority: TaskPriority::Medium,
        }
    }
}

#[derive(Clone)]
enum TaskPriority {
    Low,
    Medium,
    High,
}