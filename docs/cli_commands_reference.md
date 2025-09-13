# Claude Authentication CLI Commands Reference

This document provides a comprehensive reference for the extended CLI commands that support Claude authentication alongside the existing OpenAI authentication.

## Overview

The extended authentication system supports multiple providers:
- **OpenAI**: ChatGPT OAuth or API key authentication
- **Claude**: Claude Max OAuth or API key authentication  
- **Auto**: Intelligent provider selection based on availability and preferences

## New Commands

### `code auth`

Main authentication management command with multi-provider support.

```bash
# General usage
code auth <subcommand> [options]
```

#### Subcommands

##### `code auth login`

Authenticate with a specific provider or auto-select the best one.

```bash
# Auto-select provider (tries Claude first, falls back to OpenAI)
code auth login

# Authenticate with specific provider
code auth login --provider claude
code auth login --provider openai
code auth login --provider auto

# Use API key authentication
code auth login --provider claude --api-key sk-ant-api03-...
code auth login --provider openai --api-key sk-...

# Force re-authentication
code auth login --provider claude --force
```

**Examples:**
```bash
# Quick start with auto-selection
code auth login

# Authenticate with Claude Max
code auth login --provider claude

# Use Claude API key
code auth login --provider claude --api-key sk-ant-api03-xxx

# Re-authenticate with OpenAI
code auth login --provider openai --force
```

##### `code auth status`

Show authentication status for all or specific providers.

```bash
# Show status for all providers
code auth status

# Show status for specific provider
code auth status --provider claude
code auth status --provider openai

# Show detailed status including quotas and subscription info
code auth status --detailed
code auth status --provider claude --detailed
```

**Example Output:**
```
Authentication Status:
=====================

Provider: claude (✓ Authenticated)
  Subscription: max (Active)
  Features: unlimited_messages, priority_access
  Quota: 50000/1000000 (5.0%)
  Resets: 2025-09-14 00:00 UTC
  Token Expires: 2025-09-15 10:30 UTC

Provider: openai (✓ Authenticated)
```

##### `code auth providers`

List all available authentication providers and their capabilities.

```bash
# Show all providers
code auth providers

# Show only active/authenticated providers
code auth providers --active-only
```

**Example Output:**
```
Available Providers:
===================

Provider: Anthropic Claude
  Description: Claude AI models with Claude Max OAuth or API key authentication
  Auth Methods: OAuth (Claude Max), API Key
  Features: Chat completions, Code analysis, Long context, Constitutional AI
  Quota Management: Supported

Provider: OpenAI
  Description: OpenAI GPT models with ChatGPT OAuth or API key authentication
  Auth Methods: OAuth (ChatGPT), API Key
  Features: Chat completions, Code generation, Text analysis
```

##### `code auth switch`

Switch the active/preferred provider.

```bash
# Switch to Claude as preferred provider
code auth switch claude

# Switch to OpenAI as preferred provider
code auth switch openai

# Switch to auto-selection mode
code auth switch auto

# Force switch even if target provider not authenticated
code auth switch claude --force
```

##### `code auth quota`

Show quota information for providers that support it (primarily Claude Max).

```bash
# Show Claude quota (default)
code auth quota

# Show quota for specific provider
code auth quota --provider claude

# Show detailed quota breakdown
code auth quota --detailed
code auth quota --provider claude --detailed
```

**Example Output:**
```
Claude Quota Information:
========================

Usage: 50000/1000000 tokens (5.0%)
Remaining: 950000 tokens
Progress: [██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]
Resets: 2025-09-14 00:00 UTC
Time until reset: 14h 30m
```

##### `code auth test`

Test authentication with a provider by making a lightweight API call.

```bash
# Test auto-selected provider
code auth test

# Test specific provider
code auth test --provider claude
code auth test --provider openai
```

**Example Output:**
```
Testing authentication for claude provider...
✓ Claude provider authentication test successful
```

##### `code auth logout`

Logout from specific or all providers.

```bash
# Logout from all providers
code auth logout

# Logout from specific provider
code auth logout --provider claude
code auth logout --provider openai

# Logout from all providers explicitly
code auth logout --all
```

## Enhanced Legacy Commands

The existing `login` and `logout` commands have been enhanced with provider support while maintaining full backward compatibility.

### Enhanced `code login`

```bash
# Existing usage (still works)
code login
code login --api-key sk-...
code login status

# New provider support
code login --provider claude
code login --provider openai --api-key sk-...
code login --provider auto

# Combined with existing flags
code login --provider claude --force
```

### Enhanced `code logout`

```bash
# Existing usage (still works)
code logout

# New provider support
code logout --provider claude
code logout --provider openai
```

## Provider Selection Logic

The system uses intelligent provider selection when `--provider auto` is used or no provider is specified:

1. **Check user preference**: Uses previously set preferred provider
2. **Check Claude Max**: If authenticated with Claude Max subscription, prefer Claude
3. **Check available providers**: Use any authenticated provider
4. **Default order**: Claude → OpenAI → Error

You can influence this by:
```bash
# Set preferred provider
code auth switch claude

# Check current preference
code auth status --detailed
```

## Configuration Files

The extended authentication system stores configuration in:

- `~/.codex/auth.json` - OpenAI authentication (existing)
- `~/.codex/claude_tokens.json` - Claude authentication tokens
- `~/.codex/auth_config.json` - Provider preferences and settings

## Environment Variables

The system respects these environment variables:

```bash
# OpenAI (existing)
export OPENAI_API_KEY=sk-...

# Claude (new)
export ANTHROPIC_API_KEY=sk-ant-api03-...
export CLAUDE_API_KEY=sk-ant-api03-...  # Alternative

# The system automatically maps between ANTHROPIC_API_KEY and CLAUDE_API_KEY
```

## Error Handling and Troubleshooting

### Common Error Messages

**"No valid authentication found"**
```bash
# Solution: Authenticate with a provider
code auth login --provider claude
```

**"Subscription verification failed"**
```bash
# Solution: Check your Claude subscription
code auth status --provider claude --detailed
code auth quota --provider claude
```

**"Provider not authenticated"**
```bash
# Solution: Login to the specific provider
code auth login --provider claude
```

**"Token expired"**
```bash
# Solution: Re-authenticate
code auth login --provider claude --force
```

### Diagnostic Commands

```bash
# Check overall status
code auth status --detailed

# Test connectivity
code auth test --provider claude
code auth test --provider openai

# Check quota limits
code auth quota --detailed

# List available providers
code auth providers
```

### Reset Authentication

To completely reset authentication:

```bash
# Logout from all providers
code auth logout --all

# Remove config files (if needed)
rm ~/.codex/claude_tokens.json
rm ~/.codex/auth_config.json

# Re-authenticate
code auth login
```

## Integration with Existing Workflows

### For OpenAI Users

No changes required! Existing commands work exactly as before:

```bash
# These still work unchanged
code login
code login --api-key sk-...
code logout
```

New optional features:
```bash
# Explicitly use OpenAI
code auth login --provider openai

# Check detailed status
code auth status --provider openai --detailed
```

### For New Claude Users

```bash
# Quick setup
code auth login --provider claude

# Check status
code auth status --provider claude --detailed

# Check quota (if Claude Max user)
code auth quota
```

### For Multi-Provider Users

```bash
# Authenticate with both
code auth login --provider openai
code auth login --provider claude

# Check status of both
code auth status --detailed

# Switch between providers
code auth switch claude
code auth switch openai

# Auto-select best provider
code auth switch auto
```

## Advanced Usage

### Scripting and Automation

The CLI commands support JSON output for scripting:

```bash
# Get status in JSON format (planned feature)
code auth status --format json

# Test authentication in scripts
if code auth test --provider claude; then
    echo "Claude authentication successful"
else
    echo "Claude authentication failed"
    exit 1
fi
```

### Claude Max Optimization

For Claude Max subscribers:

```bash
# Check quota before large operations
code auth quota --detailed

# Monitor quota usage
watch "code auth quota"

# Switch to Claude for quota-sensitive work
code auth switch claude
```

### Development and Testing

```bash
# Test all providers
code auth test --provider openai
code auth test --provider claude

# Check configuration
code auth providers --active-only

# Reset for testing
code auth logout --all
code auth login --provider claude --force
```

## Migration Guide

### From OpenAI-only Setup

1. **No immediate action required** - existing setup continues to work
2. **To add Claude support**:
   ```bash
   code auth login --provider claude
   code auth status --detailed
   ```
3. **To use auto-selection**:
   ```bash
   code auth switch auto
   ```

### From Manual Environment Variables

If you currently use environment variables:

```bash
# Check current setup
echo $OPENAI_API_KEY
echo $ANTHROPIC_API_KEY

# Migrate to managed authentication
code auth login --provider openai --api-key $OPENAI_API_KEY
code auth login --provider claude --api-key $ANTHROPIC_API_KEY

# Verify migration
code auth status --detailed
```

## Security Considerations

- **Token Storage**: All tokens are stored with secure file permissions (600)
- **OAuth Security**: Uses PKCE for enhanced OAuth security
- **Audit Logging**: Authentication events are logged for security monitoring
- **Session Management**: Secure session handling with automatic cleanup

## Support and Debugging

Enable debug output:
```bash
export DEBUG=code:auth
code auth login --provider claude
```

Check logs:
```bash
# View authentication logs
tail -f ~/.codex/logs/auth.log

# View audit logs  
tail -f ~/.codex/logs/audit.log
```

Get help:
```bash
# Command help
code auth --help
code auth login --help
code auth status --help

# Version information
code --version
```