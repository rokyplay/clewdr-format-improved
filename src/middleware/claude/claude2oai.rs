use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::response::sse::Event;
use futures::{Stream, TryStreamExt};
use serde::Serialize;
use serde_json::{json, Value};

use crate::format::{
    extract_citations_from_search_result, extract_citations_from_tool_result,
    citations_to_annotations, merge_citations_into_text,
    remap_function_call_args, store_thought_signature, Citation,
};
use crate::types::claude::{ContentBlock, ContentBlockDelta, CreateMessageResponse, StreamEvent};

/// Represents the data structure for streaming events in OpenAI API format
/// Contains a choices array with deltas of content
#[derive(Debug, Serialize)]
struct StreamEventData {
    choices: Vec<StreamEventDelta>,
}

impl StreamEventData {
    /// Creates a new StreamEventData with the given content
    ///
    /// # Arguments
    /// * `content` - The event content to include
    ///
    /// # Returns
    /// A new StreamEventData instance with the content wrapped in choices array
    fn new(content: EventContent) -> Self {
        Self {
            choices: vec![StreamEventDelta { delta: content }],
        }
    }
}

/// Represents a delta update in a streaming response
/// Contains the content change for the current chunk
#[derive(Debug, Serialize)]
struct StreamEventDelta {
    delta: EventContent,
}

/// Content of an event, either regular content, reasoning, tool calls, or annotations
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EventContent {
    Content { content: String },
    Reasoning { reasoning_content: String },
    ToolCalls { tool_calls: Vec<ToolCallDelta> },
    Annotations { annotations: Vec<Value> },
    /// Combined content with annotations (for web search results)
    ContentWithAnnotations {
        content: String,
        annotations: Vec<Value>,
    },
}

/// Tool call delta for streaming
#[derive(Debug, Serialize, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolCallFunction,
}

/// Tool call function details
#[derive(Debug, Serialize, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// State for accumulating tool call arguments during streaming
#[derive(Debug, Clone, Default)]
struct ToolCallState {
    id: String,
    name: String,
    arguments: String,
}

/// State for accumulating web search results during streaming
#[derive(Debug, Clone, Default)]
struct WebSearchState {
    citations: Vec<Citation>,
    tool_use_id: String,
}

/// Creates an SSE event with the given content in OpenAI format
///
/// # Arguments
/// * `content` - The event content to include
///
/// # Returns
/// A formatted SSE Event ready to be sent to the client
pub fn build_event(content: EventContent) -> Event {
    let event = Event::default();
    let data = StreamEventData::new(content);
    event.json_data(data).unwrap()
}

/// Build a tool call event for OpenAI format
fn build_tool_call_event(state: &ToolCallState, index: usize) -> Event {
    // Apply parameter remapping before sending
    let mut args_value: Value = serde_json::from_str(&state.arguments).unwrap_or(json!({}));
    remap_function_call_args(&state.name, &mut args_value);
    let remapped_args = serde_json::to_string(&args_value).unwrap_or(state.arguments.clone());

    let tool_call = ToolCallDelta {
        index,
        id: state.id.clone(),
        type_: "function".to_string(),
        function: ToolCallFunction {
            name: state.name.clone(),
            arguments: remapped_args,
        },
    };
    build_event(EventContent::ToolCalls {
        tool_calls: vec![tool_call],
    })
}

/// Build an annotations event for web search results
fn build_annotations_event(citations: &[Citation]) -> Event {
    let annotations = citations_to_annotations(citations);
    build_event(EventContent::Annotations { annotations })
}

/// Transforms a Claude.ai event stream into an OpenAI-compatible event stream
///
/// Extracts content from Claude events and reformats them to match OpenAI's streaming format.
/// This function processes each event in the stream, identifying the delta content type
/// (text, thinking, or tool calls), and converting it to the appropriate OpenAI-compatible event format.
///
/// # Arguments
/// * `s` - The input stream of Claude.ai events
///
/// # Returns
/// A stream of OpenAI-compatible SSE events
///
/// # Type Parameters
/// * `I` - The input stream type
/// * `E` - The error type for the stream
pub fn transform_stream<I, E>(s: I) -> impl Stream<Item = Result<Event, E>>
where
    I: Stream<Item = Result<eventsource_stream::Event, E>>,
{
    // State for accumulating tool call arguments
    let tool_call_buffer: Arc<Mutex<HashMap<usize, ToolCallState>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let tool_call_index = Arc::new(Mutex::new(0usize));
    
    // State for accumulating web search results
    let web_search_buffer: Arc<Mutex<HashMap<usize, WebSearchState>>> =
        Arc::new(Mutex::new(HashMap::new()));

    s.try_filter_map(move |eventsource_stream::Event { data, .. }| {
        let buffer = tool_call_buffer.clone();
        let index_counter = tool_call_index.clone();
        let ws_buffer = web_search_buffer.clone();

        async move {
            let Ok(parsed) = serde_json::from_str::<StreamEvent>(&data) else {
                return Ok(None);
            };

            match parsed {
                StreamEvent::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    match content_block {
                        // Handle tool_use block start
                        ContentBlock::ToolUse { id, name, .. } => {
                            let mut buf = buffer.lock().unwrap();
                            buf.insert(
                                index,
                                ToolCallState {
                                    id,
                                    name,
                                    arguments: String::new(),
                                },
                            );
                        }
                        // Handle web_search_tool_result block start
                        ContentBlock::WebSearchToolResult { data } => {
                            let tool_use_id = data
                                .get("tool_use_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let citations = extract_citations_from_tool_result(&data);
                            let mut ws_buf = ws_buffer.lock().unwrap();
                            ws_buf.insert(
                                index,
                                WebSearchState {
                                    citations,
                                    tool_use_id,
                                },
                            );
                        }
                        // Handle search_result block start
                        ContentBlock::SearchResult { data } => {
                            let citations = extract_citations_from_search_result(&data);
                            let mut ws_buf = ws_buffer.lock().unwrap();
                            ws_buf.insert(
                                index,
                                WebSearchState {
                                    citations,
                                    tool_use_id: String::new(),
                                },
                            );
                        }
                        _ => {}
                    }
                    Ok(None)
                }
                StreamEvent::ContentBlockDelta { index, delta } => {
                    match delta {
                        ContentBlockDelta::TextDelta { text } => {
                            Ok(Some(build_event(EventContent::Content { content: text })))
                        }
                        ContentBlockDelta::ThinkingDelta { thinking } => {
                            Ok(Some(build_event(EventContent::Reasoning {
                                reasoning_content: thinking,
                            })))
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            // Accumulate tool call arguments
                            let mut buf = buffer.lock().unwrap();
                            if let Some(state) = buf.get_mut(&index) {
                                state.arguments.push_str(&partial_json);
                            }
                            Ok(None)
                        }
                        ContentBlockDelta::SignatureDelta { signature } => {
                            // Store signature to global storage for future requests
                            store_thought_signature(&signature);
                            Ok(None)
                        }
                    }
                }
                StreamEvent::ContentBlockStop { index } => {
                    // Check if this was a tool call block
                    {
                        let mut buf = buffer.lock().unwrap();
                        if let Some(state) = buf.remove(&index) {
                            // Get and increment the tool call index
                            let mut idx = index_counter.lock().unwrap();
                            let current_idx = *idx;
                            *idx += 1;
                            return Ok(Some(build_tool_call_event(&state, current_idx)));
                        }
                    }
                    
                    // Check if this was a web search block
                    {
                        let mut ws_buf = ws_buffer.lock().unwrap();
                        if let Some(state) = ws_buf.remove(&index) {
                            if !state.citations.is_empty() {
                                return Ok(Some(build_annotations_event(&state.citations)));
                            }
                        }
                    }
                    
                    Ok(None)
                }
                _ => Ok(None),
            }
        }
    })
}

/// Transforms a Claude response to OpenAI format (non-streaming)
///
/// This function converts a complete Claude API response to the OpenAI chat completion format,
/// including proper handling of tool calls and thinking blocks.
///
/// # Arguments
/// * `input` - The Claude API response
///
/// # Returns
/// A JSON Value in OpenAI chat completion format
pub fn transforms_json(input: CreateMessageResponse) -> Value {
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut all_citations: Vec<Citation> = Vec::new();

    for block in input.content.iter() {
        match block {
            ContentBlock::Text { text, .. } => {
                content_parts.push(text.clone());
            }
            ContentBlock::ToolUse {
                id,
                name,
                input,
                signature,
                ..
            } => {
                // Store signature if present
                if let Some(sig) = signature {
                    store_thought_signature(sig);
                }

                // Apply parameter remapping
                let mut remapped_input = input.clone();
                remap_function_call_args(name, &mut remapped_input);

                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(&remapped_input).unwrap_or_default()
                    }
                }));
            }
            ContentBlock::Thinking { signature, .. } => {
                // Store signature for future requests
                if let Some(sig) = signature {
                    store_thought_signature(sig);
                }
                // Note: thinking content is not included in OpenAI format
            }
            ContentBlock::WebSearchToolResult { data } => {
                // Extract citations from web search results
                let citations = extract_citations_from_tool_result(data);
                all_citations.extend(citations);
            }
            ContentBlock::SearchResult { data } => {
                // Extract citations from search results
                let citations = extract_citations_from_search_result(data);
                all_citations.extend(citations);
            }
            _ => {}
        }
    }

    // Merge citations into content if present
    let content = if all_citations.is_empty() {
        content_parts.join("")
    } else {
        let base_content = content_parts.join("");
        merge_citations_into_text(&base_content, &all_citations, None)
    };

    let usage = input.usage.as_ref().map(|u| {
        json!({
            "prompt_tokens": u.input_tokens,
            "completion_tokens": u.output_tokens,
            "total_tokens": u.input_tokens + u.output_tokens
        })
    });

    let finish_reason = match input.stop_reason {
        Some(crate::types::claude::StopReason::EndTurn) => "stop",
        Some(crate::types::claude::StopReason::MaxTokens) => "length",
        Some(crate::types::claude::StopReason::StopSequence) => "stop",
        Some(crate::types::claude::StopReason::ToolUse) => "tool_calls",
        Some(crate::types::claude::StopReason::Refusal) => "content_filter",
        None => "stop",
    };

    // Build message object
    let mut message = json!({
        "role": "assistant",
    });

    // Add content (null if empty and has tool calls)
    if content.is_empty() && !tool_calls.is_empty() {
        message["content"] = Value::Null;
    } else {
        message["content"] = json!(content);
    }

    // Add tool_calls if present
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    // Add annotations if we have citations
    if !all_citations.is_empty() {
        message["annotations"] = json!(citations_to_annotations(&all_citations));
    }

    json!({
        "id": input.id,
        "object": "chat.completion",
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "model": input.model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": usage
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::claude::{Role, StopReason, Usage};
    use serde_json::json;

    #[test]
    fn test_transforms_json_basic() {
        let response = CreateMessageResponse {
            content: vec![ContentBlock::Text {
                text: "Hello, world!".to_string(),
                cache_control: None,
            }],
            id: "msg_123".to_string(),
            model: "claude-3-opus".to_string(),
            role: Role::Assistant,
            stop_reason: Some(StopReason::EndTurn),
            stop_sequence: None,
            type_: "message".to_string(),
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
            }),
        };

        let result = transforms_json(response);

        assert_eq!(result["id"], "msg_123");
        assert_eq!(result["choices"][0]["message"]["content"], "Hello, world!");
        assert_eq!(result["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn test_transforms_json_with_tool_calls() {
        let response = CreateMessageResponse {
            content: vec![ContentBlock::ToolUse {
                id: "tool_123".to_string(),
                name: "Grep".to_string(),
                input: json!({"query": "search pattern"}),
                signature: None,
                cache_control: None,
            }],
            id: "msg_123".to_string(),
            model: "claude-3-opus".to_string(),
            role: Role::Assistant,
            stop_reason: Some(StopReason::ToolUse),
            stop_sequence: None,
            type_: "message".to_string(),
            usage: None,
        };

        let result = transforms_json(response);

        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        assert!(result["choices"][0]["message"]["tool_calls"].is_array());
        
        // Check parameter remapping (query -> pattern)
        let tool_call = &result["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tool_call["id"], "tool_123");
        assert_eq!(tool_call["function"]["name"], "Grep");
        
        let args: Value = serde_json::from_str(
            tool_call["function"]["arguments"].as_str().unwrap()
        ).unwrap();
        assert!(args.get("pattern").is_some());
        assert!(args.get("query").is_none());
    }

    #[test]
    fn test_stop_reason_mapping() {
        let test_cases = vec![
            (Some(StopReason::EndTurn), "stop"),
            (Some(StopReason::MaxTokens), "length"),
            (Some(StopReason::StopSequence), "stop"),
            (Some(StopReason::ToolUse), "tool_calls"),
            (Some(StopReason::Refusal), "content_filter"),
            (None, "stop"),
        ];

        for (stop_reason, expected) in test_cases {
            let response = CreateMessageResponse {
                content: vec![],
                id: "msg_123".to_string(),
                model: "claude-3-opus".to_string(),
                role: Role::Assistant,
                stop_reason,
                stop_sequence: None,
                type_: "message".to_string(),
                usage: None,
            };

            let result = transforms_json(response);
            assert_eq!(
                result["choices"][0]["finish_reason"], expected,
                "Failed for stop_reason: {:?}",
                stop_reason
            );
        }
    }
}
