//! Web Search result formatting
//!
//! This module provides utilities for converting Claude's web search results
//! to OpenAI's annotations format, and vice versa.
//!
//! Reference:
//! - claude-code-router/packages/core/src/transformer/anthropic.transformer.ts
//! - Antigravity-Manager/src-tauri/src/proxy/mappers/claude/response.rs

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Citation extracted from web search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// URL of the source
    pub url: String,
    /// Title of the source
    pub title: String,
    /// Snippet or excerpt from the source
    pub snippet: String,
    /// Start index in the text where this citation applies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<usize>,
    /// End index in the text where this citation applies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<usize>,
}

/// Web search result from Claude's API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    /// URL of the search result
    pub url: String,
    /// Title of the search result
    pub title: String,
    /// Snippet or description
    #[serde(default)]
    pub snippet: String,
    /// Encrypted content (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    /// Page age
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
}

/// Extract citations from web_search_tool_result data
///
/// Parses Claude's web search result format and extracts individual citations.
///
/// # Arguments
/// * `data` - The raw JSON data from web_search_tool_result
///
/// # Returns
/// Vector of extracted citations
pub fn extract_citations_from_tool_result(data: &Value) -> Vec<Citation> {
    let mut citations = Vec::new();

    // Handle nested content structure
    // Claude's format: { "content": [{ "type": "web_search_result", ... }] }
    if let Some(content) = data.get("content").and_then(|v| v.as_array()) {
        for item in content {
            if item.get("type").and_then(|v| v.as_str()) == Some("web_search_result") {
                if let (Some(url), Some(title)) = (
                    item.get("url").and_then(|v| v.as_str()),
                    item.get("title").and_then(|v| v.as_str()),
                ) {
                    citations.push(Citation {
                        url: url.to_string(),
                        title: title.to_string(),
                        snippet: item
                            .get("snippet")
                            .or_else(|| item.get("encrypted_content"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        start_index: None,
                        end_index: None,
                    });
                }
            }
        }
    }

    // Handle direct results array
    // Alternative format: { "results": [{ "url": "...", "title": "...", ... }] }
    if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
        for result in results {
            if let (Some(url), Some(title)) = (
                result.get("url").and_then(|v| v.as_str()),
                result.get("title").and_then(|v| v.as_str()),
            ) {
                citations.push(Citation {
                    url: url.to_string(),
                    title: title.to_string(),
                    snippet: result
                        .get("snippet")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    start_index: None,
                    end_index: None,
                });
            }
        }
    }

    citations
}

/// Extract citations from search_result content block
///
/// # Arguments
/// * `data` - The raw JSON data from search_result block
///
/// # Returns
/// Vector of extracted citations
pub fn extract_citations_from_search_result(data: &Value) -> Vec<Citation> {
    let mut citations = Vec::new();

    // search_result format: { "source": { "url": "...", "title": "..." }, "content": [...] }
    if let Some(source) = data.get("source") {
        if let (Some(url), Some(title)) = (
            source.get("url").and_then(|v| v.as_str()),
            source.get("title").and_then(|v| v.as_str()),
        ) {
            let content = data
                .get("content")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.get("text").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();

            citations.push(Citation {
                url: url.to_string(),
                title: title.to_string(),
                snippet: content,
                start_index: None,
                end_index: None,
            });
        }
    }

    citations
}

/// Convert citations to OpenAI annotations format
///
/// # Arguments
/// * `citations` - The citations to convert
///
/// # Returns
/// Vector of JSON values in OpenAI annotation format
pub fn citations_to_annotations(citations: &[Citation]) -> Vec<Value> {
    citations
        .iter()
        .map(|c| {
            json!({
                "type": "url_citation",
                "url_citation": {
                    "url": c.url,
                    "title": c.title,
                    "content": c.snippet,
                    "start_index": c.start_index.unwrap_or(0),
                    "end_index": c.end_index.unwrap_or(0)
                }
            })
        })
        .collect()
}

/// Convert OpenAI annotations to Claude web search format
///
/// # Arguments
/// * `annotations` - The OpenAI annotations to convert
///
/// # Returns
/// Vector of JSON values for Claude web_search_tool_result content
pub fn annotations_to_web_search_content(annotations: &[Value]) -> Vec<Value> {
    annotations
        .iter()
        .filter_map(|ann| {
            if ann.get("type").and_then(|v| v.as_str()) == Some("url_citation") {
                let citation = ann.get("url_citation")?;
                Some(json!({
                    "type": "web_search_result",
                    "url": citation.get("url"),
                    "title": citation.get("title"),
                    "snippet": citation.get("content")
                }))
            } else {
                None
            }
        })
        .collect()
}

/// Format citations as Markdown for text output
///
/// Creates a nicely formatted Markdown section with source links.
///
/// # Arguments
/// * `citations` - The citations to format
/// * `search_query` - Optional search query to display
///
/// # Returns
/// Formatted Markdown string
pub fn format_citations_as_markdown(citations: &[Citation], search_query: Option<&str>) -> String {
    if citations.is_empty() {
        return String::new();
    }

    let mut md = String::new();

    md.push_str("\n\n---\n");

    if let Some(query) = search_query {
        md.push_str(&format!("**🔍 已为您搜索：** {}\n\n", query));
    }

    md.push_str("**📚 来源：**\n");

    for (i, citation) in citations.iter().enumerate() {
        md.push_str(&format!(
            "{}. [{}]({})\n",
            i + 1,
            citation.title,
            citation.url
        ));
        if !citation.snippet.is_empty() {
            // Truncate long snippets
            let snippet = if citation.snippet.len() > 200 {
                format!("{}...", &citation.snippet[..200])
            } else {
                citation.snippet.clone()
            };
            md.push_str(&format!("   > {}\n", snippet.replace('\n', " ")));
        }
    }

    md
}

/// Merge web search results into response text
///
/// Appends formatted citations to the end of the response text.
///
/// # Arguments
/// * `text` - The original response text
/// * `citations` - The citations to append
/// * `search_query` - Optional search query
///
/// # Returns
/// Text with appended citations
pub fn merge_citations_into_text(
    text: &str,
    citations: &[Citation],
    search_query: Option<&str>,
) -> String {
    if citations.is_empty() {
        return text.to_string();
    }

    let md = format_citations_as_markdown(citations, search_query);
    format!("{}{}", text, md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_citations_from_tool_result() {
        let data = json!({
            "content": [
                {
                    "type": "web_search_result",
                    "url": "https://example.com",
                    "title": "Example Site",
                    "snippet": "This is an example"
                },
                {
                    "type": "web_search_result",
                    "url": "https://test.com",
                    "title": "Test Site",
                    "encrypted_content": "Encrypted content here"
                }
            ]
        });

        let citations = extract_citations_from_tool_result(&data);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].url, "https://example.com");
        assert_eq!(citations[0].title, "Example Site");
        assert_eq!(citations[0].snippet, "This is an example");
        assert_eq!(citations[1].snippet, "Encrypted content here");
    }

    #[test]
    fn test_extract_citations_from_results_array() {
        let data = json!({
            "results": [
                {
                    "url": "https://example.com",
                    "title": "Example",
                    "snippet": "Description"
                }
            ]
        });

        let citations = extract_citations_from_tool_result(&data);
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].url, "https://example.com");
    }

    #[test]
    fn test_extract_citations_from_search_result() {
        let data = json!({
            "source": {
                "url": "https://example.com",
                "title": "Example"
            },
            "content": [
                { "text": "First paragraph" },
                { "text": "Second paragraph" }
            ]
        });

        let citations = extract_citations_from_search_result(&data);
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].url, "https://example.com");
        assert!(citations[0].snippet.contains("First paragraph"));
        assert!(citations[0].snippet.contains("Second paragraph"));
    }

    #[test]
    fn test_citations_to_annotations() {
        let citations = vec![Citation {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            snippet: "Snippet".to_string(),
            start_index: Some(10),
            end_index: Some(20),
        }];

        let annotations = citations_to_annotations(&citations);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0]["type"], "url_citation");
        assert_eq!(
            annotations[0]["url_citation"]["url"],
            "https://example.com"
        );
        assert_eq!(annotations[0]["url_citation"]["start_index"], 10);
    }

    #[test]
    fn test_annotations_to_web_search_content() {
        let annotations = vec![json!({
            "type": "url_citation",
            "url_citation": {
                "url": "https://example.com",
                "title": "Example",
                "content": "Snippet"
            }
        })];

        let content = annotations_to_web_search_content(&annotations);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "web_search_result");
        assert_eq!(content[0]["url"], "https://example.com");
    }

    #[test]
    fn test_format_citations_as_markdown() {
        let citations = vec![
            Citation {
                url: "https://example.com".to_string(),
                title: "Example Site".to_string(),
                snippet: "This is a test".to_string(),
                start_index: None,
                end_index: None,
            },
        ];

        let md = format_citations_as_markdown(&citations, Some("test query"));
        assert!(md.contains("🔍 已为您搜索："));
        assert!(md.contains("test query"));
        assert!(md.contains("[Example Site](https://example.com)"));
        assert!(md.contains("This is a test"));
    }

    #[test]
    fn test_merge_citations_into_text() {
        let text = "Here is my response.";
        let citations = vec![Citation {
            url: "https://example.com".to_string(),
            title: "Source".to_string(),
            snippet: "Info".to_string(),
            start_index: None,
            end_index: None,
        }];

        let merged = merge_citations_into_text(text, &citations, None);
        assert!(merged.starts_with("Here is my response."));
        assert!(merged.contains("📚 来源："));
    }

    #[test]
    fn test_empty_citations() {
        let citations: Vec<Citation> = vec![];
        let md = format_citations_as_markdown(&citations, None);
        assert!(md.is_empty());

        let merged = merge_citations_into_text("text", &citations, None);
        assert_eq!(merged, "text");
    }
}