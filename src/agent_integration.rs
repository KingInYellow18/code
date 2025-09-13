use chrono::Duration;

use crate::agent_auth::{init_claude_auth_coordinator, get_claude_auth_coordinator};
use crate::claude_quota::ClaudeAuthType;

/// Initialize Claude authentication for agent environments
/// This should be called during application startup
pub async fn initialize_claude_auth_system() -> Result<(), String> {
    match init_claude_auth_coordinator().await {
        Ok(_coordinator) => {
            println!("✅ Claude authentication system initialized successfully");
            
            // Optionally setup the global agent manager with Claude auth
            // Note: In real integration, you would setup the agent manager here
            // if let Ok(agent_manager) = codex_core::agent_tool::AGENT_MANAGER.try_read() {
            //     // Note: This would require modifying AGENT_MANAGER to be mutable
            //     // In practice, you'd call this during the manager's initialization
            //     println!("✅ Agent manager configured with Claude authentication");
            // }
            
            Ok(())
        }
        Err(e) => {
            println!("⚠️  Claude authentication not available: {}", e);
            println!("💡 Agents will run without Claude quota management");
            Ok(()) // Not a fatal error
        }
    }
}

/// Get status of Claude authentication system
pub async fn get_claude_auth_status() -> Option<String> {
    if let Some(coordinator) = get_claude_auth_coordinator().await {
        let health = coordinator.check_auth_health().await;
        
        let status = format!(
            "Claude Auth Status: {} | Active Agents: {} | Quota Usage: {}% | Rate Limit Usage: {}%",
            if health.healthy { "🟢 Healthy" } else { "🔴 Issues" },
            health.active_agent_count,
            health.quota_usage_percentage,
            health.rate_limit_usage_percentage
        );
        
        Some(status)
    } else {
        None
    }
}

/// Setup Claude authentication for an existing agent manager
/// This is a helper function for integration
/// Note: In real integration, this would use the actual AgentManager type
pub async fn setup_agent_manager_claude_auth(
    // agent_manager: &Arc<RwLock<codex_core::agent_tool::AgentManager>>
) -> Result<(), String> {
    if let Some(_coordinator) = get_claude_auth_coordinator().await {
        // In real integration:
        // let mut manager = agent_manager.write().await;
        // manager.set_claude_auth_coordinator(coordinator);
        println!("✅ Claude authentication would be setup for agent manager");
        Ok(())
    } else {
        Err("Claude authentication coordinator not initialized".to_string())
    }
}

/// Cleanup inactive Claude agent sessions
/// Should be called periodically (e.g., every 10 minutes)
pub async fn cleanup_inactive_claude_sessions() -> usize {
    if let Some(coordinator) = get_claude_auth_coordinator().await {
        let inactive_duration = Duration::minutes(30); // 30 minutes of inactivity
        coordinator.cleanup_inactive_agents(inactive_duration).await
    } else {
        0
    }
}

/// Get detailed Claude quota and session information
pub async fn get_claude_quota_details() -> Option<serde_json::Value> {
    if let Some(coordinator) = get_claude_auth_coordinator().await {
        let health = coordinator.check_auth_health().await;
        let active_agents = coordinator.get_active_agents().await;
        
        let details = serde_json::json!({
            "health": {
                "healthy": health.healthy,
                "active_agent_count": health.active_agent_count,
                "quota_usage_percentage": health.quota_usage_percentage,
                "rate_limit_usage_percentage": health.rate_limit_usage_percentage,
                "can_allocate_new_agents": health.can_allocate_new_agents
            },
            "active_agents": active_agents,
            "system_info": {
                "auth_type": "detected",
                "concurrent_limit": "auto-detected",
                "cleanup_enabled": true
            }
        });
        
        Some(details)
    } else {
        None
    }
}

/// Integration helper for Claude Code CLI commands
pub struct ClaudeAuthCLI;

impl ClaudeAuthCLI {
    /// Show Claude authentication status
    pub async fn status() -> String {
        match get_claude_auth_status().await {
            Some(status) => status,
            None => "Claude authentication not initialized".to_string(),
        }
    }
    
    /// Show detailed quota information
    pub async fn quota_info() -> String {
        match get_claude_quota_details().await {
            Some(details) => serde_json::to_string_pretty(&details).unwrap_or_else(|_| "Error formatting details".to_string()),
            None => "Claude quota system not available".to_string(),
        }
    }
    
    /// Cleanup inactive sessions manually
    pub async fn cleanup() -> String {
        let cleaned = cleanup_inactive_claude_sessions().await;
        format!("Cleaned up {} inactive Claude sessions", cleaned)
    }
    
    /// Test Claude authentication
    pub async fn test_auth() -> String {
        match ClaudeAuthType::detect_from_env() {
            Some(auth_type) => {
                match auth_type {
                    ClaudeAuthType::Max { daily_limit } => {
                        format!("✅ Claude Max subscription detected (daily limit: {} tokens)", daily_limit)
                    }
                    ClaudeAuthType::ApiKey { requests_per_minute, tokens_per_minute } => {
                        format!("✅ Claude API key detected (limits: {} req/min, {} tokens/min)", requests_per_minute, tokens_per_minute)
                    }
                }
            }
            None => "❌ No Claude authentication found in environment".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_claude_auth_detection() {
        // This test requires environment variables to be set
        std::env::set_var("CLAUDE_API_KEY", "test-key");
        
        let auth_type = ClaudeAuthType::detect_from_env();
        assert!(auth_type.is_some());
        
        std::env::remove_var("CLAUDE_API_KEY");
    }
}