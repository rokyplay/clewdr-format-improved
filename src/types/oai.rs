use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiktoken_rs::o200k_base;

use super::claude::{CreateMessageParams as ClaudeCreateMessageParams, *};
use crate::format::{
    annotations_to_web_search_content, clean_json_schema, ensure_valid_schema,
    move_constraints_to_description, oai_image_url_to_claude, remap_oai_to_claude_args,
    remap_tool_result_args,
};
use crate::types::claude::Message;

/// OpenAI-specific role that includes "tool" for tool results
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OaiRole {
    System,
    User,
    #[default]
    Assistant,
    Tool,
}

impl From<OaiRole> for Role {
    fn from(role: OaiRole) -> Self {
        match role {
            OaiRole::System => Role::System,
            OaiRole::User => Role::User,
            OaiRole::Assistant => Role::Assistant,
            OaiRole::Tool => Role::User, // Tool results become user messages in Claude
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low = 256,
    #[default]
    Medium = 256 * 8,
    High = 256 * 8 * 8,
}

/// OpenAI format message content
/// OAI uses "content" directly at the top level, not nested like Claude
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OaiMessageContent {
    /// Simple text content (string)
    Text(String),
    /// Structured content blocks (array)
    Blocks(Vec<ContentBlock>),
    /// Null content (for assistant messages with only tool_calls)
    Null,
}

impl Default for OaiMessageContent {
    fn default() -> Self {
        OaiMessageContent::Null
    }
}

impl OaiMessageContent {
    /// Convert to Claude MessageContent format
    pub fn to_claude_format(self) -> MessageContent {
        match self {
            OaiMessageContent::Text(text) => MessageContent::Text { content: text },
            OaiMessageContent::Blocks(blocks) => MessageContent::Blocks { content: blocks },
            OaiMessageContent::Null => MessageContent::Text { content: String::new() },
        }
    }
}

/// OpenAI format message with tool support
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OaiMessage {
    pub role: OaiRole,
    /// Content field - can be string, array, or null
    #[serde(default)]
    pub content: OaiMessageContent,
    /// Tool call ID for tool role messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by assistant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OaiToolCall>>,
    /// Annotations (web search citations) for content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<Value>>,
}

/// OpenAI tool call format
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OaiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: OaiToolCallFunction,
}

/// OpenAI tool call function details
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OaiToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// OpenAI tool definition format (for request tools field)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum OaiTool {
    /// OpenAI function tool format
    #[serde(rename = "function")]
    Function { function: OaiToolFunction },
    /// Claude custom tool format (passthrough)
    #[serde(rename = "custom")]
    Custom(CustomTool),
    /// Other tool types (passthrough as raw)
    #[serde(other)]
    Other,
}

/// OpenAI function definition
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OaiToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

impl From<OaiTool> for Tool {
    fn from(oai_tool: OaiTool) -> Self {
        match oai_tool {
            OaiTool::Function { function } => {
                // Check for special built-in tools
                match function.name.as_str() {
                    "web_search" => {
                        // Convert to Claude's built-in web_search tool
                        Tool::Known(KnownTool::WebSearch20250305 {
                            name: ToolNameWebSearch::WebSearch,
                            allowed_domains: None,
                            blocked_domains: None,
                            cache_control: None,
                            max_uses: None,
                            user_location: None,
                            extra: std::collections::HashMap::new(),
                        })
                    }
                    "bash" => {
                        // Convert to Claude's built-in bash tool
                        Tool::Known(KnownTool::Bash20250124 {
                            name: ToolNameBash::Bash,
                            cache_control: None,
                            extra: std::collections::HashMap::new(),
                        })
                    }
                    "str_replace_editor" => {
                        // Convert to Claude's built-in text editor tool (older version)
                        Tool::Known(KnownTool::TextEditor20250124 {
                            name: ToolNameStrReplaceEditor::StrReplaceEditor,
                            cache_control: None,
                            extra: std::collections::HashMap::new(),
                        })
                    }
                    "str_replace_based_edit_tool" => {
                        // Convert to Claude's built-in text editor tool (newer version)
                        Tool::Known(KnownTool::TextEditor20250728 {
                            name: ToolNameStrReplaceBasedEditTool::StrReplaceBasedEditTool,
                            cache_control: None,
                            max_characters: None,
                            extra: std::collections::HashMap::new(),
                        })
                    }
                    _ => {
                        // Regular function -> custom tool
                        Tool::Custom(CustomTool {
                            name: function.name,
                            description: function.description,
                            input_schema: function.parameters.unwrap_or(json!({"type": "object", "properties": {}})),
                            cache_control: None,
                            type_: Some(CustomToolType::Custom),
                        })
                    }
                }
            }
            OaiTool::Custom(custom) => Tool::Custom(CustomTool {
                type_: Some(CustomToolType::Custom),
                ..custom
            }),
            OaiTool::Other => Tool::Raw(json!({})),
        }
    }
}

/// Convert OAI message to Claude message
fn convert_oai_message(msg: OaiMessage) -> Message {
    match msg.role {
        OaiRole::Tool => {
            // Convert tool role to user message with tool_result block
            let tool_use_id = msg.tool_call_id.unwrap_or_default();
            // Claude API requires tool_result.content to be a string or array of content blocks
            // NOT a JSON object. Keep the content as a string.
            let content_value = match msg.content {
                OaiMessageContent::Text(text) => {
                    // Keep as string - Claude API doesn't accept objects for tool_result.content
                    json!(text)
                }
                OaiMessageContent::Blocks(blocks) => json!(blocks),
                OaiMessageContent::Null => json!(""),
            };
            
            // Apply reverse parameter remapping (only if content is an object)
            let mut remapped_content = content_value.clone();
            if remapped_content.is_object() {
                remap_tool_result_args(&tool_use_id, &mut remapped_content);
            }
            
            Message {
                role: Role::User,
                content: MessageContent::Blocks {
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id,
                        content: remapped_content,
                        is_error: None,
                        cache_control: None,
                    }],
                },
            }
        }
        OaiRole::Assistant if msg.tool_calls.is_some() => {
            // Convert assistant message with tool_calls to Claude format
            let tool_calls = msg.tool_calls.unwrap();
            let mut blocks: Vec<ContentBlock> = Vec::new();
            
            // Add text content if present
            match msg.content {
                OaiMessageContent::Text(text) if !text.is_empty() => {
                    blocks.push(ContentBlock::Text {
                        text,
                        cache_control: None,
                    });
                }
                OaiMessageContent::Blocks(content) => {
                    blocks.extend(content);
                }
                _ => {}
            }
            
            // Add tool_use blocks
            for tc in tool_calls {
                let mut input: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(json!({}));
                // Apply OAI → Claude parameter remapping
                remap_oai_to_claude_args(&tc.function.name, &mut input);
                blocks.push(ContentBlock::ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                    signature: None,
                    cache_control: None,
                });
            }
            
            Message {
                role: Role::Assistant,
                content: MessageContent::Blocks { content: blocks },
            }
        }
        _ => {
            // Standard message conversion - with image format conversion
            let mut blocks: Vec<ContentBlock> = Vec::new();
            
            // First, convert message content
            match msg.content {
                OaiMessageContent::Blocks(content) => {
                    // Convert ImageUrl blocks to native Image format
                    let converted: Vec<ContentBlock> = content
                        .into_iter()
                        .map(|block| {
                            if let ContentBlock::ImageUrl { ref image_url } = block {
                                oai_image_url_to_claude(image_url).unwrap_or(block)
                            } else {
                                block
                            }
                        })
                        .collect();
                    blocks.extend(converted);
                }
                OaiMessageContent::Text(text) => {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::Text {
                            text,
                            cache_control: None,
                        });
                    }
                }
                OaiMessageContent::Null => {}
            };
            
            // Handle annotations (web search citations) -> convert to Claude web_search format
            if let Some(annotations) = msg.annotations {
                if !annotations.is_empty() {
                    let web_search_content = annotations_to_web_search_content(&annotations);
                    if !web_search_content.is_empty() {
                        tracing::debug!(
                            "[OAI→Claude] Converting {} annotations to web search content",
                            annotations.len()
                        );
                        // Add as tool result with web search content
                        blocks.push(ContentBlock::ToolResult {
                            tool_use_id: "web_search".to_string(),
                            content: json!(web_search_content),
                            is_error: None,
                            cache_control: None,
                        });
                    }
                }
            }
            
            let content = if blocks.is_empty() {
                MessageContent::Text { content: String::new() }
            } else {
                MessageContent::Blocks { content: blocks }
            };
            
            Message {
                role: msg.role.into(),
                content,
            }
        }
    }
}

impl From<CreateMessageParams> for ClaudeCreateMessageParams {
    fn from(params: CreateMessageParams) -> Self {
        let (systems, messages): (Vec<Message>, Vec<Message>) = params
            .messages
            .into_iter()
            .partition(|m| m.role == Role::System);
        let systems = systems
            .into_iter()
            .map(|m| m.content)
            .flat_map(|c| match c {
                MessageContent::Text { content } => vec![ContentBlock::Text {
                    text: content,
                    cache_control: None,
                }],
                MessageContent::Blocks { content } => content,
            })
            .filter(|b| matches!(b, ContentBlock::Text { .. }))
            .map(|b| json!(b))
            .collect::<Vec<_>>();
        let system = (!systems.is_empty()).then(|| json!(systems));
        
        // Convert OAI tools to Claude tools and clean schemas
        let tools = params.tools.map(|tools| {
            tools.into_iter().filter_map(|oai_tool| {
                let tool: Tool = oai_tool.into();
                match tool {
                    Tool::Custom(mut custom) => {
                        // Full schema cleaning pipeline:
                        // 1. Move constraints to description (before removing them)
                        move_constraints_to_description(&mut custom.input_schema);
                        // 2. Clean unsupported keywords
                        clean_json_schema(&mut custom.input_schema);
                        // 3. Ensure schema is valid
                        ensure_valid_schema(&mut custom.input_schema);
                        // Ensure type is set to custom for Claude Code API
                        custom.type_ = Some(CustomToolType::Custom);
                        Some(Tool::Custom(custom))
                    }
                    Tool::Raw(v) if v.as_object().map(|o| o.is_empty()).unwrap_or(true) => {
                        // Filter out empty tools from OaiTool::Other
                        None
                    }
                    other => Some(other),
                }
            }).collect()
        });
        
        // Convert tool_choice from Simple to Object format for Claude Code API compatibility
        // Claude Code API requires object format: {"type": "auto"} instead of "auto"
        let tool_choice = params.tool_choice.map(|tc| tc.to_object_format());
        
        Self {
            max_tokens: (params.max_tokens.or(params.max_completion_tokens))
                .unwrap_or_else(default_max_tokens),
            system,
            messages,
            model: params.model,
            stop_sequences: params.stop,
            thinking: params
                .thinking
                .or_else(|| params.reasoning_effort.map(|e| Thinking::new(e as u64))),
            temperature: params.temperature,
            stream: params.stream,
            top_k: params.top_k,
            top_p: params.top_p,
            tools,
            tool_choice,
            metadata: params.metadata,
            n: params.n,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CreateMessageParams {
    /// Maximum number of tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Input messages for the conversation
    pub messages: Vec<Message>,
    /// Model to use
    pub model: String,
    /// Reasoning effort for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<Effort>,
    /// Frequency penalty for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Temperature for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Custom stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Thinking mode configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Logit bias for token generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<Value>,
    /// Tools that the model may use (supports both OAI function and Claude custom formats)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OaiTool>>,
    /// How the model should use tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Request metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Number of completions to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
}

impl CreateMessageParams {
    pub fn count_tokens(&self) -> u32 {
        let bpe = o200k_base().expect("Failed to get encoding");
        let messages = self
            .messages
            .iter()
            .map(|msg| match msg.content {
                MessageContent::Text { ref content } => content.to_string(),
                MessageContent::Blocks { ref content } => content
                    .iter()
                    .map(|block| match block {
                        ContentBlock::Text { text, .. } => text.as_str(),
                        ContentBlock::Thinking { thinking, .. } => thinking.as_str(),
                        _ => "",
                    })
                    .collect::<String>(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        bpe.encode_with_special_tokens(&messages).len() as u32
    }
}

/// OpenAI format request with extended tool support
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct OaiCreateMessageParams {
    /// Maximum number of tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Input messages for the conversation (OAI format with tool role)
    pub messages: Vec<OaiMessage>,
    /// Model to use
    pub model: String,
    /// Reasoning effort for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<Effort>,
    /// Temperature for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Custom stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Thinking mode configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Tools that the model may use (OAI function format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OaiTool>>,
    /// How the model should use tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Request metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Number of completions to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
}

impl From<OaiCreateMessageParams> for ClaudeCreateMessageParams {
    fn from(params: OaiCreateMessageParams) -> Self {
        // Convert OAI messages to Claude format
        let converted_messages: Vec<Message> = params.messages
            .into_iter()
            .map(convert_oai_message)
            .collect();
        
        // Separate system messages
        let (systems, messages): (Vec<Message>, Vec<Message>) = converted_messages
            .into_iter()
            .partition(|m| m.role == Role::System);
        
        let systems = systems
            .into_iter()
            .map(|m| m.content)
            .flat_map(|c| match c {
                MessageContent::Text { content } => vec![ContentBlock::Text {
                    text: content,
                    cache_control: None,
                }],
                MessageContent::Blocks { content } => content,
            })
            .filter(|b| matches!(b, ContentBlock::Text { .. }))
            .map(|b| json!(b))
            .collect::<Vec<_>>();
        let system = (!systems.is_empty()).then(|| json!(systems));
        
        // Convert OAI tools to Claude tools and clean schemas
        let tools = params.tools.map(|tools| {
            tools.into_iter().filter_map(|oai_tool| {
                let tool: Tool = oai_tool.into();
                match tool {
                    Tool::Custom(mut custom) => {
                        // Apply full schema cleaning pipeline
                        move_constraints_to_description(&mut custom.input_schema);
                        clean_json_schema(&mut custom.input_schema);
                        ensure_valid_schema(&mut custom.input_schema);
                        // Ensure type is set to custom for Claude Code API
                        custom.type_ = Some(CustomToolType::Custom);
                        Some(Tool::Custom(custom))
                    }
                    Tool::Raw(v) if v.as_object().map(|o| o.is_empty()).unwrap_or(true) => {
                        // Filter out empty tools from OaiTool::Other
                        None
                    }
                    other => Some(other),
                }
            }).collect()
        });
        
        Self {
            max_tokens: (params.max_tokens.or(params.max_completion_tokens))
                .unwrap_or_else(default_max_tokens),
            system,
            messages,
            model: params.model,
            stop_sequences: params.stop,
            thinking: params
                .thinking
                .or_else(|| params.reasoning_effort.map(|e| Thinking::new(e as u64))),
            temperature: params.temperature,
            stream: params.stream,
            top_k: params.top_k,
            top_p: params.top_p,
            tools,
            tool_choice: params.tool_choice.map(|tc| tc.to_object_format()),
            metadata: params.metadata,
            n: params.n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_oai_tool_role_conversion() {
        let msg = OaiMessage {
            role: OaiRole::Tool,
            content: MessageContent::Text {
                content: r#"{"result": "success"}"#.to_string(),
            },
            tool_call_id: Some("call_123".to_string()),
            tool_calls: None,
        };
        
        let converted = convert_oai_message(msg);
        assert_eq!(converted.role, Role::User);
        
        if let MessageContent::Blocks { content } = converted.content {
            assert_eq!(content.len(), 1);
            if let ContentBlock::ToolResult { tool_use_id, .. } = &content[0] {
                assert_eq!(tool_use_id, "call_123");
            } else {
                panic!("Expected ToolResult block");
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    #[test]
    fn test_oai_assistant_with_tool_calls() {
        let msg = OaiMessage {
            role: OaiRole::Assistant,
            content: MessageContent::Text {
                content: "I'll search for that.".to_string(),
            },
            tool_call_id: None,
            tool_calls: Some(vec![OaiToolCall {
                id: "call_456".to_string(),
                type_: "function".to_string(),
                function: OaiToolCallFunction {
                    name: "web_search".to_string(),
                    arguments: r#"{"query": "test"}"#.to_string(),
                },
            }]),
        };
        
        let converted = convert_oai_message(msg);
        assert_eq!(converted.role, Role::Assistant);
        
        if let MessageContent::Blocks { content } = converted.content {
            assert_eq!(content.len(), 2); // Text + ToolUse
            assert!(matches!(&content[0], ContentBlock::Text { .. }));
            assert!(matches!(&content[1], ContentBlock::ToolUse { .. }));
        } else {
            panic!("Expected Blocks content");
        }
    }

    #[test]
    fn test_oai_role_conversion() {
        assert_eq!(Role::from(OaiRole::System), Role::System);
        assert_eq!(Role::from(OaiRole::User), Role::User);
        assert_eq!(Role::from(OaiRole::Assistant), Role::Assistant);
        assert_eq!(Role::from(OaiRole::Tool), Role::User);
    }
}
