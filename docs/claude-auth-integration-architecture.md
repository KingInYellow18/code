# Claude Authentication Integration Architecture

## Executive Summary

This document defines the technical architecture for integrating Claude authentication into the existing Code project. Following **Approach 2 (Parallel Authentication System)** from the integration plan, this architecture provides a unified authentication manager that supports both OpenAI and Claude providers with intelligent selection logic.

## Architecture Overview

### Core Components

```rust
// Central coordination hub
pub struct UnifiedAuthManager {
    openai_provider: Option<OpenAIAuthProvider>,
    claude_provider: Option<ClaudeAuthProvider>,
    config: AuthConfig,
    selection_strategy: Arc<dyn ProviderSelector>,
    quota_manager: QuotaManager,
}

// Provider abstraction
pub trait AuthProvider: Send + Sync {
    async fn authenticate(&self) -> Result<AuthToken>;
    async fn refresh_token(&self, token: &AuthToken) -> Result<AuthToken>;
    async fn validate_subscription(&self) -> Result<SubscriptionInfo>;
    fn provider_type(&self) -> ProviderType;
}

// Intelligent selection
pub trait ProviderSelector: Send + Sync {
    async fn select_optimal_provider(
        &self,
        context: &TaskContext,
        available_providers: &[Box<dyn AuthProvider>]
    ) -> Result<Box<dyn AuthProvider>>;
}
```

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    Code Application Layer                       │
├─────────────────────────────────────────────────────────────────┤
│  CLI Commands        │  TUI Interface      │  Agent System       │
│  ├─ auth login       │  ├─ provider select │  ├─ env setup       │
│  ├─ auth switch      │  ├─ status display  │  ├─ quota tracking  │
│  ├─ auth status      │  └─ oauth flows     │  └─ session mgmt    │
│  └─ auth quota       │                     │                     │
├─────────────────────────────────────────────────────────────────┤
│                  UnifiedAuthManager                             │
│  ┌─────────────────┬─────────────────┬─────────────────────────┐ │
│  │ Provider Sel.   │ Subscription    │ Session Coordination    │ │
│  │ ├─ Claude Max   │ ├─ Status Check │ ├─ Multi-agent          │ │
│  │ ├─ Cost Optim.  │ ├─ Quota Track  │ ├─ Rate Limiting        │ │
│  │ └─ User Pref.   │ └─ Renewal      │ └─ Fallback Logic       │ │
│  └─────────────────┴─────────────────┴─────────────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│  OpenAI Provider         │  Claude Provider                     │
│  ├─ ChatGPT OAuth        │  ├─ Claude Max OAuth                 │
│  ├─ API Key Auth         │  ├─ API Key Auth                     │
│  ├─ Token Refresh        │  ├─ Subscription Validation          │
│  └─ Rate Limiting        │  └─ Quota Management                 │
├─────────────────────────────────────────────────────────────────┤
│                    Storage & Configuration                      │
│  ├─ ~/.codex/auth.json (Extended format)                       │
│  ├─ ~/.codex/claude_auth.json (Claude-specific)                │
│  └─ Provider preferences and selection history                 │
└─────────────────────────────────────────────────────────────────┘
```

## Component Design

### 1. UnifiedAuthManager

The central authentication coordinator that manages multiple providers and handles intelligent selection.

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug)]
pub struct UnifiedAuthManager {
    providers: HashMap<ProviderType, Box<dyn AuthProvider>>,
    active_provider: Option<ProviderType>,
    config: AuthConfig,
    selector: Arc<dyn ProviderSelector>,
    quota_manager: Arc<QuotaManager>,
    session_manager: Arc<SessionManager>,
}

impl UnifiedAuthManager {
    pub async fn new(config: AuthConfig) -> Result<Self> {
        let mut providers = HashMap::new();
        
        // Initialize OpenAI provider (existing)
        if let Ok(openai_provider) = OpenAIAuthProvider::new(&config).await {
            providers.insert(ProviderType::OpenAI, Box::new(openai_provider));
        }
        
        // Initialize Claude provider (new)
        if let Ok(claude_provider) = ClaudeAuthProvider::new(&config).await {
            providers.insert(ProviderType::Claude, Box::new(claude_provider));
        }
        
        Ok(Self {
            providers,
            active_provider: None,
            selector: Arc::new(IntelligentSelector::new()),
            quota_manager: Arc::new(QuotaManager::new()),
            session_manager: Arc::new(SessionManager::new()),
        })
    }
    
    pub async fn authenticate_for_task(&self, task_context: &TaskContext) -> Result<AuthToken> {
        let provider = self.selector.select_optimal_provider(
            task_context,
            &self.providers.values().collect::<Vec<_>>()
        ).await?;
        
        // Check quotas before proceeding
        self.quota_manager.reserve_quota(provider.provider_type(), &task_context).await?;
        
        let token = provider.authenticate().await?;
        
        // Update active provider
        self.active_provider = Some(provider.provider_type());
        
        Ok(token)
    }
    
    pub async fn get_available_providers(&self) -> Vec<ProviderType> {
        let mut available = Vec::new();
        
        for (provider_type, provider) in &self.providers {
            if let Ok(_) = provider.validate_subscription().await {
                available.push(*provider_type);
            }
        }
        
        available
    }
}
```

### 2. Claude Authentication Provider

Implementation of Claude-specific authentication with Max subscription support.

```rust
use oauth2::{
    AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};

#[derive(Debug)]
pub struct ClaudeAuthProvider {
    config: ClaudeAuthConfig,
    oauth_client: Option<ClaudeOAuthClient>,
    api_key: Option<String>,
    current_tokens: Arc<RwLock<Option<ClaudeTokenData>>>,
    subscription_cache: Arc<RwLock<Option<SubscriptionInfo>>>,
    client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct ClaudeTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub subscription_tier: String,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionInfo {
    pub tier: String,                    // "max", "pro", "free"
    pub daily_limit: u64,
    pub current_usage: u64,
    pub reset_time: DateTime<Utc>,
    pub features: Vec<String>,
}

impl AuthProvider for ClaudeAuthProvider {
    async fn authenticate(&self) -> Result<AuthToken> {
        match &self.config.auth_mode {
            ClaudeAuthMode::MaxSubscription => {
                self.authenticate_oauth().await
            },
            ClaudeAuthMode::ApiKey => {
                self.authenticate_api_key().await
            },
        }
    }
    
    async fn refresh_token(&self, token: &AuthToken) -> Result<AuthToken> {
        if let Some(oauth_client) = &self.oauth_client {
            oauth_client.refresh_token(&token.refresh_token).await
        } else {
            Err(AuthError::RefreshNotSupported)
        }
    }
    
    async fn validate_subscription(&self) -> Result<SubscriptionInfo> {
        // Check cache first
        {
            let cache = self.subscription_cache.read().await;
            if let Some(info) = cache.as_ref() {
                if info.reset_time > Utc::now() {
                    return Ok(info.clone());
                }
            }
        }
        
        // Fetch fresh subscription info
        let subscription_info = self.fetch_subscription_info().await?;
        
        // Update cache
        {
            let mut cache = self.subscription_cache.write().await;
            *cache = Some(subscription_info.clone());
        }
        
        Ok(subscription_info)
    }
    
    fn provider_type(&self) -> ProviderType {
        ProviderType::Claude
    }
}

impl ClaudeAuthProvider {
    async fn authenticate_oauth(&self) -> Result<AuthToken> {
        let oauth_client = self.oauth_client.as_ref()
            .ok_or(AuthError::OAuthNotConfigured)?;
        
        // Check if we have valid tokens
        let current_tokens = self.current_tokens.read().await;
        if let Some(tokens) = current_tokens.as_ref() {
            if tokens.expires_at > Utc::now() + chrono::Duration::minutes(5) {
                return Ok(AuthToken {
                    access_token: tokens.access_token.clone(),
                    refresh_token: tokens.refresh_token.clone(),
                    expires_at: tokens.expires_at,
                    provider: ProviderType::Claude,
                });
            }
        }
        drop(current_tokens);
        
        // Need to refresh or re-authenticate
        oauth_client.get_valid_token().await
    }
    
    async fn authenticate_api_key(&self) -> Result<AuthToken> {
        let api_key = self.api_key.as_ref()
            .ok_or(AuthError::ApiKeyNotConfigured)?;
        
        // Validate API key by making a test request
        let response = self.client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;
        
        if response.status().is_success() {
            Ok(AuthToken {
                access_token: api_key.clone(),
                refresh_token: None,
                expires_at: Utc::now() + chrono::Duration::days(365), // API keys don't expire
                provider: ProviderType::Claude,
            })
        } else {
            Err(AuthError::InvalidApiKey)
        }
    }
    
    async fn fetch_subscription_info(&self) -> Result<SubscriptionInfo> {
        let token = self.authenticate().await?;
        
        let response = self.client
            .get("https://api.anthropic.com/v1/subscription")
            .bearer_auth(&token.access_token)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;
        
        if response.status().is_success() {
            let subscription: SubscriptionInfo = response.json().await?;
            Ok(subscription)
        } else {
            Err(AuthError::SubscriptionCheckFailed(response.status()))
        }
    }
}
```

### 3. Intelligent Provider Selection

Algorithm for choosing the optimal provider based on context and user preferences.

```rust
#[derive(Debug)]
pub struct IntelligentSelector {
    selection_cache: Arc<RwLock<HashMap<String, (ProviderType, DateTime<Utc>)>>>,
}

#[derive(Debug, Clone)]
pub enum SelectionStrategy {
    PreferClaude,
    PreferOpenAI,
    CostOptimized,
    PerformanceOptimized,
    UserPreference(ProviderType),
}

impl ProviderSelector for IntelligentSelector {
    async fn select_optimal_provider(
        &self,
        context: &TaskContext,
        available_providers: &[Box<dyn AuthProvider>],
    ) -> Result<Box<dyn AuthProvider>> {
        
        // 1. Check user explicit preference
        if let Some(preferred) = context.preferred_provider {
            if let Some(provider) = available_providers.iter()
                .find(|p| p.provider_type() == preferred) {
                return Ok(provider.clone());
            }
        }
        
        // 2. Check Claude Max subscription status
        if let Some(claude_provider) = available_providers.iter()
            .find(|p| p.provider_type() == ProviderType::Claude) {
            
            match claude_provider.validate_subscription().await {
                Ok(subscription) if subscription.tier == "max" => {
                    // Check if we have sufficient quota
                    if subscription.current_usage < subscription.daily_limit * 80 / 100 {
                        return Ok(claude_provider.clone());
                    }
                }
                _ => {} // Fall through to other options
            }
        }
        
        // 3. Cost optimization logic
        match context.task_type {
            TaskType::CodeGeneration | TaskType::Analysis => {
                // For heavy tasks, prefer Claude Max if available
                if let Some(claude_provider) = available_providers.iter()
                    .find(|p| p.provider_type() == ProviderType::Claude) {
                    if let Ok(subscription) = claude_provider.validate_subscription().await {
                        if subscription.tier == "max" || subscription.tier == "pro" {
                            return Ok(claude_provider.clone());
                        }
                    }
                }
            }
            TaskType::QuickQuery | TaskType::Validation => {
                // For light tasks, API key might be more cost-effective
                // Check recent usage patterns to decide
            }
        }
        
        // 4. Fallback to OpenAI
        if let Some(openai_provider) = available_providers.iter()
            .find(|p| p.provider_type() == ProviderType::OpenAI) {
            return Ok(openai_provider.clone());
        }
        
        // 5. Use any available provider
        available_providers.first()
            .map(|p| p.clone())
            .ok_or(AuthError::NoProvidersAvailable)
    }
}
```

### 4. Extended Storage Format

Backward-compatible extensions to the existing auth.json format.

```rust
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ExtendedAuthJson {
    // Existing OpenAI fields (maintained for compatibility)
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
    
    // New Claude integration fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_auth: Option<ClaudeAuthData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_preferences: Option<ProviderPreferences>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_provider_selection: Option<ProviderSelectionHistory>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ClaudeAuthData {
    pub auth_mode: ClaudeAuthMode,
    pub api_key: Option<String>,
    pub oauth_tokens: Option<ClaudeTokenData>,
    pub subscription_info: Option<SubscriptionInfo>,
    pub last_subscription_check: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ProviderPreferences {
    pub preferred_provider: Option<ProviderType>,
    pub fallback_enabled: bool,
    pub selection_strategy: SelectionStrategy,
    pub cost_optimization: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ProviderSelectionHistory {
    pub last_used_provider: ProviderType,
    pub selection_reason: String,
    pub timestamp: DateTime<Utc>,
    pub task_context: String,
}

// Migration logic for existing auth.json files
impl ExtendedAuthJson {
    pub fn from_legacy_auth_json(legacy: AuthDotJson) -> Self {
        Self {
            openai_api_key: legacy.openai_api_key,
            tokens: legacy.tokens,
            last_refresh: legacy.last_refresh,
            claude_auth: None,
            provider_preferences: Some(ProviderPreferences {
                preferred_provider: None, // Will be determined by user choice
                fallback_enabled: true,
                selection_strategy: SelectionStrategy::CostOptimized,
                cost_optimization: true,
            }),
            last_provider_selection: None,
        }
    }
    
    pub fn migrate_from_file(auth_file: &Path) -> Result<Self> {
        match try_read_auth_json(auth_file) {
            Ok(legacy_auth) => {
                let extended = Self::from_legacy_auth_json(legacy_auth);
                // Write back the extended format
                extended.write_to_file(auth_file)?;
                Ok(extended)
            }
            Err(_) => {
                // File doesn't exist or is corrupted, create new
                let extended = Self::new_empty();
                extended.write_to_file(auth_file)?;
                Ok(extended)
            }
        }
    }
}
```

### 5. Agent Environment Integration

Enhanced agent environment setup with multi-provider support.

```rust
use std::collections::HashMap;

#[derive(Debug)]
pub struct EnhancedAgentEnvironment {
    auth_manager: Arc<UnifiedAuthManager>,
    quota_manager: Arc<QuotaManager>,
    session_tracker: Arc<SessionTracker>,
}

impl EnhancedAgentEnvironment {
    pub async fn setup_agent_environment(
        &self, 
        agent_id: &str, 
        task_context: &TaskContext
    ) -> Result<HashMap<String, String>> {
        let mut env = HashMap::new();
        
        // Get optimal provider for this agent/task
        let auth_token = self.auth_manager
            .authenticate_for_task(task_context).await?;
        
        match auth_token.provider {
            ProviderType::Claude => {
                self.setup_claude_environment(&mut env, &auth_token, agent_id).await?;
            }
            ProviderType::OpenAI => {
                self.setup_openai_environment(&mut env, &auth_token, agent_id).await?;
            }
        }
        
        // Common environment variables
        env.insert("CLAUDE_CODE_USER_AGENT".to_string(), "Claude Code/1.0".to_string());
        env.insert("DISABLE_AUTO_UPDATE".to_string(), "1".to_string());
        env.insert("ACTIVE_AUTH_PROVIDER".to_string(), auth_token.provider.to_string());
        env.insert("AGENT_ID".to_string(), agent_id.to_string());
        
        // Session tracking
        self.session_tracker.register_agent_session(agent_id, auth_token.provider).await?;
        
        Ok(env)
    }
    
    async fn setup_claude_environment(
        &self,
        env: &mut HashMap<String, String>,
        token: &AuthToken,
        agent_id: &str,
    ) -> Result<()> {
        env.insert("ANTHROPIC_API_KEY".to_string(), token.access_token.clone());
        env.insert("CLAUDE_API_KEY".to_string(), token.access_token.clone());
        
        // Get subscription info for quota management
        if let Some(claude_provider) = self.auth_manager.get_claude_provider().await {
            let subscription = claude_provider.validate_subscription().await?;
            env.insert("CLAUDE_SUBSCRIPTION_TIER".to_string(), subscription.tier);
            env.insert("CLAUDE_DAILY_LIMIT".to_string(), subscription.daily_limit.to_string());
            env.insert("CLAUDE_CURRENT_USAGE".to_string(), subscription.current_usage.to_string());
        }
        
        // Allocate quota for this agent
        let quota = self.quota_manager.allocate_quota(
            ProviderType::Claude, 
            agent_id, 
            EstimatedUsage::from_task_context(&task_context)
        ).await?;
        
        env.insert("CLAUDE_AGENT_QUOTA".to_string(), quota.allocated_tokens.to_string());
        
        Ok(())
    }
    
    async fn setup_openai_environment(
        &self,
        env: &mut HashMap<String, String>,
        token: &AuthToken,
        agent_id: &str,
    ) -> Result<()> {
        // Existing OpenAI environment setup logic
        env.insert("OPENAI_API_KEY".to_string(), token.access_token.clone());
        
        // Enhanced with quota tracking
        let quota = self.quota_manager.allocate_quota(
            ProviderType::OpenAI, 
            agent_id, 
            EstimatedUsage::from_task_context(&task_context)
        ).await?;
        
        env.insert("OPENAI_AGENT_QUOTA".to_string(), quota.allocated_tokens.to_string());
        
        Ok(())
    }
}
```

### 6. TUI Provider Selection Integration

Enhanced authentication flow in the TUI with provider selection.

```rust
#[derive(Debug)]
pub enum EnhancedSignInState {
    SelectProvider,
    OpenAIAuth(OpenAIAuthState),
    ClaudeAuth(ClaudeAuthState),
    ProviderSuccess(ProviderType),
    ConfigurePreferences,
}

#[derive(Debug)]
pub struct ClaudeAuthState {
    pub auth_mode: ClaudeAuthMode,
    pub subscription_status: Option<SubscriptionInfo>,
    pub oauth_flow: Option<ClaudeOAuthFlow>,
    pub verification_url: Option<String>,
}

impl AuthModeWidget {
    fn render_provider_selection(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![
            Line::from(vec![
                Span::raw("> "),
                Span::styled("Choose your AI provider", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ];
        
        // OpenAI option
        let openai_available = self.check_openai_availability();
        let openai_style = if openai_available {
            Style::default().fg(crate::colors::text())
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        
        lines.push(Line::from(vec![
            Span::raw("  1. "),
            Span::styled("OpenAI (ChatGPT)", openai_style),
        ]));
        lines.push(Line::from("     ├─ ChatGPT Plus, Pro, Team plans"));
        lines.push(Line::from("     └─ API Key usage-based billing"));
        lines.push(Line::from(""));
        
        // Claude option
        let claude_available = self.check_claude_availability();
        let claude_style = if claude_available {
            Style::default().fg(crate::colors::text())
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        
        lines.push(Line::from(vec![
            Span::raw("  2. "),
            Span::styled("Claude (Anthropic)", claude_style),
        ]));
        lines.push(Line::from("     ├─ Claude Max unlimited usage"));
        lines.push(Line::from("     └─ API Key usage-based billing"));
        lines.push(Line::from(""));
        
        // Smart selection option
        lines.push(Line::from(vec![
            Span::raw("  3. "),
            Span::styled("Smart Selection", Style::default().fg(crate::colors::info())),
        ]));
        lines.push(Line::from("     └─ Automatically choose optimal provider"));
        lines.push(Line::from(""));
        
        // Usage recommendations
        lines.push(Line::from(vec![
            Span::raw("💡 "),
            Span::styled("Recommendations:", Style::default().add_modifier(Modifier::BOLD)),
        ]));
        
        if let Some(recommendation) = self.get_provider_recommendation() {
            lines.push(Line::from(format!("   {}", recommendation)));
        }
        
        lines.push(Line::from(""));
        lines.push(Line::from("  Press 1, 2, 3, or Enter to continue")
            .style(Style::default().fg(crate::colors::text_dim())));
        
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
    
    fn start_claude_auth(&mut self) {
        match self.detect_claude_subscription() {
            Ok(Some(subscription_info)) if subscription_info.tier == "max" => {
                // User has Claude Max - start OAuth flow
                self.start_claude_oauth_flow();
            }
            _ => {
                // Prompt for API key
                self.start_claude_api_key_flow();
            }
        }
    }
    
    fn start_claude_oauth_flow(&mut self) {
        let oauth_flow = ClaudeOAuthFlow::new();
        
        match oauth_flow.generate_auth_url() {
            Ok(auth_url) => {
                // Open browser for OAuth flow
                if let Err(_) = open::that(&auth_url) {
                    // Browser opening failed, show URL to user
                    self.sign_in_state = EnhancedSignInState::ClaudeAuth(ClaudeAuthState {
                        auth_mode: ClaudeAuthMode::MaxSubscription,
                        subscription_status: None,
                        oauth_flow: Some(oauth_flow),
                        verification_url: Some(auth_url),
                    });
                } else {
                    self.sign_in_state = EnhancedSignInState::ClaudeAuth(ClaudeAuthState {
                        auth_mode: ClaudeAuthMode::MaxSubscription,
                        subscription_status: None,
                        oauth_flow: Some(oauth_flow),
                        verification_url: None,
                    });
                }
                
                self.start_oauth_completion_polling();
            }
            Err(e) => {
                self.error = Some(format!("Failed to start Claude authentication: {}", e));
                self.sign_in_state = EnhancedSignInState::SelectProvider;
            }
        }
        
        self.event_tx.send(AppEvent::RequestRedraw);
    }
}
```

## Integration Points & Responsibilities

### File Modifications Required

1. **`codex-rs/core/src/auth.rs`** - Extend existing AuthManager
2. **`codex-rs/core/src/claude_auth.rs`** - New Claude authentication module
3. **`codex-rs/tui/src/onboarding/auth.rs`** - TUI provider selection
4. **`codex-rs/core/src/agent_tool.rs`** - Enhanced agent environment
5. **`codex-rs/core/Cargo.toml`** - Add OAuth2 and crypto dependencies

### New Dependencies

```toml
[dependencies]
oauth2 = "4.4"                    # OAuth 2.0 client
pkce = "0.2"                     # PKCE implementation
ring = "0.17"                    # Cryptography for secure storage
```

## Migration Strategy

### Phase 1: Core Infrastructure
- Implement UnifiedAuthManager
- Add Claude authentication module
- Extend storage format

### Phase 2: Integration
- Update TUI for provider selection
- Enhance agent environment setup
- Implement quota management

### Phase 3: Polish & Testing
- Add comprehensive error handling
- Implement migration logic
- User experience optimization

## Security Considerations

### Token Storage Security
```rust
pub struct SecureTokenStorage {
    encryption_key: [u8; 32],
    storage_path: PathBuf,
}

impl SecureTokenStorage {
    pub fn store_encrypted_tokens(&self, tokens: &TokenData) -> Result<()> {
        let plaintext = serde_json::to_vec(tokens)?;
        let encrypted = self.encrypt(&plaintext)?;
        
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)  // Unix: read/write for owner only
            .open(&self.storage_path)?;
        
        file.write_all(&encrypted)?;
        Ok(())
    }
}
```

### OAuth Security
- PKCE implementation for authorization code flow
- State parameter validation
- Secure redirect URL handling
- Token rotation on refresh

## Performance Optimizations

### Caching Strategy
- Subscription status caching (15-minute TTL)
- Provider availability caching
- Token validation caching

### Batch Operations
- Parallel provider health checks
- Bulk quota allocations
- Concurrent subscription validations

## Error Handling & Fallback

### Graceful Degradation
1. Claude Max → Claude API Key
2. Claude API Key → OpenAI
3. Provider-specific errors with user guidance
4. Automatic retry with exponential backoff

### User Experience
- Clear error messages
- Recovery suggestions
- Provider status indicators
- Quota usage visualizations

This architecture provides a robust, secure, and user-friendly integration of Claude authentication while maintaining full backward compatibility with existing OpenAI authentication.