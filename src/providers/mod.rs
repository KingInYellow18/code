/// # Providers Module
///
/// This module provides various AI provider implementations for the unified authentication system.
/// Each provider implements a common interface while handling the specific requirements
/// of different AI services.

pub mod claude_code;

// Re-export provider types
pub use claude_code::{ClaudeCodeProvider, ClaudeCodeError, ClaudeCodeConfig, ClaudeCodeMessage};

use crate::configuration::{ProviderType, AuthConfig, UnifiedAuthManager};
use async_trait::async_trait;
use std::path::PathBuf;

/// Common interface for all AI providers
#[async_trait]
pub trait AIProvider {
    /// Get the provider type
    fn provider_type(&self) -> ProviderType;

    /// Check if the provider is available and authenticated
    async fn is_available(&self) -> bool;

    /// Get authentication status
    async fn get_auth_status(&self) -> Result<AuthStatus, Box<dyn std::error::Error + Send + Sync>>;

    /// Send a message and receive a response stream
    async fn send_message(
        &self,
        system_prompt: &str,
        messages: Vec<Message>,
    ) -> Result<ResponseStream, Box<dyn std::error::Error + Send + Sync>>;

    /// Get supported capabilities
    fn get_capabilities(&self) -> ProviderCapabilities;
}

/// Authentication status for a provider
#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub subscription_tier: Option<String>,
    pub auth_method: String,
    pub quota_remaining: Option<u64>,
    pub error_message: Option<String>,
}

/// Provider capabilities
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub supports_images: bool,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub max_tokens: Option<u64>,
    pub supported_models: Vec<String>,
}

/// Message structure for provider communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

/// Message content that can be text or mixed content
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Array(Vec<ContentBlock>),
}

/// Individual content blocks (text, image, etc.)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text(TextBlock),
    #[serde(rename = "image")]
    Image(ImageBlock),
}

/// Text content block
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextBlock {
    pub text: String,
}

/// Image content block
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageBlock {
    pub source: ImageSource,
}

/// Image source information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: Option<String>,
}

/// Response stream for streaming AI responses
pub type ResponseStream = tokio_stream::wrappers::ReceiverStream<Result<ResponseChunk, String>>;

/// Individual chunks in a response stream
#[derive(Debug, Clone)]
pub enum ResponseChunk {
    Text(String),
    Usage(UsageStats),
    Done,
    Error(String),
}

/// Usage statistics for a request
#[derive(Debug, Clone)]
pub struct UsageStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_cost_usd: f64,
}

/// Factory for creating provider instances
pub struct ProviderFactory {
    codex_home: PathBuf,
}

impl ProviderFactory {
    pub fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    /// Create a provider instance based on type
    pub async fn create_provider(&self, provider_type: ProviderType) -> Result<Box<dyn AIProvider + Send + Sync>, crate::providers::claude_code::UnifiedAuthError> {
        match provider_type {
            ProviderType::Claude => {
                let config = ClaudeCodeConfig::from_codex_home(&self.codex_home)?;
                let provider = ClaudeCodeProvider::new(config).await?;
                Ok(Box::new(provider))
            }
            ProviderType::OpenAI => {
                // TODO: Implement OpenAI provider wrapper
                Err(crate::providers::claude_code::UnifiedAuthError::ConfigError("OpenAI provider not yet implemented in new system".to_string()))
            }
        }
    }
}

/// Helper function to filter messages for providers that don't support images
pub fn filter_messages_for_text_only(messages: Vec<Message>) -> Vec<Message> {
    messages.into_iter().map(|mut msg| {
        if let MessageContent::Array(ref mut blocks) = msg.content {
            for block in blocks.iter_mut() {
                if let ContentBlock::Image(img_block) = block {
                    *block = ContentBlock::Text(TextBlock {
                        text: format!(
                            "[Image ({}): {} - not supported by this provider]",
                            img_block.source.source_type,
                            img_block.source.media_type
                        ),
                    });
                }
            }
        }
        msg
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_filtering() {
        let messages = vec![
            Message {
                role: "user".to_string(),
                content: MessageContent::Array(vec![
                    ContentBlock::Text(TextBlock {
                        text: "Hello".to_string(),
                    }),
                    ContentBlock::Image(ImageBlock {
                        source: ImageSource {
                            source_type: "base64".to_string(),
                            media_type: "image/png".to_string(),
                            data: Some("base64data".to_string()),
                        },
                    }),
                ]),
            }
        ];

        let filtered = filter_messages_for_text_only(messages);

        if let MessageContent::Array(blocks) = &filtered[0].content {
            assert_eq!(blocks.len(), 2);
            if let ContentBlock::Text(text_block) = &blocks[1] {
                assert!(text_block.text.contains("not supported"));
            } else {
                panic!("Expected text block after filtering");
            }
        } else {
            panic!("Expected array content");
        }
    }
}