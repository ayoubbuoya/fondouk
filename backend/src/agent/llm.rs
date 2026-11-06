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
    ChatCompletionRequestUserMessageContent, CreateChatCompletionRequest,
    CreateChatCompletionRequestArgs, CreateChatCompletionResponse, FinishReason, ReasoningEffort,
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
        let request = build_request(&self.model, system, user)?;
        let response = tokio::time::timeout(CALL_TIMEOUT, self.client.chat().create(request))
            .await
            .context("LLM call timed out")?
            .context("LLM call failed")?;
        reply_text(response)
    }
}

/// Builds the chat request for one decision.
///
/// `reasoning_effort: "none"` (D64): reasoning models otherwise spend the
/// whole `MAX_COMPLETION_TOKENS` budget thinking and return no `content`.
/// It is the standard OpenAI field; OpenRouter accepts it at the top level.
fn build_request(model: &str, system: &str, user: &str) -> Result<CreateChatCompletionRequest> {
    let messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessage::from(ChatCompletionRequestSystemMessageContent::from(
            system,
        ))
        .into(),
        ChatCompletionRequestUserMessage::from(ChatCompletionRequestUserMessageContent::from(user))
            .into(),
    ];
    CreateChatCompletionRequestArgs::default()
        .model(model.to_string())
        .messages(messages)
        .temperature(TEMPERATURE)
        .max_completion_tokens(MAX_COMPLETION_TOKENS)
        .reasoning_effort(ReasoningEffort::None)
        .build()
        .context("could not build the chat request")
}

/// Returns the text of the first choice.
///
/// When there is no text, the error names the `finish_reason` (D64), so the
/// log and `decisions.error_detail` say *why* (e.g. `length` = token cap hit).
/// It never includes model output, which may echo the private persona.
fn reply_text(response: CreateChatCompletionResponse) -> Result<String> {
    let Some(choice) = response.choices.into_iter().next() else {
        bail!("LLM reply has no text content (no choices)");
    };
    match choice.message.content {
        Some(text) => Ok(text),
        None => bail!(
            "LLM reply has no text content (finish_reason={})",
            finish_reason_name(choice.finish_reason.as_ref())
        ),
    }
}

/// The API's own wire name, so the message matches provider docs.
fn finish_reason_name(reason: Option<&FinishReason>) -> &'static str {
    match reason {
        Some(FinishReason::Stop) => "stop",
        Some(FinishReason::Length) => "length",
        Some(FinishReason::ToolCalls) => "tool_calls",
        Some(FinishReason::ContentFilter) => "content_filter",
        Some(FinishReason::FunctionCall) => "function_call",
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{Value, json};

    use super::*;

    /// Last request body the fake server received.
    type Captured = Arc<Mutex<Option<Value>>>;

    /// A fake OpenAI-compatible server that answers with `message` and
    /// `finish_reason`, and records the request body. Returns the base URL.
    async fn fake_llm(message: Value, finish_reason: &str, captured: Captured) -> String {
        let finish_reason = finish_reason.to_string();
        let app = Router::new()
            .route(
                "/chat/completions",
                post(
                    |State((message, finish_reason, captured)): State<(Value, String, Captured)>,
                     Json(body): Json<Value>| async move {
                        *captured.lock().unwrap() = Some(body);
                        Json(json!({
                            "id": "chatcmpl-test",
                            "object": "chat.completion",
                            "created": 0,
                            "model": "fake-model",
                            "choices": [{
                                "index": 0,
                                "message": message,
                                "finish_reason": finish_reason
                            }]
                        }))
                    },
                ),
            )
            .with_state((message, finish_reason, captured));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn client(base_url: &str) -> LlmClient {
        LlmClient::new(&LlmConfig {
            base_url: base_url.to_string(),
            api_key: "test-key".to_string(),
            model: "fake-model".to_string(),
        })
    }

    #[tokio::test]
    async fn request_turns_reasoning_off() {
        let captured = Captured::default();
        let message = json!({ "role": "assistant", "content": "ok" });
        let url = fake_llm(message, "stop", captured.clone()).await;

        client(&url).complete("system", "user").await.unwrap();

        let body = captured.lock().unwrap().clone().expect("no request seen");
        assert_eq!(body["reasoning_effort"], "none");
        assert_eq!(body["max_completion_tokens"], MAX_COMPLETION_TOKENS);
    }

    #[tokio::test]
    async fn empty_content_error_names_finish_reason() {
        // What a reasoning model returns when the token cap runs out mid-thought.
        let message = json!({ "role": "assistant", "content": null });
        let url = fake_llm(message, "length", Captured::default()).await;

        let err = client(&url).complete("system", "user").await.unwrap_err();

        assert_eq!(
            err.to_string(),
            "LLM reply has no text content (finish_reason=length)"
        );
    }

    #[tokio::test]
    async fn normal_reply_returns_text() {
        let message = json!({ "role": "assistant", "content": "{\"action\":\"HOLD\"}" });
        let url = fake_llm(message, "stop", Captured::default()).await;

        let text = client(&url).complete("system", "user").await.unwrap();

        assert_eq!(text, "{\"action\":\"HOLD\"}");
    }
}
