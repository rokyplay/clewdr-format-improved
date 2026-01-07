//! Thinking mode utilities
//!
//! This module provides utilities for handling Claude's thinking mode,
//! including signature validation, conversation state analysis, and
//! thinking block recovery mechanisms.
//!
//! Reference:
//! - antigravity-claude-proxy/src/format/thinking-utils.js
//! - Antigravity-Manager/src-tauri/src/proxy/mappers/claude/request.rs

use crate::types::claude::{ContentBlock, Message, MessageContent, Role};

/// Minimum signature length to be considered valid
/// Signatures shorter than this are likely incomplete or placeholder
pub const MIN_SIGNATURE_LENGTH: usize = 10;

/// Check if a message has a valid thinking block with signature
///
/// A valid thinking block must have a signature of at least MIN_SIGNATURE_LENGTH.
///
/// # Arguments
/// * `message` - The message to check
///
/// # Returns
/// true if the message contains a valid thinking block
pub fn message_has_valid_thinking(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks { content } => content.iter().any(|block| {
            if let ContentBlock::Thinking { signature, .. } = block {
                signature
                    .as_ref()
                    .map(|s| s.len() >= MIN_SIGNATURE_LENGTH)
                    .unwrap_or(false)
            } else {
                false
            }
        }),
        MessageContent::Text { .. } => false,
    }
}

/// Check if a message contains tool use blocks
///
/// # Arguments
/// * `message` - The message to check
///
/// # Returns
/// true if the message contains any tool use blocks
pub fn message_has_tool_use(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks { content } => content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse { .. })),
        MessageContent::Text { .. } => false,
    }
}

/// Check if a message contains tool result blocks
///
/// # Arguments
/// * `message` - The message to check
///
/// # Returns
/// true if the message contains any tool result blocks
pub fn message_has_tool_result(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks { content } => content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolResult { .. })),
        MessageContent::Text { .. } => false,
    }
}

/// Conversation state analysis result
#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    /// Whether the conversation is currently in a tool loop
    pub in_tool_loop: bool,
    /// Whether there's an interrupted tool call (tool_use without matching result)
    pub interrupted_tool: bool,
    /// Whether the current turn has valid thinking
    pub turn_has_thinking: bool,
    /// Number of tool results in the last user message
    pub tool_result_count: usize,
    /// Whether the last assistant message has tool calls
    pub last_assistant_has_tools: bool,
}

/// Analyze the conversation state for thinking mode handling
///
/// This function examines the message history to determine:
/// - If we're in a tool loop (assistant tool_use followed by user tool_result)
/// - If there are interrupted tool calls
/// - If the current turn has valid thinking blocks
///
/// # Arguments
/// * `messages` - The message history to analyze
///
/// # Returns
/// ConversationState with the analysis results
pub fn analyze_conversation_state(messages: &[Message]) -> ConversationState {
    let mut state = ConversationState::default();

    if messages.is_empty() {
        return state;
    }

    // Find the last assistant message
    let last_assistant_idx = messages
        .iter()
        .rposition(|m| m.role == Role::Assistant);

    if let Some(idx) = last_assistant_idx {
        let last_assistant = &messages[idx];
        state.last_assistant_has_tools = message_has_tool_use(last_assistant);
        state.turn_has_thinking = message_has_valid_thinking(last_assistant);

        // Check if there's a user message after the assistant message
        if idx + 1 < messages.len() {
            let after_assistant = &messages[idx + 1..];
            
            // Count tool results in subsequent user messages
            for msg in after_assistant {
                if msg.role == Role::User {
                    if let MessageContent::Blocks { content } = &msg.content {
                        state.tool_result_count += content
                            .iter()
                            .filter(|b| matches!(b, ContentBlock::ToolResult { .. }))
                            .count();
                    }
                }
            }

            // We're in a tool loop if:
            // 1. Last assistant has tool use
            // 2. Next message(s) have tool results
            state.in_tool_loop =
                state.last_assistant_has_tools && state.tool_result_count > 0;

            // Interrupted if we have tool use but no tool results yet
            // and the conversation hasn't ended
            state.interrupted_tool =
                state.last_assistant_has_tools && state.tool_result_count == 0;
        } else {
            // Assistant message is the last message
            state.interrupted_tool = state.last_assistant_has_tools;
        }
    }

    state
}

/// Check if thinking should be disabled due to message history
///
/// This is necessary when:
/// 1. The last assistant message has tool calls but no thinking block
/// 2. The conversation was started without thinking mode
///
/// Enabling thinking mode mid-conversation can cause issues with some providers.
///
/// # Arguments
/// * `messages` - The message history
///
/// # Returns
/// true if thinking should be disabled for compatibility
pub fn should_disable_thinking_due_to_history(messages: &[Message]) -> bool {
    // Find the last assistant message
    for msg in messages.iter().rev() {
        if msg.role == Role::Assistant {
            let has_tool_use = message_has_tool_use(msg);
            let has_thinking = message_has_valid_thinking(msg);

            // If has tool calls but no thinking -> incompatible
            if has_tool_use && !has_thinking {
                tracing::info!("[Thinking] Detected ToolUse without Thinking in history - disabling");
                return true;
            }

            // Only check the most recent assistant message
            return false;
        }
    }

    false
}

/// Check if there's a valid signature for function calls
///
/// When using thinking mode with tools, we need a valid signature
/// to continue the conversation. This checks both the global storage
/// and the message history.
///
/// # Arguments
/// * `messages` - The message history
/// * `global_sig` - Optional globally stored signature
///
/// # Returns
/// true if a valid signature is available
pub fn has_valid_signature_for_function_calls(
    messages: &[Message],
    global_sig: &Option<String>,
) -> bool {
    // Check global storage first
    if let Some(sig) = global_sig {
        if sig.len() >= MIN_SIGNATURE_LENGTH {
            return true;
        }
    }

    // Check message history for thinking blocks with signatures
    for msg in messages.iter().rev() {
        if msg.role == Role::Assistant {
            if let MessageContent::Blocks { content } = &msg.content {
                for block in content {
                    if let ContentBlock::Thinking {
                        signature: Some(sig),
                        ..
                    } = block
                    {
                        if sig.len() >= MIN_SIGNATURE_LENGTH {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Check if thinking recovery is needed
///
/// Recovery is needed when:
/// 1. We're in a tool loop or have interrupted tools
/// 2. The current turn doesn't have valid thinking
///
/// # Arguments
/// * `messages` - The message history
///
/// # Returns
/// true if thinking recovery is needed
pub fn needs_thinking_recovery(messages: &[Message]) -> bool {
    let state = analyze_conversation_state(messages);

    if !state.in_tool_loop && !state.interrupted_tool {
        return false;
    }

    !state.turn_has_thinking
}

/// Strip invalid thinking blocks from messages
///
/// Removes thinking blocks that:
/// 1. Have no signature
/// 2. Have a signature that's too short
/// 3. Are from an incompatible model family (if checking cross-model)
///
/// # Arguments
/// * `messages` - The messages to process (modified in place)
pub fn strip_invalid_thinking_blocks(messages: &mut [Message]) {
    for msg in messages.iter_mut() {
        if msg.role != Role::Assistant {
            continue;
        }

        if let MessageContent::Blocks { content } = &mut msg.content {
            content.retain(|block| {
                if let ContentBlock::Thinking { signature, .. } = block {
                    // Keep only thinking blocks with valid signatures
                    signature
                        .as_ref()
                        .map(|s| s.len() >= MIN_SIGNATURE_LENGTH)
                        .unwrap_or(false)
                } else {
                    // Keep all non-thinking blocks
                    true
                }
            });
        }
    }
}

/// Extract all signatures from message history
///
/// # Arguments
/// * `messages` - The message history
///
/// # Returns
/// Vector of (signature, index) tuples for all valid signatures found
pub fn extract_signatures(messages: &[Message]) -> Vec<(String, usize)> {
    let mut signatures = Vec::new();

    for (idx, msg) in messages.iter().enumerate() {
        if msg.role != Role::Assistant {
            continue;
        }

        if let MessageContent::Blocks { content } = &msg.content {
            for block in content {
                if let ContentBlock::Thinking {
                    signature: Some(sig),
                    ..
                } = block
                {
                    if sig.len() >= MIN_SIGNATURE_LENGTH {
                        signatures.push((sig.clone(), idx));
                    }
                }
            }
        }
    }

    signatures
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_text_message(role: Role, text: &str) -> Message {
        Message {
            role,
            content: MessageContent::Text {
                content: text.to_string(),
            },
        }
    }

    fn create_blocks_message(role: Role, blocks: Vec<ContentBlock>) -> Message {
        Message {
            role,
            content: MessageContent::Blocks { content: blocks },
        }
    }

    #[test]
    fn test_message_has_valid_thinking() {
        let valid = create_blocks_message(
            Role::Assistant,
            vec![ContentBlock::Thinking {
                thinking: "thinking...".to_string(),
                signature: Some("valid_signature_12345".to_string()),
                cache_control: None,
            }],
        );
        assert!(message_has_valid_thinking(&valid));

        let invalid_short = create_blocks_message(
            Role::Assistant,
            vec![ContentBlock::Thinking {
                thinking: "thinking...".to_string(),
                signature: Some("short".to_string()),
                cache_control: None,
            }],
        );
        assert!(!message_has_valid_thinking(&invalid_short));

        let no_signature = create_blocks_message(
            Role::Assistant,
            vec![ContentBlock::Thinking {
                thinking: "thinking...".to_string(),
                signature: None,
                cache_control: None,
            }],
        );
        assert!(!message_has_valid_thinking(&no_signature));

        let text_only = create_text_message(Role::Assistant, "hello");
        assert!(!message_has_valid_thinking(&text_only));
    }

    #[test]
    fn test_message_has_tool_use() {
        let with_tool = create_blocks_message(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "123".to_string(),
                name: "test_tool".to_string(),
                input: json!({}),
                signature: None,
                cache_control: None,
            }],
        );
        assert!(message_has_tool_use(&with_tool));

        let without_tool = create_blocks_message(
            Role::Assistant,
            vec![ContentBlock::Text {
                text: "hello".to_string(),
                cache_control: None,
            }],
        );
        assert!(!message_has_tool_use(&without_tool));
    }

    #[test]
    fn test_analyze_conversation_state_tool_loop() {
        let messages = vec![
            create_text_message(Role::User, "hello"),
            create_blocks_message(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "123".to_string(),
                    name: "test".to_string(),
                    input: json!({}),
                    signature: None,
                    cache_control: None,
                }],
            ),
            create_blocks_message(
                Role::User,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "123".to_string(),
                    content: json!("result"),
                    is_error: None,
                    cache_control: None,
                }],
            ),
        ];

        let state = analyze_conversation_state(&messages);
        assert!(state.in_tool_loop);
        assert!(!state.interrupted_tool);
        assert_eq!(state.tool_result_count, 1);
    }

    #[test]
    fn test_analyze_conversation_state_interrupted() {
        let messages = vec![
            create_text_message(Role::User, "hello"),
            create_blocks_message(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "123".to_string(),
                    name: "test".to_string(),
                    input: json!({}),
                    signature: None,
                    cache_control: None,
                }],
            ),
        ];

        let state = analyze_conversation_state(&messages);
        assert!(!state.in_tool_loop);
        assert!(state.interrupted_tool);
    }

    #[test]
    fn test_should_disable_thinking_due_to_history() {
        // Tool use without thinking -> should disable
        let messages_no_thinking = vec![
            create_text_message(Role::User, "hello"),
            create_blocks_message(
                Role::Assistant,
                vec![ContentBlock::ToolUse {
                    id: "123".to_string(),
                    name: "test".to_string(),
                    input: json!({}),
                    signature: None,
                    cache_control: None,
                }],
            ),
        ];
        assert!(should_disable_thinking_due_to_history(&messages_no_thinking));

        // Tool use with valid thinking -> should not disable
        let messages_with_thinking = vec![
            create_text_message(Role::User, "hello"),
            create_blocks_message(
                Role::Assistant,
                vec![
                    ContentBlock::Thinking {
                        thinking: "thinking...".to_string(),
                        signature: Some("valid_signature_12345".to_string()),
                        cache_control: None,
                    },
                    ContentBlock::ToolUse {
                        id: "123".to_string(),
                        name: "test".to_string(),
                        input: json!({}),
                        signature: None,
                        cache_control: None,
                    },
                ],
            ),
        ];
        assert!(!should_disable_thinking_due_to_history(&messages_with_thinking));
    }

    #[test]
    fn test_has_valid_signature_for_function_calls() {
        let messages = vec![create_blocks_message(
            Role::Assistant,
            vec![ContentBlock::Thinking {
                thinking: "test".to_string(),
                signature: Some("valid_signature_12345".to_string()),
                cache_control: None,
            }],
        )];

        assert!(has_valid_signature_for_function_calls(&messages, &None));
        assert!(has_valid_signature_for_function_calls(
            &[],
            &Some("global_signature_12345".to_string())
        ));
        assert!(!has_valid_signature_for_function_calls(&[], &None));
        assert!(!has_valid_signature_for_function_calls(
            &[],
            &Some("short".to_string())
        ));
    }

    #[test]
    fn test_strip_invalid_thinking_blocks() {
        let mut messages = vec![create_blocks_message(
            Role::Assistant,
            vec![
                ContentBlock::Thinking {
                    thinking: "valid".to_string(),
                    signature: Some("valid_signature_12345".to_string()),
                    cache_control: None,
                },
                ContentBlock::Thinking {
                    thinking: "invalid".to_string(),
                    signature: Some("short".to_string()),
                    cache_control: None,
                },
                ContentBlock::Text {
                    text: "hello".to_string(),
                    cache_control: None,
                },
            ],
        )];

        strip_invalid_thinking_blocks(&mut messages);

        if let MessageContent::Blocks { content } = &messages[0].content {
            assert_eq!(content.len(), 2); // valid thinking + text
            assert!(matches!(content[0], ContentBlock::Thinking { .. }));
            assert!(matches!(content[1], ContentBlock::Text { .. }));
        } else {
            panic!("Expected Blocks content");
        }
    }

    #[test]
    fn test_extract_signatures() {
        let messages = vec![
            create_text_message(Role::User, "hello"),
            create_blocks_message(
                Role::Assistant,
                vec![ContentBlock::Thinking {
                    thinking: "test".to_string(),
                    signature: Some("signature_one_12345".to_string()),
                    cache_control: None,
                }],
            ),
            create_text_message(Role::User, "continue"),
            create_blocks_message(
                Role::Assistant,
                vec![ContentBlock::Thinking {
                    thinking: "more".to_string(),
                    signature: Some("signature_two_12345".to_string()),
                    cache_control: None,
                }],
            ),
        ];

        let sigs = extract_signatures(&messages);
        assert_eq!(sigs.len(), 2);
        assert_eq!(sigs[0].0, "signature_one_12345");
        assert_eq!(sigs[0].1, 1);
        assert_eq!(sigs[1].0, "signature_two_12345");
        assert_eq!(sigs[1].1, 3);
    }
}