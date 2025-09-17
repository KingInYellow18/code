/// # Claude Code Provider
///
/// Implements integration with Claude Code CLI, providing access to Claude models
/// through the official Anthropic Claude Code application. This provider supports
/// both subscription-based and API key authentication methods.

use super::{
    AIProvider, AuthStatus, Message, MessageContent, ProviderCapabilities, ResponseChunk,
    ResponseStream, UsageStats, filter_messages_for_text_only,
};
use crate::configuration::ProviderType;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Claude Code provider implementation
#[derive(Debug, Clone)]
pub struct ClaudeCodeProvider {
    config: ClaudeCodeConfig,
    capabilities: ProviderCapabilities,
}

/// Configuration for Claude Code provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    /// Path to the Claude Code binary
    pub claude_path: String,
    /// Default model to use
    pub default_model: String,
    /// Request timeout in seconds
    pub timeout_seconds: u64,
    /// Maximum number of conversation turns
    pub max_turns: u32,
    /// Enable verbose logging
    pub verbose: bool,
    /// Codex home directory
    pub codex_home: PathBuf,
}

/// Claude Code CLI response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub subtype: Option<String>,
    pub result: Option<String>,
    pub is_error: Option<bool>,
    pub content: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "apiKeySource")]
    pub api_key_source: Option<String>,
    #[serde(rename = "total_cost_usd")]
    pub total_cost_usd: Option<f64>,
    pub usage: Option<serde_json::Value>,
    #[serde(rename = "modelUsage")]
    pub model_usage: Option<serde_json::Value>,
    pub message: Option<serde_json::Value>,
    pub error: Option<String>,
    pub session_id: Option<String>,
    pub uuid: Option<String>,
}

/// Claude Code specific errors
#[derive(Debug, thiserror::Error)]
pub enum ClaudeCodeError {
    #[error("Claude Code binary not found at path: {path}")]
    BinaryNotFound { path: String },

    #[error("Claude Code authentication failed: {message}")]
    AuthenticationFailed { message: String },

    #[error("Claude Code process error: {message}")]
    ProcessError { message: String },

    #[error("Response parsing error: {message}")]
    ParseError { message: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Timeout error: operation took longer than {seconds}s")]
    TimeoutError { seconds: u64 },

    #[error("Claude Code CLI returned error: {error}")]
    CLIError { error: String },
}

/// Unified auth error placeholder - to be unified with main auth system
#[derive(Debug, thiserror::Error)]
pub enum UnifiedAuthError {
    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Secure storage error: {0}")]
    SecureStorage(#[from] crate::security::SecureStorageError),
}

impl From<ClaudeCodeError> for UnifiedAuthError {
    fn from(error: ClaudeCodeError) -> Self {
        UnifiedAuthError::ProviderError(format!("Claude Code: {}", error))
    }
}

impl ClaudeCodeConfig {
    /// Create configuration from codex home directory
    pub fn from_codex_home(codex_home: &Path) -> Result<Self, ClaudeCodeError> {
        let config_file = codex_home.join("claude_code_config.json");

        // Try to load existing config
        if config_file.exists() {
            let content = std::fs::read_to_string(&config_file)
                .map_err(ClaudeCodeError::IoError)?;
            let config: ClaudeCodeConfig = serde_json::from_str(&content)
                .map_err(ClaudeCodeError::SerializationError)?;
            return Ok(config);
        }

        // Create default config
        let claude_path = which::which("claude")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "claude".to_string());

        let config = ClaudeCodeConfig {
            claude_path,
            default_model: "claude-sonnet-4-20250514".to_string(),
            timeout_seconds: 600,
            max_turns: 1,
            verbose: false,
            codex_home: codex_home.to_path_buf(),
        };

        // Save default config
        let content = serde_json::to_string_pretty(&config)
            .map_err(ClaudeCodeError::SerializationError)?;
        std::fs::write(&config_file, content)
            .map_err(ClaudeCodeError::IoError)?;

        Ok(config)
    }

    /// Validate that Claude Code is available
    pub async fn validate(&self) -> Result<(), ClaudeCodeError> {
        // Check if binary exists
        let binary_path = if self.claude_path == "claude" {
            which::which("claude")
                .map_err(|_| ClaudeCodeError::BinaryNotFound {
                    path: self.claude_path.clone()
                })?
        } else {
            PathBuf::from(&self.claude_path)
        };

        if !binary_path.exists() {
            return Err(ClaudeCodeError::BinaryNotFound {
                path: self.claude_path.clone(),
            });
        }

        // Check if Claude Code is authenticated by testing a simple command
        let output = Command::new(&self.claude_path)
            .args(&["--print", "--output-format", "json", "test"])
            .output()
            .await
            .map_err(|e| ClaudeCodeError::ProcessError {
                message: format!("Failed to test Claude CLI: {}", e),
            })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            // Check if it's an authentication error specifically
            if error_msg.contains("not authenticated") || error_msg.contains("login") {
                return Err(ClaudeCodeError::AuthenticationFailed {
                    message: format!("Claude Code not authenticated: {}", error_msg),
                });
            }
            // Other errors might be acceptable (like API errors) as long as auth works
        }

        Ok(())
    }
}

impl ClaudeCodeProvider {
    /// Create a new Claude Code provider
    pub async fn new(config: ClaudeCodeConfig) -> Result<Self, ClaudeCodeError> {
        // Validate configuration
        config.validate().await?;

        let capabilities = ProviderCapabilities {
            supports_images: false, // Claude Code CLI doesn't support images
            supports_streaming: true,
            supports_tools: true,
            max_tokens: Some(200_000), // Claude Sonnet 4 max tokens
            supported_models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
                "claude-3-opus-20240229".to_string(),
            ],
        };

        Ok(Self {
            config,
            capabilities,
        })
    }

    /// Check authentication status using Claude Code CLI
    async fn check_auth_status(&self) -> Result<AuthStatus, ClaudeCodeError> {
        // Test authentication with a simple command
        let output = Command::new(&self.config.claude_path)
            .args(&["--print", "--output-format", "json", "test"])
            .output()
            .await
            .map_err(|e| ClaudeCodeError::ProcessError {
                message: format!("Failed to test Claude CLI: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            // Check if it's an authentication error
            if stderr.contains("not authenticated") || stderr.contains("login") || stderr.contains("sign in") {
                return Ok(AuthStatus {
                    authenticated: false,
                    subscription_tier: None,
                    auth_method: "unknown".to_string(),
                    quota_remaining: None,
                    error_message: Some(format!("Not authenticated: {}", stderr)),
                });
            }
            // For other errors, we might still be authenticated but have API issues
        }

        // Try to parse the JSON response to get additional info
        if let Ok(response_data) = serde_json::from_str::<serde_json::Value>(&stdout) {
            // Check if we have valid response structure
            if response_data.get("type").is_some() {
                // Extract subscription info if available
                let auth_method = response_data.get("modelUsage")
                    .and_then(|usage| usage.as_object())
                    .map(|models| {
                        if models.keys().any(|k| k.contains("claude")) {
                            "subscription".to_string()
                        } else {
                            "api_key".to_string()
                        }
                    })
                    .unwrap_or_else(|| "claude_code".to_string());

                return Ok(AuthStatus {
                    authenticated: true,
                    subscription_tier: None, // Claude Code doesn't expose this directly
                    auth_method,
                    quota_remaining: None,
                    error_message: None,
                });
            }
        }

        // Fallback: if we get here and command succeeded, assume authenticated
        Ok(AuthStatus {
            authenticated: output.status.success(),
            subscription_tier: None,
            auth_method: "claude_code".to_string(),
            quota_remaining: None,
            error_message: if output.status.success() { None } else { Some(stderr.to_string()) },
        })
    }

    /// Spawn Claude Code process and return the child process
    async fn spawn_claude_process(
        &self,
        system_prompt: &str,
        messages: Vec<Message>,
    ) -> Result<tokio::process::Child, ClaudeCodeError> {
        let mut cmd = Command::new(&self.config.claude_path);

        // Build command with correct Claude CLI arguments
        cmd.args(&[
            "--print", // Print mode for non-interactive output
            "--output-format", "stream-json", // Streaming JSON output
            "--model", &self.config.default_model,
        ]);

        // Add system prompt using append-system-prompt option
        if !system_prompt.is_empty() {
            cmd.args(&["--append-system-prompt", system_prompt]);
        }

        if self.config.verbose {
            cmd.arg("--verbose");
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| ClaudeCodeError::ProcessError {
                message: format!("Failed to spawn Claude Code process: {}", e),
            })?;

        // Send the user message as text input to stdin
        if let Some(stdin) = child.stdin.as_mut() {
            let filtered_messages = filter_messages_for_text_only(messages);

            // Convert messages to text format - take the last user message
            let user_input = filtered_messages.iter()
                .filter(|msg| msg.role == "user")
                .last()
                .map(|msg| msg.content.clone())
                .unwrap_or_else(|| MessageContent::Text("Hello".to_string()));

            let text_content = match user_input {
                MessageContent::Text(text) => text,
                MessageContent::Array(_) => "Hello".to_string(),
            };
            stdin.write_all(text_content.as_bytes()).await
                .map_err(ClaudeCodeError::IoError)?;
            stdin.flush().await
                .map_err(ClaudeCodeError::IoError)?;
        }

        // Close stdin to signal end of input
        drop(child.stdin.take());

        Ok(child)
    }

    /// Parse Claude Code response stream
    async fn parse_response_stream(
        &self,
        mut child: tokio::process::Child,
    ) -> Result<ResponseStream, ClaudeCodeError> {
        let stdout = child.stdout.take()
            .ok_or_else(|| ClaudeCodeError::ProcessError {
                message: "Failed to get stdout from Claude process".to_string(),
            })?;

        let (tx, rx) = mpsc::channel(32);
        let tx_clone = tx.clone();

        // Spawn task to read stdout
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<ClaudeCodeMessage>(&line) {
                    Ok(message) => {
                        match message.message_type.as_str() {
                            "assistant" => {
                                // Handle assistant message content
                                if let Some(msg_obj) = &message.message {
                                    if let Some(content_array) = msg_obj.get("content") {
                                        if let Some(array) = content_array.as_array() {
                                            for content_item in array {
                                                if let Some(text) = content_item.get("text") {
                                                    if let Some(text_str) = text.as_str() {
                                                        if tx.send(Ok(ResponseChunk::Text(text_str.to_string()))).await.is_err() {
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else if let Some(content) = message.content {
                                    // Fallback for simple content
                                    if tx.send(Ok(ResponseChunk::Text(content))).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            "result" => {
                                // Extract usage statistics from the result
                                let mut input_tokens = 0u64;
                                let mut output_tokens = 0u64;
                                let mut total_cost = 0.0f64;

                                // Try to get usage from the usage field
                                if let Some(usage_obj) = &message.usage {
                                    input_tokens = usage_obj.get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    output_tokens = usage_obj.get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                }

                                // Try to get cost from total_cost_usd field
                                total_cost = message.total_cost_usd.unwrap_or(0.0);

                                let usage = UsageStats {
                                    input_tokens,
                                    output_tokens,
                                    total_cost_usd: total_cost,
                                };
                                if tx.send(Ok(ResponseChunk::Usage(usage))).await.is_err() {
                                    break;
                                }
                            }
                            "system" => {
                                // Ignore system messages (initialization)
                            }
                            _ => {
                                // Check if this is an error result
                                if message.is_error.unwrap_or(false) {
                                    let error_msg = message.result.unwrap_or_else(|| {
                                        message.error.unwrap_or("Unknown error".to_string())
                                    });
                                    if tx.send(Ok(ResponseChunk::Error(error_msg))).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to parse Claude response: {}", e);
                        if tx.send(Ok(ResponseChunk::Error(error_msg))).await.is_err() {
                            break;
                        }
                    }
                }
            }

            // Send done signal
            let _ = tx.send(Ok(ResponseChunk::Done)).await;
        });

        // Spawn task to wait for process completion
        tokio::spawn(async move {
            if let Ok(status) = child.wait().await {
                if !status.success() {
                    let error_msg = format!("Claude Code process exited with status: {}", status);
                    let _ = tx_clone.send(Ok(ResponseChunk::Error(error_msg))).await;
                }
            }
        });

        Ok(ReceiverStream::new(rx))
    }
}

#[async_trait]
impl AIProvider for ClaudeCodeProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Claude
    }

    async fn is_available(&self) -> bool {
        self.config.validate().await.is_ok()
    }

    async fn get_auth_status(&self) -> Result<AuthStatus, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.check_auth_status().await?)
    }

    async fn send_message(
        &self,
        system_prompt: &str,
        messages: Vec<Message>,
    ) -> Result<ResponseStream, Box<dyn std::error::Error + Send + Sync>> {
        let child = self.spawn_claude_process(system_prompt, messages).await?;
        let stream = self.parse_response_stream(child).await?;
        Ok(stream)
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }
}

impl ClaudeCodeProvider {
    /// Check if the provider supports a specific feature
    pub async fn supports_feature(&self, feature: &str) -> bool {
        match feature {
            "streaming" => self.capabilities.supports_streaming,
            "images" => self.capabilities.supports_images,
            "tools" => self.capabilities.supports_tools,
            "large_context" => true, // Claude supports large context windows
            "code_execution" => true, // Claude Code supports code execution
            "file_upload" => false, // Not supported via CLI
            "web_search" => false, // Not supported via CLI
            "function_calling" => self.capabilities.supports_tools,
            "multi_turn" => true, // Supports conversation
            "system_prompt" => true, // Supports system prompts
            _ => false, // Unknown features are not supported
        }
    }

    /// Get detailed model information
    pub fn get_model_info(&self, model: &str) -> Option<ModelInfo> {
        match model {
            "claude-sonnet-4-20250514" => Some(ModelInfo {
                name: model.to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_images: false,
                supports_tools: true,
                cost_per_input_token: 0.000003,
                cost_per_output_token: 0.000015,
            }),
            "claude-3-5-sonnet-20241022" => Some(ModelInfo {
                name: model.to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_images: true,
                supports_tools: true,
                cost_per_input_token: 0.000003,
                cost_per_output_token: 0.000015,
            }),
            "claude-3-5-haiku-20241022" => Some(ModelInfo {
                name: model.to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_images: false,
                supports_tools: true,
                cost_per_input_token: 0.00000025,
                cost_per_output_token: 0.00000125,
            }),
            _ => None,
        }
    }

    /// Estimate cost for a given input
    pub fn estimate_cost(&self, input_tokens: u64, output_tokens: u64, model: Option<&str>) -> f64 {
        let model_name = model.unwrap_or(&self.config.default_model);

        if let Some(model_info) = self.get_model_info(model_name) {
            (input_tokens as f64 * model_info.cost_per_input_token) +
            (output_tokens as f64 * model_info.cost_per_output_token)
        } else {
            // Default estimation for unknown models
            (input_tokens as f64 * 0.000003) + (output_tokens as f64 * 0.000015)
        }
    }
}

/// Model information structure
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub supports_images: bool,
    pub supports_tools: bool,
    pub cost_per_input_token: f64,
    pub cost_per_output_token: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_config_creation() {
        let temp_dir = tempdir().unwrap();
        let config = ClaudeCodeConfig::from_codex_home(temp_dir.path()).unwrap();

        assert_eq!(config.default_model, "claude-sonnet-4-20250514");
        assert_eq!(config.max_turns, 1);
        assert_eq!(config.timeout_seconds, 600);
    }

    #[test]
    fn test_message_parsing() {
        let json = r#"{"type": "assistant", "content": "Hello world"}"#;
        let message: ClaudeCodeMessage = serde_json::from_str(json).unwrap();

        assert_eq!(message.message_type, "assistant");
        assert_eq!(message.content, Some("Hello world".to_string()));
    }

    #[test]
    fn test_result_message_parsing() {
        let json = r#"{"type": "result", "total_cost_usd": 0.001, "usage": {"input_tokens": 10, "output_tokens": 20}}"#;
        let message: ClaudeCodeMessage = serde_json::from_str(json).unwrap();

        assert_eq!(message.message_type, "result");
        assert_eq!(message.total_cost_usd, Some(0.001));

        // Check usage field
        assert!(message.usage.is_some());
        let usage = message.usage.unwrap();
        assert_eq!(usage["input_tokens"], 10);
        assert_eq!(usage["output_tokens"], 20);
    }

    #[test]
    fn test_provider_capabilities() {
        let temp_dir = tempdir().unwrap();
        let config = ClaudeCodeConfig::from_codex_home(temp_dir.path()).unwrap();

        // Note: This test won't validate the binary since it might not exist in test env
        // but we can test capability settings
        let capabilities = ProviderCapabilities {
            supports_images: false,
            supports_streaming: true,
            supports_tools: true,
            max_tokens: Some(200_000),
            supported_models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
            ],
        };

        assert!(!capabilities.supports_images);
        assert!(capabilities.supports_streaming);
        assert!(capabilities.supports_tools);
        assert_eq!(capabilities.max_tokens, Some(200_000));
    }
}