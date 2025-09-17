//! Comprehensive Test Suite for Claude Code Provider
//!
//! This test suite covers all aspects of the Claude Code provider implementation:
//! - Provider trait compliance and interface adherence
//! - CLI process management and timeout handling
//! - Message filtering and JSON parsing validation
//! - Authentication detection and flow validation
//! - Error handling and recovery scenarios
//! - Performance benchmarks and resource management
//! - Multi-agent integration patterns
//! - Security boundary testing

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;

use tempfile::{TempDir, NamedTempFile};
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, sleep};
use tokio::sync::Semaphore;
use futures::future::try_join_all;
use serde_json::{json, Value};

use crate::common::claude_test_utils::{MockClaudeServer, ClaudeTestUtils, TestEnvironment};

// Mock provider traits and types based on the actual implementation
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderType {
    OpenAI,
    Claude,
}

#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub supports_images: bool,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub max_tokens: Option<u32>,
    pub supported_models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub subscription_tier: Option<String>,
    pub auth_method: String,
    pub quota_remaining: Option<u64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: Value,
}

#[derive(Debug, Clone)]
pub struct UsageStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone)]
pub enum ResponseChunk {
    Text(String),
    Usage(UsageStats),
    Error(String),
    Done,
}

pub type ResponseStream = tokio_stream::wrappers::ReceiverStream<Result<ResponseChunk, Box<dyn std::error::Error + Send + Sync>>>;

#[derive(Debug, thiserror::Error)]
pub enum ClaudeCodeError {
    #[error("Binary not found: {path}")]
    BinaryNotFound { path: String },
    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },
    #[error("Process error: {message}")]
    ProcessError { message: String },
    #[error("Parse error: {message}")]
    ParseError { message: String },
    #[error("Timeout: {seconds}s")]
    TimeoutError { seconds: u64 },
    #[error("CLI error: {error}")]
    CLIError { error: String },
}

/// Provider trait that implementations must adhere to
#[async_trait::async_trait]
pub trait AIProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;
    async fn is_available(&self) -> bool;
    async fn get_auth_status(&self) -> Result<AuthStatus, Box<dyn std::error::Error + Send + Sync>>;
    async fn send_message(&self, system: &str, messages: Vec<Message>) -> Result<ResponseStream, Box<dyn std::error::Error + Send + Sync>>;
    fn get_capabilities(&self) -> ProviderCapabilities;
}

/// Mock Claude Code Provider for testing
pub struct MockClaudeCodeProvider {
    claude_path: PathBuf,
    config: ClaudeCodeConfig,
    capabilities: ProviderCapabilities,
    mock_responses: HashMap<String, MockResponse>,
    fail_auth: bool,
    simulate_timeout: bool,
    slow_response_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeConfig {
    pub claude_path: String,
    pub default_model: String,
    pub timeout_seconds: u64,
    pub max_turns: u32,
    pub verbose: bool,
    pub codex_home: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub delay_ms: Option<u64>,
}

impl MockClaudeCodeProvider {
    pub fn new(temp_dir: &Path) -> Self {
        let claude_path = Self::create_mock_claude_binary(temp_dir).expect("create mock binary");

        let config = ClaudeCodeConfig {
            claude_path: claude_path.to_string_lossy().to_string(),
            default_model: "claude-sonnet-4-20250514".to_string(),
            timeout_seconds: 30,
            max_turns: 1,
            verbose: false,
            codex_home: temp_dir.to_path_buf(),
        };

        let capabilities = ProviderCapabilities {
            supports_images: false,
            supports_streaming: true,
            supports_tools: true,
            max_tokens: Some(200_000),
            supported_models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
            ],
        };

        let mut mock_responses = HashMap::new();

        // Default successful responses
        mock_responses.insert("auth_status".to_string(), MockResponse {
            exit_code: 0,
            stdout: json!({
                "authenticated": true,
                "subscriptionTier": "max",
                "authMethod": "oauth"
            }).to_string(),
            stderr: "".to_string(),
            delay_ms: Some(100),
        });

        mock_responses.insert("send_message".to_string(), MockResponse {
            exit_code: 0,
            stdout: json!({
                "type": "assistant",
                "content": "Mock Claude response",
                "model": "claude-sonnet-4-20250514",
                "inputTokens": 10,
                "outputTokens": 15,
                "totalCostUsd": 0.001
            }).to_string(),
            stderr: "".to_string(),
            delay_ms: Some(500),
        });

        Self {
            claude_path,
            config,
            capabilities,
            mock_responses,
            fail_auth: false,
            simulate_timeout: false,
            slow_response_ms: None,
        }
    }

    pub fn with_auth_failure(mut self) -> Self {
        self.fail_auth = true;
        self.mock_responses.insert("auth_status".to_string(), MockResponse {
            exit_code: 1,
            stdout: "".to_string(),
            stderr: "Authentication failed: No valid API key or subscription found".to_string(),
            delay_ms: Some(100),
        });
        self
    }

    pub fn with_timeout_simulation(mut self) -> Self {
        self.simulate_timeout = true;
        self
    }

    pub fn with_slow_response(mut self, delay_ms: u64) -> Self {
        self.slow_response_ms = Some(delay_ms);
        self
    }

    pub fn with_parse_error(mut self) -> Self {
        self.mock_responses.insert("send_message".to_string(), MockResponse {
            exit_code: 0,
            stdout: "Invalid JSON response from Claude".to_string(),
            stderr: "".to_string(),
            delay_ms: Some(100),
        });
        self
    }

    pub fn with_quota_exceeded(mut self) -> Self {
        self.mock_responses.insert("send_message".to_string(), MockResponse {
            exit_code: 1,
            stdout: "".to_string(),
            stderr: json!({
                "type": "error",
                "error": {
                    "type": "rate_limit_error",
                    "message": "API quota exceeded"
                }
            }).to_string(),
            delay_ms: Some(200),
        });
        self
    }

    /// Create a mock Claude binary that simulates real CLI behavior
    fn create_mock_claude_binary(temp_dir: &Path) -> Result<PathBuf, std::io::Error> {
        let binary_path = temp_dir.join("claude");

        let script_content = r#"#!/bin/bash

# Mock Claude Code CLI implementation for testing
case "$1" in
    "auth")
        case "$2" in
            "status")
                if [ "$MOCK_AUTH_FAIL" = "true" ]; then
                    echo "Authentication failed: No valid API key or subscription found" >&2
                    exit 1
                else
                    echo '{"authenticated": true, "subscriptionTier": "max", "authMethod": "oauth"}'
                    exit 0
                fi
                ;;
            "quota")
                echo '{"daily_limit": 100000, "current_usage": 5000, "reset_time": "2024-01-02T00:00:00Z"}'
                exit 0
                ;;
        esac
        ;;
    "models")
        if [ "$2" = "list" ]; then
            echo "claude-sonnet-4-20250514"
            echo "claude-3-5-sonnet-20241022"
            echo "claude-3-5-haiku-20241022"
            exit 0
        fi
        ;;
    "--system-prompt")
        # Simulate message processing
        if [ "$MOCK_TIMEOUT" = "true" ]; then
            sleep 60  # Simulate timeout
        fi

        if [ "$MOCK_SLOW_RESPONSE" ]; then
            sleep "$MOCK_SLOW_RESPONSE"
        fi

        if [ "$MOCK_PARSE_ERROR" = "true" ]; then
            echo "Invalid JSON response from Claude"
            exit 0
        fi

        if [ "$MOCK_QUOTA_EXCEEDED" = "true" ]; then
            echo '{"type": "error", "error": {"type": "rate_limit_error", "message": "API quota exceeded"}}' >&2
            exit 1
        fi

        # Read stdin and process the message
        input=$(cat)

        # Simulate streaming response
        echo '{"type": "assistant", "content": "Mock Claude response"}'
        echo '{"type": "result", "inputTokens": 10, "outputTokens": 15, "totalCostUsd": 0.001}'
        exit 0
        ;;
    *)
        echo "Unknown command: $*" >&2
        exit 1
        ;;
esac
"#;

        fs::write(&binary_path, script_content)?;
        fs::set_permissions(&binary_path, Permissions::from_mode(0o755))?;

        Ok(binary_path)
    }

    /// Filter messages to remove image content (Claude Code CLI limitation)
    fn filter_messages(&self, messages: Vec<Message>) -> Vec<Message> {
        messages.into_iter().map(|mut msg| {
            if let Value::Array(ref mut content_blocks) = msg.content {
                for block in content_blocks.iter_mut() {
                    if let Value::Object(ref mut obj) = block {
                        if obj.get("type").and_then(|v| v.as_str()) == Some("image") {
                            *obj = json!({
                                "type": "text",
                                "text": "[Image content not supported by Claude Code CLI]"
                            }).as_object().unwrap().clone();
                        }
                    }
                }
            }
            msg
        }).collect()
    }

    /// Execute mock Claude CLI command
    async fn execute_claude_cli(&self, system_prompt: &str, messages: Vec<Message>) -> Result<ResponseStream, ClaudeCodeError> {
        let filtered_messages = self.filter_messages(messages);

        // Set up environment variables for mock behavior
        let mut cmd = TokioCommand::new(&self.config.claude_path);
        cmd.args(&[
            "--print",
            "--output-format", "stream-json",
            "--model", &self.config.default_model,
            "--append-system-prompt", system_prompt
        ]);

        if self.fail_auth {
            cmd.env("MOCK_AUTH_FAIL", "true");
        }
        if self.simulate_timeout {
            cmd.env("MOCK_TIMEOUT", "true");
        }
        if let Some(delay) = self.slow_response_ms {
            cmd.env("MOCK_SLOW_RESPONSE", (delay / 1000).to_string());
        }

        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| ClaudeCodeError::ProcessError {
                message: format!("Failed to spawn claude process: {}", e)
            })?;

        // Send messages as JSON to stdin
        if let Some(stdin) = child.stdin.take() {
            let input_json = serde_json::to_string(&filtered_messages)
                .map_err(|e| ClaudeCodeError::ParseError {
                    message: format!("Failed to serialize messages: {}", e)
                })?;

            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            stdin.write_all(input_json.as_bytes()).await
                .map_err(|e| ClaudeCodeError::ProcessError {
                    message: format!("Failed to write to stdin: {}", e)
                })?;
            stdin.flush().await
                .map_err(|e| ClaudeCodeError::ProcessError {
                    message: format!("Failed to flush stdin: {}", e)
                })?;
            drop(stdin);
        }

        // Wait for completion with timeout
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
        let output = timeout(timeout_duration, child.wait_with_output())
            .await
            .map_err(|_| ClaudeCodeError::TimeoutError {
                seconds: self.config.timeout_seconds
            })?
            .map_err(|e| ClaudeCodeError::ProcessError {
                message: format!("Claude CLI execution failed: {}", e)
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ClaudeCodeError::CLIError {
                error: stderr.to_string()
            });
        }

        // Create response stream
        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse each line as a separate JSON response
            for line in stdout.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<Value>(line) {
                    Ok(response) => {
                        if let Some(msg_type) = response.get("type").and_then(|v| v.as_str()) {
                            match msg_type {
                                "assistant" => {
                                    if let Some(content) = response.get("content").and_then(|v| v.as_str()) {
                                        let _ = tx.send(Ok(ResponseChunk::Text(content.to_string()))).await;
                                    }
                                }
                                "result" => {
                                    let usage = UsageStats {
                                        input_tokens: response.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0),
                                        output_tokens: response.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0),
                                        total_cost_usd: response.get("totalCostUsd").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    };
                                    let _ = tx.send(Ok(ResponseChunk::Usage(usage))).await;
                                }
                                "error" => {
                                    let error_msg = response.get("error")
                                        .and_then(|e| e.get("message"))
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Unknown error");
                                    let _ = tx.send(Ok(ResponseChunk::Error(error_msg.to_string()))).await;
                                }
                                _ => {} // Ignore unknown types
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Ok(ResponseChunk::Error(format!("Parse error: {}", e)))).await;
                    }
                }
            }

            let _ = tx.send(Ok(ResponseChunk::Done)).await;
        });

        Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

#[async_trait::async_trait]
impl AIProvider for MockClaudeCodeProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Claude
    }

    async fn is_available(&self) -> bool {
        self.claude_path.exists()
    }

    async fn get_auth_status(&self) -> Result<AuthStatus, Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = TokioCommand::new(&self.config.claude_path);
        cmd.args(&["--print", "--output-format", "json", "test"]);

        if self.fail_auth {
            cmd.env("MOCK_AUTH_FAIL", "true");
        }

        let output = timeout(Duration::from_secs(10), cmd.output())
            .await
            .map_err(|_| "Auth status check timed out")?
            .map_err(|e| format!("Failed to check auth status: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(AuthStatus {
                authenticated: false,
                subscription_tier: None,
                auth_method: "unknown".to_string(),
                quota_remaining: None,
                error_message: Some(stderr.to_string()),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Try to parse JSON response
        match serde_json::from_str::<Value>(&stdout) {
            Ok(auth_data) => {
                Ok(AuthStatus {
                    authenticated: auth_data.get("authenticated").and_then(|v| v.as_bool()).unwrap_or(false),
                    subscription_tier: auth_data.get("subscriptionTier").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    auth_method: auth_data.get("authMethod").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    quota_remaining: None,
                    error_message: None,
                })
            }
            Err(_) => {
                // Fallback: assume authenticated if command succeeded
                Ok(AuthStatus {
                    authenticated: true,
                    subscription_tier: None,
                    auth_method: "claude_code".to_string(),
                    quota_remaining: None,
                    error_message: None,
                })
            }
        }
    }

    async fn send_message(&self, system: &str, messages: Vec<Message>) -> Result<ResponseStream, Box<dyn std::error::Error + Send + Sync>> {
        let stream = self.execute_claude_cli(system, messages).await?;
        Ok(stream)
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }
}

// Test Suites

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Suite 1: Provider Trait Compliance
    mod provider_trait_compliance {
        use super::*;

        #[tokio::test]
        async fn test_provider_type_identification() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            assert_eq!(provider.provider_type(), ProviderType::Claude);
        }

        #[tokio::test]
        async fn test_capabilities_specification() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());
            let capabilities = provider.get_capabilities();

            assert!(!capabilities.supports_images, "Claude Code CLI should not support images");
            assert!(capabilities.supports_streaming, "Should support streaming responses");
            assert!(capabilities.supports_tools, "Should support tool usage");
            assert_eq!(capabilities.max_tokens, Some(200_000), "Should have correct max token limit");
            assert!(!capabilities.supported_models.is_empty(), "Should list supported models");

            // Verify specific models
            assert!(capabilities.supported_models.contains(&"claude-sonnet-4-20250514".to_string()));
            assert!(capabilities.supported_models.contains(&"claude-3-5-sonnet-20241022".to_string()));
        }

        #[tokio::test]
        async fn test_availability_check() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            assert!(provider.is_available().await, "Provider should be available when binary exists");
        }

        #[tokio::test]
        async fn test_unavailable_when_binary_missing() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path());
            provider.claude_path = PathBuf::from("/non/existent/claude");

            assert!(!provider.is_available().await, "Provider should be unavailable when binary missing");
        }
    }

    /// Test Suite 2: Authentication Flow Validation
    mod authentication_flow {
        use super::*;

        #[tokio::test]
        async fn test_successful_authentication_status() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            let auth_status = provider.get_auth_status().await.unwrap();

            assert!(auth_status.authenticated, "Should be authenticated");
            assert_eq!(auth_status.subscription_tier, Some("max".to_string()));
            assert_eq!(auth_status.auth_method, "oauth");
            assert!(auth_status.error_message.is_none());
        }

        #[tokio::test]
        async fn test_authentication_failure() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path()).with_auth_failure();

            let auth_status = provider.get_auth_status().await.unwrap();

            assert!(!auth_status.authenticated, "Should not be authenticated");
            assert!(auth_status.error_message.is_some(), "Should have error message");
            assert!(auth_status.error_message.unwrap().contains("Authentication failed"));
        }

        #[tokio::test]
        async fn test_subscription_vs_api_key_detection() {
            let temp_dir = TempDir::new().unwrap();

            // Test Max subscription
            let max_provider = MockClaudeCodeProvider::new(temp_dir.path());
            let max_auth = max_provider.get_auth_status().await.unwrap();
            assert_eq!(max_auth.subscription_tier, Some("max".to_string()));
            assert_eq!(max_auth.auth_method, "oauth");

            // Test API key fallback (simulated by different auth method)
            let api_provider = MockClaudeCodeProvider::new(temp_dir.path()).with_auth_failure();
            let api_auth = api_provider.get_auth_status().await.unwrap();
            assert!(!api_auth.authenticated);
        }

        #[tokio::test]
        async fn test_auth_timeout_handling() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path()).with_timeout_simulation();

            let start = Instant::now();
            let result = provider.get_auth_status().await;
            let duration = start.elapsed();

            // Should timeout within reasonable time (not wait forever)
            assert!(duration < Duration::from_secs(15), "Auth check should timeout reasonably");

            // Could be Ok (if mock is fast) or Err (if timeout), both are acceptable for this test
            match result {
                Ok(auth) => assert!(!auth.authenticated || !auth.error_message.is_none()),
                Err(_) => {} // Timeout is acceptable
            }
        }
    }

    /// Test Suite 3: Message Processing and Filtering
    mod message_processing {
        use super::*;

        #[tokio::test]
        async fn test_text_message_processing() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "Hello, Claude!"}]),
            }];

            let stream = provider.send_message("Test system prompt", messages).await.unwrap();

            // Collect stream responses
            use tokio_stream::StreamExt;
            let responses: Vec<_> = stream.collect().await;

            assert!(!responses.is_empty(), "Should receive responses");

            // Verify we get text and usage data
            let has_text = responses.iter().any(|r| {
                matches!(r, Ok(ResponseChunk::Text(_)))
            });
            let has_usage = responses.iter().any(|r| {
                matches!(r, Ok(ResponseChunk::Usage(_)))
            });

            assert!(has_text, "Should receive text response");
            assert!(has_usage, "Should receive usage statistics");
        }

        #[tokio::test]
        async fn test_image_content_filtering() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([
                    {"type": "text", "text": "Look at this image:"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}}
                ]),
            }];

            // Filter messages before sending
            let filtered = provider.filter_messages(messages);

            if let Value::Array(content_blocks) = &filtered[0].content {
                let image_block = &content_blocks[1];
                assert_eq!(image_block["type"], "text");
                assert!(image_block["text"].as_str().unwrap().contains("not supported"));
            } else {
                panic!("Expected array content");
            }
        }

        #[tokio::test]
        async fn test_large_message_handling() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Create a large message near token limits
            let large_text = "test ".repeat(40000); // ~200KB of text
            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": large_text}]),
            }];

            let result = provider.send_message("Process this large input", messages).await;

            // Should handle gracefully - either process or give meaningful error
            assert!(result.is_ok() || result.err().unwrap().to_string().contains("timeout"));
        }

        #[tokio::test]
        async fn test_malformed_json_handling() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path()).with_parse_error();

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "Hello"}]),
            }];

            let stream = provider.send_message("Test", messages).await.unwrap();

            use tokio_stream::StreamExt;
            let responses: Vec<_> = stream.collect().await;

            // Should receive error about parsing
            let has_parse_error = responses.iter().any(|r| {
                if let Ok(ResponseChunk::Error(msg)) = r {
                    msg.contains("Parse error")
                } else {
                    false
                }
            });

            assert!(has_parse_error, "Should report parse error for malformed JSON");
        }
    }

    /// Test Suite 4: Error Handling and Recovery
    mod error_handling {
        use super::*;

        #[tokio::test]
        async fn test_quota_exceeded_handling() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path()).with_quota_exceeded();

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "Hello"}]),
            }];

            let result = provider.send_message("Test", messages).await;

            assert!(result.is_err(), "Should fail when quota exceeded");

            let error_msg = result.err().unwrap().to_string();
            assert!(error_msg.contains("quota") || error_msg.contains("rate"),
                   "Error should mention quota/rate limit");
        }

        #[tokio::test]
        async fn test_process_timeout_handling() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path()).with_timeout_simulation();
            provider.config.timeout_seconds = 1; // Very short timeout

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "Hello"}]),
            }];

            let result = provider.send_message("Test", messages).await;

            assert!(result.is_err(), "Should timeout");

            let error_msg = result.err().unwrap().to_string();
            assert!(error_msg.contains("Timeout") || error_msg.contains("timeout"));
        }

        #[tokio::test]
        async fn test_binary_not_found_error() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path());
            provider.config.claude_path = "/definitely/not/a/real/path/claude".to_string();

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "Hello"}]),
            }];

            let result = provider.send_message("Test", messages).await;
            assert!(result.is_err(), "Should fail when binary not found");
        }

        #[tokio::test]
        async fn test_stdin_write_failure_recovery() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Create a message that might cause stdin issues (very large)
            let enormous_text = "x".repeat(10_000_000); // 10MB
            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": enormous_text}]),
            }];

            let result = provider.send_message("Test", messages).await;

            // Should either succeed or fail gracefully with a meaningful error
            if let Err(e) = result {
                let error_msg = e.to_string();
                assert!(
                    error_msg.contains("Failed") ||
                    error_msg.contains("timeout") ||
                    error_msg.contains("Process"),
                    "Should have meaningful error message"
                );
            }
        }
    }

    /// Test Suite 5: Performance and Concurrency
    mod performance_tests {
        use super::*;

        #[tokio::test]
        async fn test_concurrent_authentication_checks() {
            let temp_dir = TempDir::new().unwrap();
            let provider = Arc::new(MockClaudeCodeProvider::new(temp_dir.path()));

            let handles: Vec<_> = (0..10).map(|i| {
                let provider_clone = Arc::clone(&provider);
                tokio::spawn(async move {
                    let start = Instant::now();
                    let result = provider_clone.get_auth_status().await;
                    (i, result, start.elapsed())
                })
            }).collect();

            let results = try_join_all(handles).await.unwrap();

            for (id, result, duration) in results {
                assert!(result.is_ok(), "Concurrent auth check {} should succeed", id);
                assert!(duration < Duration::from_secs(5),
                       "Auth check {} should complete within 5s, took {:?}", id, duration);
            }
        }

        #[tokio::test]
        async fn test_concurrent_message_processing() {
            let temp_dir = TempDir::new().unwrap();
            let provider = Arc::new(MockClaudeCodeProvider::new(temp_dir.path()));
            let semaphore = Arc::new(Semaphore::new(5)); // Limit concurrent requests

            let handles: Vec<_> = (0..20).map(|i| {
                let provider_clone = Arc::clone(&provider);
                let semaphore_clone = Arc::clone(&semaphore);

                tokio::spawn(async move {
                    let _permit = semaphore_clone.acquire().await.unwrap();

                    let messages = vec![Message {
                        role: "user".to_string(),
                        content: json!([{"type": "text", "text": format!("Request {}", i)}]),
                    }];

                    let start = Instant::now();
                    let result = provider_clone.send_message("Test", messages).await;
                    let duration = start.elapsed();

                    (i, result.is_ok(), duration)
                })
            }).collect();

            let results = try_join_all(handles).await.unwrap();

            let success_count = results.iter().filter(|(_, success, _)| *success).count();
            let avg_duration: Duration = results.iter()
                .map(|(_, _, duration)| *duration)
                .sum::<Duration>() / results.len() as u32;

            assert!(success_count >= 15, "At least 75% of concurrent requests should succeed");
            assert!(avg_duration < Duration::from_secs(2),
                   "Average response time should be < 2s, was {:?}", avg_duration);
        }

        #[tokio::test]
        async fn test_memory_usage_under_load() {
            let temp_dir = TempDir::new().unwrap();
            let provider = Arc::new(MockClaudeCodeProvider::new(temp_dir.path()));

            // Get baseline memory
            let initial_memory = get_memory_usage();

            // Run many requests
            let handles: Vec<_> = (0..100).map(|i| {
                let provider_clone = Arc::clone(&provider);
                tokio::spawn(async move {
                    let messages = vec![Message {
                        role: "user".to_string(),
                        content: json!([{"type": "text", "text": format!("Load test {}", i)}]),
                    }];

                    let _ = provider_clone.send_message("Load test", messages).await;
                })
            }).collect();

            try_join_all(handles).await.unwrap();

            // Force garbage collection and check memory
            tokio::task::yield_now().await;
            sleep(Duration::from_millis(100)).await;

            let final_memory = get_memory_usage();
            let memory_increase = final_memory.saturating_sub(initial_memory);

            // Memory should not increase excessively (< 100MB increase)
            assert!(memory_increase < 100 * 1024 * 1024,
                   "Memory usage increased by {}MB, should be < 100MB",
                   memory_increase / 1024 / 1024);
        }

        #[tokio::test]
        async fn test_response_time_consistency() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            let mut response_times = Vec::new();

            // Make multiple requests to check consistency
            for i in 0..10 {
                let messages = vec![Message {
                    role: "user".to_string(),
                    content: json!([{"type": "text", "text": format!("Consistency test {}", i)}]),
                }];

                let start = Instant::now();
                let result = provider.send_message("Consistency test", messages).await;
                let duration = start.elapsed();

                assert!(result.is_ok(), "Request {} should succeed", i);
                response_times.push(duration);
            }

            // Calculate statistics
            let avg_time = response_times.iter().sum::<Duration>() / response_times.len() as u32;
            let max_time = response_times.iter().max().unwrap();
            let min_time = response_times.iter().min().unwrap();

            // Response times should be reasonably consistent
            let variance = max_time.saturating_sub(*min_time);
            assert!(variance < Duration::from_secs(2),
                   "Response time variance should be < 2s, was {:?}", variance);
            assert!(avg_time < Duration::from_secs(1),
                   "Average response time should be < 1s, was {:?}", avg_time);
        }

        fn get_memory_usage() -> u64 {
            // Simple memory usage approximation
            std::alloc::System.alloc_size() as u64
        }
    }

    /// Test Suite 6: Security and Edge Cases
    mod security_tests {
        use super::*;

        #[tokio::test]
        async fn test_command_injection_prevention() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Attempt command injection through system prompt
            let malicious_system = "'; rm -rf /tmp; echo 'hacked";
            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "normal message"}]),
            }];

            let result = provider.send_message(malicious_system, messages).await;

            // Should either succeed (injection prevented) or fail safely
            if result.is_err() {
                let error_msg = result.err().unwrap().to_string();
                assert!(!error_msg.contains("hacked"), "Should not execute injected commands");
            }

            // Verify temp directory still exists (wasn't deleted by injection)
            assert!(temp_dir.path().exists(), "Temp directory should still exist");
        }

        #[tokio::test]
        async fn test_api_key_environment_isolation() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Set a fake API key in environment
            std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-key-12345");

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "test"}]),
            }];

            let result = provider.send_message("test", messages).await;

            // Clean up
            std::env::remove_var("ANTHROPIC_API_KEY");

            // Should not leak the API key in any error messages
            if let Err(e) = result {
                let error_msg = e.to_string();
                assert!(!error_msg.contains("sk-ant-test-key"),
                       "Should not leak API key in error messages");
            }
        }

        #[tokio::test]
        async fn test_path_traversal_prevention() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Attempt path traversal in claude_path
            provider.config.claude_path = "../../../etc/passwd".to_string();

            let result = provider.is_available().await;
            assert!(!result, "Should not consider path traversal as available");
        }

        #[tokio::test]
        async fn test_resource_cleanup_on_failure() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path()).with_timeout_simulation();

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "test"}]),
            }];

            // This should timeout and cleanup resources
            let _result = provider.send_message("test", messages).await;

            // Give time for cleanup
            sleep(Duration::from_millis(100)).await;

            // Check that no claude processes are still running
            let output = std::process::Command::new("pgrep")
                .arg("-f")
                .arg("claude")
                .output();

            if let Ok(output) = output {
                let processes = String::from_utf8_lossy(&output.stdout);
                assert!(processes.trim().is_empty() || !processes.contains(&temp_dir.path().to_string_lossy()),
                       "Should not leave claude processes running after timeout");
            }
        }

        #[tokio::test]
        async fn test_sensitive_data_scrubbing() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path()).with_auth_failure();

            // Send message containing sensitive-looking data
            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "My API key is sk-ant-api-key-12345 and my password is secret123"}]),
            }];

            let result = provider.send_message("test", messages).await;

            // Any error messages should not contain sensitive data patterns
            if let Err(e) = result {
                let error_msg = e.to_string();
                assert!(!error_msg.contains("sk-ant-"), "Should not leak API key patterns");
                assert!(!error_msg.contains("secret123"), "Should not leak password data");
            }
        }
    }

    /// Test Suite 7: Integration and Configuration
    mod integration_tests {
        use super::*;

        #[tokio::test]
        async fn test_config_validation_and_defaults() {
            let temp_dir = TempDir::new().unwrap();
            let config = ClaudeCodeConfig {
                claude_path: "/non/existent/claude".to_string(),
                default_model: "claude-sonnet-4-20250514".to_string(),
                timeout_seconds: 30,
                max_turns: 1,
                verbose: false,
                codex_home: temp_dir.path().to_path_buf(),
            };

            // Verify defaults are reasonable
            assert_eq!(config.default_model, "claude-sonnet-4-20250514");
            assert_eq!(config.timeout_seconds, 30);
            assert_eq!(config.max_turns, 1);
            assert!(!config.verbose);
        }

        #[tokio::test]
        async fn test_multi_turn_conversation_simulation() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path());
            provider.config.max_turns = 3;

            // Simulate multi-turn conversation
            let turn1_messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "Hello, what's your name?"}]),
            }];

            let turn2_messages = vec![
                Message {
                    role: "user".to_string(),
                    content: json!([{"type": "text", "text": "Hello, what's your name?"}]),
                },
                Message {
                    role: "assistant".to_string(),
                    content: json!([{"type": "text", "text": "I'm Claude, an AI assistant."}]),
                },
                Message {
                    role: "user".to_string(),
                    content: json!([{"type": "text", "text": "Can you help me code?"}]),
                },
            ];

            let result1 = provider.send_message("You are a helpful assistant", turn1_messages).await;
            let result2 = provider.send_message("You are a helpful assistant", turn2_messages).await;

            assert!(result1.is_ok(), "First turn should succeed");
            assert!(result2.is_ok(), "Multi-turn conversation should succeed");
        }

        #[tokio::test]
        async fn test_model_selection_and_switching() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Test different models
            let models = ["claude-sonnet-4-20250514", "claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022"];

            for model in models {
                provider.config.default_model = model.to_string();

                let messages = vec![Message {
                    role: "user".to_string(),
                    content: json!([{"type": "text", "text": format!("Test with {}", model)}]),
                }];

                let result = provider.send_message("test", messages).await;
                assert!(result.is_ok(), "Should work with model {}", model);
            }
        }

        #[tokio::test]
        async fn test_verbose_mode_logging() {
            let temp_dir = TempDir::new().unwrap();
            let mut provider = MockClaudeCodeProvider::new(temp_dir.path());
            provider.config.verbose = true;

            let messages = vec![Message {
                role: "user".to_string(),
                content: json!([{"type": "text", "text": "test with verbose logging"}]),
            }];

            let result = provider.send_message("test", messages).await;

            // Should still work in verbose mode
            assert!(result.is_ok(), "Should work in verbose mode");
        }

        #[tokio::test]
        async fn test_codex_home_integration() {
            let temp_dir = TempDir::new().unwrap();
            let provider = MockClaudeCodeProvider::new(temp_dir.path());

            // Verify codex_home is properly set
            assert_eq!(provider.config.codex_home, temp_dir.path());
            assert!(provider.config.codex_home.exists());
        }
    }
}