# NPM Publishing Guide for Code with Claude Authentication

This guide explains how to publish your Claude-enhanced Code fork to npm for testing and distribution.

---

## 📋 **Overview**

The Code project uses a complex publishing system with:
- **Main package**: `@just-every/code` (JavaScript CLI wrapper)
- **Platform binaries**: 5 separate packages for different architectures
- **Rust build system**: Compiles native binaries for all platforms
- **GitHub Actions**: Automated cross-platform building and publishing

---

## 🎯 **Publishing Strategy for Your Fork**

### **Recommended Approach: Scoped Beta Package**

Publish under your own scope to avoid conflicts:
- **Package name**: `@kinginyyellow/code-claude`
- **Version format**: `0.2.144-claude-beta.0`
- **Distribution tag**: `beta` (prevents accidental installs)

### **Required Changes**

#### 1. **Update Main Package Configuration**

Edit `/codex-cli/.pack/package/package.json`:

```json
{
  "name": "@kinginyyellow/code-claude",
  "version": "0.2.144-claude-beta.0",
  "description": "Claude Code with Claude Max authentication - Enhanced fork with OAuth integration",
  "bin": {
    "code-claude": "bin/coder.js",
    "claude-code": "bin/coder.js"
  },
  "repository": {
    "type": "git",
    "url": "git+https://github.com/KingInYellow18/code.git"
  },
  "publishConfig": {
    "access": "public",
    "tag": "beta"
  },
  "optionalDependencies": {
    "@kinginyyellow/code-claude-darwin-arm64": "0.2.144-claude-beta.0",
    "@kinginyyellow/code-claude-darwin-x64": "0.2.144-claude-beta.0",
    "@kinginyyellow/code-claude-linux-x64-musl": "0.2.144-claude-beta.0",
    "@kinginyyellow/code-claude-linux-arm64-musl": "0.2.144-claude-beta.0",
    "@kinginyyellow/code-claude-win32-x64": "0.2.144-claude-beta.0"
  }
}
```

#### 2. **Update Platform Package Names**

Edit `/codex-cli/postinstall.js` to use your scoped package names:

```javascript
// Around line 90-100, update the package name mapping
const packageName = `@kinginyyellow/code-claude-${platform}-${arch}`;
```

#### 3. **Modify prepublishOnly Guard**

Edit `/codex-cli/.pack/package/package.json` to allow local testing:

```json
"scripts": {
  "prepublishOnly": "node -e \"console.log('Publishing @kinginyyellow/code-claude beta version...');\""
}
```

---

## 🔧 **Step-by-Step Publishing Process**

### **Prerequisites**

1. **NPM Account Setup**:
   ```bash
   # Create npm account if needed
   npm adduser
   
   # Login to npm
   npm login
   
   # Verify login
   npm whoami
   ```

2. **Verify Repository**:
   ```bash
   # Ensure you're on main branch with latest changes
   git checkout main
   git pull origin main
   ```

### **Option 1: Local Testing Publish (Recommended)**

#### Step 1: Build the Project
```bash
# Build Rust binaries
cd codex-rs
cargo build --release --bin code --bin code-tui --bin code-exec

# Verify binaries exist
ls -la target/release/code*
```

#### Step 2: Prepare Package
```bash
cd ../codex-cli/.pack/package

# Update package.json with your scope
# (Edit the file as shown above)

# Test package structure
npm pack --dry-run
```

#### Step 3: Test Locally
```bash
# Create a local test installation
npm pack
npm install -g ./kinginyyellow-code-claude-0.2.144-claude-beta.0.tgz

# Test the installation
code-claude --version
claude-code auth login --provider claude
```

#### Step 4: Publish Beta Version
```bash
# Publish to npm with beta tag
npm publish --tag beta

# Verify publication
npm view @kinginyyellow/code-claude@beta
```

### **Option 2: GitHub Actions Automated Publish**

#### Step 1: Set up NPM Token
```bash
# Generate npm token
npm token create --read-write

# Add to GitHub repository secrets as NPM_TOKEN
```

#### Step 2: Update GitHub Actions
Create `.github/workflows/publish-beta.yml`:

```yaml
name: Publish Beta Version

on:
  push:
    branches: [ main ]
    paths:
      - 'codex-cli/**'
      - 'codex-rs/**'
      - '.github/workflows/publish-beta.yml'

jobs:
  publish-beta:
    runs-on: ubuntu-latest
    if: contains(github.event.head_commit.message, '[publish-beta]')
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          registry-url: 'https://registry.npmjs.org'
          
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          
      - name: Build Rust binaries
        run: |
          cd codex-rs
          cargo build --release --bin code --bin code-tui --bin code-exec
          
      - name: Publish to npm
        run: |
          cd codex-cli/.pack/package
          npm publish --tag beta
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

---

## 📦 **Testing Your Published Package**

### **Installation Testing**

```bash
# Install your beta package
npm install -g @kinginyyellow/code-claude@beta

# Test basic functionality
code-claude --version
claude-code --help

# Test Claude authentication
claude-code auth login --provider claude

# Test code generation
claude-code exec "Create a hello world in Python"
```

### **Uninstall and Cleanup**

```bash
# Remove test installation
npm uninstall -g @kinginyyellow/code-claude

# Install specific version if needed
npm install -g @kinginyyellow/code-claude@0.2.144-claude-beta.0
```

---

## 🚀 **Publishing Workflow**

### **Beta Release Cycle**

1. **Development**: Make changes on feature branches
2. **Integration**: Merge to main branch
3. **Version Update**: Increment beta version (`0.2.144-claude-beta.1`)
4. **Publish**: `npm publish --tag beta`
5. **Test**: Install and validate functionality
6. **Iterate**: Repeat until stable

### **Stable Release Process**

When ready for stable release:

```bash
# Update to stable version
# Edit package.json: "version": "0.2.145-claude.0"

# Publish as latest
npm publish

# Tag in git
git tag v0.2.145-claude.0
git push origin v0.2.145-claude.0
```

---

## ⚠️ **Important Considerations**

### **Binary Dependencies**

Your package depends on platform-specific binaries that need to be:
1. **Built for each platform** (macOS, Linux, Windows on x64/ARM64)
2. **Published as separate packages** (5 platform packages)
3. **Referenced correctly** in optionalDependencies

**Note**: For initial testing, you can publish with just your current platform binary and add others later.

### **Version Management**

- **Beta versions**: Use `-claude-beta.X` suffix
- **Stable versions**: Use `-claude.X` suffix  
- **Keep upstream compatibility**: Can sync with upstream versions

### **Security Considerations**

- **Never commit npm tokens** to repository
- **Use GitHub secrets** for automated publishing
- **Test thoroughly** before publishing stable versions
- **Monitor download stats** and user feedback

---

## 📞 **Troubleshooting**

### **Common Issues**

1. **"Package already exists"**:
   ```bash
   # Increment version number
   npm version patch --preid=beta
   ```

2. **"Permission denied"**:
   ```bash
   # Re-login to npm
   npm logout && npm login
   ```

3. **Binary not found**:
   ```bash
   # Check binary exists and is executable
   ls -la ../codex-rs/target/release/code
   chmod +x ../codex-rs/target/release/code
   ```

4. **prepublishOnly blocking**:
   ```bash
   # Temporarily disable or modify the guard
   # Or set CI=true environment variable
   CI=true npm publish --tag beta
   ```

---

## 🎯 **Quick Start Commands**

```bash
# 1. Set up npm authentication
npm login

# 2. Update package configuration (edit package.json)
# 3. Build project
cd codex-rs && cargo build --release

# 4. Test package locally
cd ../codex-cli/.pack/package
npm pack --dry-run

# 5. Publish beta version
npm publish --tag beta

# 6. Test installation
npm install -g @kinginyyellow/code-claude@beta
```

---

This guide provides a complete workflow for publishing and testing your Claude-enhanced Code fork on npm while maintaining clear separation from the original package.