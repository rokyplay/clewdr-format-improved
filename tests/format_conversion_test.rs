//! Format conversion integration tests
//!
//! These tests verify the format conversion logic between Claude and OpenAI APIs.

use serde_json::{json, Value};

// Note: These tests are designed to be run when the project can be compiled.
// For now, they serve as documentation of the expected behavior.

/// Test data for Claude → OpenAI conversion
mod claude_to_oai {
    use super::*;

    /// Sample Claude response with text content
    pub fn sample_text_response() -> Value {
        json!({
            "content": [
                {
                    "type": "text",
                    "text": "Hello, world!"
                }
            ],
            "id": "msg_123",
            "model": "claude-3-opus",
            "role": "assistant",
            "stop_reason": "end_turn",
            "type": "message",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        })
    }

    /// Sample Claude response with tool use
    pub fn sample_tool_use_response() -> Value {
        json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "tool_123",
                    "name": "Grep",
                    "input": {
                        "query": "search pattern",
                        "path": "/some/path"
                    }
                }
            ],
            "id": "msg_456",
            "model": "claude-3-opus",
            "role": "assistant",
            "stop_reason": "tool_use",
            "type": "message"
        })
    }

    /// Sample Claude response with web search results
    pub fn sample_web_search_response() -> Value {
        json!({
            "content": [
                {
                    "type": "text",
                    "text": "Based on my search, here are the results:"
                },
                {
                    "type": "web_search_tool_result",
                    "content": [
                        {
                            "type": "web_search_result",
                            "url": "https://example.com/article",
                            "title": "Example Article",
                            "snippet": "This is an example article about..."
                        }
                    ]
                }
            ],
            "id": "msg_789",
            "model": "claude-3-opus",
            "role": "assistant",
            "stop_reason": "end_turn",
            "type": "message"
        })
    }

    /// Sample Claude response with thinking block
    pub fn sample_thinking_response() -> Value {
        json!({
            "content": [
                {
                    "type": "thinking",
                    "thinking": "Let me analyze this problem...",
                    "signature": "valid_signature_abc123"
                },
                {
                    "type": "text",
                    "text": "Here is my answer."
                }
            ],
            "id": "msg_101",
            "model": "claude-3-opus",
            "role": "assistant",
            "stop_reason": "end_turn",
            "type": "message"
        })
    }

    /// Expected OpenAI format for text response
    pub fn expected_oai_text_response() -> Value {
        json!({
            "id": "msg_123",
            "object": "chat.completion",
            "model": "claude-3-opus",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello, world!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        })
    }

    /// Expected OpenAI format for tool use response
    /// Note: query should be remapped to pattern for Grep tool
    pub fn expected_oai_tool_response() -> Value {
        json!({
            "id": "msg_456",
            "object": "chat.completion",
            "model": "claude-3-opus",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "tool_123",
                        "type": "function",
                        "function": {
                            "name": "Grep",
                            "arguments": "{\"pattern\":\"search pattern\",\"path\":\"/some/path\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })
    }
}

/// Test data for OpenAI → Claude conversion
mod oai_to_claude {
    use super::*;

    /// Sample OpenAI request with tool role message
    pub fn sample_tool_result_message() -> Value {
        json!({
            "role": "tool",
            "tool_call_id": "call_123",
            "content": "{\"result\": \"success\", \"data\": [1, 2, 3]}"
        })
    }

    /// Sample OpenAI request with assistant tool_calls
    pub fn sample_assistant_with_tool_calls() -> Value {
        json!({
            "role": "assistant",
            "content": "I'll search for that.",
            "tool_calls": [{
                "id": "call_456",
                "type": "function",
                "function": {
                    "name": "web_search",
                    "arguments": "{\"query\": \"test search\"}"
                }
            }]
        })
    }

    /// Expected Claude format for tool result
    pub fn expected_claude_tool_result() -> Value {
        json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": "call_123",
                "content": {"result": "success", "data": [1, 2, 3]}
            }]
        })
    }

    /// Expected Claude format for assistant with tool use
    pub fn expected_claude_assistant_with_tool_use() -> Value {
        json!({
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "I'll search for that."
                },
                {
                    "type": "tool_use",
                    "id": "call_456",
                    "name": "web_search",
                    "input": {"query": "test search"}
                }
            ]
        })
    }
}

/// Test data for streaming events
mod streaming {
    use super::*;

    /// Sample Claude streaming events
    pub fn sample_stream_events() -> Vec<Value> {
        vec![
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": "Hello"
                }
            }),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "text_delta",
                    "text": ", world!"
                }
            }),
            json!({
                "type": "content_block_stop",
                "index": 0
            }),
            json!({
                "type": "message_stop"
            }),
        ]
    }

    /// Sample Claude streaming events with tool call
    pub fn sample_tool_call_stream_events() -> Vec<Value> {
        vec![
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "tool_use",
                    "id": "tool_123",
                    "name": "Read"
                }
            }),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"path\":"
                }
            }),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "\"/some/file.txt\"}"
                }
            }),
            json!({
                "type": "content_block_stop",
                "index": 0
            }),
        ]
    }

    /// Expected OpenAI streaming event format
    pub fn expected_oai_stream_event() -> Value {
        json!({
            "choices": [{
                "delta": {
                    "content": "Hello"
                }
            }]
        })
    }

    /// Expected OpenAI tool call event format
    /// Note: path should be remapped to file_path for Read tool
    pub fn expected_oai_tool_call_event() -> Value {
        json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "tool_123",
                        "type": "function",
                        "function": {
                            "name": "Read",
                            "arguments": "{\"file_path\":\"/some/file.txt\"}"
                        }
                    }]
                }
            }]
        })
    }
}

/// Test data for image format conversion
mod images {
    use super::*;

    /// Sample OpenAI image_url format (data URI)
    pub fn sample_oai_data_uri_image() -> Value {
        json!({
            "type": "image_url",
            "image_url": {
                "url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg=="
            }
        })
    }

    /// Sample OpenAI image_url format (HTTP URL)
    pub fn sample_oai_http_image() -> Value {
        json!({
            "type": "image_url",
            "image_url": {
                "url": "https://example.com/image.png"
            }
        })
    }

    /// Expected Claude native image format
    pub fn expected_claude_image() -> Value {
        json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": "iVBORw0KGgoAAAANSUhEUg=="
            }
        })
    }

    /// Sample Claude document format
    pub fn sample_claude_document() -> Value {
        json!({
            "type": "document",
            "source": {
                "type": "base64",
                "media_type": "application/pdf",
                "data": "JVBERi0xLjQ="
            }
        })
    }
}

/// Test data for web search/citations
mod web_search {
    use super::*;

    /// Sample Claude web_search_tool_result
    pub fn sample_web_search_result() -> Value {
        json!({
            "type": "web_search_tool_result",
            "tool_use_id": "search_123",
            "content": [
                {
                    "type": "web_search_result",
                    "url": "https://example.com/article1",
                    "title": "First Article",
                    "snippet": "This is the first article about the topic."
                },
                {
                    "type": "web_search_result",
                    "url": "https://example.com/article2",
                    "title": "Second Article",
                    "snippet": "Another relevant article with more details."
                }
            ]
        })
    }

    /// Expected OpenAI annotations format
    pub fn expected_oai_annotations() -> Value {
        json!([
            {
                "type": "url_citation",
                "url_citation": {
                    "url": "https://example.com/article1",
                    "title": "First Article",
                    "content": "This is the first article about the topic.",
                    "start_index": 0,
                    "end_index": 0
                }
            },
            {
                "type": "url_citation",
                "url_citation": {
                    "url": "https://example.com/article2",
                    "title": "Second Article",
                    "content": "Another relevant article with more details.",
                    "start_index": 0,
                    "end_index": 0
                }
            }
        ])
    }
}

/// Test data for parameter remapping
mod param_remapping {
    use super::*;

    /// Grep tool: query → pattern
    pub fn grep_before() -> Value {
        json!({"query": "search text", "path": "/dir"})
    }

    pub fn grep_after() -> Value {
        json!({"pattern": "search text", "path": "/dir"})
    }

    /// Glob tool: query → pattern
    pub fn glob_before() -> Value {
        json!({"query": "*.rs"})
    }

    pub fn glob_after() -> Value {
        json!({"pattern": "*.rs"})
    }

    /// Read tool: path → file_path
    pub fn read_before() -> Value {
        json!({"path": "/file.txt"})
    }

    pub fn read_after() -> Value {
        json!({"file_path": "/file.txt"})
    }

    /// Write tool: path → file_path
    pub fn write_before() -> Value {
        json!({"path": "/file.txt", "content": "data"})
    }

    pub fn write_after() -> Value {
        json!({"file_path": "/file.txt", "content": "data"})
    }
}

/// Test data for schema cleaning
mod schema_cleaning {
    use super::*;

    /// Schema with unsupported keywords
    pub fn schema_with_unsupported() -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 100,
                    "pattern": "^[a-z]+$"
                },
                "age": {
                    "type": ["integer", "null"],
                    "minimum": 0,
                    "maximum": 150
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    /// Schema after cleaning
    pub fn schema_cleaned() -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Constraints: minLength=1, maxLength=100, pattern=^[a-z]+$"
                },
                "age": {
                    "type": "integer",
                    "description": "Constraints: minimum=0, maximum=150"
                }
            },
            "required": ["name"]
        })
    }
}

/// Test runner (manual verification)
/// 
/// Run this to print all test data for manual verification:
/// ```
/// cargo test -- --nocapture format_conversion_test::print_all_test_data
/// ```
#[test]
#[ignore]
fn print_all_test_data() {
    println!("=== Claude → OpenAI Conversion ===\n");
    println!("Text Response Input:");
    println!("{}\n", serde_json::to_string_pretty(&claude_to_oai::sample_text_response()).unwrap());
    println!("Expected Output:");
    println!("{}\n", serde_json::to_string_pretty(&claude_to_oai::expected_oai_text_response()).unwrap());

    println!("\n=== Tool Use Response ===\n");
    println!("Input:");
    println!("{}\n", serde_json::to_string_pretty(&claude_to_oai::sample_tool_use_response()).unwrap());
    println!("Expected Output:");
    println!("{}\n", serde_json::to_string_pretty(&claude_to_oai::expected_oai_tool_response()).unwrap());

    println!("\n=== OpenAI → Claude Conversion ===\n");
    println!("Tool Result Input:");
    println!("{}\n", serde_json::to_string_pretty(&oai_to_claude::sample_tool_result_message()).unwrap());
    println!("Expected Output:");
    println!("{}\n", serde_json::to_string_pretty(&oai_to_claude::expected_claude_tool_result()).unwrap());

    println!("\n=== Parameter Remapping ===\n");
    println!("Grep: {:?} → {:?}", param_remapping::grep_before(), param_remapping::grep_after());
    println!("Read: {:?} → {:?}", param_remapping::read_before(), param_remapping::read_after());
}