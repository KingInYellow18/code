//! Unit Tests for Claude Code Provider Implementation
//!
//! Comprehensive test suite for provider trait compliance, message processing,
//! authentication flows, and core functionality validation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use serde_json;

// Mock types for testing (would be real imports in actual implementation)
type ProviderResult<T> = Result<T, ProviderError>;
type Message = serde_json::Value;
type Response = serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("Process execution failed: {0}")]
    ProcessFailed(String),
    #[error("Timeout occurred: {0}")]
    Timeout(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Provider trait that Claude Code implementation must comply with
pub trait AIProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports_images(&self) -> bool;
    fn supports_streaming(&self) -> bool;
    fn max_tokens(&self) -> Option<u32>;

    async fn authenticate(&self) -> ProviderResult<bool>;
    async fn send_message(&self, system: &str, messages: Vec<Message>) -> ProviderResult<Response>;
    async fn get_models(&self) -> ProviderResult<Vec<String>>;
    async fn check_quota(&self) -> ProviderResult<QuotaInfo>;
}

#[derive(Debug, Clone)]
pub struct QuotaInfo {
    pub daily_limit: Option<u64>,
    pub current_usage: u64,
    pub reset_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Claude Code Provider Implementation
pub struct ClaudeCodeProvider {
    claude_path: PathBuf,
    model_id: String,
    api_key: Option<String>,
    timeout_ms: u64,
    use_subscription: bool,
}

impl ClaudeCodeProvider {
    pub fn new(claude_path: PathBuf, model_id: String) -> Self {
        Self {
            claude_path,
            model_id,
            api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            timeout_ms: 30000,
            use_subscription: false,
        }
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_subscription(mut self, use_subscription: bool) -> Self {
        self.use_subscription = use_subscription;
        self
    }

    /// Filter messages to remove unsupported content (like images)
    fn filter_messages(&self, messages: Vec<Message>) -> Vec<Message> {
        messages.into_iter().map(|mut msg| {
            if let Some(content) = msg.get_mut("content") {
                if let Some(blocks) = content.as_array_mut() {
                    for block in blocks.iter_mut() {
                        if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                            if block_type == "image" {
                                *block = serde_json::json!({
                                    "type": "text",
                                    "text": "[Image content not supported by Claude Code CLI]"
                                });
                            }
                        }
                    }
                }
            }
            msg
        }).collect()
    }

    /// Execute Claude Code CLI command
    async fn execute_claude_cli(&self, system_prompt: &str, messages: Vec<Message>) -> ProviderResult<Response> {
        let filtered_messages = self.filter_messages(messages);

        let mut cmd = TokioCommand::new(&self.claude_path);
        cmd.args([
            "--print",
            "--output-format", "stream-json",
            "--model", &self.model_id,
            "--append-system-prompt", system_prompt,
            "--verbose"
        ]);

        if !self.use_subscription {
            if let Some(ref api_key) = self.api_key {
                cmd.env("ANTHROPIC_API_KEY", api_key);
            }
        }

        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| ProviderError::ProcessFailed(format!("Failed to spawn claude process: {}", e)))?;

        // Send messages as JSON to stdin
        if let Some(stdin) = child.stdin.take() {
            let input_json = serde_json::to_string(&filtered_messages)
                .map_err(|e| ProviderError::ParseError(format!("Failed to serialize messages: {}", e)))?;

            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            stdin.write_all(input_json.as_bytes()).await
                .map_err(|e| ProviderError::IoError(e))?;
            stdin.flush().await
                .map_err(|e| ProviderError::IoError(e))?;
            drop(stdin);
        }

        // Wait for completion with timeout
        let output = timeout(Duration::from_millis(self.timeout_ms), child.wait_with_output())
            .await
            .map_err(|_| ProviderError::Timeout(format!("Claude CLI execution timed out after {}ms", self.timeout_ms)))?
            .map_err(|e| ProviderError::ProcessFailed(format!("Claude CLI execution failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::ProcessFailed(format!("Claude CLI failed: {}", stderr)));
        }

        // Parse stdout as JSON response
        let stdout = String::from_utf8_lossy(&output.stdout);
        let response: Response = serde_json::from_str(&stdout)
            .map_err(|e| ProviderError::ParseError(format!("Failed to parse Claude response: {}", e)))?;

        Ok(response)
    }
}

impl AIProvider for ClaudeCodeProvider {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn supports_images(&self) -> bool {
        false // Claude Code CLI doesn't support images
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_tokens(&self) -> Option<u32> {
        Some(200000) // Claude's context window
    }

    async fn authenticate(&self) -> ProviderResult<bool> {
        // Check if Claude CLI is available and authenticated
        let mut cmd = TokioCommand::new(&self.claude_path);
        cmd.args(["--print", "--output-format", "json", "test"]);

        if !self.use_subscription {
            if let Some(ref api_key) = self.api_key {
                cmd.env("ANTHROPIC_API_KEY", api_key);
            }
        }

        let output = timeout(Duration::from_millis(5000), cmd.output())
            .await
            .map_err(|_| ProviderError::Timeout("Authentication check timed out".to_string()))?
            .map_err(|e| ProviderError::AuthenticationFailed(format!("Failed to check auth status: {}", e)))?;

        Ok(output.status.success())
    }

    async fn send_message(&self, system: &str, messages: Vec<Message>) -> ProviderResult<Response> {
        self.execute_claude_cli(system, messages).await
    }

    async fn get_models(&self) -> ProviderResult<Vec<String>> {
        let mut cmd = TokioCommand::new(&self.claude_path);
        cmd.args(["models", "list"]);

        let output = timeout(Duration::from_millis(10000), cmd.output())
            .await
            .map_err(|_| ProviderError::Timeout("Model list timed out".to_string()))?
            .map_err(|e| ProviderError::ProcessFailed(format!("Failed to list models: {}", e)))?;

        if !output.status.success() {
            return Err(ProviderError::ProcessFailed("Failed to list models".to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let models: Vec<String> = stdout.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .collect();

        Ok(models)
    }

    async fn check_quota(&self) -> ProviderResult<QuotaInfo> {
        let mut cmd = TokioCommand::new(&self.claude_path);
        cmd.args(["auth", "quota"]);

        if !self.use_subscription {
            if let Some(ref api_key) = self.api_key {
                cmd.env("ANTHROPIC_API_KEY", api_key);
            }
        }

        let output = timeout(Duration::from_millis(5000), cmd.output())
            .await
            .map_err(|_| ProviderError::Timeout("Quota check timed out".to_string()))?
            .map_err(|e| ProviderError::ProcessFailed(format!("Failed to check quota: {}", e)))?;

        if !output.status.success() {
            return Ok(QuotaInfo {
                daily_limit: None,
                current_usage: 0,
                reset_time: None,
            });
        }

        // Parse quota response (implementation would depend on actual Claude CLI output format)
        let stdout = String::from_utf8_lossy(&output.stdout);
        let quota_data: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|_| serde_json::json!({}));

        Ok(QuotaInfo {
            daily_limit: quota_data.get("daily_limit").and_then(|v| v.as_u64()),
            current_usage: quota_data.get("current_usage").and_then(|v| v.as_u64()).unwrap_or(0),
            reset_time: quota_data.get("reset_time").and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    fn create_mock_claude_cli() -> NamedTempFile {
        let file = NamedTempFile::new().expect("create temp file");
        let script_content = r#"#!/bin/bash
case "$1" in
    "auth")
        case "$2" in
            "status")
                exit 0
                ;;
            "quota")
                echo '{"daily_limit": 100000, "current_usage": 5000, "reset_time": "2024-01-02T00:00:00Z"}'
                exit 0
                ;;
        esac
        ;;
    "models")
        echo "claude-3-5-sonnet-20241022"
        echo "claude-3-haiku-20240307"
        exit 0
        ;;
    *)
        # Simulate Claude response
        echo '{"id": "msg_test", "type": "message", "role": "assistant", "content": [{"type": "text", "text": "Test response"}]}'
        exit 0
        ;;
esac
"#;

        fs::write(file.path(), script_content).expect("write script");
        fs::set_permissions(file.path(), std::fs::Permissions::from_mode(0o755))
            .expect("set permissions");

        file
    }

    #[tokio::test]
    async fn test_provider_trait_compliance() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        );

        // Test trait methods
        assert_eq!(provider.name(), "claude-code");
        assert!(!provider.supports_images());
        assert!(provider.supports_streaming());
        assert_eq!(provider.max_tokens(), Some(200000));
    }

    #[tokio::test]
    async fn test_authentication_flow() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        );

        let auth_result = provider.authenticate().await;
        assert!(auth_result.is_ok());
        assert!(auth_result.unwrap());
    }

    #[tokio::test]
    async fn test_message_filtering() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        );

        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [
                    {"type": "text", "text": "Hello"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}}
                ]
            })
        ];

        let filtered = provider.filter_messages(messages);
        let content = &filtered[0]["content"][1];

        assert_eq!(content["type"], "text");
        assert_eq!(content["text"], "[Image content not supported by Claude Code CLI]");
    }

    #[tokio::test]
    async fn test_model_listing() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        );

        let models = provider.get_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert!(models.contains(&"claude-3-5-sonnet-20241022".to_string()));
        assert!(models.contains(&"claude-3-haiku-20240307".to_string()));
    }

    #[tokio::test]
    async fn test_quota_checking() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        );

        let quota = provider.check_quota().await.unwrap();
        assert_eq!(quota.daily_limit, Some(100000));
        assert_eq!(quota.current_usage, 5000);
        assert!(quota.reset_time.is_some());
    }

    #[tokio::test]
    async fn test_timeout_handling() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        ).with_timeout(1); // 1ms timeout

        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [{"type": "text", "text": "Hello"}]
        })];

        let result = provider.send_message("Test system prompt", messages).await;
        assert!(result.is_err());

        if let Err(ProviderError::Timeout(_)) = result {
            // Expected timeout error
        } else {
            panic!("Expected timeout error, got: {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_subscription_vs_api_key() {
        let mock_cli = create_mock_claude_cli();

        // Test with subscription
        let subscription_provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        ).with_subscription(true);

        assert!(subscription_provider.use_subscription);

        // Test with API key
        let api_provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        ).with_subscription(false);

        assert!(!api_provider.use_subscription);
    }

    #[tokio::test]
    async fn test_error_handling() {
        // Test with non-existent Claude CLI
        let provider = ClaudeCodeProvider::new(
            PathBuf::from("/non/existent/claude"),
            "claude-3-5-sonnet-20241022".to_string()
        );

        let result = provider.authenticate().await;
        assert!(result.is_err());

        match result {
            Err(ProviderError::ProcessFailed(_)) => {},
            Err(ProviderError::AuthenticationFailed(_)) => {},
            other => panic!("Expected ProcessFailed or AuthenticationFailed, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_concurrent_requests() {
        let mock_cli = create_mock_claude_cli();
        let provider = Arc::new(ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        ));

        let mut handles = vec![];

        // Spawn 5 concurrent authentication requests
        for i in 0..5 {
            let provider_clone = Arc::clone(&provider);
            let handle = tokio::spawn(async move {
                let result = provider_clone.authenticate().await;
                (i, result)
            });
            handles.push(handle);
        }

        // Wait for all to complete
        let results = futures::future::join_all(handles).await;

        for result in results {
            let (id, auth_result) = result.unwrap();
            assert!(auth_result.is_ok(), "Request {} failed: {:?}", id, auth_result);
        }
    }

    #[tokio::test]
    async fn test_large_message_handling() {
        let mock_cli = create_mock_claude_cli();
        let provider = ClaudeCodeProvider::new(
            mock_cli.path().to_path_buf(),
            "claude-3-5-sonnet-20241022".to_string()
        );

        // Create a large message (simulating big context)
        let large_text = "test ".repeat(10000); // 50KB of text
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [{"type": "text", "text": large_text}]
        })];

        let result = provider.send_message("Process this large input", messages).await;
        // Should handle large messages without crashing
        assert!(result.is_ok() || matches!(result, Err(ProviderError::Timeout(_))));
    }
}