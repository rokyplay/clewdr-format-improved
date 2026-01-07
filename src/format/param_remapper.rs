//! Parameter remapping for tool calls
//!
//! This module provides utilities to remap function call parameters between
//! different API formats. Gemini sometimes uses different parameter names
//! than what Claude Code expects.
//!
//! Reference: Antigravity-Manager/src-tauri/src/proxy/mappers/claude/response.rs

use serde_json::Value;

/// Remap function call arguments for Gemini → Claude compatibility
///
/// Gemini sometimes uses different parameter names than specified in tool schemas.
/// This function remaps known parameter name differences.
///
/// # Arguments
/// * `tool_name` - The name of the tool being called
/// * `args` - The arguments object to remap (modified in place)
///
/// # Known Remappings
/// - `Grep`: `query` → `pattern`
/// - `Glob`: `query` → `pattern`
/// - `Read`: `path` → `file_path`
pub fn remap_function_call_args(tool_name: &str, args: &mut Value) {
    let Some(obj) = args.as_object_mut() else {
        return;
    };

    match tool_name {
        "Grep" => {
            // Gemini uses "query", Claude Code expects "pattern"
            if let Some(query) = obj.remove("query") {
                if !obj.contains_key("pattern") {
                    obj.insert("pattern".to_string(), query);
                    tracing::debug!("[ParamRemap] Grep: query → pattern");
                }
            }
        }
        "Glob" => {
            // Similar remapping for Glob
            if let Some(query) = obj.remove("query") {
                if !obj.contains_key("pattern") {
                    obj.insert("pattern".to_string(), query);
                    tracing::debug!("[ParamRemap] Glob: query → pattern");
                }
            }
        }
        "Read" => {
            // Gemini might use "path" vs "file_path"
            if let Some(path) = obj.remove("path") {
                if !obj.contains_key("file_path") {
                    obj.insert("file_path".to_string(), path);
                    tracing::debug!("[ParamRemap] Read: path → file_path");
                }
            }
        }
        "Write" => {
            // Similar to Read
            if let Some(path) = obj.remove("path") {
                if !obj.contains_key("file_path") {
                    obj.insert("file_path".to_string(), path);
                    tracing::debug!("[ParamRemap] Write: path → file_path");
                }
            }
        }
        "Edit" => {
            // Edit tool might have similar issues
            if let Some(path) = obj.remove("path") {
                if !obj.contains_key("file_path") {
                    obj.insert("file_path".to_string(), path);
                    tracing::debug!("[ParamRemap] Edit: path → file_path");
                }
            }
        }
        "ListDir" | "LS" => {
            // Directory listing tools
            if let Some(path) = obj.remove("path") {
                if !obj.contains_key("directory") {
                    obj.insert("directory".to_string(), path);
                    tracing::debug!("[ParamRemap] {}: path → directory", tool_name);
                }
            }
        }
        _ => {
            // No remapping needed for other tools
        }
    }
}

/// Apply remapping to a tool use block
///
/// Convenience function that extracts the tool name and remaps arguments.
///
/// # Arguments
/// * `name` - The tool name
/// * `input` - The input arguments (modified in place)
pub fn remap_tool_use(name: &str, input: &mut Value) {
    remap_function_call_args(name, input);
}

/// Reverse remap function call arguments for OAI → Claude compatibility
///
/// This is the reverse of `remap_function_call_args`. It converts OAI parameter
/// names back to Claude's expected format.
///
/// Note: For tool results, we generally don't need to remap since the tool
/// produces the result in its own format. However, this function is provided
/// for completeness when converting from OAI format back to Claude.
///
/// # Arguments
/// * `_tool_use_id` - The tool use ID (for context, not currently used)
/// * `_args` - The arguments object to remap (modified in place)
///
/// # Known Remappings (reverse)
/// - `pattern` → `query` (for Grep, Glob responses)
/// - `file_path` → `path` (for Read, Write, Edit responses)
pub fn remap_tool_result_args(_tool_use_id: &str, _args: &mut Value) {
    // Tool results generally don't need remapping since they're output from
    // the tool, not input to it. The tool defines its own output format.
    //
    // However, if a client sends back modified tool results in a different
    // format, we might need to handle that here.
    //
    // For now, this is a no-op placeholder for future compatibility.
}

/// Reverse remap for OAI tool_calls to Claude tool_use
///
/// Converts OAI parameter names to Claude format when receiving requests.
///
/// # Arguments
/// * `tool_name` - The name of the tool being called
/// * `args` - The arguments object to remap (modified in place)
///
/// # Known Remappings (OAI → Claude)
/// - `pattern` → `query` (some clients might use pattern)
pub fn remap_oai_to_claude_args(tool_name: &str, args: &mut Value) {
    let Some(obj) = args.as_object_mut() else {
        return;
    };

    match tool_name {
        "Grep" | "Glob" => {
            // Some OAI clients might use "pattern" directly
            // If so, keep it as-is since that's what Claude Code expects
            // This function is mainly for documentation purposes
        }
        "Read" | "Write" | "Edit" => {
            // Some clients might use "file_path" directly
            // If so, keep it as-is
        }
        "web_search" => {
            // Ensure query parameter is properly formatted
            if let Some(q) = obj.get("q").cloned() {
                if !obj.contains_key("query") {
                    obj.insert("query".to_string(), q);
                    obj.remove("q");
                    tracing::debug!("[ParamRemap] web_search: q → query");
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_grep_remapping() {
        let mut args = json!({
            "query": "search pattern",
            "path": "/some/path"
        });

        remap_function_call_args("Grep", &mut args);

        assert!(args.get("query").is_none());
        assert_eq!(args["pattern"], "search pattern");
        assert_eq!(args["path"], "/some/path"); // path should remain unchanged
    }

    #[test]
    fn test_glob_remapping() {
        let mut args = json!({
            "query": "*.rs"
        });

        remap_function_call_args("Glob", &mut args);

        assert!(args.get("query").is_none());
        assert_eq!(args["pattern"], "*.rs");
    }

    #[test]
    fn test_read_remapping() {
        let mut args = json!({
            "path": "/some/file.txt"
        });

        remap_function_call_args("Read", &mut args);

        assert!(args.get("path").is_none());
        assert_eq!(args["file_path"], "/some/file.txt");
    }

    #[test]
    fn test_no_overwrite_existing() {
        let mut args = json!({
            "query": "old query",
            "pattern": "existing pattern"
        });

        remap_function_call_args("Grep", &mut args);

        // Should not overwrite existing pattern
        assert_eq!(args["pattern"], "existing pattern");
        // query should still be removed
        assert!(args.get("query").is_none());
    }

    #[test]
    fn test_unknown_tool_no_change() {
        let mut args = json!({
            "query": "test",
            "path": "/test"
        });

        let original = args.clone();
        remap_function_call_args("UnknownTool", &mut args);

        assert_eq!(args, original);
    }

    #[test]
    fn test_non_object_args() {
        let mut args = json!("string value");

        // Should not panic
        remap_function_call_args("Grep", &mut args);

        assert_eq!(args, json!("string value"));
    }

    #[test]
    fn test_oai_to_claude_web_search() {
        let mut args = json!({
            "q": "search query"
        });

        remap_oai_to_claude_args("web_search", &mut args);

        assert!(args.get("q").is_none());
        assert_eq!(args["query"], "search query");
    }

    #[test]
    fn test_remap_tool_result_args() {
        // Tool result remapping is currently a no-op
        let mut args = json!({
            "result": "success"
        });
        let original = args.clone();

        remap_tool_result_args("call_123", &mut args);

        assert_eq!(args, original);
    }
}