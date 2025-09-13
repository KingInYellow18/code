# Claude Authentication Integration Test Suite

## Overview

This test suite provides comprehensive validation for Claude authentication integration as specified in Phase 5 of the integration plan. It ensures robust authentication while maintaining backward compatibility with existing OpenAI authentication.

## Test Structure

```
tests/
├── claude_auth_test_plan.md           # Comprehensive test planning document
├── claude_auth_integration_tests.rs   # Integration tests (Phase 5 priorities)
├── claude_auth_security_tests.rs      # Security validation tests
├── claude_auth_performance_tests.rs   # Performance benchmarks
├── common/
│   └── claude_test_utils.rs           # Shared utilities and mock infrastructure
└── README.md                          # This file
```

## Test Categories

### 1. Integration Tests (Primary Phase 5 Focus)

**File**: `claude_auth_integration_tests.rs`

Key tests implementing Phase 5 requirements:
- `test_claude_openai_fallback()` - Seamless fallback when Claude unavailable
- `test_multi_agent_quota_management()` - Quota allocation across multiple agents
- `test_provider_switching()` - Provider transitions without data loss
- `test_claude_max_subscription_detection()` - Subscription tier detection
- `test_no_openai_regression()` - Backward compatibility validation

### 2. Security Tests

**File**: `claude_auth_security_tests.rs`

Security validation covering:
- Token storage encryption and file permissions (0o600)
- OAuth PKCE challenge/verifier validation
- Session management security and isolation
- API key protection and masking
- Credential isolation between providers
- Authentication audit logging

### 3. Performance Tests

**File**: `claude_auth_performance_tests.rs`

Performance benchmarks ensuring:
- Sub-100ms cached authentication
- Efficient token refresh operations
- Multi-agent coordination performance
- Memory usage validation
- Provider selection speed optimization
- High concurrency load testing

### 4. Test Infrastructure

**File**: `common/claude_test_utils.rs`

Mock infrastructure and utilities:
- `MockClaudeServer` - Complete Claude API simulation
- `ClaudeTestUtils` - Authentication object creation helpers
- `TestEnvironment` - Integrated test environment setup
- `ClaudeTestAssertions` - Specialized assertion helpers
- `TestFixtures` - Test data and response fixtures

## Running Tests

### Prerequisites

Ensure required dependencies are available:
```toml
[dev-dependencies]
wiremock = "0.6"           # Mock HTTP servers
tempfile = "3"             # Temporary file handling
tokio-test = "0.4"         # Async test utilities
pretty_assertions = "1.4"  # Enhanced assertions
```

### Execute Test Suites

#### Run All Claude Authentication Tests
```bash
cargo test claude_auth --package codex-core
```

#### Run Specific Test Categories
```bash
# Integration tests only
cargo test test_claude_openai_fallback --package codex-core
cargo test test_multi_agent_quota_management --package codex-core
cargo test test_provider_switching --package codex-core

# Security tests only
cargo test test_token_storage_encryption --package codex-core
cargo test test_oauth_pkce_validation --package codex-core

# Performance tests only
cargo test test_authentication_caching --package codex-core
cargo test test_token_refresh_optimization --package codex-core
```

#### Run with Detailed Output
```bash
cargo test claude_auth --package codex-core -- --nocapture
```

#### Run Performance Benchmarks
```bash
cargo test test_authentication_under_load --package codex-core --release
cargo test test_multi_agent_coordination_performance --package codex-core --release
```

## Validation Criteria

### Technical Validation Checkpoints (From Integration Plan)

- ✅ **No regression in OpenAI authentication** - `test_no_openai_regression()`
- ✅ **Agent environment properly configured** - `test_provider_switching()`
- ✅ **Provider switching seamless** - `test_provider_switching()`
- ✅ **Claude Max subscription detection working** - `test_claude_max_subscription_detection()`
- ✅ **Quota management prevents overruns** - `test_multi_agent_quota_management()`
- ✅ **Fallback mechanisms reliable** - `test_claude_openai_fallback()`

### Performance Requirements

- **Authentication Caching**: < 100ms for cached tokens
- **Token Refresh**: < 5s for refresh operations  
- **Provider Selection**: < 50ms for optimal provider selection
- **Quota Allocation**: < 100ms for quota assignment
- **Memory Usage**: < 100MB total increase during operations

### Security Requirements

- **File Permissions**: 0o600 for token storage files
- **Token Encryption**: No plaintext credentials in storage
- **OAuth Security**: PKCE validation and state parameter verification
- **Session Isolation**: Independent user sessions
- **Credential Separation**: Provider-specific credential isolation

### Coverage Targets

- **Unit Tests**: >90% coverage for new Claude authentication code
- **Integration Tests**: All major user workflows covered
- **Security Tests**: All authentication paths validated
- **Performance Tests**: All critical operations benchmarked

## Test Environment Setup

### Mock Claude API Server

The test suite includes a comprehensive mock Claude API server supporting:
- Multiple subscription tiers (Max, Pro, Free)
- OAuth authentication flows
- Rate limiting and quota exceeded scenarios
- Error conditions and edge cases

Example usage:
```rust
let claude_server = MockClaudeServer::with_max_subscription().await;
let claude_auth = ClaudeTestUtils::create_max_subscription_auth();
// Test with realistic Claude API responses
```

### Test Environment Helper

Integrated test environment setup:
```rust
let mut env = TestEnvironment::new().await;
env.with_claude_max().await;
env.with_openai().await;
// Both providers configured and ready for testing
```

## Debugging Tests

### Common Issues

1. **Test Timeouts**
   - Increase timeout for slow operations: `timeout(Duration::from_secs(30), operation)`
   - Check mock server responses are properly configured

2. **Authentication Failures**
   - Verify mock server endpoints match expected URLs
   - Check token expiration times in test data
   - Ensure proper environment variable setup

3. **Performance Test Failures**
   - Run performance tests in release mode for accurate timing
   - Consider system load when validating timing requirements
   - Check for resource contention in concurrent tests

### Debug Output

Enable detailed logging for test debugging:
```bash
RUST_LOG=debug cargo test claude_auth --package codex-core -- --nocapture
```

### Test Data Inspection

Examine test artifacts:
```rust
// Print auth data for debugging
println!("Auth data: {:?}", auth_manager.get_current_auth().await);

// Inspect environment variables
println!("Environment: {:?}", auth_manager.get_agent_environment("test_agent").await);
```

## Integration with CI/CD

### GitHub Actions Configuration

Example CI configuration:
```yaml
- name: Run Claude Authentication Tests
  run: |
    cargo test claude_auth --package codex-core
    cargo test claude_auth --package codex-core --release  # Performance tests
```

### Pre-commit Validation

Recommended pre-commit hook:
```bash
#!/bin/bash
cargo test claude_auth --package codex-core --quiet
if [ $? -ne 0 ]; then
    echo "Claude authentication tests failed"
    exit 1
fi
```

## Contributing

### Adding New Tests

1. **Follow existing patterns**: Use the established test structure and naming conventions
2. **Use test utilities**: Leverage `claude_test_utils.rs` for consistent setup
3. **Include assertions**: Use `ClaudeTestAssertions` for standardized validation
4. **Document test purpose**: Clear comments explaining what is being tested
5. **Performance considerations**: Mark performance-sensitive tests appropriately

### Test Naming Conventions

- `test_{functionality}_{scenario}()` - e.g., `test_quota_management_concurrent_agents()`
- Group related tests in logical modules
- Use descriptive assertion messages

### Mock Server Extensions

When adding new Claude API endpoints:
1. Add to appropriate `MockClaudeServer` method
2. Include both success and error scenarios
3. Use realistic response data matching actual Claude API
4. Document any new mock behaviors

## Reference

- **Integration Plan**: `/docs/claude-auth-integration-plan.md`
- **Existing Auth Tests**: `/codex-rs/mcp-server/tests/suite/auth.rs`
- **Core Auth Module**: `/codex-rs/core/src/auth.rs`
- **Agent Environment**: `/codex-rs/core/src/agent_tool.rs` (lines 649-657)

This test suite ensures the Claude authentication integration meets all Phase 5 requirements while maintaining the reliability and security of the existing authentication system.