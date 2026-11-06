//! A thin client for any OpenAI-compatible chat API (D16, D17): OpenRouter in
//! development, NEAR AI Cloud for the demo. One call = one chat completion.
//!
//! No retries here: `decide` retries once on its own terms (SPEC F4, parse
//! failure -> retry once -> HOLD). async-openai only retries when a retry
//! middleware is installed, and we install none.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs,
};

use crate::config::LlmConfig;

/// Low temperature: decisions should be repeatable (SPEC F4.4: 0 to 0.2).
const TEMPERATURE: f32 = 0.1;

/// One action is a small JSON object; this caps cost if a model rambles.
const MAX_COMPLETION_TOKENS: u32 = 300;

/// Upper bound for one LLM call, so a hung provider cannot stall a tick.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Chat client bound to one provider and one model.
#[derive(Clone)]
pub struct LlmClient {
    client: Client<OpenAIConfig>,
    model: String,
}

impl LlmClient {
    pub fn new(config: &LlmConfig) -> Self {
        let openai_config = OpenAIConfig::new()
            .with_api_base(config.base_url.clone())
            .with_api_key(config.api_key.clone());
        Self {
            client: Client::with_config(openai_config),
            model: config.model.clone(),
        }
    }

    /// The model id, stored with every decision and part of the cache key.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Sends one system + one user message and returns the reply text.
    ///
    /// # Errors
    /// Network or API error, timeout, or a reply without text content.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String> {
        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessage::from(
                ChatCompletionRequestSystemMessageContent::from(system),
            )
            .into(),
            ChatCompletionRequestUserMessage::from(ChatCompletionRequestUserMessageContent::from(
                user,
            ))
            .into(),
        ];
        let request = CreateChatCompletionRequestArgs::default()
            .model(self.model.clone())
            .messages(messages)
            .temperature(TEMPERATURE)
            .max_completion_tokens(MAX_COMPLETION_TOKENS)
            .build()
            .context("could not build the chat request")?;

        let response = tokio::time::timeout(CALL_TIMEOUT, self.client.chat().create(request))
            .await
            .context("LLM call timed out")?
            .context("LLM call failed")?;

        let Some(text) = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
        else {
            bail!("LLM reply has no text content");
        };
        Ok(text)
    }
}
