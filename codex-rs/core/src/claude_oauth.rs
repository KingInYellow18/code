use oauth2::{
    AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse, AuthUrl, TokenUrl,
};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use std::collections::HashMap;
use serde::Deserialize;
use chrono::{Duration, Utc};
use crate::claude_auth::{ClaudeTokenData, ClaudeOAuthConfig};

/// PKCE OAuth flow manager for Claude authentication
pub struct ClaudeOAuthFlow {
    client: BasicClient,
    pkce_verifier: Option<PkceCodeVerifier>,
    csrf_token: Option<CsrfToken>,
    config: ClaudeOAuthConfig,
}

/// OAuth authorization URL with state
#[derive(Debug, Clone)]
pub struct AuthorizationUrl {
    pub url: String,
    pub state: String,
}

/// OAuth callback parameters
#[derive(Debug, Deserialize)]
pub struct OAuthCallback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// OAuth token exchange response
#[derive(Debug, Deserialize)]
pub struct TokenExchangeResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub subscription_tier: Option<String>,
}

impl ClaudeOAuthFlow {
    /// Create new OAuth flow with default configuration
    pub fn new() -> Result<Self, oauth2::url::ParseError> {
        let config = ClaudeOAuthConfig::default();
        Self::with_config(config)
    }

    /// Create OAuth flow with custom configuration
    pub fn with_config(config: ClaudeOAuthConfig) -> Result<Self, oauth2::url::ParseError> {
        let client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            None, // Client secret not needed for PKCE
            AuthUrl::new(config.auth_url.clone())?,
            Some(TokenUrl::new(config.token_url.clone())?),
        ).set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        Ok(Self {
            client,
            pkce_verifier: None,
            csrf_token: None,
            config,
        })
    }

    /// Generate authorization URL with PKCE challenge
    pub fn generate_auth_url(&mut self) -> AuthorizationUrl {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        self.pkce_verifier = Some(pkce_verifier);

        let mut auth_request = self.client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(pkce_challenge);

        // Add scopes
        for scope in &self.config.scopes {
            auth_request = auth_request.add_scope(Scope::new(scope.clone()));
        }

        let (auth_url, csrf_token) = auth_request.url();
        self.csrf_token = Some(csrf_token.clone());

        AuthorizationUrl {
            url: auth_url.to_string(),
            state: csrf_token.secret().clone(),
        }
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code_for_tokens(
        &self,
        callback: OAuthCallback,
    ) -> Result<ClaudeTokenData, OAuthError> {
        // Validate CSRF token
        if let Some(expected_state) = &self.csrf_token {
            if Some(expected_state.secret()) != callback.state.as_ref() {
                return Err(OAuthError::InvalidState);
            }
        }

        // Check for OAuth errors
        if let Some(error) = callback.error {
            return Err(OAuthError::AuthorizationError {
                error: error.clone(),
                description: callback.error_description,
            });
        }

        // Get authorization code
        let code = callback.code.ok_or(OAuthError::MissingCode)?;
        let auth_code = AuthorizationCode::new(code);

        // Get PKCE verifier
        let pkce_verifier = self.pkce_verifier
            .as_ref()
            .ok_or(OAuthError::MissingPkceVerifier)?;

        // Exchange code for token
        let token_result = self.client
            .exchange_code(auth_code)
            .set_pkce_verifier(*pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|e| OAuthError::TokenExchange(e))?;

        // Extract subscription tier from extra fields if available
        let subscription_tier = "unknown".to_string(); // TODO: Parse from extra fields

        Ok(ClaudeTokenData {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result.refresh_token()
                .map(|t| t.secret().clone()),
            expires_at: Utc::now() + Duration::seconds(
                token_result.expires_in()
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(3600)
            ),
            subscription_tier,
            scope: token_result.scopes()
                .map(|scopes| {
                    scopes.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                }),
        })
    }

    /// Start local OAuth server for handling the callback
    pub async fn start_oauth_server(&self) -> Result<OAuthServer, std::io::Error> {
        OAuthServer::new(1456).await
    }

    /// Get the redirect URI configured for this flow
    pub fn redirect_uri(&self) -> &str {
        &self.config.redirect_uri
    }

    /// Get the client ID
    pub fn client_id(&self) -> &str {
        &self.config.client_id
    }
}

impl Default for ClaudeOAuthFlow {
    fn default() -> Self {
        Self::new().expect("Default OAuth configuration should be valid")
    }
}

/// OAuth error types
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("Invalid CSRF state parameter")]
    InvalidState,
    
    #[error("Authorization error: {error}")]
    AuthorizationError {
        error: String,
        description: Option<String>,
    },
    
    #[error("Missing authorization code in callback")]
    MissingCode,
    
    #[error("Missing PKCE verifier")]
    MissingPkceVerifier,
    
    #[error("Token exchange failed: {0}")]
    TokenExchange(#[from] oauth2::RequestTokenError<oauth2::reqwest::Error<reqwest::Error>, oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>>),
    
    #[error("Server error: {0}")]
    ServerError(#[from] std::io::Error),
    
    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),
}

/// Simple OAuth callback server
pub struct OAuthServer {
    port: u16,
    shutdown_handle: Option<tokio::sync::oneshot::Sender<()>>,
}

impl OAuthServer {
    /// Create and start OAuth callback server
    pub async fn new(port: u16) -> Result<Self, std::io::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        let server = Self {
            port,
            shutdown_handle: Some(tx),
        };

        // Start the server in the background
        tokio::spawn(async move {
            if let Err(e) = run_oauth_server(port, rx).await {
                eprintln!("OAuth server error: {}", e);
            }
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(server)
    }

    /// Get the port the server is running on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the callback URL for this server
    pub fn callback_url(&self) -> String {
        format!("http://localhost:{}/callback", self.port)
    }

    /// Wait for OAuth callback and return the result
    pub async fn wait_for_callback(&self) -> Result<OAuthCallback, OAuthError> {
        // This is a simplified implementation - in practice, you'd want to
        // set up a proper channel to receive the callback data
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        Err(OAuthError::ServerError(std::io::Error::other("Timeout waiting for callback")))
    }

    /// Shutdown the server
    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_handle.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for OAuthServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_handle.take() {
            let _ = tx.send(());
        }
    }
}

/// Run the OAuth callback server
async fn run_oauth_server(
    port: u16,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), std::io::Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("OAuth callback server listening on http://localhost:{}/callback", port);

    loop {
        tokio::select! {
            // Handle incoming connections
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        tokio::spawn(handle_oauth_callback(stream));
                    }
                    Err(e) => {
                        eprintln!("Failed to accept connection: {}", e);
                    }
                }
            }
            // Handle shutdown signal
            _ = &mut shutdown => {
                println!("Shutting down OAuth callback server");
                break;
            }
        }
    }

    Ok(())
}

/// Handle individual OAuth callback request
async fn handle_oauth_callback(mut stream: tokio::net::TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..n]);

    // Parse the HTTP request to extract query parameters
    let callback_params = if let Some(first_line) = request.lines().next() {
        if let Some(path_and_query) = first_line.split(' ').nth(1) {
            if let Some(query_start) = path_and_query.find('?') {
                let query = &path_and_query[query_start + 1..];
                parse_query_string(query)
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    // Create response
    let response_body = if callback_params.contains_key("code") {
        r#"
        <!DOCTYPE html>
        <html>
        <head><title>Authentication Successful</title></head>
        <body>
            <h1>✅ Authentication Successful</h1>
            <p>You have successfully authenticated with Claude. You can now close this window and return to the terminal.</p>
            <script>
                setTimeout(() => window.close(), 3000);
            </script>
        </body>
        </html>
        "#.to_string()
    } else if callback_params.contains_key("error") {
        let error = callback_params.get("error").unwrap_or(&"unknown".to_string());
        let description = callback_params.get("error_description").unwrap_or(&"".to_string());
        
        format!(r"
        <!DOCTYPE html>
        <html>
        <head><title>Authentication Error</title></head>
        <body>
            <h1>❌ Authentication Error</h1>
            <p>Error: {}</p>
            <p>Description: {}</p>
            <p>Please return to the terminal and try again.</p>
            <script>
                setTimeout(() => window.close(), 5000);
            </script>
        </body>
        </html>
        ", error, description)
    } else {
        r"
        <!DOCTYPE html>
        <html>
        <head><title>Invalid Request</title></head>
        <body>
            <h1>⚠️ Invalid Request</h1>
            <p>The authentication callback was not properly formatted.</p>
        </body>
        </html>
        ".to_string()
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}

/// Parse URL query string into key-value pairs
fn parse_query_string(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => {
                    // URL decode the values
                    let key = urlencoding::decode(key).ok()?.into_owned();
                    let value = urlencoding::decode(value).ok()?.into_owned();
                    Some((key, value))
                }
                _ => None,
            }
        })
        .collect()
}

/// Complete OAuth flow with browser interaction
pub struct BrowserOAuthFlow {
    oauth_flow: ClaudeOAuthFlow,
    server: Option<OAuthServer>,
}

impl BrowserOAuthFlow {
    /// Create new browser-based OAuth flow
    pub fn new() -> Result<Self, OAuthError> {
        let oauth_flow = ClaudeOAuthFlow::new()?;
        Ok(Self {
            oauth_flow,
            server: None,
        })
    }

    /// Start the complete OAuth flow
    pub async fn start(&mut self) -> Result<String, OAuthError> {
        // Start OAuth server
        let server = OAuthServer::new(1456).await?;
        let auth_url_info = self.oauth_flow.generate_auth_url();
        self.server = Some(server);

        // Open browser (or return URL for manual opening)
        if let Err(_) = open_browser(&auth_url_info.url) {
            println!("Please open this URL in your browser to authenticate:");
            println!("{}", auth_url_info.url);
        }

        Ok(auth_url_info.url)
    }

    /// Wait for completion and return tokens
    pub async fn wait_for_completion(&self) -> Result<ClaudeTokenData, OAuthError> {
        if let Some(server) = &self.server {
            let callback = server.wait_for_callback().await?;
            self.oauth_flow.exchange_code_for_tokens(callback).await
        } else {
            Err(OAuthError::ServerError(std::io::Error::other("OAuth flow not started")))
        }
    }
}

impl Default for BrowserOAuthFlow {
    fn default() -> Self {
        Self::new().expect("Default OAuth flow should work")
    }
}

/// Attempt to open URL in default browser
fn open_browser(url: &str) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/c", "start", url])
            .output()?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .output()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .output()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_flow_creation() {
        let flow = ClaudeOAuthFlow::new();
        assert!(flow.is_ok());
    }

    #[test]
    fn test_auth_url_generation() {
        let mut flow = ClaudeOAuthFlow::new().unwrap();
        let auth_url = flow.generate_auth_url();
        
        assert!(auth_url.url.contains("auth.anthropic.com"));
        assert!(auth_url.url.contains("client_id="));
        assert!(auth_url.url.contains("code_challenge="));
        assert!(!auth_url.state.is_empty());
    }

    #[test]
    fn test_query_string_parsing() {
        let query = "code=test_code&state=test_state&scope=api";
        let params = parse_query_string(query);
        
        assert_eq!(params.get("code"), Some(&"test_code".to_string()));
        assert_eq!(params.get("state"), Some(&"test_state".to_string()));
        assert_eq!(params.get("scope"), Some(&"api".to_string()));
    }

    #[tokio::test]
    async fn test_oauth_server_creation() {
        let server = OAuthServer::new(0).await; // Use port 0 for random port
        assert!(server.is_ok());
        
        let server = server.unwrap();
        assert!(server.port() > 0);
        assert!(server.callback_url().contains("localhost"));
    }
}