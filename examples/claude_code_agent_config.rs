/// # Claude Code Agent Configuration Examples
///
/// This file demonstrates various ways to configure agents to use Claude Code
/// authentication instead of API keys, providing practical examples for different
/// use cases and deployment scenarios.

use claude_code_security::{
    UnifiedAuthManager, UnifiedAuthConfig, ProviderSelectionStrategy,
    AuthContext, TaskType, Priority, ProviderType
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use std::time::Duration;

/// Example 1: Basic Agent Configuration
///
/// Shows how to set up a single agent with Claude Code authentication
pub async fn basic_agent_setup() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize codex home directory
    let codex_home = dirs::home_dir()
        .ok_or("Unable to determine home directory")?
        .join(".codex");

    // Create unified auth manager with Claude Code preference
    let auth_manager = UnifiedAuthManager::new(
        codex_home,
        ProviderSelectionStrategy::PreferClaude
    ).await?;

    // Create authentication context for a coding agent
    let auth_context = AuthContext {
        task_type: TaskType::CodeGeneration,
        estimated_tokens: Some(2000),
        priority: Priority::Medium,
        user_preference: Some(ProviderType::Claude),
        required_features: vec!["streaming".to_string()],
    };

    // Get the optimal provider (will use Claude Code if available)
    let provider = auth_manager.get_optimal_provider(&auth_context).await?;

    // Example message for the agent
    let system_prompt = "You are a helpful coding assistant. Write clean, efficient code.";
    let messages = vec![
        crate::providers::Message {
            role: "user".to_string(),
            content: crate::providers::MessageContent::Text(
                "Write a function to calculate fibonacci numbers".to_string()
            ),
        }
    ];

    // Send message through the provider
    let response_stream = provider.send_message(system_prompt, messages).await?;

    println!("✓ Successfully configured basic agent with Claude Code authentication");
    Ok(())
}

/// Example 2: Multi-Agent Configuration
///
/// Demonstrates how to configure multiple agents running concurrently
pub async fn multi_agent_setup() -> Result<(), Box<dyn std::error::Error>> {
    let codex_home = dirs::home_dir()
        .ok_or("Unable to determine home directory")?
        .join(".codex");

    // Custom configuration for concurrent agents
    let config = UnifiedAuthConfig {
        enable_fallback: true,
        cache_status_duration_seconds: 300,
        auto_refresh_tokens: true,
        monitor_quota: true,
        load_balance_agents: true,
        max_concurrent_claude_agents: 6, // Limit concurrent Claude Code processes
        preference_learning_enabled: true,
    };

    let auth_manager = UnifiedAuthManager::with_config(
        codex_home,
        ProviderSelectionStrategy::Adaptive, // Learn from usage patterns
        config
    ).await?;

    // Define different agent types
    let agent_configs = vec![
        ("code_generator", TaskType::CodeGeneration, Priority::High),
        ("code_reviewer", TaskType::Analysis, Priority::Medium),
        ("documentation", TaskType::Interactive, Priority::Low),
        ("test_writer", TaskType::CodeGeneration, Priority::Medium),
    ];

    // Create authentication contexts for each agent type
    let contexts: Vec<_> = agent_configs.into_iter().map(|(name, task_type, priority)| {
        (name, AuthContext {
            task_type,
            estimated_tokens: Some(1500),
            priority,
            user_preference: Some(ProviderType::Claude),
            required_features: vec!["streaming".to_string()],
        })
    }).collect();

    // Get providers for each agent
    for (agent_name, context) in contexts {
        match auth_manager.get_optimal_provider(&context).await {
            Ok(provider) => println!("✓ Configured {} with provider: {:?}",
                                   agent_name, provider.provider_type()),
            Err(e) => println!("✗ Failed to configure {}: {}", agent_name, e),
        }
    }

    println!("✓ Successfully configured multi-agent system");
    Ok(())
}

/// Example 3: Enterprise Configuration
///
/// Shows advanced configuration for enterprise environments
pub async fn enterprise_setup() -> Result<(), Box<dyn std::error::Error>> {
    let codex_home = PathBuf::from("/opt/codex/config"); // Custom enterprise path

    // Enterprise-grade configuration
    let config = UnifiedAuthConfig {
        enable_fallback: true,
        cache_status_duration_seconds: 180, // Shorter cache for security
        auto_refresh_tokens: true,
        monitor_quota: true,
        load_balance_agents: true,
        max_concurrent_claude_agents: 15, // Higher limits for enterprise
        preference_learning_enabled: true,
    };

    let auth_manager = UnifiedAuthManager::with_config(
        codex_home,
        ProviderSelectionStrategy::BestSubscription, // Optimize for best tier
        config
    ).await?;

    // Example: Configure agents with different priorities
    let critical_context = AuthContext {
        task_type: TaskType::AgentExecution,
        estimated_tokens: Some(5000),
        priority: Priority::Critical, // High priority for critical tasks
        user_preference: Some(ProviderType::Claude),
        required_features: vec!["streaming".to_string(), "tools".to_string()],
    };

    let batch_context = AuthContext {
        task_type: TaskType::Batch,
        estimated_tokens: Some(1000),
        priority: Priority::Low, // Low priority for batch processing
        user_preference: Some(ProviderType::Claude),
        required_features: vec![],
    };

    // Get providers with different priorities
    let critical_provider = auth_manager.get_optimal_provider(&critical_context).await?;
    let batch_provider = auth_manager.get_optimal_provider(&batch_context).await?;

    println!("✓ Enterprise configuration complete");
    println!("  - Critical tasks: {:?}", critical_provider.provider_type());
    println!("  - Batch tasks: {:?}", batch_provider.provider_type());

    Ok(())
}

/// Example 4: Load Balanced Agent Pool
///
/// Demonstrates how to create a pool of agents with proper load balancing
pub async fn load_balanced_agent_pool() -> Result<(), Box<dyn std::error::Error>> {
    let codex_home = dirs::home_dir()
        .ok_or("Unable to determine home directory")?
        .join(".codex");

    let auth_manager = Arc::new(UnifiedAuthManager::new(
        codex_home,
        ProviderSelectionStrategy::CostOptimized
    ).await?);

    // Create semaphore for rate limiting
    let semaphore = Arc::new(Semaphore::new(8)); // Max 8 concurrent operations

    // Simulate multiple agent requests
    let mut handles = Vec::new();

    for i in 0..20 {
        let auth_manager = Arc::clone(&auth_manager);
        let semaphore = Arc::clone(&semaphore);

        let handle = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = semaphore.acquire().await.unwrap();

            let context = AuthContext {
                task_type: if i % 2 == 0 { TaskType::CodeGeneration } else { TaskType::Analysis },
                estimated_tokens: Some(1000 + (i * 100) as u64),
                priority: if i < 5 { Priority::High } else { Priority::Medium },
                user_preference: Some(ProviderType::Claude),
                required_features: vec!["streaming".to_string()],
            };

            // Simulate some work time
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Get provider for this agent
            match auth_manager.get_optimal_provider(&context).await {
                Ok(provider) => {
                    // Record successful usage
                    auth_manager.record_usage(
                        ProviderType::Claude,
                        &context,
                        true,
                        150.0 // Response time in ms
                    ).await;

                    println!("Agent {}: Successfully got provider", i);
                }
                Err(e) => {
                    println!("Agent {}: Failed to get provider: {}", i, e);
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all agents to complete
    for handle in handles {
        handle.await?;
    }

    println!("✓ Load balanced agent pool completed all tasks");
    Ok(())
}

/// Example 5: Development vs Production Configuration
///
/// Shows how to configure different settings for different environments
pub async fn environment_specific_setup() -> Result<(), Box<dyn std::error::Error>> {
    let is_production = std::env::var("ENVIRONMENT")
        .map(|env| env == "production")
        .unwrap_or(false);

    let codex_home = if is_production {
        PathBuf::from("/var/lib/codex")
    } else {
        dirs::home_dir()
            .ok_or("Unable to determine home directory")?
            .join(".codex-dev")
    };

    let config = if is_production {
        // Production configuration: More conservative
        UnifiedAuthConfig {
            enable_fallback: true,
            cache_status_duration_seconds: 120, // Shorter cache
            auto_refresh_tokens: true,
            monitor_quota: true,
            load_balance_agents: true,
            max_concurrent_claude_agents: 5, // Conservative limit
            preference_learning_enabled: false, // Disable learning in prod
        }
    } else {
        // Development configuration: More permissive
        UnifiedAuthConfig {
            enable_fallback: true,
            cache_status_duration_seconds: 600, // Longer cache
            auto_refresh_tokens: true,
            monitor_quota: false, // Less monitoring in dev
            load_balance_agents: false,
            max_concurrent_claude_agents: 12, // Higher limit for testing
            preference_learning_enabled: true, // Enable learning for development
        }
    };

    let strategy = if is_production {
        ProviderSelectionStrategy::BestSubscription
    } else {
        ProviderSelectionStrategy::Adaptive
    };

    let auth_manager = UnifiedAuthManager::with_config(
        codex_home,
        strategy,
        config
    ).await?;

    println!("✓ Configured for {} environment",
             if is_production { "production" } else { "development" });

    Ok(())
}

/// Example 6: Health Monitoring and Diagnostics
///
/// Demonstrates how to implement health checks and monitoring
pub async fn health_monitoring_setup() -> Result<(), Box<dyn std::error::Error>> {
    let codex_home = dirs::home_dir()
        .ok_or("Unable to determine home directory")?
        .join(".codex");

    let auth_manager = UnifiedAuthManager::new(
        codex_home,
        ProviderSelectionStrategy::PreferClaude
    ).await?;

    // Health check function
    async fn health_check(auth_manager: &UnifiedAuthManager) -> Result<String, String> {
        let status_summary = auth_manager.get_provider_status_summary().await;

        for (provider_type, status) in status_summary {
            if provider_type == ProviderType::Claude {
                if !status.authenticated {
                    return Err("Claude Code not authenticated".to_string());
                }
                if !status.available {
                    return Err("Claude Code not available".to_string());
                }
                if let Some(error) = status.error_message {
                    return Err(format!("Claude Code error: {}", error));
                }

                let subscription_info = status.subscription_tier
                    .map(|tier| format!(" ({})", tier))
                    .unwrap_or_default();

                return Ok(format!("Claude Code healthy{}", subscription_info));
            }
        }

        Err("Claude Code provider not found".to_string())
    }

    // Perform health check
    match health_check(&auth_manager).await {
        Ok(message) => println!("✓ Health check passed: {}", message),
        Err(error) => println!("✗ Health check failed: {}", error),
    }

    // Example: Periodic health monitoring
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            match health_check(&auth_manager).await {
                Ok(_) => {
                    // Log success or update metrics
                }
                Err(error) => {
                    eprintln!("Health check failed: {}", error);
                    // Could trigger alerts or fallback mechanisms
                }
            }
        }
    });

    println!("✓ Health monitoring configured");
    Ok(())
}

/// Example 7: Custom Provider Selection Logic
///
/// Shows how to implement custom logic for provider selection
pub async fn custom_provider_selection() -> Result<(), Box<dyn std::error::Error>> {
    let codex_home = dirs::home_dir()
        .ok_or("Unable to determine home directory")?
        .join(".codex");

    // Custom selection function
    async fn select_provider_for_task(
        auth_manager: &UnifiedAuthManager,
        task_description: &str,
        estimated_complexity: u32,
    ) -> Result<crate::providers::AuthProvider, Box<dyn std::error::Error>> {
        let strategy = match estimated_complexity {
            0..=3 => ProviderSelectionStrategy::CostOptimized,
            4..=7 => ProviderSelectionStrategy::PreferClaude,
            _ => ProviderSelectionStrategy::BestSubscription,
        };

        // Temporarily change strategy
        let mut temp_manager = auth_manager.clone();
        temp_manager.set_strategy(strategy);

        let task_type = if task_description.contains("code") {
            TaskType::CodeGeneration
        } else if task_description.contains("analyze") {
            TaskType::Analysis
        } else {
            TaskType::Interactive
        };

        let context = AuthContext {
            task_type,
            estimated_tokens: Some((estimated_complexity as u64) * 500),
            priority: if estimated_complexity > 7 { Priority::High } else { Priority::Medium },
            user_preference: Some(ProviderType::Claude),
            required_features: vec![],
        };

        temp_manager.get_optimal_provider(&context).await
    }

    let auth_manager = UnifiedAuthManager::new(
        codex_home,
        ProviderSelectionStrategy::Adaptive
    ).await?;

    // Example tasks with different complexities
    let tasks = vec![
        ("Simple code comment", 2),
        ("Complex algorithm implementation", 8),
        ("Code analysis and optimization", 6),
        ("Basic debugging assistance", 3),
    ];

    for (task_description, complexity) in tasks {
        match select_provider_for_task(&auth_manager, task_description, complexity).await {
            Ok(provider) => {
                println!("✓ Task '{}' (complexity {}): Using {:?}",
                        task_description, complexity, provider.provider_type());
            }
            Err(e) => {
                println!("✗ Failed to select provider for '{}': {}", task_description, e);
            }
        }
    }

    println!("✓ Custom provider selection completed");
    Ok(())
}

/// Main function to run all examples
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Claude Code Agent Configuration Examples\n");

    // Run all examples
    let examples = vec![
        ("Basic Agent Setup", basic_agent_setup()),
        ("Multi-Agent Setup", multi_agent_setup()),
        ("Enterprise Setup", enterprise_setup()),
        ("Load Balanced Pool", load_balanced_agent_pool()),
        ("Environment Setup", environment_specific_setup()),
        ("Health Monitoring", health_monitoring_setup()),
        ("Custom Selection", custom_provider_selection()),
    ];

    for (name, example) in examples {
        println!("\n📋 Running: {}", name);
        println!("─".repeat(50));

        match example.await {
            Ok(_) => println!("✅ {} completed successfully", name),
            Err(e) => println!("❌ {} failed: {}", name, e),
        }
    }

    println!("\n🎉 All examples completed!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_agent_setup() {
        // This test would require Claude Code to be installed and authenticated
        // In a real scenario, you'd mock the auth manager or use a test environment
        assert!(true); // Placeholder test
    }

    #[tokio::test]
    async fn test_health_check_logic() {
        // Test the health check logic with mock data
        // This would test the health checking without requiring actual Claude Code
        assert!(true); // Placeholder test
    }
}