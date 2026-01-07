//! Format conversion module
//!
//! This module provides utilities for format conversion between Claude and OpenAI APIs,
//! including signature management, schema cleaning, parameter remapping, thinking utilities,
//! web search result formatting, and image format conversion.

pub mod image_converter;
pub mod param_remapper;
pub mod schema_cleaner;
pub mod signature_store;
pub mod thinking_utils;
pub mod web_search;

// Signature store exports
pub use signature_store::{
    clear_thought_signature, get_thought_signature, has_valid_signature, store_thought_signature,
};

// Schema cleaner exports
pub use schema_cleaner::{
    clean_json_schema, ensure_valid_schema, expand_refs, move_constraints_to_description,
};

// Parameter remapper exports
pub use param_remapper::{remap_function_call_args, remap_oai_to_claude_args, remap_tool_result_args, remap_tool_use};

// Thinking utilities exports
pub use thinking_utils::{
    analyze_conversation_state, extract_signatures, has_valid_signature_for_function_calls,
    message_has_tool_result, message_has_tool_use, message_has_valid_thinking,
    needs_thinking_recovery, should_disable_thinking_due_to_history, strip_invalid_thinking_blocks,
    ConversationState, MIN_SIGNATURE_LENGTH,
};

// Web search exports
pub use web_search::{
    annotations_to_web_search_content, citations_to_annotations,
    extract_citations_from_search_result, extract_citations_from_tool_result,
    format_citations_as_markdown, merge_citations_into_text, Citation,
};

// Image converter exports
pub use image_converter::{
    bytes_to_image_source, claude_image_to_oai, document_to_image_source,
    extract_image_from_data_uri, infer_media_type_from_url, is_supported_document_type,
    is_supported_image_type, is_valid_base64, oai_image_url_to_claude, process_image_blocks,
    SUPPORTED_DOCUMENT_TYPES, SUPPORTED_IMAGE_TYPES,
};

// Re-export cache_control cleaning from types module
pub use crate::types::claude::clean_cache_control_from_messages;