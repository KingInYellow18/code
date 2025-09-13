# Claude Authentication Configuration Reference

## Overview

This document provides a comprehensive reference for configuring Claude authentication in the Code project. It covers all configuration options, best practices, and advanced settings for optimal Claude integration.

## Configuration Hierarchy

Code uses a hierarchical configuration system:

1. **Command-line flags** (highest priority)
2. **Environment variables**
3. **Configuration files**
4. **Default values** (lowest priority)

## Configuration Files

### Primary Configuration File

**Location:** `~/.codex/config.toml`

This is the main configuration file that supports both OpenAI and Claude settings:

```toml
# Main configuration file: ~/.codex/config.toml

# Global settings
preferred_auth_provider = "claude"  # "claude" | "openai" | "auto"
model_reasoning_effort = "medium"   # "low" | "medium" | "high"
approval_policy = "on_request"      # "never" | "on_request" | "always"

# Claude-specific configuration
[claude]
subscription_check_interval = "24h"
quota_warning_threshold = 0.8
auto_fallback_enabled = true
max_concurrent_requests = 10
retry_attempts = 3
retry_delay_ms = 1000

# OpenAI configuration (preserved)
[openai]
model = "gpt-4"
max_tokens = 4096
temperature = 0.7

# Provider-specific profiles
[profiles.claude-max]
model = "claude-3-opus-20240229"
model_provider = "claude"
approval_policy = "never"
model_reasoning_effort = "high"
max_tokens = 4096

[profiles.claude-pro]
model = "claude-3-sonnet-20240229"
model_provider = "claude"
approval_policy = "on_request"
model_reasoning_effort = "medium"
max_tokens = 4096

[profiles.claude-api]
model = "claude-3-haiku-20240307"
model_provider = "claude"
approval_policy = "on_request"
model_reasoning_effort = "low"
max_tokens = 2048

[profiles.openai-gpt4]
model = "gpt-4"
model_provider = "openai"
approval_policy = "on_request"
model_reasoning_effort = "high"

# TUI theme configuration
[tui.theme]
name = "claude-optimized"
claude_accent_color = "#FF6B35"
quota_warning_color = "#FFA500"
```

### Authentication Configuration

**Location:** `~/.codex/auth_config.json`

This file stores provider preferences and authentication metadata:

```json
{
  "preferred_provider": "claude",
  "auto_fallback_enabled": true,
  "provider_priorities": ["claude", "openai"],
  "last_provider_check": "2025-09-13T10:30:00Z",
  "quota_warnings_enabled": true,
  "oauth_settings": {
    "auto_refresh": true,
    "refresh_threshold_minutes": 30
  }
}
```

### Claude Authentication Tokens

**Location:** `~/.codex/claude_auth.json`

This file stores Claude OAuth tokens and API keys (automatically managed):

```json
{
  "mode": "oauth",
  "subscription_tier": "max",
  "tokens": {
    "access_token": "encrypted_token_data",
    "refresh_token": "encrypted_refresh_token",
    "expires_at": "2025-09-14T10:30:00Z"
  },
  "subscription_info": {
    "tier": "max",
    "features": ["unlimited_messages", "priority_access"],
    "quota_limit": 1000000,
    "quota_reset": "2025-09-14T00:00:00Z"
  },
  "api_endpoints": {
    "base_url": "https://api.anthropic.com",
    "auth_url": "https://console.anthropic.com/oauth",
    "subscription_url": "https://api.anthropic.com/v1/subscription"
  }
}
```

## Environment Variables

### Core Authentication Variables

```bash
# Claude authentication
export ANTHROPIC_API_KEY=sk-ant-api03-...      # Claude API key
export CLAUDE_API_KEY=sk-ant-api03-...         # Alternative Claude API key (auto-mapped)

# OpenAI authentication (preserved)
export OPENAI_API_KEY=sk-...                   # OpenAI API key
export OPENAI_BASE_URL=https://api.openai.com  # Alternative OpenAI endpoint

# Provider selection
export CODE_AUTH_PROVIDER=claude               # "claude" | "openai" | "auto"
export CODE_PREFERRED_MODEL=claude-3-opus-20240229

# Configuration override
export CODEX_HOME=/custom/config/path          # Custom config directory
```

### OAuth Configuration Variables

```bash
# OAuth settings (advanced users)
export CLAUDE_OAUTH_CLIENT_ID=your-client-id
export CLAUDE_OAUTH_REDIRECT_URI=http://localhost:1456/callback
export CLAUDE_OAUTH_SCOPE="api subscription"
export CLAUDE_OAUTH_AUTO_REFRESH=true

# OAuth server settings
export CLAUDE_OAUTH_PORT=1456
export CLAUDE_OAUTH_TIMEOUT=300               # 5 minutes
```

### Debug and Logging Variables

```bash
# Enable debug logging
export DEBUG=code:auth,code:claude
export CODE_LOG_LEVEL=debug                   # "error" | "warn" | "info" | "debug"
export CODE_LOG_FILE=~/.codex/logs/auth.log

# Performance monitoring
export CODE_AUTH_METRICS=true
export CODE_QUOTA_MONITORING=true
```

### Network Configuration Variables

```bash
# Proxy settings
export HTTP_PROXY=http://proxy.company.com:8080
export HTTPS_PROXY=http://proxy.company.com:8080
export NO_PROXY=localhost,127.0.0.1

# SSL/TLS settings
export SSL_VERIFY=true                        # Set to false to disable SSL verification (not recommended)
export CURL_CA_BUNDLE=/path/to/ca-bundle.crt # Custom CA bundle

# Request timeouts
export CODE_REQUEST_TIMEOUT=30000             # 30 seconds in milliseconds
export CODE_CONNECT_TIMEOUT=10000             # 10 seconds
```

## Configuration Options Reference

### Global Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `preferred_auth_provider` | string | `"auto"` | Default provider: `"claude"`, `"openai"`, or `"auto"` |
| `model_reasoning_effort` | string | `"medium"` | Global reasoning level: `"low"`, `"medium"`, `"high"` |
| `approval_policy` | string | `"on_request"` | When to ask for approval: `"never"`, `"on_request"`, `"always"` |
| `sandbox_mode` | string | `"workspace_write"` | Sandbox level for code execution |

### Claude-Specific Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `subscription_check_interval` | duration | `"24h"` | How often to verify subscription status |
| `quota_warning_threshold` | float | `0.8` | Warn when quota usage exceeds this percentage |
| `auto_fallback_enabled` | boolean | `true` | Automatically switch to OpenAI when Claude quota is exhausted |
| `max_concurrent_requests` | integer | `10` | Maximum concurrent API requests to Claude |
| `retry_attempts` | integer | `3` | Number of retry attempts for failed requests |
| `retry_delay_ms` | integer | `1000` | Delay between retry attempts in milliseconds |
| `context_window_size` | integer | `200000` | Maximum context window size (tokens) |
| `request_timeout_ms` | integer | `30000` | Request timeout in milliseconds |

### OAuth Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `auto_refresh` | boolean | `true` | Automatically refresh OAuth tokens |
| `refresh_threshold_minutes` | integer | `30` | Refresh tokens when they expire within this time |
| `oauth_port` | integer | `1456` | Port for OAuth callback server |
| `oauth_timeout_seconds` | integer | `300` | Timeout for OAuth flow completion |

### Quota Management Settings

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `quota_warnings_enabled` | boolean | `true` | Show quota warning notifications |
| `quota_check_interval` | duration | `"1h"` | How often to check quota usage |
| `quota_cache_duration` | duration | `"5m"` | Cache quota information for this duration |
| `low_quota_threshold` | float | `0.9` | Threshold for low quota warnings |

## Advanced Configuration

### Custom Provider Profiles

You can create custom profiles for different use cases:

```toml
# Development profile - faster, cheaper model
[profiles.dev]
model = "claude-3-haiku-20240307"
model_provider = "claude"
max_tokens = 1024
temperature = 0.3
approval_policy = "never"

# Production profile - highest quality
[profiles.prod]
model = "claude-3-opus-20240229"
model_provider = "claude"
max_tokens = 4096
temperature = 0.1
approval_policy = "on_request"
model_reasoning_effort = "high"

# Research profile - maximum context
[profiles.research]
model = "claude-3-sonnet-20240229"
model_provider = "claude"
max_tokens = 8192
context_window_size = 200000
model_reasoning_effort = "high"
```

Usage:
```bash
# Switch to development profile
code --profile dev "Quick code review"

# Use production profile
code --profile prod "Generate production deployment script"
```

### Conditional Provider Selection

Configure automatic provider selection based on context:

```toml
[provider_selection]
# Use Claude for code-related tasks
code_tasks_provider = "claude"
# Use OpenAI for general chat
general_chat_provider = "openai"

# File type associations
[provider_selection.by_file_type]
".py" = "claude"
".js" = "claude"
".md" = "openai"
".txt" = "openai"

# Task type associations
[provider_selection.by_task_type]
"code_generation" = "claude"
"code_review" = "claude"
"documentation" = "openai"
"general_chat" = "auto"
```

### Network and Security Configuration

```toml
[network]
# Request settings
timeout_seconds = 30
max_retries = 3
retry_backoff_ms = [1000, 2000, 4000]

# Rate limiting
requests_per_minute = 60
burst_size = 10

# Security settings
[security]
verify_ssl = true
check_certificates = true
allow_insecure_requests = false

# OAuth security
oauth_state_length = 32
oauth_nonce_length = 16
pkce_challenge_method = "S256"
```

### Logging Configuration

```toml
[logging]
level = "info"                    # "error" | "warn" | "info" | "debug"
file = "~/.codex/logs/auth.log"
max_size_mb = 10
max_files = 5
format = "json"                   # "json" | "text"

# Log different components at different levels
[logging.components]
auth = "debug"
claude = "info"
openai = "warn"
network = "error"
```

## Best Practices

### Security Best Practices

1. **Use OAuth when possible:**
   ```toml
   # Prefer OAuth over API keys
   preferred_auth_provider = "claude"
   
   [claude]
   prefer_oauth = true
   ```

2. **Secure file permissions:**
   ```bash
   # Ensure proper file permissions
   chmod 700 ~/.codex
   chmod 600 ~/.codex/claude_auth.json
   chmod 600 ~/.codex/config.toml
   ```

3. **Rotate credentials regularly:**
   ```toml
   [security]
   api_key_rotation_days = 90
   oauth_refresh_days = 30
   ```

### Performance Best Practices

1. **Optimize quota usage:**
   ```toml
   [claude]
   quota_warning_threshold = 0.7  # Get warnings earlier
   auto_fallback_enabled = true   # Prevent service interruption
   ```

2. **Configure request optimization:**
   ```toml
   [network]
   max_concurrent_requests = 5   # Don't overwhelm the API
   request_timeout_ms = 15000    # Shorter timeout for responsiveness
   ```

3. **Use appropriate models:**
   ```toml
   # Use faster models for simple tasks
   [profiles.quick]
   model = "claude-3-haiku-20240307"
   max_tokens = 1024
   
   # Use powerful models for complex tasks
   [profiles.complex]
   model = "claude-3-opus-20240229"
   max_tokens = 4096
   ```

### Development vs Production

#### Development Configuration

```toml
# ~/.codex/config.dev.toml
preferred_auth_provider = "claude"
approval_policy = "never"
model_reasoning_effort = "low"

[claude]
auto_fallback_enabled = true
quota_warning_threshold = 0.5

[logging]
level = "debug"
```

#### Production Configuration

```toml
# ~/.codex/config.prod.toml
preferred_auth_provider = "auto"
approval_policy = "on_request"
model_reasoning_effort = "medium"

[claude]
auto_fallback_enabled = true
quota_warning_threshold = 0.8
max_concurrent_requests = 3

[logging]
level = "warn"

[security]
verify_ssl = true
oauth_refresh_threshold_minutes = 60
```

Load different configs:
```bash
# Development
export CODEX_CONFIG=~/.codex/config.dev.toml
code "Debug this function"

# Production
export CODEX_CONFIG=~/.codex/config.prod.toml
code --read-only "Analyze this production issue"
```

## Team and Enterprise Configuration

### Shared Team Configuration

```toml
# team-config.toml - shared across team
[team]
name = "Engineering Team"
default_provider = "claude"
billing_account = "team-account-id"

[claude]
# Conservative settings for team usage
quota_warning_threshold = 0.6
max_concurrent_requests = 5
auto_fallback_enabled = true

[profiles.team-standard]
model = "claude-3-sonnet-20240229"
model_provider = "claude"
max_tokens = 2048
approval_policy = "on_request"

# Restrict certain operations
[restrictions]
allow_file_modifications = true
allow_network_requests = false
max_session_duration = "8h"
```

### Enterprise Configuration

```toml
# enterprise-config.toml
[enterprise]
organization = "ACME Corp"
sso_enabled = true
audit_logging = true
compliance_mode = "SOC2"

[claude]
# Enterprise quota management
quota_allocation_strategy = "team_based"
quota_monitoring = true
usage_reporting = true

[security]
require_mfa = true
session_timeout = "1h"
ip_whitelist = ["10.0.0.0/8", "192.168.0.0/16"]

[audit]
log_all_requests = true
log_retention_days = 365
compliance_export = true
```

## Migration and Upgrade Configuration

### Version Compatibility

```toml
[compatibility]
config_version = "1.0"
min_code_version = "0.3.0"
migration_strategy = "backup_and_migrate"

# Handle breaking changes
[migration]
auto_backup = true
backup_directory = "~/.codex/backups"
prompt_before_migration = true
```

### Configuration Migration

When upgrading, Code automatically migrates old configurations:

```bash
# Manual migration command
code config migrate --from-version 0.2.0 --to-version 0.3.0

# Backup current config
code config backup --name pre-claude-migration

# Restore from backup if needed
code config restore --name pre-claude-migration
```

## Troubleshooting Configuration Issues

### Configuration Validation

```bash
# Validate configuration file
code config validate

# Check which config is being used
code config show --resolved

# Test configuration
code config test --provider claude
```

### Common Configuration Errors

1. **Invalid TOML syntax:**
   ```bash
   # Check syntax
   code config validate ~/.codex/config.toml
   ```

2. **Missing required fields:**
   ```bash
   # Show required fields
   code config template > ~/.codex/config.toml
   ```

3. **Permission issues:**
   ```bash
   # Fix permissions
   chmod 700 ~/.codex
   chmod 600 ~/.codex/*.toml
   ```

## Configuration Templates

### Minimal Configuration

```toml
# Minimal working configuration
preferred_auth_provider = "claude"

[claude]
auto_fallback_enabled = true
```

### Complete Configuration Template

```bash
# Generate complete template
code config template --complete > ~/.codex/config.toml

# Generate provider-specific template  
code config template --provider claude > ~/.codex/claude-config.toml
```

## Environment-Specific Configurations

### Docker Configuration

```dockerfile
# Dockerfile with Code configuration
FROM node:18-alpine

# Install Code
RUN npm install -g @just-every/code

# Copy configuration
COPY config.toml /root/.codex/config.toml

# Set environment variables
ENV ANTHROPIC_API_KEY=sk-ant-api03-...
ENV CODE_AUTH_PROVIDER=claude
ENV CODE_LOG_LEVEL=info

# Run Code
CMD ["code", "--read-only", "Analyze this codebase"]
```

### CI/CD Configuration

```yaml
# GitHub Actions example
env:
  ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
  CODE_AUTH_PROVIDER: claude
  CODE_LOG_LEVEL: warn
  CODEX_HOME: /tmp/.codex

steps:
  - name: Setup Code config
    run: |
      mkdir -p /tmp/.codex
      echo 'preferred_auth_provider = "claude"' > /tmp/.codex/config.toml
      echo '[claude]' >> /tmp/.codex/config.toml
      echo 'auto_fallback_enabled = false' >> /tmp/.codex/config.toml
```

This configuration reference provides comprehensive coverage of all Claude authentication configuration options. For specific use cases or additional questions, consult the [troubleshooting guide](claude-auth-troubleshooting.md) or reach out to our support team.