# Claude Max Quick Start Guide
## 5-Minute Setup for Claude Authentication Without API Keys

Get up and running with Claude Max authentication in minutes. No API keys required!

---

## ⚡ **1-Minute Setup**

### Prerequisites
- ✅ Claude Max subscription
- ✅ Modern browser (Chrome, Firefox, Safari, Edge)

### Installation & Authentication
```bash
# 1. Install Claude Code CLI
npm install -g @anthropic/claude-code

# 2. Start authentication
code auth login --provider claude
# Browser opens automatically → Login with Claude Max → Done!

# 3. Verify setup
code auth status --provider claude
```

**That's it!** You're now using Claude without API keys. 🎉

---

## 🚀 **Quick Commands**

### Essential Commands
```bash
# Generate code
code exec "Create a Python function to calculate fibonacci"

# Check your quota
code auth quota

# Switch providers if needed
code auth switch --provider openai
code auth switch --provider claude

# Get help
code auth --help
```

### Status Check
```bash
code auth status --detailed
```
Expected output:
```
Authentication Status:
┌──────────┬────────────────────┬─────────────┬──────────────┐
│ Provider │ Status             │ Subscription│ Quota Used   │
├──────────┼────────────────────┼─────────────┼──────────────┤
│ claude   │ ✅ Authenticated   │ Max         │ 1,234/1M     │
└──────────┴────────────────────┴─────────────┴──────────────┘
```

---

## 🔧 **Common Issues & Quick Fixes**

### Issue: Browser doesn't open
```bash
code auth login --provider claude --manual
# Then manually visit the URL shown
```

### Issue: "Not a Claude Max subscriber"
```bash
# Upgrade your Claude subscription at https://claude.ai
# Then re-authenticate:
code auth logout --provider claude
code auth login --provider claude
```

### Issue: Authentication expires
```bash
code auth refresh --provider claude
```

### Issue: Quota exhausted
```bash
# Check usage
code auth quota --detailed

# Enable OpenAI fallback
code config set auth.enable_fallback true
```

---

## 🎯 **Configuration Shortcuts**

### Make Claude your default
```bash
code config set auth.preferred_provider claude
```

### Enable smart fallback
```bash
code config set auth.enable_fallback true
code config set auth.fallback_order "claude,openai"
```

### Set quota alerts
```bash
code config set claude.quota_alerts true
code config set claude.alert_threshold 0.8
```

---

## 📱 **Usage Examples**

### Basic Code Generation
```bash
# Python
code exec "Write a Python web scraper using requests and BeautifulSoup"

# JavaScript
code exec "Create a React component for a user profile card"

# System tasks
code exec "Write a bash script to backup a directory"
```

### Multi-Agent Workflows
```bash
# Create multiple agents for complex tasks
code agents create --count 3 --provider claude
code exec --agents 3 "Design, implement, and test a REST API"
```

### Project-Specific Setup
```bash
# Set Claude for current project only
cd my-project
code config set --local auth.preferred_provider claude
```

---

## 📞 **Need Help?**

### Quick Debugging
```bash
# Enable debug mode
export DEBUG=claude:auth
code auth status --provider claude

# Check logs
code auth logs --provider claude
```

### Get Support
- **Documentation**: [Full Setup Guide](./claude-max-setup-guide.md)
- **Troubleshooting**: [Troubleshooting Guide](./claude-auth-troubleshooting.md)
- **Issues**: [GitHub Issues](https://github.com/anthropics/claude-code/issues)

---

## 🏆 **Pro Tips**

1. **Bookmark** your quota page: `code auth quota --web`
2. **Set up aliases** for common commands:
   ```bash
   alias cauth="code auth status --provider claude"
   alias cquota="code auth quota"
   alias cswitch="code auth switch --provider claude"
   ```
3. **Use tab completion** for faster typing
4. **Monitor usage** with `code auth quota --watch`

---

**Ready to code with Claude Max!** 🚀

For detailed configuration, troubleshooting, and advanced features, see the [Complete Setup Guide](./claude-max-setup-guide.md).