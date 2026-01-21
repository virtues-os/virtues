//! LLM Client Implementation
//!
//! Provides HTTP client for Tollbooth API proxy (OpenAI-compatible API)
//! Tollbooth handles routing to providers via litellm-rs with budget enforcement.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

use crate::tollbooth;

/// LLM request structure
#[derive(Debug, Clone, Serialize)]
pub struct LLMRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

/// LLM response structure
#[derive(Debug, Clone, Deserialize)]
pub struct LLMResponse {
    pub content: String,
    pub model: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// LLM client trait for abstraction
#[async_trait]
pub trait LLMClient: Send + Sync {
    async fn generate(&self, request: LLMRequest) -> Result<LLMResponse, String>;
}

/// Tollbooth client (OpenAI-compatible API with budget enforcement)
/// Routes to providers via litellm-rs: Anthropic, OpenAI, Google, etc.
#[derive(Clone)]
pub struct TollboothClient {
    secret: String,
    user_id: String,
    client: reqwest::Client,
    base_url: String,
}

impl TollboothClient {
    /// Create a new Tollbooth client from environment
    /// Uses system user ID for background operations (no specific user context)
    pub fn from_env() -> Result<Self, String> {
        let secret = env::var("TOLLBOOTH_INTERNAL_SECRET")
            .map_err(|_| "TOLLBOOTH_INTERNAL_SECRET not set in environment".to_string())?;
        let base_url = env::var("TOLLBOOTH_URL").unwrap_or_else(|_| {
            tracing::warn!("TOLLBOOTH_URL not set, using default localhost:9002");
            "http://localhost:9002".to_string()
        });

        // Validate secret length
        tollbooth::validate_secret(&secret).map_err(|e| e.to_string())?;

        Ok(Self {
            secret,
            user_id: tollbooth::SYSTEM_USER_ID.to_string(),
            client: crate::http_client::tollbooth_client(),
            base_url,
        })
    }

    /// Create a new Tollbooth client with explicit secret and user ID
    pub fn new(secret: String, user_id: String) -> Self {
        Self {
            secret,
            user_id,
            client: crate::http_client::tollbooth_client(),
            base_url: "http://localhost:9002".to_string(),
        }
    }

    /// Create with custom base URL (for testing or different deployment)
    pub fn with_base_url(secret: String, user_id: String, base_url: String) -> Self {
        Self {
            secret,
            user_id,
            client: crate::http_client::tollbooth_client(),
            base_url,
        }
    }
}

// OpenAI-compatible API structures
#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    object: String,
    #[allow(dead_code)]
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: OpenAIUsage,
}

#[derive(Deserialize)]
struct Choice {
    #[allow(dead_code)]
    index: u32,
    message: Message,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[async_trait]
impl LLMClient for TollboothClient {
    async fn generate(&self, request: LLMRequest) -> Result<LLMResponse, String> {
        // Build messages array
        let mut messages = Vec::new();

        // Add system message if provided
        if let Some(system) = request.system {
            messages.push(Message {
                role: "system".to_string(),
                content: system,
            });
        }

        // Add user message
        messages.push(Message {
            role: "user".to_string(),
            content: request.prompt,
        });

        let chat_request = ChatCompletionRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: false,
        };

        let response = tollbooth::with_tollbooth_auth(
            self.client
                .post(format!("{}/v1/chat/completions", self.base_url)),
            &self.user_id,
            &self.secret,
        )
        .header("Content-Type", "application/json")
        .json(&chat_request)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            return Err(format!("Tollbooth API error ({}): {}", status, error_text));
        }

        let chat_response: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Extract content from first choice
        let content = chat_response
            .choices
            .first()
            .ok_or_else(|| "No choices in response".to_string())?
            .message
            .content
            .clone();

        Ok(LLMResponse {
            content,
            model: chat_response.model,
            usage: Usage {
                input_tokens: chat_response.usage.prompt_tokens,
                output_tokens: chat_response.usage.completion_tokens,
            },
        })
    }
}

/// Alias for backwards compatibility
pub type AIGatewayClient = TollboothClient;

#[cfg(test)]
mod tests {
    use super::*;

    // Test secret that meets minimum length requirement (32 chars)
    const TEST_SECRET: &str = "this-is-a-test-secret-32-chars!";

    #[test]
    fn test_tollbooth_client_creation() {
        env::set_var("TOLLBOOTH_INTERNAL_SECRET", TEST_SECRET);
        env::set_var("TOLLBOOTH_URL", "http://localhost:9002");
        let client = TollboothClient::from_env();
        assert!(client.is_ok());
    }

    #[test]
    fn test_missing_secret() {
        env::remove_var("TOLLBOOTH_INTERNAL_SECRET");
        let client = TollboothClient::from_env();
        assert!(client.is_err());
    }

    #[test]
    fn test_custom_base_url() {
        let client = TollboothClient::with_base_url(
            TEST_SECRET.to_string(),
            "test-user".to_string(),
            "https://tollbooth.example.com".to_string(),
        );
        assert_eq!(client.base_url, "https://tollbooth.example.com");
    }
}
