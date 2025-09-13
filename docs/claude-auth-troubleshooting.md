# Claude Authentication Troubleshooting Guide

## Overview

This guide helps you diagnose and resolve common issues with Claude authentication in the Code project. For setup instructions, see the [Claude Authentication Setup Guide](claude-auth-setup-guide.md).

## Quick Diagnostics

Before diving into specific issues, run these commands to get an overview:

```bash
# Check overall authentication status
code auth status --detailed

# Test provider connectivity
code auth test --provider claude
code auth test --provider openai

# Check quota limits (Claude Max users)
code auth quota --detailed

# List available providers
code auth providers
```

## Common Issues and Solutions

### Authentication Issues

#### Issue: "Provider not authenticated"

**Symptoms:**
```
Error: Provider 'claude' not authenticated
Run 'code auth login --provider claude' to authenticate
```

**Solutions:**

1. **Authenticate with Claude:**
   ```bash
   code auth login --provider claude
   ```

2. **If using API key:**
   ```bash
   export ANTHROPIC_API_KEY=sk-ant-api03-...
   code auth test --provider claude
   ```

3. **Force re-authentication:**
   ```bash
   code auth login --provider claude --force
   ```

4. **Check for existing authentication:**
   ```bash
   code auth status --provider claude
   ls -la ~/.codex/claude_auth.json
   ```

#### Issue: "Subscription verification failed"

**Symptoms:**
```
Error: Unable to verify Claude subscription
Check your subscription status at console.anthropic.com
```

**Solutions:**

1. **Check subscription status:**
   - Visit https://console.anthropic.com
   - Verify your subscription is active
   - Check if payment is current

2. **Re-authenticate:**
   ```bash
   code auth login --provider claude --force
   ```

3. **Check subscription in Code:**
   ```bash
   code auth status --provider claude --detailed
   ```

4. **Verify API access:**
   ```bash
   curl -H "Authorization: Bearer $(code auth token --provider claude)" \
        https://api.anthropic.com/v1/subscription
   ```

#### Issue: "Token expired"

**Symptoms:**
```
Error: Authentication token has expired
Token expired at: 2025-09-13 10:30 UTC
```

**Solutions:**

1. **Automatic refresh (should happen automatically):**
   ```bash
   code auth status --provider claude
   ```

2. **Manual refresh:**
   ```bash
   code auth login --provider claude --force
   ```

3. **Check token expiration:**
   ```bash
   code auth status --provider claude --detailed
   ```

### OAuth Flow Issues

#### Issue: Browser doesn't open automatically

**Symptoms:**
- Authentication starts but no browser window appears
- URL is displayed but not clickable

**Solutions:**

1. **Copy URL manually:**
   ```bash
   code auth login --provider claude
   # Copy the displayed URL to your browser
   ```

2. **Check default browser:**
   ```bash
   # Linux
   xdg-open https://google.com  # Should open browser

   # macOS  
   open https://google.com

   # Windows
   start https://google.com
   ```

3. **Set browser environment variable:**
   ```bash
   export BROWSER=/usr/bin/firefox  # or your preferred browser
   code auth login --provider claude
   ```

#### Issue: "Localhost connection refused"

**Symptoms:**
```
Error: Connection refused to localhost:1456
OAuth callback server failed to start
```

**Solutions:**

1. **Check port availability:**
   ```bash
   # Check if port 1456 is in use
   lsof -i :1456
   netstat -tlnp | grep 1456
   ```

2. **Kill conflicting processes:**
   ```bash
   # Kill processes using port 1456
   sudo lsof -t -i:1456 | xargs kill -9
   ```

3. **Try different port (advanced):**
   ```bash
   export CLAUDE_OAUTH_PORT=1457
   code auth login --provider claude
   ```

4. **Check firewall settings:**
   ```bash
   # Allow localhost connections
   sudo ufw allow from 127.0.0.1 to any port 1456
   ```

#### Issue: OAuth redirect errors

**Symptoms:**
```
Error: OAuth callback received error: access_denied
The user denied the authorization request
```

**Solutions:**

1. **Retry authorization:**
   ```bash
   code auth login --provider claude --force
   ```

2. **Check browser cookies/cache:**
   - Clear browser cache and cookies for console.anthropic.com
   - Try incognito/private browsing mode

3. **Check account permissions:**
   - Ensure you're logged into the correct Anthropic account
   - Verify account has necessary permissions

### Quota and Rate Limiting Issues

#### Issue: "Quota exceeded"

**Symptoms:**
```
Error: Claude quota exceeded
Used: 1000000/1000000 tokens (100%)
Quota resets: 2025-09-14 00:00 UTC
```

**Solutions:**

1. **Check quota status:**
   ```bash
   code auth quota --detailed
   ```

2. **Switch to alternative provider:**
   ```bash
   code auth switch openai
   ```

3. **Wait for quota reset:**
   ```bash
   # Check when quota resets
   code auth quota | grep "Resets"
   ```

4. **Enable auto-fallback:**
   ```bash
   # Add to ~/.codex/config.toml
   echo "[claude]" >> ~/.codex/config.toml
   echo "auto_fallback_enabled = true" >> ~/.codex/config.toml
   ```

#### Issue: "Rate limit exceeded"

**Symptoms:**
```
Error: Rate limit exceeded
Requests per minute: 50/50
Wait time: 45 seconds
```

**Solutions:**

1. **Wait and retry:**
   ```bash
   # Wait for rate limit reset
   sleep 60
   code auth test --provider claude
   ```

2. **Check rate limits:**
   ```bash
   code auth status --provider claude --detailed
   ```

3. **Switch providers temporarily:**
   ```bash
   code auth switch openai
   ```

### File Permission Issues

#### Issue: "Permission denied" accessing auth files

**Symptoms:**
```
Error: Permission denied: ~/.codex/claude_auth.json
Unable to read authentication file
```

**Solutions:**

1. **Fix file permissions:**
   ```bash
   chmod 600 ~/.codex/claude_auth.json
   chmod 700 ~/.codex/
   ```

2. **Check file ownership:**
   ```bash
   ls -la ~/.codex/
   # Files should be owned by your user
   ```

3. **Recreate auth files:**
   ```bash
   rm ~/.codex/claude_auth.json
   code auth login --provider claude
   ```

4. **Check directory permissions:**
   ```bash
   mkdir -p ~/.codex
   chmod 700 ~/.codex
   ```

### Network and Connectivity Issues

#### Issue: "Connection timeout"

**Symptoms:**
```
Error: Connection timeout to api.anthropic.com
Request timed out after 30 seconds
```

**Solutions:**

1. **Check internet connectivity:**
   ```bash
   ping api.anthropic.com
   curl -I https://api.anthropic.com/v1/health
   ```

2. **Check proxy settings:**
   ```bash
   echo $HTTP_PROXY
   echo $HTTPS_PROXY
   
   # If using proxy, configure:
   export HTTPS_PROXY=http://proxy.company.com:8080
   ```

3. **DNS resolution issues:**
   ```bash
   nslookup api.anthropic.com
   
   # Try alternative DNS
   export DNS_SERVER=8.8.8.8
   ```

4. **Corporate firewall:**
   ```bash
   # Check if HTTPS requests are blocked
   curl -v https://api.anthropic.com/v1/health
   ```

#### Issue: "SSL certificate verification failed"

**Symptoms:**
```
Error: SSL certificate verification failed
certificate verify failed: unable to get local issuer certificate
```

**Solutions:**

1. **Update certificates:**
   ```bash
   # Ubuntu/Debian
   sudo apt update && sudo apt install ca-certificates
   
   # macOS
   brew upgrade ca-certificates
   
   # CentOS/RHEL
   sudo yum update ca-certificates
   ```

2. **Temporary workaround (not recommended for production):**
   ```bash
   export CURL_CA_BUNDLE=""
   export SSL_VERIFY=false
   ```

3. **Corporate CA certificates:**
   ```bash
   # Add corporate CA to system certificate store
   sudo cp corporate-ca.crt /usr/local/share/ca-certificates/
   sudo update-ca-certificates
   ```

### Configuration Issues

#### Issue: Provider selection not working

**Symptoms:**
- `code auth switch claude` doesn't take effect
- Wrong provider being used despite configuration

**Solutions:**

1. **Check configuration:**
   ```bash
   code auth status --detailed
   cat ~/.codex/auth_config.json
   ```

2. **Verify provider preference:**
   ```bash
   code auth providers --active-only
   ```

3. **Reset configuration:**
   ```bash
   rm ~/.codex/auth_config.json
   code auth switch claude
   ```

4. **Check config file syntax:**
   ```bash
   # Validate JSON syntax
   python -m json.tool ~/.codex/auth_config.json
   ```

#### Issue: Environment variables not recognized

**Symptoms:**
- `ANTHROPIC_API_KEY` set but not being used
- Provider shows as not authenticated

**Solutions:**

1. **Check environment variables:**
   ```bash
   echo $ANTHROPIC_API_KEY
   echo $CLAUDE_API_KEY
   env | grep -E "(ANTHROPIC|CLAUDE)"
   ```

2. **Verify variable format:**
   ```bash
   # Correct format
   export ANTHROPIC_API_KEY=sk-ant-api03-...
   
   # Not this
   export ANTHROPIC_API_KEY="sk-ant-api03-..."  # quotes can cause issues
   ```

3. **Test environment variable:**
   ```bash
   code auth test --provider claude
   ```

4. **Force environment variable usage:**
   ```bash
   unset ANTHROPIC_API_KEY
   export ANTHROPIC_API_KEY=sk-ant-api03-...
   code auth login --provider claude --api-key $ANTHROPIC_API_KEY
   ```

## Advanced Troubleshooting

### Debug Mode

Enable detailed logging for troubleshooting:

```bash
# Enable debug logging
export DEBUG=code:auth
export CODE_LOG_LEVEL=debug

# Run command with debug output
code auth login --provider claude

# Check log files
tail -f ~/.codex/logs/auth.log
tail -f ~/.codex/logs/debug.log
```

### Network Debugging

Debug network issues:

```bash
# Test API connectivity
curl -v -H "Authorization: Bearer sk-ant-api03-..." \
     https://api.anthropic.com/v1/messages \
     -H "Content-Type: application/json" \
     -d '{"model":"claude-3-sonnet-20240229","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}'

# Test OAuth endpoints
curl -v https://console.anthropic.com/.well-known/openid_configuration

# Check SSL/TLS
openssl s_client -connect api.anthropic.com:443 -servername api.anthropic.com
```

### File System Debugging

Check file system issues:

```bash
# Check disk space
df -h ~/.codex

# Check file system permissions
ls -la ~/.codex/
stat ~/.codex/claude_auth.json

# Check for file locks
lsof ~/.codex/claude_auth.json

# Validate JSON files
python -m json.tool ~/.codex/claude_auth.json
python -m json.tool ~/.codex/auth_config.json
```

### System-Specific Issues

#### macOS Issues

**Keychain access problems:**
```bash
# Reset keychain access
security delete-generic-password -s "code-claude-auth"
code auth login --provider claude
```

**Permission issues:**
```bash
# Fix macOS permissions
sudo chown -R $(whoami) ~/.codex
chmod -R u+rwX ~/.codex
```

#### Linux Issues

**SELinux issues:**
```bash
# Check SELinux status
sestatus

# Temporarily disable if needed
sudo setenforce 0
```

**AppArmor issues:**
```bash
# Check AppArmor status
sudo apparmor_status | grep code

# Disable profile if needed
sudo ln -s /etc/apparmor.d/usr.bin.code /etc/apparmor.d/disable/
sudo apparmor_parser -R /etc/apparmor.d/usr.bin.code
```

#### Windows Issues

**PowerShell execution policy:**
```powershell
# Check execution policy
Get-ExecutionPolicy

# Set to allow local scripts
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

**Windows path issues:**
```bash
# Check path separators in config
cat $USERPROFILE/.codex/auth_config.json

# Use forward slashes or escaped backslashes
```

## Recovery Procedures

### Complete Authentication Reset

If all else fails, perform a complete reset:

```bash
# 1. Backup existing config (optional)
cp -r ~/.codex ~/.codex.backup

# 2. Remove authentication files
rm ~/.codex/claude_auth.json
rm ~/.codex/auth_config.json
rm ~/.codex/auth.json  # Only if you want to reset OpenAI too

# 3. Clear environment variables
unset ANTHROPIC_API_KEY
unset CLAUDE_API_KEY
unset OPENAI_API_KEY

# 4. Restart authentication
code auth login --provider claude

# 5. Verify setup
code auth status --detailed
```

### Recover from Backup

If you need to restore previous configuration:

```bash
# Restore from backup
cp -r ~/.codex.backup/* ~/.codex/

# Verify restoration
code auth status --detailed
```

## Getting Additional Help

### Collect Diagnostic Information

When seeking help, collect this information:

```bash
# System information
uname -a
code --version

# Authentication status
code auth status --detailed

# Configuration files (remove sensitive data!)
ls -la ~/.codex/
cat ~/.codex/config.toml

# Network connectivity
curl -I https://api.anthropic.com/v1/health

# Log files (recent entries)
tail -50 ~/.codex/logs/auth.log
```

### Contact Support

- **GitHub Issues**: https://github.com/just-every/code/issues
- **Documentation**: https://github.com/just-every/code/docs
- **Community Discord**: [Join our community](https://discord.gg/code)

When reporting issues, include:
- Operating system and version
- Code version (`code --version`)
- Steps to reproduce the issue
- Error messages (remove API keys!)
- Output of diagnostic commands

### Known Issues and Workarounds

#### Issue: OAuth state parameter mismatch
**Workaround**: Clear browser cache and retry

#### Issue: Claude API rate limits hit frequently
**Workaround**: Use auto-fallback to OpenAI or implement request queuing

#### Issue: File permission errors on shared systems
**Workaround**: Use `CODEX_HOME` environment variable to point to writable directory

## Prevention Tips

### Regular Maintenance

1. **Check authentication status monthly:**
   ```bash
   code auth status --detailed
   ```

2. **Monitor quota usage:**
   ```bash
   code auth quota --detailed
   ```

3. **Keep tokens fresh:**
   ```bash
   # OAuth tokens refresh automatically
   # API keys should be rotated periodically
   ```

4. **Backup configuration:**
   ```bash
   cp ~/.codex/config.toml ~/config-backup-$(date +%Y%m%d).toml
   ```

### Best Practices

1. **Use OAuth when possible** - More reliable than API keys
2. **Set up auto-fallback** - Ensures continuity when quotas are exceeded
3. **Monitor logs** - Check for authentication warnings
4. **Keep Code updated** - Install updates promptly for bug fixes
5. **Test in development** - Verify authentication before production use

This troubleshooting guide should help you resolve most Claude authentication issues. For issues not covered here, please check our GitHub issues or reach out to the community.