use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration};
use thiserror::Error;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use sha2::{Sha256, Digest};

/// Enhanced OAuth security with PKCE and state validation
#[derive(Debug)]
pub struct SecureOAuthFlow {
    pkce_verifier: PkceCodeVerifier,
    state_parameter: String,
    nonce: String,
    redirect_uri: String,
    client_id: String,
    session_id: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum OAuthSecurityError {
    #[error("Invalid state parameter: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },
    #[error("PKCE verification failed")]
    PkceVerificationFailed,
    #[error("Invalid nonce")]
    InvalidNonce,
    #[error("OAuth session expired")]
    SessionExpired,
    #[error("Invalid redirect URI")]
    InvalidRedirectUri,
    #[error("Cryptographic error: {0}")]
    CryptographicError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceCodeVerifier {
    pub verifier: String,
    pub challenge: String,
    pub challenge_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSecurityState {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub created_at: DateTime<Utc>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub authorization_url: String,
    pub state: String,
    pub pkce_challenge: String,
    pub nonce: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExchangeRequest {
    pub code: String,
    pub state: String,
    pub code_verifier: String,
    pub redirect_uri: String,
}

impl SecureOAuthFlow {
    /// Create a new secure OAuth flow with PKCE and state validation
    pub fn new(client_id: String, redirect_uri: String) -> Result<Self, OAuthSecurityError> {
        let pkce_verifier = Self::generate_pkce_codes()?;
        let state_parameter = Self::generate_secure_random_string(32);
        let nonce = Self::generate_secure_random_string(32);
        let session_id = Self::generate_session_id();
        let created_at = Utc::now();
        let expires_at = created_at + Duration::minutes(10); // 10-minute session timeout

        Ok(Self {
            pkce_verifier,
            state_parameter,
            nonce,
            redirect_uri,
            client_id,
            session_id,
            created_at,
            expires_at,
        })
    }

    /// Generate authorization URL with security parameters
    pub fn generate_authorization_url(&self, authorization_endpoint: &str, scopes: &[&str]) -> Result<AuthorizationRequest, OAuthSecurityError> {
        self.check_session_validity()?;

        let mut params = HashMap::new();
        params.insert("response_type", "code".to_string());
        params.insert("client_id", self.client_id.clone());
        params.insert("redirect_uri", self.redirect_uri.clone());
        params.insert("scope", scopes.join(" "));
        params.insert("state", self.state_parameter.clone());
        params.insert("nonce", self.nonce.clone());
        params.insert("code_challenge", self.pkce_verifier.challenge.clone());
        params.insert("code_challenge_method", self.pkce_verifier.challenge_method.clone());
        
        // Additional security parameters
        params.insert("response_mode", "query".to_string());
        params.insert("prompt", "consent".to_string()); // Force consent screen

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let authorization_url = format!("{}?{}", authorization_endpoint, query_string);

        Ok(AuthorizationRequest {
            authorization_url,
            state: self.state_parameter.clone(),
            pkce_challenge: self.pkce_verifier.challenge.clone(),
            nonce: self.nonce.clone(),
            session_id: self.session_id.clone(),
        })
    }

    /// Validate OAuth callback parameters
    pub fn validate_callback(&self, code: &str, state: &str, error: Option<&str>) -> Result<TokenExchangeRequest, OAuthSecurityError> {
        self.check_session_validity()?;

        // Check for OAuth errors
        if let Some(error_msg) = error {
            return Err(OAuthSecurityError::CryptographicError(format!("OAuth error: {}", error_msg)));
        }

        // Validate state parameter
        if state != self.state_parameter {
            return Err(OAuthSecurityError::InvalidState {
                expected: self.state_parameter.clone(),
                actual: state.to_string(),
            });
        }

        // Validate authorization code format
        if code.is_empty() || code.len() < 10 {
            return Err(OAuthSecurityError::CryptographicError("Invalid authorization code format".to_string()));
        }

        Ok(TokenExchangeRequest {
            code: code.to_string(),
            state: state.to_string(),
            code_verifier: self.pkce_verifier.verifier.clone(),
            redirect_uri: self.redirect_uri.clone(),
        })
    }

    /// Verify PKCE challenge against verifier
    pub fn verify_pkce(&self, code_verifier: &str) -> Result<(), OAuthSecurityError> {
        if code_verifier != self.pkce_verifier.verifier {
            return Err(OAuthSecurityError::PkceVerificationFailed);
        }

        // Verify that the challenge was correctly generated from the verifier
        let expected_challenge = Self::generate_pkce_challenge(&self.pkce_verifier.verifier)?;
        if expected_challenge != self.pkce_verifier.challenge {
            return Err(OAuthSecurityError::PkceVerificationFailed);
        }

        Ok(())
    }

    /// Validate ID token nonce
    pub fn validate_id_token_nonce(&self, id_token_nonce: &str) -> Result<(), OAuthSecurityError> {
        if id_token_nonce != self.nonce {
            return Err(OAuthSecurityError::InvalidNonce);
        }
        Ok(())
    }

    /// Get security state for persistence
    pub fn get_security_state(&self) -> OAuthSecurityState {
        OAuthSecurityState {
            state: self.state_parameter.clone(),
            nonce: self.nonce.clone(),
            pkce_verifier: self.pkce_verifier.verifier.clone(),
            created_at: self.created_at,
            session_id: self.session_id.clone(),
        }
    }

    /// Restore from security state
    pub fn from_security_state(
        state: OAuthSecurityState,
        client_id: String,
        redirect_uri: String,
    ) -> Result<Self, OAuthSecurityError> {
        let pkce_challenge = Self::generate_pkce_challenge(&state.pkce_verifier)?;
        
        Ok(Self {
            pkce_verifier: PkceCodeVerifier {
                verifier: state.pkce_verifier,
                challenge: pkce_challenge,
                challenge_method: "S256".to_string(),
            },
            state_parameter: state.state,
            nonce: state.nonce,
            redirect_uri,
            client_id,
            session_id: state.session_id,
            created_at: state.created_at,
            expires_at: state.created_at + Duration::minutes(10),
        })
    }

    /// Check if session is still valid
    fn check_session_validity(&self) -> Result<(), OAuthSecurityError> {
        if Utc::now() > self.expires_at {
            return Err(OAuthSecurityError::SessionExpired);
        }
        Ok(())
    }

    /// Generate PKCE code verifier and challenge
    fn generate_pkce_codes() -> Result<PkceCodeVerifier, OAuthSecurityError> {
        let verifier = Self::generate_code_verifier();
        let challenge = Self::generate_pkce_challenge(&verifier)?;

        Ok(PkceCodeVerifier {
            verifier,
            challenge,
            challenge_method: "S256".to_string(),
        })
    }

    /// Generate cryptographically secure code verifier
    fn generate_code_verifier() -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Generate PKCE challenge from verifier using SHA256
    fn generate_pkce_challenge(verifier: &str) -> Result<String, OAuthSecurityError> {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge_bytes = hasher.finalize();
        Ok(URL_SAFE_NO_PAD.encode(challenge_bytes))
    }

    /// Generate cryptographically secure random string
    fn generate_secure_random_string(length: usize) -> String {
        let mut bytes = vec![0u8; length];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Generate unique session ID
    fn generate_session_id() -> String {
        let timestamp = Utc::now().timestamp_millis();
        let mut random_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random_bytes);
        let random_part = URL_SAFE_NO_PAD.encode(random_bytes);
        format!("oauth_{}_{}", timestamp, random_part)
    }
}

/// OAuth Security Manager for handling multiple concurrent flows
#[derive(Debug)]
pub struct OAuthSecurityManager {
    active_flows: HashMap<String, SecureOAuthFlow>,
    max_concurrent_flows: usize,
}

impl OAuthSecurityManager {
    /// Create new OAuth security manager
    pub fn new(max_concurrent_flows: usize) -> Self {
        Self {
            active_flows: HashMap::new(),
            max_concurrent_flows,
        }
    }

    /// Start new OAuth flow
    pub fn start_flow(&mut self, client_id: String, redirect_uri: String) -> Result<String, OAuthSecurityError> {
        // Clean up expired flows
        self.cleanup_expired_flows();

        // Check concurrent flow limit
        if self.active_flows.len() >= self.max_concurrent_flows {
            return Err(OAuthSecurityError::CryptographicError("Too many concurrent OAuth flows".to_string()));
        }

        let flow = SecureOAuthFlow::new(client_id, redirect_uri)?;
        let session_id = flow.session_id.clone();
        
        self.active_flows.insert(session_id.clone(), flow);
        Ok(session_id)
    }

    /// Get OAuth flow by session ID
    pub fn get_flow(&self, session_id: &str) -> Option<&SecureOAuthFlow> {
        self.active_flows.get(session_id)
    }

    /// Complete OAuth flow and remove from active flows
    pub fn complete_flow(&mut self, session_id: &str) -> Option<SecureOAuthFlow> {
        self.active_flows.remove(session_id)
    }

    /// Cancel OAuth flow
    pub fn cancel_flow(&mut self, session_id: &str) -> bool {
        self.active_flows.remove(session_id).is_some()
    }

    /// Clean up expired flows
    fn cleanup_expired_flows(&mut self) {
        let now = Utc::now();
        self.active_flows.retain(|_, flow| now <= flow.expires_at);
    }

    /// Get number of active flows
    pub fn active_flow_count(&self) -> usize {
        self.active_flows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_flow_creation() {
        let flow = SecureOAuthFlow::new(
            "test_client_id".to_string(),
            "http://localhost:1455/callback".to_string(),
        ).unwrap();

        assert!(!flow.state_parameter.is_empty());
        assert!(!flow.nonce.is_empty());
        assert!(!flow.pkce_verifier.verifier.is_empty());
        assert!(!flow.pkce_verifier.challenge.is_empty());
        assert_eq!(flow.pkce_verifier.challenge_method, "S256");
    }

    #[test]
    fn test_pkce_verification() {
        let flow = SecureOAuthFlow::new(
            "test_client_id".to_string(),
            "http://localhost:1455/callback".to_string(),
        ).unwrap();

        // Valid PKCE verification
        assert!(flow.verify_pkce(&flow.pkce_verifier.verifier).is_ok());

        // Invalid PKCE verification
        assert!(flow.verify_pkce("invalid_verifier").is_err());
    }

    #[test]
    fn test_state_validation() {
        let flow = SecureOAuthFlow::new(
            "test_client_id".to_string(),
            "http://localhost:1455/callback".to_string(),
        ).unwrap();

        // Valid state
        let result = flow.validate_callback("test_code", &flow.state_parameter, None);
        assert!(result.is_ok());

        // Invalid state
        let result = flow.validate_callback("test_code", "invalid_state", None);
        assert!(matches!(result, Err(OAuthSecurityError::InvalidState { .. })));
    }

    #[test]
    fn test_oauth_security_manager() {
        let mut manager = OAuthSecurityManager::new(2);

        let session_id_1 = manager.start_flow(
            "client_1".to_string(),
            "http://localhost:1455/callback".to_string(),
        ).unwrap();

        let session_id_2 = manager.start_flow(
            "client_2".to_string(),
            "http://localhost:1456/callback".to_string(),
        ).unwrap();

        assert_eq!(manager.active_flow_count(), 2);

        // Should fail - max concurrent flows reached
        let result = manager.start_flow(
            "client_3".to_string(),
            "http://localhost:1457/callback".to_string(),
        );
        assert!(result.is_err());

        // Complete one flow
        assert!(manager.complete_flow(&session_id_1).is_some());
        assert_eq!(manager.active_flow_count(), 1);

        // Should succeed now
        let _session_id_3 = manager.start_flow(
            "client_3".to_string(),
            "http://localhost:1457/callback".to_string(),
        ).unwrap();
        assert_eq!(manager.active_flow_count(), 2);
    }

    #[test]
    fn test_pkce_challenge_generation() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected_challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        
        let challenge = SecureOAuthFlow::generate_pkce_challenge(verifier).unwrap();
        assert_eq!(challenge, expected_challenge);
    }
}