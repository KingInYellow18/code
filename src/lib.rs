//! Claude Code Security Implementation
//! 
//! This crate provides comprehensive security enhancements for Claude authentication
//! integration in the Code project, implementing all security measures from the 
//! Claude Authentication Integration Plan.

pub mod security;
pub mod claude_auth;
pub mod configuration;

pub use security::{
    SecureTokenStorage,
    SecureOAuthFlow,
    OAuthSecurityManager,
    SecurityAuditLogger,
    SessionSecurityManager,
    SecurityManager,
    SecurityConfig,
    SecurityError,
};

pub use claude_auth::{
    SecureClaudeAuth,
    ClaudeAuthConfig,
    ClaudeAuthError,
    ClaudeTokenData,
    ClaudeSubscriptionInfo,
    AuthenticationResult,
};

pub use configuration::{
    UnifiedConfigManager,
    UnifiedAuthManager,
    AuthConfig,
    ProviderType,
    ProviderPreference,
    FallbackStrategy,
    ConfigIntegration,
    create_unified_auth_manager,
    integration_helpers,
};

// Performance optimization modules
pub mod performance;
pub use performance::{
    PerformanceCoordinator, PerformanceMetrics, PerformanceTargets,
    integration::{OptimizedAuthManager, PerformanceStatistics, OptimizationConfig},
    benchmarks::{PerformanceBenchmarks, BenchmarkSuiteResults, run_phase5_compliance_benchmark},
};

/// Initialize the complete security subsystem
pub fn init_security_system() -> Result<SecurityManager, SecurityError> {
    security::init_security()
}

/// Initialize Claude authentication with default security
pub fn init_claude_auth_system() -> Result<SecureClaudeAuth, ClaudeAuthError> {
    let config = claude_auth::default_claude_config();
    claude_auth::init_claude_auth(config, None)
}

/// Initialize complete configuration management system
pub async fn init_configuration_system(codex_home: std::path::PathBuf) -> Result<UnifiedConfigManager, configuration::ConfigError> {
    UnifiedConfigManager::new(codex_home)
}

/// Initialize unified authentication manager with configuration
pub async fn init_unified_auth_system(
    codex_home: std::path::PathBuf,
    originator: String,
) -> Result<std::sync::Arc<UnifiedAuthManager>, configuration::UnifiedAuthError> {
    create_unified_auth_manager(codex_home, originator).await
}