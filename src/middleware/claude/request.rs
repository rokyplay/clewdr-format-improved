use std::{
    hash::{DefaultHasher, Hash, Hasher},
    mem,
    sync::LazyLock,
    vec,
};

use axum::{
    Json,
    extract::{FromRequest, Request},
};
use http::header::USER_AGENT;
use serde_json::{Value, json};

use crate::{
    config::CLEWDR_CONFIG,
    error::ClewdrError,
    format::{
        analyze_conversation_state, clean_cache_control_from_messages, clear_thought_signature,
        extract_signatures, get_thought_signature, has_valid_signature_for_function_calls,
        message_has_tool_result, needs_thinking_recovery, process_image_blocks,
        should_disable_thinking_due_to_history, strip_invalid_thinking_blocks,
    },
    middleware::claude::{ClaudeApiFormat, ClaudeContext},
    types::{
        claude::{
            ContentBlock, CreateMessageParams, Message, MessageContent, Role, Thinking, Usage,
        },
        oai::OaiCreateMessageParams,
    },
};

/// A custom extractor that unifies different API formats
///
/// This extractor processes incoming requests, handling differences between
/// Claude and OpenAI API formats, and applies preprocessing to ensure consistent
/// handling throughout the application. It also detects and handles test messages
/// from client applications.
///
/// # Functionality
///
/// - Extracts and normalizes message parameters from different API formats
/// - Detects and processes "thinking mode" requests by modifying model names
/// - Identifies test messages and handles them appropriately
/// - Attempts to retrieve responses from cache before processing requests
/// - Provides format information via the FormatInfo extension
pub struct ClaudeWebPreprocess(pub CreateMessageParams, pub ClaudeContext);

/// Contains information about the API format and streaming status
///
/// This structure is passed through the request pipeline to inform
/// handlers and response processors about the API format being used
/// and whether the response should be streamed.
#[derive(Debug, Clone)]
pub struct ClaudeWebContext {
    /// Whether the response should be streamed
    pub(super) stream: bool,
    /// The API format being used (Claude or OpenAI)
    pub(super) api_format: ClaudeApiFormat,
    /// The stop sequence used for the request
    pub(super) stop_sequences: Vec<String>,
    /// User information about input and output tokens
    pub(super) usage: Usage,
}

/// Predefined test message in Claude format for connection testing
///
/// This is a standard test message sent by clients like SillyTavern
/// to verify connectivity. The system detects these messages and
/// responds with a predefined test response to confirm service availability.
static TEST_MESSAGE_CLAUDE: LazyLock<Message> = LazyLock::new(|| {
    Message::new_blocks(
        Role::User,
        vec![ContentBlock::Text {
            text: "Hi".to_string(),
            cache_control: None,
        }],
    )
});

/// Predefined test message in OpenAI format for connection testing
static TEST_MESSAGE_OAI: LazyLock<Message> = LazyLock::new(|| Message::new_text(Role::User, "Hi"));

struct NormalizeRequest(CreateMessageParams, ClaudeApiFormat);

fn sanitize_messages(msgs: Vec<Message>) -> Vec<Message> {
    msgs.into_iter()
        .filter_map(|m| {
            let role = m.role;
            let content = match m.content {
                MessageContent::Text { content } => {
                    let trimmed = content.trim().to_string();
                    if role == Role::Assistant && trimmed.is_empty() {
                        return None;
                    }
                    MessageContent::Text { content: trimmed }
                }
                MessageContent::Blocks { content } => {
                    let mut new_blocks: Vec<ContentBlock> = content
                        .into_iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text, .. } => {
                                let t = text.trim().to_string();
                                if t.is_empty() {
                                    None
                                } else {
                                    Some(ContentBlock::Text { text: t, cache_control: None })
                                }
                            }
                            other => Some(other),
                        })
                        .collect();
                    if role == Role::Assistant && new_blocks.is_empty() {
                        return None;
                    }
                    MessageContent::Blocks {
                        content: mem::take(&mut new_blocks),
                    }
                }
            };
            Some(Message { role, content })
        })
        .collect()
}

impl<S> FromRequest<S> for NormalizeRequest
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let uri = req.uri().to_string();
        let format = if uri.contains("chat/completions") {
            ClaudeApiFormat::OpenAI
        } else {
            ClaudeApiFormat::Claude
        };
        
        // Extract raw bytes first for debugging
        let bytes = axum::body::Bytes::from_request(req, &()).await
            .map_err(|e| ClewdrError::InternalError { msg: format!("Failed to read body: {e}") })?;
        
        // Parse JSON based on format
        let Json(mut body) = match format {
            ClaudeApiFormat::OpenAI => {
                match serde_json::from_slice::<OaiCreateMessageParams>(&bytes) {
                    Ok(json) => Json(json.into()),
                    Err(e) => {
                        // Save raw request for debugging
                        let debug_path = "log/debug_raw_request.json";
                        if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            let _ = std::fs::write(debug_path, serde_json::to_string_pretty(&json_value).unwrap_or_default());
                            tracing::error!("[DEBUG] Saved raw request to {} - Error: {}", debug_path, e);
                        } else {
                            let _ = std::fs::write(debug_path, &bytes);
                            tracing::error!("[DEBUG] Saved raw bytes to {} - Parse error: {}", debug_path, e);
                        }
                        return Err(ClewdrError::DeserializeError { msg: format!("Failed to deserialize the JSON body into the target type: {e}") });
                    }
                }
            }
            ClaudeApiFormat::Claude => {
                match serde_json::from_slice::<CreateMessageParams>(&bytes) {
                    Ok(json) => Json(json),
                    Err(e) => {
                        let debug_path = "log/debug_raw_request.json";
                        if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            let _ = std::fs::write(debug_path, serde_json::to_string_pretty(&json_value).unwrap_or_default());
                            tracing::error!("[DEBUG] Saved raw request to {} - Error: {}", debug_path, e);
                        } else {
                            let _ = std::fs::write(debug_path, &bytes);
                            tracing::error!("[DEBUG] Saved raw bytes to {} - Parse error: {}", debug_path, e);
                        }
                        return Err(ClewdrError::DeserializeError { msg: format!("Failed to deserialize the JSON body into the target type: {e}") });
                    }
                }
            }
        };
        // Sanitize messages: trim whitespace and drop whitespace-only assistant turns
        body.messages = sanitize_messages(body.messages);
        
        // Process image_url blocks in messages (OpenAI -> Claude conversion)
        body.messages = body
            .messages
            .into_iter()
            .map(|mut msg| {
                if let MessageContent::Blocks { content } = msg.content {
                    // Use process_image_blocks for conversion
                    msg.content = MessageContent::Blocks {
                        content: process_image_blocks(content),
                    };
                }
                msg
            })
            .collect();
        
        // Clean cache_control from historical messages (prevents API errors)
        clean_cache_control_from_messages(&mut body.messages);
        
        // Handle thinking mode
        if body.model.ends_with("-thinking") {
            body.model = body.model.trim_end_matches("-thinking").to_string();
            body.thinking.get_or_insert(Thinking::new(4096));
        }
        
        // Check if thinking should be disabled due to conversation history
        if body.thinking.is_some() && should_disable_thinking_due_to_history(&body.messages) {
            tracing::info!("[Format] Disabling thinking mode due to incompatible history");
            body.thinking = None;
        }
        
        // Strip invalid thinking blocks from history
        strip_invalid_thinking_blocks(&mut body.messages);
        
        // Analyze conversation state
        let state = analyze_conversation_state(&body.messages);
        if state.in_tool_loop {
            tracing::debug!("[Format] In tool loop with {} results", state.tool_result_count);
        }
        
        // Log tool result status for debugging
        if let Some(last_user) = body.messages.iter().rev().find(|m| m.role == Role::User) {
            if message_has_tool_result(last_user) {
                tracing::debug!("[Format] Last user message contains tool result");
            }
        }
        
        // Extract and log all signatures for debugging
        let signatures = extract_signatures(&body.messages);
        if !signatures.is_empty() {
            tracing::debug!("[Format] Found {} signatures in history", signatures.len());
        }
        
        // Check if thinking recovery is needed
        if body.thinking.is_some() && needs_thinking_recovery(&body.messages) {
            let global_sig = get_thought_signature();
            if has_valid_signature_for_function_calls(&body.messages, &global_sig) {
                tracing::debug!("[Format] Valid signature available for thinking recovery");
            } else {
                tracing::warn!("[Format] Thinking recovery needed but no valid signature found");
            }
        }
        
        Ok(Self(body, format))
    }
}

impl<S> FromRequest<S> for ClaudeWebPreprocess
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let NormalizeRequest(body, format) = NormalizeRequest::from_request(req, &()).await?;

        // Check for test messages and respond appropriately
        if !body.stream.unwrap_or_default()
            && (body.messages == vec![TEST_MESSAGE_CLAUDE.to_owned()]
                || body.messages == vec![TEST_MESSAGE_OAI.to_owned()])
        {
            // Respond with a test message
            return Err(ClewdrError::TestMessage);
        }

        // Determine streaming status and API format
        let stream = body.stream.unwrap_or_default();

        let input_tokens = body.count_tokens();
        let info = ClaudeWebContext {
            stream,
            api_format: format,
            stop_sequences: body.stop_sequences.to_owned().unwrap_or_default(),
            usage: Usage {
                input_tokens,
                output_tokens: 0, // Placeholder for output token count
            },
        };

        Ok(Self(body, ClaudeContext::Web(info)))
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeContext {
    /// Whether the response should be streamed
    pub(super) stream: bool,
    /// The API format being used (Claude or OpenAI)
    pub(super) api_format: ClaudeApiFormat,
    /// The hash of the system messages for caching purposes
    pub(super) system_prompt_hash: Option<u64>,
    // Usage information for the request
    pub(super) usage: Usage,
}

pub struct ClaudeCodePreprocess(pub CreateMessageParams, pub ClaudeContext);

impl<S> FromRequest<S> for ClaudeCodePreprocess
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let ua = req
            .headers()
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_lowercase();
        let is_from_cc = ua.contains("claude-code") || ua.contains("claude-cli");

        // Log incoming request info
        tracing::info!("[CLAUDE_CODE_PREPROCESS] User-Agent: {}", ua);
        tracing::info!("[CLAUDE_CODE_PREPROCESS] Is from Claude Code client: {}", is_from_cc);

        let NormalizeRequest(mut body, format) = NormalizeRequest::from_request(req, &()).await?;

        // Log the incoming request body for debugging
        if let Ok(json_str) = serde_json::to_string_pretty(&body) {
            let log_path = "log/claude_code_incoming_request.json";
            if let Err(e) = std::fs::write(log_path, &json_str) {
                tracing::warn!("[CLAUDE_CODE_PREPROCESS] Failed to write incoming request log: {}", e);
            } else {
                tracing::info!("[CLAUDE_CODE_PREPROCESS] Incoming request saved to {}", log_path);
            }
        }

        // Handle thinking mode by modifying the model name
        if (body.model.contains("opus-4-1")
            || body.model.contains("sonnet-4-5")
            || body.model.contains("opus-4-5"))
            && body.temperature.is_some()
        {
            body.top_p = None; // temperature and top_p cannot be used together in Opus-4-1
        }

        // Check for test messages and respond appropriately
        if !body.stream.unwrap_or_default()
            && (body.messages == vec![TEST_MESSAGE_CLAUDE.to_owned()]
                || body.messages == vec![TEST_MESSAGE_OAI.to_owned()])
        {
            // Respond with a test message
            return Err(ClewdrError::TestMessage);
        }

        // Determine streaming status and API format
        let stream = body.stream.unwrap_or_default();

        // Check if system prompt already contains Claude Code identifier
        // The official Claude Code system prompt contains: "You are an agent for Claude Code"
        let has_claude_code_system = match &body.system {
            Some(Value::String(s)) => s.contains("Claude Code"),
            Some(Value::Array(arr)) => arr.iter().any(|v| {
                v.get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.contains("Claude Code"))
                    .unwrap_or(false)
            }),
            _ => false,
        };

        tracing::info!("[CLAUDE_CODE_PREPROCESS] Has Claude Code system prompt: {}", has_claude_code_system);

        // Add Claude Code prelude if not already present
        // This is required for Claude Code API to work correctly
        if !has_claude_code_system {
            const PRELUDE_TEXT: &str = "You are an agent for Claude Code, Anthropic's official CLI for Claude. Given the user's message, you should use the tools available to complete the task. Do what has been asked; nothing more, nothing less. When you complete the task simply respond with a detailed writeup.";
            let prelude_blk = ContentBlock::Text {
                text: CLEWDR_CONFIG
                    .load()
                    .custom_system
                    .clone()
                    .unwrap_or_else(|| PRELUDE_TEXT.to_string()),
                cache_control: None,
            };
            tracing::info!("[CLAUDE_CODE_PREPROCESS] Injecting Claude Code prelude system prompt");
            match body.system {
                Some(Value::String(ref text)) => {
                    let text_content = ContentBlock::Text {
                        text: text.to_owned(),
                        cache_control: None,
                    };
                    body.system = Some(json!([prelude_blk, text_content]));
                }
                Some(Value::Array(ref mut a)) => {
                    a.insert(0, json!(prelude_blk));
                }
                _ => {
                    body.system = Some(json!([prelude_blk]));
                }
            }
        }

        // Log the final system prompt after processing
        if let Some(ref system) = body.system {
            let system_str = system.to_string();
            tracing::debug!("[CLAUDE_CODE_PREPROCESS] Final system prompt length: {} chars", system_str.len());
        }

        // Save the processed request (with injected system prompt) for debugging
        if let Ok(json_str) = serde_json::to_string_pretty(&body) {
            let log_path = "log/claude_code_processed_request.json";
            if let Err(e) = std::fs::write(log_path, &json_str) {
                tracing::warn!("[CLAUDE_CODE_PREPROCESS] Failed to write processed request log: {}", e);
            } else {
                tracing::info!("[CLAUDE_CODE_PREPROCESS] Processed request saved to {}", log_path);
            }
        }

        let cache_systems = body
            .system
            .as_ref()
            .ok_or(ClewdrError::BadRequest {
                msg: "Empty system prompt",
            })?
            .as_array()
            .ok_or(ClewdrError::BadRequest {
                msg: "System prompt is not an array",
            })?
            .iter()
            .filter(|s| s["cache_control"].as_object().is_some())
            .collect::<Vec<_>>();
        let system_prompt_hash = (!cache_systems.is_empty()).then(|| {
            let mut hasher = DefaultHasher::new();
            cache_systems.hash(&mut hasher);
            hasher.finish()
        });

        let input_tokens = body.count_tokens();

        let info = ClaudeCodeContext {
            stream,
            api_format: format,
            system_prompt_hash,
            usage: Usage {
                input_tokens,
                output_tokens: 0, // Placeholder for output token count
            },
        };

        Ok(Self(body, ClaudeContext::Code(info)))
    }
}
