#!/bin/bash
# Claude Code Beta Publishing Script
# Publishes @kinginyyellow/code-claude for testing

set -e

echo "🚀 Claude Code Beta Publishing Script"
echo "======================================"

# Check if we're in the right directory
if [ ! -f "codex-rs/Cargo.toml" ]; then
    echo "❌ Error: Run this script from the project root directory"
    exit 1
fi

# Check npm authentication
echo "📋 Checking npm authentication..."
if ! npm whoami > /dev/null 2>&1; then
    echo "❌ Error: Not logged into npm. Run 'npm login' first."
    exit 1
fi

NPM_USER=$(npm whoami)
echo "✅ Logged in as: $NPM_USER"

# Build Rust binaries
echo "🔨 Building Rust binaries..."
cd codex-rs
if ! cargo build --release --bin code --bin code-tui --bin code-exec; then
    echo "❌ Error: Failed to build Rust binaries"
    exit 1
fi

# Verify binaries exist
if [ ! -f "target/release/code" ]; then
    echo "❌ Error: Main binary not found at target/release/code"
    exit 1
fi

echo "✅ Rust binaries built successfully"
cd ..

# Prepare publishing directory
echo "📦 Preparing package for publishing..."
PUBLISH_DIR="codex-cli/.pack/package"

if [ ! -d "$PUBLISH_DIR" ]; then
    echo "❌ Error: Publishing directory not found: $PUBLISH_DIR"
    exit 1
fi

cd "$PUBLISH_DIR"

# Backup original package.json
cp package.json package.json.backup

# Copy our beta configuration
cp ../../../publish-beta-package.json package.json

echo "✅ Package configuration updated"

# Dry run to check package contents
echo "🧪 Testing package contents..."
if ! npm pack --dry-run; then
    echo "❌ Error: Package validation failed"
    # Restore original package.json
    mv package.json.backup package.json
    exit 1
fi

# Ask for confirmation
echo ""
echo "📋 About to publish:"
echo "   Package: @kinginyyellow/code-claude"
echo "   Version: 0.2.144-claude-beta.0"
echo "   Tag: beta"
echo "   Registry: https://registry.npmjs.org"
echo ""
read -p "Continue with publishing? (y/N): " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "❌ Publishing cancelled"
    # Restore original package.json
    mv package.json.backup package.json
    exit 1
fi

# Publish the package
echo "🚀 Publishing to npm..."
if npm publish --tag beta; then
    echo ""
    echo "🎉 Successfully published @kinginyyellow/code-claude@beta!"
    echo ""
    echo "📥 To install:"
    echo "   npm install -g @kinginyyellow/code-claude@beta"
    echo ""
    echo "🧪 To test:"
    echo "   code-claude --version"
    echo "   claude-code auth login --provider claude"
    echo ""
    echo "📊 To view on npm:"
    echo "   https://www.npmjs.com/package/@kinginyyellow/code-claude/v/0.2.144-claude-beta.0"
else
    echo "❌ Error: Publishing failed"
    # Restore original package.json
    mv package.json.backup package.json
    exit 1
fi

# Restore original package.json
mv package.json.backup package.json

echo "✅ Original package.json restored"
echo "🎯 Beta publishing complete!"