# Claude Authentication Migration Guide

## Overview

This guide helps you migrate from an OpenAI-only setup to a dual-provider system that includes Claude authentication, or migrate from manual environment variables to the integrated authentication system.

## Migration Scenarios

### Scenario 1: OpenAI-Only to Dual Provider Setup

**Your current setup:**
- Using OpenAI ChatGPT login or API key
- No Claude authentication
- Want to add Claude while keeping OpenAI

### Scenario 2: Environment Variables to Managed Authentication

**Your current setup:**
- Using `OPENAI_API_KEY` and/or `ANTHROPIC_API_KEY` environment variables
- Manual credential management
- Want centralized authentication management

### Scenario 3: Other Claude Tools to Code Integration

**Your current setup:**
- Using other Claude CLI tools (claude-cli, anthropic-cli, etc.)
- Have existing Claude credentials
- Want to consolidate authentication in Code

### Scenario 4: Manual Configuration to Auto-Selection

**Your current setup:**
- Manually switching between providers
- Static provider selection
- Want intelligent auto-selection

## Pre-Migration Checklist

Before starting migration, complete these steps:

```bash
# 1. Check current Code version
code --version
# Should be 0.3.0 or later for Claude support

# 2. Backup existing configuration
mkdir -p ~/.codex/backup-$(date +%Y%m%d)
cp -r ~/.codex/* ~/.codex/backup-$(date +%Y%m%d)/

# 3. Check current authentication status
code auth status --detailed

# 4. Test current setup
code "Test current authentication"

# 5. Note current environment variables
env | grep -E "(OPENAI|ANTHROPIC|CLAUDE)" > ~/.codex/backup-$(date +%Y%m%d)/env-vars.txt
```

## Migration Scenarios

### Scenario 1: OpenAI-Only to Dual Provider

This is the most common migration scenario and requires no changes to your existing setup.

#### Step 1: Verify Current OpenAI Setup

```bash
# Check current OpenAI authentication
code auth status --provider openai

# Expected output:
# Provider: openai (✓ Authenticated)
#   Method: ChatGPT OAuth (or API Key)
#   Status: Active
```

#### Step 2: Add Claude Authentication

**Option A: Claude Max/Pro Subscription (Recommended)**

```bash
# Add Claude with OAuth
code auth login --provider claude

# Follow the browser OAuth flow
# This opens: https://console.anthropic.com/oauth/authorize?...
```

**Option B: Claude API Key**

```bash
# Add Claude with API key
code auth login --provider claude --api-key sk-ant-api03-...

# Or set environment variable first
export ANTHROPIC_API_KEY=sk-ant-api03-...
code auth test --provider claude
```

#### Step 3: Configure Provider Preferences

```bash
# Check both providers are authenticated
code auth status --detailed

# Expected output:
# Provider: claude (✓ Authenticated)
#   Subscription: max (Active)
#   Quota: 5000/1000000 (0.5%)
#
# Provider: openai (✓ Authenticated)
#   Method: ChatGPT OAuth
#   Status: Active

# Set preferred provider
code auth switch claude    # Use Claude by default
# or
code auth switch auto      # Intelligent auto-selection
```

#### Step 4: Test Dual Provider Setup

```bash
# Test Claude
code auth test --provider claude
code --provider claude "Hello from Claude"

# Test OpenAI
code auth test --provider openai  
code --provider openai "Hello from OpenAI"

# Test auto-selection
code "This should use the preferred provider"
```

#### Step 5: Configure Fallback (Optional)

```bash
# Enable automatic fallback from Claude to OpenAI
echo '[claude]' >> ~/.codex/config.toml
echo 'auto_fallback_enabled = true' >> ~/.codex/config.toml

# Test fallback behavior
code auth quota --provider claude  # Check current usage
```

### Scenario 2: Environment Variables to Managed Authentication

#### Step 1: Document Current Environment Variables

```bash
# Save current environment variables
echo "Current environment variables:" > ~/.codex/migration-log.txt
echo "OPENAI_API_KEY: $(echo $OPENAI_API_KEY | cut -c1-20)..." >> ~/.codex/migration-log.txt
echo "ANTHROPIC_API_KEY: $(echo $ANTHROPIC_API_KEY | cut -c1-20)..." >> ~/.codex/migration-log.txt
echo "CLAUDE_API_KEY: $(echo $CLAUDE_API_KEY | cut -c1-20)..." >> ~/.codex/migration-log.txt
```

#### Step 2: Migrate OpenAI Credentials

```bash
# If using OPENAI_API_KEY environment variable
if [ ! -z "$OPENAI_API_KEY" ]; then
    echo "Migrating OpenAI API key..."
    code auth login --provider openai --api-key "$OPENAI_API_KEY"
    echo "OpenAI migration completed"
fi

# Verify OpenAI migration
code auth status --provider openai
```

#### Step 3: Migrate Claude Credentials

```bash
# If using ANTHROPIC_API_KEY or CLAUDE_API_KEY
if [ ! -z "$ANTHROPIC_API_KEY" ]; then
    echo "Migrating Claude API key..."
    code auth login --provider claude --api-key "$ANTHROPIC_API_KEY"
    echo "Claude migration completed"
elif [ ! -z "$CLAUDE_API_KEY" ]; then
    echo "Migrating Claude API key..."
    code auth login --provider claude --api-key "$CLAUDE_API_KEY"
    echo "Claude migration completed"
fi

# Verify Claude migration
code auth status --provider claude
```

#### Step 4: Test Migrated Setup

```bash
# Test both providers work without environment variables
unset OPENAI_API_KEY
unset ANTHROPIC_API_KEY
unset CLAUDE_API_KEY

# Test OpenAI
code auth test --provider openai

# Test Claude
code auth test --provider claude

# Test overall functionality
code "Test message after migration"
```

#### Step 5: Clean Up Environment Variables (Optional)

```bash
# Remove from shell profile
# Edit ~/.bashrc, ~/.zshrc, etc. to remove:
# export OPENAI_API_KEY=...
# export ANTHROPIC_API_KEY=...

# Or keep them as backup
echo "# Backup of original environment variables" >> ~/.codex/env-backup.sh
echo "export OPENAI_API_KEY_BACKUP=$OPENAI_API_KEY" >> ~/.codex/env-backup.sh
echo "export ANTHROPIC_API_KEY_BACKUP=$ANTHROPIC_API_KEY" >> ~/.codex/env-backup.sh
```

### Scenario 3: Other Claude Tools to Code Integration

#### Step 1: Identify Existing Claude Setup

```bash
# Check if other Claude tools are installed
which claude-cli
which anthropic-cli

# Check their configuration
ls -la ~/.anthropic/
ls -la ~/.claude/

# Test existing credentials
curl -H "Authorization: Bearer $ANTHROPIC_API_KEY" \
     https://api.anthropic.com/v1/messages \
     -H "Content-Type: application/json" \
     -d '{"model":"claude-3-haiku-20240307","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}'
```

#### Step 2: Import Credentials to Code

```bash
# Import API key if available
if [ ! -z "$ANTHROPIC_API_KEY" ]; then
    code auth login --provider claude --api-key "$ANTHROPIC_API_KEY"
fi

# Or set up OAuth for enhanced features
code auth login --provider claude
```

#### Step 3: Migrate Configuration

```bash
# Check other tools' configuration for preferences
cat ~/.anthropic/config.toml
cat ~/.claude/config.json

# Apply similar settings to Code
cat >> ~/.codex/config.toml << EOF
[claude]
# Migrated from other Claude tools
max_tokens = 4096
temperature = 0.7
EOF
```

#### Step 4: Test Integration

```bash
# Test Code with Claude
code auth test --provider claude
code "Compare this with your previous Claude tool output"

# Verify you can still use other tools if needed
# claude-cli "Same test message"
```

### Scenario 4: Manual to Auto-Selection

#### Step 1: Current Manual Setup

```bash
# Document current manual switching pattern
code auth status --detailed

# Test both providers
code auth test --provider openai
code auth test --provider claude
```

#### Step 2: Configure Auto-Selection Rules

```bash
# Set up auto-selection
code auth switch auto

# Configure selection preferences in config.toml
cat >> ~/.codex/config.toml << EOF
[provider_selection]
# Prefer Claude for code tasks
code_tasks_provider = "claude"
# Use OpenAI for general chat
general_chat_provider = "openai"

# Auto-selection priority
priority_order = ["claude", "openai"]

[claude]
# Enable fallback when Claude quota is low
auto_fallback_enabled = true
quota_warning_threshold = 0.8
EOF
```

#### Step 3: Test Auto-Selection

```bash
# Test auto-selection behavior
code "Generate a Python function"  # Should use Claude for code
code "What's the weather like?"     # May use either provider

# Check which provider was used
code auth status --detailed
```

## Post-Migration Validation

After completing any migration scenario, validate your setup:

### Validation Checklist

```bash
# 1. Check authentication status
code auth status --detailed

# 2. Test both providers individually
code auth test --provider openai
code auth test --provider claude

# 3. Test provider switching
code auth switch claude
code "Test Claude provider"
code auth switch openai  
code "Test OpenAI provider"
code auth switch auto
code "Test auto-selection"

# 4. Test quota management (Claude Max users)
code auth quota --detailed

# 5. Test fallback behavior (if configured)
# This requires temporarily exhausting quota or simulating failure

# 6. Test configuration persistence
code --version
# Restart terminal session
code auth status --detailed
```

### Performance Validation

```bash
# Test response times
time code "Quick test message"

# Test concurrent requests
for i in {1..3}; do
    code "Test message $i" &
done
wait

# Monitor quota usage
code auth quota --detailed
```

## Common Migration Issues and Solutions

### Issue: "Provider authentication conflict"

**Symptom:** Both providers authenticated but wrong one being used

**Solution:**
```bash
# Clear provider preference and re-select
rm ~/.codex/auth_config.json
code auth switch claude  # or preferred provider
```

### Issue: "Environment variables still taking precedence"

**Symptom:** Managed authentication not being used despite setup

**Solution:**
```bash
# Temporarily unset environment variables
unset OPENAI_API_KEY ANTHROPIC_API_KEY CLAUDE_API_KEY

# Test managed authentication
code auth test --provider claude

# If working, remove from shell profile permanently
```

### Issue: "OAuth tokens not refreshing"

**Symptom:** OAuth authentication expires and doesn't auto-refresh

**Solution:**
```bash
# Force token refresh
code auth login --provider claude --force

# Check OAuth settings
cat ~/.codex/claude_auth.json | grep -E "(expires|refresh)"

# Enable auto-refresh in config
echo '[oauth]' >> ~/.codex/config.toml
echo 'auto_refresh = true' >> ~/.codex/config.toml
```

### Issue: "Quota not updating"

**Symptom:** Quota information not showing or outdated

**Solution:**
```bash
# Force quota refresh
code auth status --provider claude --force-refresh

# Check quota cache settings
code auth quota --detailed

# Clear quota cache
rm ~/.codex/cache/claude_quota.json
```

## Rollback Procedures

If migration fails or you need to rollback:

### Complete Rollback

```bash
# 1. Stop Code processes
pkill -f code

# 2. Restore from backup
rm -rf ~/.codex/*
cp -r ~/.codex/backup-$(date +%Y%m%d)/* ~/.codex/

# 3. Restore environment variables
source ~/.codex/backup-$(date +%Y%m%d)/env-vars.txt

# 4. Test original setup
code "Test rollback"
```

### Partial Rollback (Remove Claude, Keep OpenAI)

```bash
# Remove Claude authentication
code auth logout --provider claude
rm ~/.codex/claude_auth.json

# Keep OpenAI authentication
code auth status --provider openai

# Test OpenAI still works
code "Test OpenAI after Claude removal"
```

### Rollback to Environment Variables

```bash
# Remove managed authentication
code auth logout --all

# Restore environment variables
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...

# Test environment variable authentication
code "Test with environment variables"
```

## Migration Best Practices

### Planning

1. **Read documentation first** - Review all setup guides before starting
2. **Test in development** - Use a development environment if possible
3. **Backup everything** - Backup configs, credentials, and environment
4. **Plan rollback** - Have a rollback plan ready before starting

### Execution

1. **Migrate gradually** - Add Claude while keeping OpenAI
2. **Test frequently** - Validate each step before proceeding
3. **Monitor quotas** - Watch quota usage during migration
4. **Document changes** - Keep notes of what you changed

### Post-Migration

1. **Monitor performance** - Watch for any performance changes
2. **Optimize configuration** - Adjust settings based on usage
3. **Train users** - If team migration, train team members
4. **Plan maintenance** - Set up regular authentication maintenance

## Team Migration

For team environments:

### Preparation

```bash
# Create team migration plan
cat > team-migration-plan.md << EOF
# Team Claude Migration Plan

## Timeline
- Week 1: Individual developer testing
- Week 2: Staging environment migration  
- Week 3: Production environment migration

## Roles
- Migration lead: [Name]
- Testing lead: [Name]
- Rollback coordinator: [Name]

## Communication
- Slack channel: #claude-migration
- Status updates: Daily standup
- Emergency contact: [Contact]
EOF
```

### Execution

```bash
# Create shared configuration template
cat > team-config.toml << EOF
preferred_auth_provider = "auto"

[claude]
auto_fallback_enabled = true
quota_warning_threshold = 0.7

[team]
name = "Engineering Team"
shared_quota_monitoring = true
EOF

# Distribute to team members
scp team-config.toml user@dev-server:~/.codex/config.toml
```

### Validation

```bash
# Team validation script
#!/bin/bash
echo "Team Migration Validation"
echo "========================"

for user in alice bob charlie; do
    echo "Testing user: $user"
    ssh $user@dev-server 'code auth status --detailed'
    ssh $user@dev-server 'code auth test --provider claude'
    echo "---"
done
```

## Maintenance After Migration

### Regular Tasks

```bash
# Weekly authentication health check
code auth status --detailed
code auth test --provider claude
code auth test --provider openai

# Monthly quota review
code auth quota --detailed

# Quarterly credential rotation
code auth login --provider claude --force
```

### Monitoring

```bash
# Set up quota monitoring
echo '[monitoring]' >> ~/.codex/config.toml
echo 'quota_alerts = true' >> ~/.codex/config.toml
echo 'alert_threshold = 0.8' >> ~/.codex/config.toml

# Check authentication logs
tail -f ~/.codex/logs/auth.log
```

This migration guide covers all common scenarios for adopting Claude authentication. For specific edge cases or additional help, consult the [troubleshooting guide](claude-auth-troubleshooting.md) or contact support.