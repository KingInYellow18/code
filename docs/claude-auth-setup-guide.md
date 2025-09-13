# Claude Authentication Setup Guide

## Overview

This guide walks you through setting up Claude authentication in the Code project. Claude integration provides access to Claude Max subscriptions, intelligent provider selection, and enhanced AI capabilities alongside your existing OpenAI setup.

## Quick Start

### For Claude Max Subscribers (Recommended)

If you have a Claude Max subscription, this is the fastest way to get started:

```bash
# Authenticate with Claude Max
code auth login --provider claude

# Check your subscription status
code auth status --provider claude --detailed

# View your quota
code auth quota

# Start using Code with Claude!
code "Help me refactor this function to be more readable"
```

### For API Key Users

If you have a Claude API key:

```bash
# Set your API key
export ANTHROPIC_API_KEY=sk-ant-api03-...

# Or authenticate directly
code auth login --provider claude --api-key sk-ant-api03-...

# Verify authentication
code auth test --provider claude
```

## Detailed Setup Instructions

### Step 1: Install or Update Code

Ensure you have the latest version of Code with Claude support:

```bash
# Update to latest version
npm install -g @just-every/code@latest

# Verify version (should be 0.3.0 or later)
code --version
```

### Step 2: Choose Authentication Method

#### Option A: Claude Max Subscription (OAuth)

**Requirements:**
- Active Claude Max subscription
- Web browser access for OAuth flow

**Setup:**

1. **Start authentication process:**
   ```bash
   code auth login --provider claude
   ```

2. **Complete OAuth flow:**
   - A browser window will open automatically
   - If not, copy the displayed URL to your browser
   - Sign in with your Anthropic account
   - Authorize Code to access your account

3. **Verify setup:**
   ```bash
   code auth status --provider claude --detailed
   ```

   You should see:
   ```
   Provider: claude (✓ Authenticated)
     Subscription: max (Active)
     Features: unlimited_messages, priority_access
     Quota: 5000/1000000 (0.5%)
     Resets: 2025-09-14 00:00 UTC
   ```

#### Option B: Claude Pro Subscription (OAuth)

Similar to Claude Max but with different quota limits:

```bash
# Same process as Claude Max
code auth login --provider claude

# Pro users will see different quotas
code auth quota --detailed
```

#### Option C: API Key Authentication

**Requirements:**
- Claude API key from console.anthropic.com

**Setup:**

1. **Get your API key:**
   - Visit https://console.anthropic.com
   - Navigate to API Keys
   - Create a new key or copy existing key

2. **Authenticate using environment variable:**
   ```bash
   export ANTHROPIC_API_KEY=sk-ant-api03-...
   code auth test --provider claude
   ```

3. **Or authenticate directly:**
   ```bash
   code auth login --provider claude --api-key sk-ant-api03-...
   ```

4. **Verify authentication:**
   ```bash
   code auth status --provider claude
   ```

### Step 3: Configure Provider Preferences

#### Set Claude as Default Provider

```bash
# Make Claude your preferred provider
code auth switch claude

# Verify the change
code auth status --detailed
```

#### Enable Auto-Selection

```bash
# Let Code choose the best provider automatically
code auth switch auto

# Auto-selection prioritizes:
# 1. Claude Max (if authenticated)
# 2. Claude Pro (if authenticated) 
# 3. OpenAI (if authenticated)
# 4. Claude API key (if authenticated)
```

#### Keep Both Providers

You can authenticate with both Claude and OpenAI and switch between them:

```bash
# Authenticate with both
code auth login --provider openai
code auth login --provider claude

# Check status of both
code auth status --detailed

# Switch as needed
code auth switch claude  # For Claude tasks
code auth switch openai  # For OpenAI tasks
```

## Advanced Configuration

### Config File Settings

Add Claude preferences to your `~/.codex/config.toml`:

```toml
# Preferred authentication provider
preferred_auth_provider = "claude"  # or "openai" or "auto"

# Claude-specific settings
[claude]
subscription_check_interval = "24h"
quota_warning_threshold = 0.8  # Warn at 80% quota usage
auto_fallback_enabled = true   # Fall back to OpenAI if quota exceeded

# Provider-specific model settings
[profiles.claude-max]
model = "claude-3-opus-20240229"
model_provider = "claude"
approval_policy = "never"
model_reasoning_effort = "high"

[profiles.claude-api]
model = "claude-3-sonnet-20240229"
model_provider = "claude"
approval_policy = "on_request"
```

### Environment Variables

Configure environment variables for automated setups:

```bash
# Claude authentication
export ANTHROPIC_API_KEY=sk-ant-api03-...
export CLAUDE_API_KEY=sk-ant-api03-...  # Alternative name (auto-mapped)

# Provider preference
export CODE_AUTH_PROVIDER=claude  # or "openai" or "auto"

# OAuth settings (advanced)
export CLAUDE_OAUTH_CLIENT_ID=your-client-id
export CLAUDE_OAUTH_REDIRECT_URI=http://localhost:1456/callback
```

### File Locations

Claude authentication stores data in these locations:

```
~/.codex/
├── auth.json              # OpenAI authentication (preserved)
├── claude_auth.json       # Claude OAuth tokens
├── auth_config.json       # Provider preferences
└── config.toml            # User configuration
```

## Integration with Existing Workflows

### For Existing OpenAI Users

Your existing setup continues to work unchanged:

```bash
# These commands work exactly as before
code login
code logout
code "Write me a Python script"
```

Adding Claude is completely optional:

```bash
# Add Claude without affecting OpenAI
code auth login --provider claude

# Use both providers
code auth switch claude    # Use Claude for next task
code "Analyze this code"   # Uses Claude
code auth switch openai    # Switch back to OpenAI
code "Generate tests"      # Uses OpenAI
```

### For Teams

#### Shared Configuration

Teams can share configuration templates:

```bash
# Create team config template
cat > team-config.toml << EOF
preferred_auth_provider = "claude"

[claude]
auto_fallback_enabled = true
quota_warning_threshold = 0.9

[profiles.team-claude]
model = "claude-3-opus-20240229"
model_provider = "claude"
approval_policy = "on_request"
EOF

# Team members copy to their config
cp team-config.toml ~/.codex/config.toml
```

#### Environment Setup

For CI/CD and shared environments:

```bash
# Set team API keys
export ANTHROPIC_API_KEY=$TEAM_CLAUDE_API_KEY
export OPENAI_API_KEY=$TEAM_OPENAI_API_KEY

# Configure provider preference
export CODE_AUTH_PROVIDER=auto

# Test authentication
code auth test --provider claude
code auth test --provider openai
```

## Troubleshooting

### Common Issues

#### "Provider not authenticated"

```bash
# Check authentication status
code auth status --detailed

# Re-authenticate if needed
code auth login --provider claude --force
```

#### "Subscription verification failed"

```bash
# Check your Claude subscription
code auth status --provider claude --detailed

# Verify subscription at console.anthropic.com
# Re-authenticate if subscription changed
code auth login --provider claude --force
```

#### "Token expired"

```bash
# OAuth tokens are automatically refreshed
# If manual refresh needed:
code auth login --provider claude --force
```

#### "Quota exceeded"

```bash
# Check quota status
code auth quota --detailed

# Switch to backup provider
code auth switch openai

# Or wait for quota reset (shown in status)
```

#### Browser Issues with OAuth

**Problem:** Browser doesn't open automatically

```bash
# Copy the URL manually
code auth login --provider claude
# Copy the displayed URL to your browser
```

**Problem:** Localhost connection issues

```bash
# Check if port 1456 is available
lsof -i :1456

# Kill conflicting processes if needed
sudo lsof -t -i:1456 | xargs kill -9

# Try authentication again
code auth login --provider claude
```

**Problem:** Headless server setup

```bash
# From your local machine, create SSH tunnel
ssh -L 1456:localhost:1456 user@server

# Run authentication on server
code auth login --provider claude

# Complete OAuth flow in local browser
```

### Diagnostic Commands

```bash
# Check overall status
code auth status --detailed

# Test all providers
code auth test --provider claude
code auth test --provider openai

# View quota information
code auth quota --detailed

# List available providers
code auth providers

# Check configuration
cat ~/.codex/auth_config.json

# View logs (if enabled)
tail -f ~/.codex/logs/auth.log
```

### Reset Authentication

If you need to start fresh:

```bash
# Logout from all providers
code auth logout --all

# Remove config files
rm ~/.codex/claude_auth.json
rm ~/.codex/auth_config.json

# Re-authenticate
code auth login --provider claude
```

## Security Considerations

### Token Storage

- OAuth tokens are stored with 600 permissions (owner read/write only)
- Tokens are encrypted at rest
- Automatic token refresh prevents long-lived credentials

### OAuth Security

- Uses PKCE (Proof Key for Code Exchange) for enhanced security
- State parameter prevents CSRF attacks
- Redirect URI validation

### API Key Security

- Never log API keys
- Store in secure environment variables
- Use least-privilege API keys when possible

### Best Practices

1. **Use OAuth when possible** - More secure than API keys
2. **Rotate API keys regularly** - Set reminders for key rotation
3. **Monitor quota usage** - Set up alerts for high usage
4. **Use team accounts** - Avoid sharing personal credentials
5. **Enable audit logging** - Track authentication events

## Migration Guide

### From Manual Environment Variables

If you currently use environment variables:

```bash
# Current setup (still works)
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...

# Migrate to managed authentication
code auth login --provider claude --api-key $ANTHROPIC_API_KEY
code auth login --provider openai --api-key $OPENAI_API_KEY

# Verify migration
code auth status --detailed

# Optional: remove environment variables
unset ANTHROPIC_API_KEY
unset OPENAI_API_KEY
```

### From OpenAI-Only Setup

No changes required! Add Claude alongside:

```bash
# Your existing OpenAI setup is preserved
code auth status --provider openai

# Add Claude
code auth login --provider claude

# Use auto-selection
code auth switch auto
```

### From Other Claude Tools

If you use other Claude CLI tools:

```bash
# Check if you have existing Claude credentials
echo $ANTHROPIC_API_KEY

# Import into Code
code auth login --provider claude --api-key $ANTHROPIC_API_KEY

# Or use OAuth for enhanced features
code auth login --provider claude
```

## FAQ

**Q: Do I need to choose between OpenAI and Claude?**

A: No! You can use both. Code will intelligently select the best provider or you can manually switch between them.

**Q: What's the difference between Claude Max and Claude Pro?**

A: Claude Max offers unlimited messages and higher quotas. Both work with Code's OAuth integration.

**Q: Can I use Claude API keys and OAuth together?**

A: OAuth is preferred when available. If you have both, OAuth takes priority.

**Q: Will this affect my existing OpenAI setup?**

A: Not at all. Existing OpenAI authentication continues to work unchanged.

**Q: How do I know which provider Code is using?**

A: Use `code auth status --detailed` or look for the provider indicator in the TUI footer.

**Q: Can I switch providers mid-conversation?**

A: Yes! Use `code auth switch <provider>` or `/switch <provider>` in the TUI.

**Q: What happens if I run out of Claude quota?**

A: Code can automatically fall back to OpenAI if configured, or you can manually switch providers.

**Q: Is my data secure with Claude authentication?**

A: Yes. Code uses industry-standard OAuth 2.0 with PKCE and stores tokens securely with proper file permissions.

## Getting Help

- **Documentation**: https://github.com/just-every/code/docs
- **Issues**: https://github.com/just-every/code/issues
- **Discord**: [Join our community](https://discord.gg/code)

## Next Steps

After setup, explore these advanced features:

- [Configuration Management](configuration-management.md)
- [CLI Commands Reference](cli_commands_reference.md)
- [Troubleshooting Guide](claude-auth-troubleshooting.md)
- [Migration Guide](claude-auth-migration-guide.md)