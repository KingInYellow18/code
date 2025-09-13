/// Example demonstrating Claude authentication integration for agent environments
/// 
/// This example shows how to:
/// 1. Initialize Claude authentication system
/// 2. Setup quota management for multiple agents
/// 3. Coordinate agent sessions to prevent quota conflicts
/// 4. Monitor and manage Claude usage

use std::sync::Arc;
use tokio::time::{sleep, Duration};

use claude_auth_integration::{
    ClaudeQuotaManager, ClaudeAuthType, AgentAuthCoordinator,
    initialize_claude_auth_system, get_claude_auth_coordinator, ClaudeAuthCLI
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Claude Authentication Integration Example\n");
    
    // Set up environment for testing
    std::env::set_var("CLAUDE_API_KEY", "test-claude-key-12345");
    std::env::set_var("ANTHROPIC_API_KEY", "test-anthropic-key-67890");
    
    // 1. Initialize Claude authentication system
    println!("1️⃣ Initializing Claude authentication system...");
    match initialize_claude_auth_system().await {
        Ok(_) => println!("✅ System initialized successfully\n"),
        Err(e) => println!("❌ Failed to initialize: {}\n", e),
    }
    
    // 2. Test authentication detection
    println!("2️⃣ Testing Claude authentication detection...");
    let auth_test = ClaudeAuthCLI::test_auth().await;
    println!("{}\n", auth_test);
    
    // 3. Show initial status
    println!("3️⃣ Initial Claude authentication status...");
    let status = ClaudeAuthCLI::status().await;
    println!("{}\n", status);
    
    // 4. Simulate multiple agents using Claude authentication
    println!("4️⃣ Simulating multi-agent Claude usage...");
    if let Some(coordinator) = get_claude_auth_coordinator().await {
        
        // Try to allocate multiple agents
        let mut agent_ids = Vec::new();
        for i in 1..=5 {
            let agent_id = format!("claude_agent_{}", i);
            
            match coordinator.setup_claude_agent_auth(&agent_id).await {
                Ok(auth_env) => {
                    println!("✅ Agent {} authenticated successfully", agent_id);
                    println!("   Session ID: {}", auth_env.session_id);
                    println!("   Quota allocated: {} tokens", 
                        auth_env.quota_allocation.as_ref().map(|q| q.max_tokens_allocated).unwrap_or(0));
                    agent_ids.push(agent_id);
                }
                Err(e) => {
                    println!("❌ Agent {} failed to authenticate: {}", agent_id, e);
                    break;
                }
            }
        }
        
        println!();
        
        // 5. Simulate agent activity
        println!("5️⃣ Simulating agent activity...");
        for (i, agent_id) in agent_ids.iter().enumerate() {
            let tokens_used = (i + 1) as u64 * 100; // Simulate different usage
            let requests_made = (i + 1) as u32;
            
            match coordinator.update_agent_activity(agent_id, tokens_used, requests_made).await {
                Ok(_) => println!("📊 Agent {} used {} tokens, made {} requests", 
                    agent_id, tokens_used, requests_made),
                Err(e) => println!("❌ Failed to update activity for {}: {}", agent_id, e),
            }
        }
        
        println!();
        
        // 6. Check rate limiting
        println!("6️⃣ Testing rate limiting...");
        for agent_id in &agent_ids {
            match coordinator.can_agent_make_request(agent_id).await {
                Ok(can_make_request) => {
                    println!("🚦 Agent {} can make request: {}", agent_id, can_make_request);
                }
                Err(e) => println!("❌ Rate limit check failed for {}: {}", agent_id, e),
            }
        }
        
        println!();
        
        // 7. Show detailed quota information
        println!("7️⃣ Current quota details...");
        let quota_info = ClaudeAuthCLI::quota_info().await;
        println!("{}\n", quota_info);
        
        // 8. Simulate agent completion and cleanup
        println!("8️⃣ Simulating agent completion...");
        for agent_id in &agent_ids {
            match coordinator.release_agent_auth(agent_id).await {
                Ok(_) => println!("🧹 Agent {} authentication released", agent_id),
                Err(e) => println!("❌ Failed to release auth for {}: {}", agent_id, e),
            }
        }
        
        println!();
        
        // 9. Final status check
        println!("9️⃣ Final system status...");
        let final_status = ClaudeAuthCLI::status().await;
        println!("{}\n", final_status);
        
    } else {
        println!("❌ Claude authentication coordinator not available\n");
    }
    
    // 10. Demonstrate manual quota management
    println!("🔟 Direct quota manager usage example...");
    demonstrate_quota_manager().await?;
    
    println!("✅ Claude Authentication Integration Example Complete!");
    
    Ok(())
}

/// Demonstrate direct usage of ClaudeQuotaManager
async fn demonstrate_quota_manager() -> Result<(), Box<dyn std::error::Error>> {
    // Create a quota manager for API key usage
    let quota_manager = Arc::new(ClaudeQuotaManager::new_api_key(60, 100_000, 3));
    
    println!("📋 Created quota manager for API key (60 req/min, 100k tokens/min, 3 concurrent agents)");
    
    // Test quota allocation
    let agent_ids = vec!["demo_agent_1", "demo_agent_2", "demo_agent_3"];
    
    for agent_id in &agent_ids {
        match quota_manager.allocate_agent_quota(agent_id).await {
            Ok(allocation) => {
                println!("✅ Allocated quota for {}: {} req/min, {} tokens", 
                    agent_id, allocation.max_requests_per_minute, allocation.max_tokens_allocated);
            }
            Err(e) => println!("❌ Failed to allocate quota for {}: {}", agent_id, e),
        }
    }
    
    // Try to allocate a fourth agent (should fail due to concurrent limit)
    match quota_manager.allocate_agent_quota("demo_agent_4").await {
        Ok(_) => println!("❌ Unexpected: fourth agent was allocated (should have failed)"),
        Err(e) => println!("✅ Expected failure for fourth agent: {}", e),
    }
    
    // Check quota status
    let status = quota_manager.get_quota_status().await;
    println!("📊 Quota Status: {} active agents, {} req/min used", 
        status.active_agent_count, status.current_requests_per_minute);
    
    // Simulate some activity
    for agent_id in &agent_ids {
        quota_manager.update_agent_activity(agent_id, 50, 2).await?;
    }
    
    // Release agents
    for agent_id in &agent_ids {
        quota_manager.release_agent_quota(agent_id).await?;
        println!("🧹 Released quota for {}", agent_id);
    }
    
    let final_status = quota_manager.get_quota_status().await;
    println!("📊 Final Status: {} active agents", final_status.active_agent_count);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_quota_manager_creation() {
        let manager = ClaudeQuotaManager::new_max_subscription(10000, 5);
        assert!(manager.can_allocate_agent().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_agent_coordination() {
        let manager = Arc::new(ClaudeQuotaManager::new_api_key(60, 100_000, 2));
        let coordinator = AgentAuthCoordinator::new(manager);
        
        let auth_result = coordinator.setup_claude_agent_auth("test_agent").await;
        assert!(auth_result.is_ok());
        
        let cleanup_result = coordinator.release_agent_auth("test_agent").await;
        assert!(cleanup_result.is_ok());
    }
}