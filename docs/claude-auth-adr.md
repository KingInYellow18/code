# ADR-001: Claude Authentication Integration

**Status:** Approved  
**Date:** 2025-09-13  
**Decision Makers:** Integration Specialist, System Architecture Team  
**Stakeholders:** Code Project Users, Claude Max Subscribers, Development Team  

## Context

The Code project currently supports OpenAI authentication via ChatGPT OAuth and API keys. With the growing adoption of Claude Max subscriptions and the potential for cost optimization, there is a need to integrate Claude authentication while maintaining existing OpenAI functionality.

## Decision

We will implement **Approach 2: Parallel Authentication System** to integrate Claude authentication alongside the existing OpenAI authentication, with intelligent provider selection based on subscription status and user preferences.

## Rationale

### Options Considered

1. **Direct Port Implementation** - Complete replacement with multi-provider system
2. **Parallel Authentication System** - Add Claude alongside OpenAI ✅ **SELECTED**
3. **Microservice Authentication** - Separate authentication service
4. **Hybrid Solution** - Combination of multiple approaches

### Decision Factors

| Factor | Weight | Approach 2 Score | Rationale |
|--------|--------|------------------|-----------|
| Technical Feasibility | 25% | 9/10 | Leverages existing infrastructure |
| Implementation Risk | 20% | 8/10 | Non-breaking changes, gradual rollout |
| Time to Market | 20% | 9/10 | Fastest implementation path |
| User Experience | 10% | 9/10 | Seamless provider switching |
| Maintainability | 15% | 7/10 | Manageable complexity increase |
| Future Flexibility | 10% | 6/10 | Extensible for additional providers |

**Overall Score: 8.2/10**

## Architecture Overview

### Core Components

```
UnifiedAuthManager
├── OpenAI Provider (existing)
│   ├── ChatGPT OAuth
│   └── API Key Auth
├── Claude Provider (new)
│   ├── Claude Max OAuth
│   └── API Key Auth
├── Intelligent Provider Selection
├── Quota Management System
└── Extended Storage Format
```

### Key Design Decisions

1. **Provider Abstraction**: Common `AuthProvider` trait for consistent interface
2. **Intelligent Selection**: Algorithm considers subscription status, quotas, and cost optimization
3. **Backward Compatibility**: Extended auth.json format maintains existing structure
4. **Zero-Downtime Migration**: Atomic file operations with rollback support
5. **Security First**: PKCE OAuth, encrypted storage, CSRF protection

## Implementation Plan

### Phase 1: Foundation (Weeks 1-2)
- Core Claude authentication module
- UnifiedAuthManager implementation
- Extended storage format

### Phase 2: Integration (Weeks 3-4)
- Provider selection algorithm
- TUI provider selection interface
- Agent environment enhancement

### Phase 3: Advanced Features (Weeks 5-6)
- Quota management system
- OAuth flow implementation
- Migration strategy

### Phase 4: Polish & Testing (Weeks 7-8)
- Comprehensive testing
- Performance optimization
- Documentation

## Technical Specifications

### File Modifications
- `codex-rs/core/src/auth.rs` - Extend AuthManager
- `codex-rs/core/src/claude_auth.rs` - New Claude module
- `codex-rs/tui/src/onboarding/auth.rs` - Provider selection UI
- `codex-rs/core/src/agent_tool.rs` - Enhanced agent environment

### New Dependencies
```toml
oauth2 = "4.4"      # OAuth 2.0 client
pkce = "0.2"        # PKCE implementation
ring = "0.17"       # Cryptography
axum = "0.7"        # Local redirect server
```

### Storage Format Extension
```json
{
  "OPENAI_API_KEY": "...",          // Existing
  "tokens": {...},                  // Existing  
  "last_refresh": "...",           // Existing
  "claude_auth": {                 // New
    "auth_mode": "MaxSubscription",
    "oauth_tokens": {...},
    "subscription_info": {...}
  },
  "provider_preferences": {        // New
    "preferred_provider": "Claude",
    "fallback_enabled": true,
    "selection_strategy": "CostOptimized"
  }
}
```

## Benefits

### For Users
- **Cost Optimization**: Automatic selection of most cost-effective provider
- **Claude Max Utilization**: Full utilization of unlimited Claude Max subscriptions
- **Seamless Experience**: Transparent provider switching
- **Enhanced Reliability**: Fallback between providers

### For Development
- **Backward Compatibility**: No breaking changes to existing workflows
- **Extensible Architecture**: Easy addition of future providers
- **Comprehensive Testing**: Robust test coverage for all scenarios
- **Clear Migration Path**: Step-by-step migration with rollback support

## Risks & Mitigations

### Technical Risks
1. **OAuth Flow Complexity**
   - *Risk*: PKCE implementation challenges
   - *Mitigation*: Use proven oauth2 crate, comprehensive testing

2. **Token Management**
   - *Risk*: Token refresh failures
   - *Mitigation*: Robust retry logic, fallback mechanisms

3. **Storage Migration**
   - *Risk*: Auth.json corruption during migration
   - *Mitigation*: Atomic operations, backup strategy, rollback support

### Operational Risks
1. **User Confusion**
   - *Risk*: Too many provider options
   - *Mitigation*: Smart defaults, clear UI guidance

2. **Support Complexity**
   - *Risk*: Increased support burden
   - *Mitigation*: Comprehensive documentation, diagnostic tools

## Success Metrics

### Technical Metrics
- Zero regression in existing OpenAI authentication
- < 200ms provider selection latency
- 99.9% OAuth flow success rate
- 100% backward compatibility

### User Metrics
- Successful Claude Max authentication rate > 95%
- User-reported authentication issues < 1%
- Cost optimization adoption rate > 50%

### Business Metrics
- Increased Claude Max subscription utilization
- Reduced API costs for eligible users
- Enhanced user satisfaction scores

## Monitoring & Observability

### Key Metrics to Track
- Provider selection decisions and rationale
- Authentication success/failure rates by provider
- Quota utilization and exhaustion events
- Migration completion rate and issues
- Cost optimization effectiveness

### Alerting
- OAuth flow failures > 5% in 15 minutes
- Provider unavailability > 1 minute
- Quota exhaustion warnings
- Migration rollback events

## Rollback Plan

### Immediate Rollback (< 1 hour)
1. Restore backup auth.json files
2. Disable Claude provider in configuration
3. Force fallback to OpenAI authentication

### Full Rollback (< 4 hours)
1. Revert all code changes via Git
2. Rebuild and redeploy application
3. Run data consistency checks
4. Validate OpenAI authentication works

## Future Considerations

### Potential Enhancements
- Additional provider support (Google, Cohere, etc.)
- Advanced cost optimization algorithms
- Usage analytics and reporting
- Enterprise SSO integration

### Architecture Evolution
- Provider marketplace concept
- Plugin-based provider system
- Cloud-based configuration management
- Advanced quota sharing strategies

## Conclusion

The Parallel Authentication System approach provides the optimal balance of functionality, risk, and implementation speed. It enables Claude authentication integration while preserving the robust existing OpenAI authentication system, setting the foundation for future multi-provider capabilities.

The comprehensive architecture design, implementation plan, and risk mitigation strategies ensure successful delivery of this critical enhancement to the Code project.

---

**Approved by:** Integration Specialist  
**Review Date:** 2025-09-13  
**Next Review:** 2025-12-13 (Post-implementation review)