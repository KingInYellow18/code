# Claude Authentication Integration for Agent Environments

## Overview

This implementation provides comprehensive Claude authentication and quota management for multi-agent environments in Claude Code. It ensures proper quota allocation, prevents conflicts between concurrent agents, and manages authentication credentials across agent sessions.

## Architecture

### Core Components

1. **ClaudeQuotaManager** (`src/claude_quota.rs`)
   - Tracks usage across all agents
   - Manages rate limiting and concurrent agent limits
   - Supports both Claude Max subscriptions and API keys
   - Provides real-time quota monitoring

2. **AgentAuthCoordinator** (`src/agent_auth.rs`) 
   - Sets up authentication environment for each agent
   - Coordinates with quota manager for allocation
   - Manages agent sessions and cleanup
   - Provides health monitoring

3. **Agent Integration** (`src/agent_integration.rs`)
   - CLI commands for monitoring and management
   - Initialization and setup helpers
   - Status reporting and diagnostics

### Integration with Existing Agent System

The implementation extends the existing `agent_tool.rs` with:

- **Agent struct enhancements**: Added `claude_session_id` and `uses_claude_auth` fields
- **AgentManager extensions**: Added `claude_auth_coordinator` and setup methods
- **Environment variable overlay**: Claude auth variables automatically applied
- **Lifecycle management**: Quota allocation on creation, release on completion

## Features

### Quota Management

- **Concurrent Agent Limits**: Configurable maximum number of simultaneous Claude agents
- **Usage Tracking**: Real-time monitoring of token consumption and API requests
- **Rate Limiting**: Prevents exceeding Claude API rate limits
- **Automatic Cleanup**: Removes inactive agent sessions

### Authentication Types

#### Claude Max Subscription
```rust
ClaudeQuotaManager::new_max_subscription(daily_limit: 10000, concurrent_limit: 5)
```
- Daily token limits
- Usage tracking across agents
- Quota allocation per agent

#### API Key Authentication  
```rust
ClaudeQuotaManager::new_api_key(requests_per_minute: 60, tokens_per_minute: 100_000, concurrent_limit: 8)
```
- Rate limiting by requests/minute
- Token budget allocation
- Concurrent request management

### Environment Variable Management

The system automatically manages these environment variables for each agent:

- `CLAUDE_API_KEY` / `ANTHROPIC_API_KEY` (bidirectional mapping)
- `CLAUDE_BASE_URL` / `ANTHROPIC_BASE_URL` 
- `CLAUDE_AGENT_ID` (unique identifier)
- `CLAUDE_SESSION_ID` (session tracking)
- `CLAUDE_MAX_REQUESTS_PER_MINUTE` (allocated rate limit)
- `CLAUDE_MAX_TOKENS_ALLOCATED` (allocated token budget)

## Usage Examples

### Basic Initialization

```rust
use claude_auth_integration::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Claude authentication system
    initialize_claude_auth_system().await?;
    
    // Setup agent manager with Claude auth
    let coordinator = get_claude_auth_coordinator().await.unwrap();
    
    // Setup authentication for an agent
    let auth_env = coordinator.setup_claude_agent_auth("my_agent").await?;
    
    // Agent runs with quota allocation...
    
    // Release when done
    coordinator.release_agent_auth("my_agent").await?;
    
    Ok(())
}
```

### CLI Commands

```bash
# Check authentication status
ClaudeAuthCLI::status().await

# Get detailed quota information  
ClaudeAuthCLI::quota_info().await

# Test authentication detection
ClaudeAuthCLI::test_auth().await

# Cleanup inactive sessions
ClaudeAuthCLI::cleanup().await
```

### Monitoring and Health Checks

```rust
let coordinator = get_claude_auth_coordinator().await.unwrap();

// Check system health
let health = coordinator.check_auth_health().await;
println!("Healthy: {}", health.healthy);
println!("Active agents: {}", health.active_agent_count); 
println!("Quota usage: {}%", health.quota_usage_percentage);

// Get active agents
let active_agents = coordinator.get_active_agents().await;

// Cleanup inactive sessions (30 minutes)
let cleaned = coordinator.cleanup_inactive_agents(Duration::minutes(30)).await;
```

## Integration with Agent Creation

The system automatically integrates with existing agent creation:

```rust
// In AgentManager::create_agent_internal()
let needs_claude_auth = model.to_lowercase() == "claude";
if needs_claude_auth {
    if let Some(coordinator) = self.claude_auth_coordinator {
        let auth_env = coordinator.setup_claude_agent_auth(&agent_id).await?;
        // Agent gets Claude authentication automatically
    }
}
```

## Configuration

### Environment Variables

Required for initialization:
- `CLAUDE_API_KEY` or `ANTHROPIC_API_KEY` - API authentication
- Optional: `CLAUDE_BASE_URL` / `ANTHROPIC_BASE_URL` - Custom endpoints

### Defaults

- **Claude Max**: 10,000 tokens/day, 5 concurrent agents
- **API Key**: 60 req/min, 100k tokens/min, 8 concurrent agents  
- **Session timeout**: 30 minutes of inactivity
- **Rate limit reset**: Every 60 seconds

## Error Handling

The system gracefully handles:
- **Quota exhaustion**: Prevents new agent allocation when limits reached
- **Rate limiting**: Blocks requests when rate limits approached
- **Authentication failures**: Falls back to non-authenticated operation
- **Session cleanup**: Automatically removes stale sessions

## Testing

Run the comprehensive example:

```bash
cargo run --example claude_auth_example
```

This demonstrates:
- Multi-agent quota allocation
- Rate limiting behavior
- Session coordination
- Health monitoring
- Cleanup operations

## Implementation Status

✅ **Complete Features:**
- Claude quota management and rate limiting
- Multi-agent session coordination
- Environment variable enhancement  
- Integration with existing agent system
- Health monitoring and status reporting
- CLI commands for management
- Comprehensive testing and examples

✅ **Key Integration Points:**
- Extended `agent_tool.rs` with Claude authentication
- Added quota allocation to agent lifecycle
- Environment variable overlay in execution
- Automatic cleanup on agent completion

## Future Enhancements

- **Persistent quota tracking**: Store usage across restarts
- **Dynamic quota adjustment**: Adjust limits based on subscription changes
- **Advanced rate limiting**: Per-agent rate limit customization
- **Metrics export**: Integration with monitoring systems
- **Dashboard UI**: Web interface for quota monitoring

## Security Considerations

- API keys stored in environment variables only
- No persistent storage of credentials
- Session IDs for tracking (not authentication)
- Automatic cleanup prevents credential leakage
- Rate limiting prevents abuse

## Performance Impact

- **Minimal overhead**: Quota tracking uses atomic operations
- **Async operations**: Non-blocking authentication setup
- **Memory efficient**: HashMap storage for active sessions
- **Cleanup**: Automatic removal of inactive sessions
- **Scalable**: Designed for hundreds of concurrent agents