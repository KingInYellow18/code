# Claude Max Authentication Setup Guide
## End-to-End Configuration Without API Keys

This guide walks you through setting up Claude authentication using Claude Max subscriptions, eliminating the need for API keys while enabling seamless Claude Code integration.

---

## 🎯 **Overview**

Claude Max Authentication allows you to:
- ✅ **Authenticate without API keys** using your Claude Max subscription
- ✅ **Access higher rate limits** compared to API key authentication
- ✅ **Seamless integration** with existing Claude Code workflows
- ✅ **Automatic quota management** for multi-agent scenarios
- ✅ **Intelligent fallback** to OpenAI when needed

---

## 📋 **Prerequisites**

### Required Subscriptions
- **Claude Max Subscription** (required for OAuth authentication)
- **GitHub Account** (for Claude Code installation)

### System Requirements
- **Operating System**: macOS, Linux, or Windows with WSL
- **Node.js**: Version 16+ (for Claude Code CLI)
- **Git**: For repository management
- **Modern Browser**: Chrome, Firefox, Safari, or Edge

---

## 🚀 **Quick Start**

### Step 1: Install Claude Code CLI

```bash
# Install via npm (recommended)
npm install -g @anthropic/claude-code

# Or install via Homebrew (macOS/Linux)
brew install claude-code

# Verify installation
code --version
```

### Step 2: Initialize Claude Authentication

```bash
# Start the authentication process
code auth login --provider claude

# Alternative: Use the interactive provider selection
code auth login
# Select "Claude" when prompted
```

### Step 3: Complete OAuth Flow

1. **Browser Opens Automatically**: Claude Code will open your default browser
2. **Login to Claude**: Use your Claude Max account credentials
3. **Authorize Claude Code**: Grant permissions for API access
4. **Return to CLI**: The browser will redirect back and authentication completes

### Step 4: Verify Authentication

```bash
# Check authentication status
code auth status --provider claude

# View subscription details
code auth quota

# Test Claude Code functionality
code exec "Write a simple hello world in Python"
```

---

## 🔧 **Detailed Configuration**

### Authentication Flow Walkthrough

#### 1. **Initial Setup**
```bash
# Check available providers
code auth providers

# Expected output:
# Available Providers:
# - openai (ChatGPT, API Key)
# - claude (Claude Max, Claude Pro, API Key)
# - auto (Intelligent selection)
```

#### 2. **Claude Max Authentication**
```bash
# Start Claude authentication
code auth login --provider claude

# Output:
# Starting Claude authentication...
# Opening browser for Claude Max OAuth...
# Please complete authentication in your browser.
```

#### 3. **Browser Authentication Steps**
1. **Redirect to Claude**: Browser opens `https://auth.anthropic.com/oauth/authorize`
2. **Login Screen**: Enter Claude Max credentials
3. **Authorization Request**: Claude Code requests permissions:
   - API access for code generation
   - Subscription tier verification
   - Usage quota management
4. **Grant Permissions**: Click "Authorize Claude Code"
5. **Success Redirect**: Browser shows "Authentication Successful"

#### 4. **CLI Confirmation**
```bash
# CLI displays success message:
# ✅ Claude authentication successful!
# Subscription: Claude Max
# Daily Quota: 1,000,000 tokens
# Rate Limit: 60 requests/minute
```

### Configuration Verification

#### Check Authentication Status
```bash
# Detailed status check
code auth status --detailed

# Expected output:
# Authentication Status:
# ┌──────────┬────────────────────┬─────────────┬──────────────┐
# │ Provider │ Status             │ Subscription│ Quota Used   │
# ├──────────┼────────────────────┼─────────────┼──────────────┤
# │ claude   │ ✅ Authenticated   │ Max         │ 15,234/1M    │
# │ openai   │ ❌ Not configured  │ -           │ -            │
# └──────────┴────────────────────┴─────────────┴──────────────┘
```

#### Quota Management
```bash
# View quota details
code auth quota --detailed

# Expected output:
# Claude Max Quota Status:
# Daily Limit: 1,000,000 tokens
# Used Today: 15,234 tokens (1.5%)
# Remaining: 984,766 tokens
# Rate Limit: 60 requests/minute
# Concurrent Agents: 0/10
# 
# Resets at: 2025-01-14 00:00:00 UTC
```

---

## 🎨 **Advanced Configuration**

### Provider Preferences

#### Set Claude as Default Provider
```bash
# Set preferred provider
code config set auth.preferred_provider claude

# Enable automatic fallback
code config set auth.enable_fallback true

# Set fallback order
code config set auth.fallback_order "claude,openai"
```

#### Provider-Specific Settings
```bash
# Claude Max specific settings
code config set claude.max_requests_per_minute 60
code config set claude.max_concurrent_agents 10
code config set claude.quota_warning_threshold 0.8

# Subscription check interval
code config set claude.subscription_check_interval "24h"
```

### Environment Variables

#### For Development
```bash
# Optional: Override default settings
export CLAUDE_MAX_SUBSCRIPTION=true
export CLAUDE_PREFERRED_PROVIDER=claude
export CLAUDE_ENABLE_FALLBACK=true

# Debug authentication issues
export DEBUG=claude:auth
export CLAUDE_AUTH_VERBOSE=true
```

#### For CI/CD (Not Recommended)
```bash
# Note: OAuth is preferred over API keys
# Only use API keys for automated systems
export ANTHROPIC_API_KEY="sk-ant-api03-..."
```

### Configuration Files

#### User Configuration (`~/.codex/config.toml`)
```toml
[auth]
preferred_provider = "claude"
enable_fallback = true
fallback_order = ["claude", "openai"]

[claude]
max_requests_per_minute = 60
max_concurrent_agents = 10
quota_warning_threshold = 0.8
subscription_check_interval = "24h"

[openai]
# OpenAI configuration (if needed for fallback)
enabled = true
```

#### Project Configuration (`./.codex/config.toml`)
```toml
[auth]
# Override for this project
preferred_provider = "claude"

[agents]
# Agent-specific settings
default_provider = "claude"
max_concurrent = 5
quota_per_agent = 10000
```

---

## 🔄 **Switching Between Providers**

### Manual Provider Switching
```bash
# Switch to Claude for current session
code auth switch --provider claude

# Switch to OpenAI for current session
code auth switch --provider openai

# Reset to automatic selection
code auth switch --provider auto
```

### Command-Specific Provider Override
```bash
# Use Claude for specific command
code exec --provider claude "Explain quantum computing"

# Use OpenAI for specific command
code exec --provider openai "Write a Python script"

# Let system choose optimal provider
code exec --provider auto "Create a React component"
```

### Project-Level Provider Settings
```bash
# Set provider for current project
code config set --local auth.preferred_provider claude

# Check current project settings
code config get --local auth.preferred_provider
```

---

## 🧪 **Testing Your Setup**

### Basic Functionality Tests

#### 1. **Authentication Test**
```bash
# Test authentication connectivity
code auth test --provider claude

# Expected output:
# Testing Claude authentication...
# ✅ Authentication successful
# ✅ Subscription verified (Claude Max)
# ✅ API connectivity confirmed
# ✅ Quota information retrieved
```

#### 2. **Code Generation Test**
```bash
# Simple code generation
code exec "Create a hello world function in JavaScript"

# Expected behavior:
# - Uses Claude Max authentication
# - Generates JavaScript function
# - Shows quota usage update
```

#### 3. **Multi-Agent Test**
```bash
# Test multiple agents
code agents create --count 3 --provider claude

# Expected output:
# Creating 3 agents with Claude authentication...
# ✅ Agent 1: Authenticated (Quota: 10,000 tokens)
# ✅ Agent 2: Authenticated (Quota: 10,000 tokens)
# ✅ Agent 3: Authenticated (Quota: 10,000 tokens)
# Total allocated: 30,000/1,000,000 tokens
```

### Advanced Testing

#### 4. **Quota Management Test**
```bash
# Test quota enforcement
code quota test --simulate-high-usage

# Expected behavior:
# - Simulates high token usage
# - Shows quota warnings at 80%
# - Demonstrates graceful degradation
```

#### 5. **Fallback Test**
```bash
# Test fallback to OpenAI (requires OpenAI auth)
code auth test --provider claude --simulate-failure

# Expected behavior:
# - Detects Claude unavailability
# - Falls back to OpenAI automatically
# - Continues operation seamlessly
```

---

## 🔍 **Troubleshooting**

### Common Issues

#### Issue 1: Browser Authentication Fails
**Symptoms:**
- Browser doesn't open automatically
- Authentication page doesn't load
- OAuth redirect fails

**Solutions:**
```bash
# Manual browser authentication
code auth login --provider claude --manual

# Use different browser
code auth login --provider claude --browser firefox

# Check network connectivity
curl -I https://auth.anthropic.com

# Clear authentication cache
code auth logout --provider claude
code auth login --provider claude
```

#### Issue 2: Subscription Not Recognized
**Symptoms:**
- Authentication succeeds but quota shows 0
- "Claude Pro" instead of "Claude Max"
- Limited rate limits

**Solutions:**
```bash
# Refresh subscription status
code auth refresh --provider claude

# Manual subscription check
code auth quota --refresh

# Re-authenticate with fresh tokens
code auth logout --provider claude
code auth login --provider claude
```

#### Issue 3: Quota Exhaustion
**Symptoms:**
- "Quota exceeded" errors
- Automatic fallback to OpenAI
- Rate limiting messages

**Solutions:**
```bash
# Check quota status
code auth quota --detailed

# Enable OpenAI fallback
code config set auth.enable_fallback true

# Reduce concurrent agents
code config set claude.max_concurrent_agents 5

# Monitor usage patterns
code auth usage --analyze
```

### Advanced Debugging

#### Enable Debug Logging
```bash
# Comprehensive debug output
export DEBUG=claude:*
code auth login --provider claude

# Authentication-specific debugging
export DEBUG=claude:auth
code exec "test command"

# Network debugging
export DEBUG=claude:network
code auth status --provider claude
```

#### Authentication File Inspection
```bash
# Check authentication files (secure)
ls -la ~/.codex/
# Should show:
# - auth.json (OpenAI credentials)
# - claude_auth.json (Claude credentials)

# Verify file permissions
ls -l ~/.codex/claude_auth.json
# Should show: -rw------- (600 permissions)
```

#### Network Connectivity Tests
```bash
# Test Claude API connectivity
curl -H "Authorization: Bearer $(code auth token --provider claude)" \
  https://api.anthropic.com/v1/subscription

# Test OAuth endpoints
curl -I https://auth.anthropic.com/oauth/authorize
```

---

## 🛡️ **Security Best Practices**

### Token Security
- ✅ **Never share authentication tokens** in code or logs
- ✅ **Use OAuth instead of API keys** when possible
- ✅ **Regularly rotate credentials** (OAuth tokens refresh automatically)
- ✅ **Monitor authentication logs** for suspicious activity

### Environment Security
```bash
# Secure file permissions
chmod 600 ~/.codex/claude_auth.json

# Clear environment variables after use
unset ANTHROPIC_API_KEY

# Use project-specific configurations
code config set --local auth.preferred_provider claude
```

### Network Security
- ✅ **Use HTTPS only** for all authentication
- ✅ **Verify SSL certificates** for Claude API endpoints
- ✅ **Monitor network traffic** for authentication requests
- ✅ **Use corporate VPN** if required by organization

---

## 📈 **Optimization Tips**

### Performance Optimization

#### 1. **Authentication Caching**
```bash
# Enable authentication caching
code config set auth.cache_enabled true
code config set auth.cache_duration "1h"

# Pre-authenticate for better performance
code auth preload --provider claude
```

#### 2. **Quota Management**
```bash
# Optimize quota allocation
code config set claude.quota_optimization true
code config set claude.batch_requests true

# Set conservative limits
code config set claude.max_concurrent_agents 5
code config set claude.tokens_per_agent 5000
```

#### 3. **Fallback Configuration**
```bash
# Configure intelligent fallback
code config set auth.fallback_strategy "smart"
code config set auth.fallback_threshold 0.9

# Preload fallback authentication
code auth preload --provider openai
```

### Cost Optimization

#### Monitor Usage Patterns
```bash
# Analyze usage patterns
code auth analytics --provider claude --period 7d

# Set up usage alerts
code config set claude.quota_alerts true
code config set claude.alert_threshold 0.8
```

#### Efficient Agent Management
```bash
# Use agent pooling
code config set agents.pooling_enabled true
code config set agents.max_pool_size 5

# Implement request batching
code config set claude.batch_size 10
code config set claude.batch_timeout "5s"
```

---

## 🔗 **Integration Examples**

### CI/CD Integration

#### GitHub Actions
```yaml
# .github/workflows/claude-code.yml
name: Claude Code Integration
on: [push, pull_request]

jobs:
  claude-code-analysis:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Claude Code
        run: |
          npm install -g @anthropic/claude-code
          
      - name: Authenticate Claude
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          # Note: Use API key for CI/CD, OAuth for development
          code auth login --provider claude --api-key
          
      - name: Run Code Analysis
        run: |
          code analyze --provider claude --output results.json
```

#### Docker Integration
```dockerfile
# Dockerfile
FROM node:18-alpine

# Install Claude Code
RUN npm install -g @anthropic/claude-code

# Copy authentication configuration
COPY .codex/config.toml /root/.codex/config.toml

# Set up authentication environment
ENV CLAUDE_PREFERRED_PROVIDER=claude
ENV CLAUDE_ENABLE_FALLBACK=true

# Application setup
WORKDIR /app
COPY . .

CMD ["code", "serve", "--provider", "claude"]
```

### IDE Integration

#### VS Code Integration
```json
// .vscode/settings.json
{
  "claude-code.auth.preferredProvider": "claude",
  "claude-code.auth.enableFallback": true,
  "claude-code.claude.maxConcurrentAgents": 3,
  "claude-code.claude.quotaWarningThreshold": 0.8
}
```

#### JetBrains Integration
```xml
<!-- .idea/claude-code.xml -->
<component name="ClaudeCodeSettings">
  <option name="preferredProvider" value="claude" />
  <option name="enableFallback" value="true" />
  <option name="maxConcurrentAgents" value="3" />
</component>
```

---

## 📚 **Additional Resources**

### Official Documentation
- [Claude Code CLI Reference](./cli_commands_reference.md)
- [Authentication API Documentation](./claude-auth-api-reference.md)
- [Configuration Management Guide](./claude-auth-configuration.md)
- [Troubleshooting Guide](./claude-auth-troubleshooting.md)

### Community Resources
- **GitHub Repository**: https://github.com/anthropics/claude-code
- **Discord Community**: https://discord.gg/claude-code
- **Stack Overflow**: Tag `claude-code`
- **Documentation Issues**: https://github.com/anthropics/claude-code/issues

### Support Channels
- **Technical Support**: support@anthropic.com
- **Billing Questions**: billing@anthropic.com
- **Feature Requests**: GitHub Issues
- **Security Issues**: security@anthropic.com

---

## 🎯 **Quick Reference**

### Essential Commands
```bash
# Authentication
code auth login --provider claude
code auth status --provider claude
code auth logout --provider claude

# Configuration
code config set auth.preferred_provider claude
code config get auth.preferred_provider

# Quota Management
code auth quota
code auth quota --refresh

# Testing
code auth test --provider claude
code exec "test command"

# Troubleshooting
code auth refresh --provider claude
code auth clear-cache
```

### Key Configuration Files
- `~/.codex/claude_auth.json` - Claude authentication tokens
- `~/.codex/config.toml` - User configuration
- `./.codex/config.toml` - Project configuration
- `~/.codex/auth.log` - Authentication logs

### Important Environment Variables
- `ANTHROPIC_API_KEY` - Claude API key (fallback)
- `CLAUDE_PREFERRED_PROVIDER` - Default provider
- `CLAUDE_ENABLE_FALLBACK` - Enable OpenAI fallback
- `DEBUG=claude:*` - Enable debug logging

---

*Last Updated: January 13, 2025*  
*Claude Code Version: 2.0.0+ with Claude Max Integration*  
*Documentation Version: 1.0*

---

## 💡 **Pro Tips**

1. **Use OAuth over API keys** whenever possible for better security and rate limits
2. **Enable fallback authentication** to ensure continuous operation
3. **Monitor quota usage** to optimize costs and performance
4. **Use project-specific configurations** for team collaboration
5. **Keep authentication tokens secure** and never commit them to version control
6. **Test authentication setup** in staging before production deployment
7. **Set up usage alerts** to prevent quota exhaustion
8. **Use debug logging** for troubleshooting authentication issues

Happy coding with Claude Max! 🚀