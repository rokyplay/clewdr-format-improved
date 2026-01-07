use std::{fmt::Write, mem};

use base64::{Engine, prelude::BASE64_STANDARD};
use futures::{StreamExt, stream};
use itertools::Itertools;
use serde_json::Value;
use tracing::warn;
use wreq::multipart::{Form, Part};

use crate::{
    claude_web_state::ClaudeWebState,
    config::CLEWDR_CONFIG,
    types::{
        claude::{ContentBlock, CreateMessageParams, ImageSource, Message, MessageContent, Role},
        claude_web::request::*,
    },
    utils::{TIME_ZONE, print_out_text},
};

impl ClaudeWebState {
    pub fn transform_request(&self, mut value: CreateMessageParams) -> Option<WebRequestBody> {
        let system = value.system.take();
        let msgs = mem::take(&mut value.messages);
        let system = merge_system(system.unwrap_or_default());
        let merged = merge_messages(msgs, system)?;

        let mut tools = vec![];
        if CLEWDR_CONFIG.load().web_search {
            tools.push(Tool::web_search());
        }
        Some(WebRequestBody {
            max_tokens_to_sample: value.max_tokens,
            attachments: vec![Attachment::new(merged.paste)],
            files: vec![],
            model: if self.is_pro() {
                Some(value.model)
            } else {
                None
            },
            rendering_mode: if value.stream.unwrap_or_default() {
                "messages".to_string()
            } else {
                "raw".to_string()
            },
            prompt: merged.prompt,
            timezone: TIME_ZONE.to_string(),
            images: merged.images,
            tools,
        })
    }

    /// Upload images to the Claude.ai
    pub async fn upload_images(&self, imgs: Vec<ImageSource>) -> Vec<String> {
        // upload images
        stream::iter(imgs)
            .filter_map(async |img| {
                // check if the image is base64
                if img.type_ != "base64" {
                    warn!("Image type is not base64");
                    return None;
                }
                // decode the image
                let bytes = BASE64_STANDARD
                    .decode(img.data)
                    .inspect_err(|e| {
                        warn!("Failed to decode image: {}", e);
                    })
                    .ok()?;
                // choose the file name based on the media type
                let file_name = match img.media_type.to_lowercase().as_str() {
                    "image/png" => "image.png",
                    "image/jpeg" => "image.jpg",
                    "image/jpg" => "image.jpg",
                    "image/gif" => "image.gif",
                    "image/webp" => "image.webp",
                    "application/pdf" => "document.pdf",
                    _ => "file",
                };
                // create the part and form
                let part = Part::bytes(bytes).file_name(file_name);
                let form = Form::new().part("file", part);
                let endpoint = self
                    .endpoint
                    .join(&format!("api/{}/upload", self.org_uuid.as_ref()?))
                    .expect("Url parse error");
                // send the request into future
                let res = self
                    .build_request(http::Method::POST, endpoint)
                    .multipart(form)
                    .send()
                    .await
                    .inspect_err(|e| {
                        warn!("Failed to upload image: {}", e);
                    })
                    .ok()?;
                #[derive(serde::Deserialize)]
                struct UploadResponse {
                    file_uuid: String,
                }
                // get the response json
                let json = res
                    .json::<UploadResponse>()
                    .await
                    .inspect_err(|e| {
                        warn!("Failed to parse image response: {}", e);
                    })
                    .ok()?;
                // extract the file_uuid
                Some(json.file_uuid)
            })
            .collect::<Vec<_>>()
            .await
    }
}

/// Merged messages and images
#[derive(Default, Debug)]
struct Merged {
    pub paste: String,
    pub prompt: String,
    pub images: Vec<ImageSource>,
}

/// Merges multiple messages into a single text prompt, handling system instructions
/// and extracting any images from the messages
///
/// # Arguments
/// * `msgs` - Vector of messages to merge
/// * `system` - System instructions to prepend
///
/// # Returns
/// * `Option<Merged>` - Merged prompt text, images, and additional metadata, or None if merging fails
fn merge_messages(msgs: Vec<Message>, system: String) -> Option<Merged> {
    if msgs.is_empty() {
        return None;
    }
    let h = CLEWDR_CONFIG
        .load()
        .custom_h
        .to_owned()
        .unwrap_or("Human".to_string());
    let a = CLEWDR_CONFIG
        .load()
        .custom_a
        .to_owned()
        .unwrap_or("Assistant".to_string());

    let user_real_roles = CLEWDR_CONFIG.load().use_real_roles;
    let line_breaks = if user_real_roles { "\n\n\x08" } else { "\n\n" };
    let system = system.trim().to_string();
    let size = size_of_val(&msgs);
    // preallocate string to avoid reallocations
    let mut w = String::with_capacity(size);

    let mut imgs: Vec<ImageSource> = vec![];

    let chunks = msgs
        .into_iter()
        .filter_map(|m| match m.content {
            MessageContent::Blocks { content } => {
                // collect all text blocks, join them with new line
                let blocks = content
                    .into_iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text, .. } => Some(text.trim().to_string()),
                        ContentBlock::Image { source, .. } => {
                            // push image to the list
                            imgs.push(source);
                            None
                        }
                        ContentBlock::ImageUrl { image_url } => {
                            // oai image - supports both data URI and HTTP URLs
                            if let Some(source) = extract_image_from_url(&image_url.url) {
                                imgs.push(source);
                            }
                            None
                        }
                        ContentBlock::Document { source, .. } => {
                            // Document content (PDF, etc.)
                            // Convert to ImageSource format for upload
                            if source.type_ == "base64" {
                                if let Some(data) = source.data {
                                    imgs.push(ImageSource {
                                        type_: "base64".to_string(),
                                        media_type: source.media_type.unwrap_or_else(|| "application/pdf".to_string()),
                                        data,
                                    });
                                }
                            }
                            None
                        }
                        ContentBlock::Thinking { thinking, .. } => {
                            // Include thinking content in text (for debugging/visibility)
                            // Skip if empty
                            if thinking.trim().is_empty() {
                                None
                            } else {
                                Some(format!("<thinking>{}</thinking>", thinking.trim()))
                            }
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if blocks.is_empty() {
                    None
                } else {
                    Some((m.role, blocks))
                }
            }
            MessageContent::Text { content } => {
                // plain text
                let content = content.trim().to_string();
                if content.is_empty() {
                    None
                } else {
                    Some((m.role, content))
                }
            }
        })
        // chunk by role
        .chunk_by(|m| m.0);
    // join same role with new line
    let mut msgs = chunks.into_iter().map(|(role, grp)| {
        let txt = grp.into_iter().map(|m| m.1).collect::<Vec<_>>().join("\n");
        (role, txt)
    });
    // first message does not need prefix
    if !system.is_empty() {
        w += system.as_str();
    } else {
        let first = msgs.next()?;
        w += first.1.as_str();
    }
    for (role, text) in msgs {
        let prefix = match role {
            Role::System => {
                warn!("System message should be merged into the first message");
                continue;
            }
            Role::User => format!("{h}: "),
            Role::Assistant => format!("{a}: "),
        };
        write!(w, "{line_breaks}{prefix}{text}").ok()?;
    }
    print_out_text(w.to_owned(), "paste.txt");

    // prompt polyfill
    let p = CLEWDR_CONFIG.load().custom_prompt.to_owned();

    Some(Merged {
        paste: w,
        prompt: p,
        images: imgs,
    })
}

/// Merges system message content into a single string
/// Handles both string and array formats for system messages
///
/// # Arguments
/// * `sys` - System message content as a JSON Value
///
/// # Returns
/// Merged system message as a string
fn merge_system(sys: Value) -> String {
    match sys {
        Value::String(s) => s,
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v["text"].as_str())
            .map(|v| v.trim())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Extract image from URL (data URI or HTTP URL)
///
/// Supports:
/// - Data URIs: `data:image/png;base64,iVBORw0KGgo...`
/// - HTTP/HTTPS URLs: Downloads and converts to base64
///
/// # Arguments
/// * `url` - The image URL or data URI
///
/// # Returns
/// * `Option<ImageSource>` - Extracted image source, or None if extraction fails
fn extract_image_from_url(url: &str) -> Option<ImageSource> {
    // Handle data URI
    if url.starts_with("data:") {
        return extract_image_from_data_uri(url);
    }

    // Handle HTTP/HTTPS URLs
    // Note: For now, we log a warning and return None
    // A full implementation would require async downloading
    if url.starts_with("http://") || url.starts_with("https://") {
        // For HTTP URLs, we need to infer the media type from the URL or headers
        // This is a placeholder - actual implementation would need async download
        warn!("HTTP image URLs are not yet supported for direct download: {}", url);
        
        // Try to infer media type from extension
        let media_type = infer_media_type_from_url(url);
        
        // Return a placeholder that indicates URL-based image
        // The caller should handle this appropriately
        return Some(ImageSource {
            type_: "url".to_string(),
            media_type,
            data: url.to_string(), // Store URL in data field for URL type
        });
    }

    None
}

/// Extract image from data URI
fn extract_image_from_data_uri(url: &str) -> Option<ImageSource> {
    let (metadata, base64_data) = url.split_once(',')?;
    let (media_type, type_) = metadata.strip_prefix("data:")?.split_once(';')?;

    Some(ImageSource {
        type_: type_.to_string(),
        media_type: media_type.to_string(),
        data: base64_data.to_owned(),
    })
}

/// Infer media type from URL extension
fn infer_media_type_from_url(url: &str) -> String {
    // Remove query string and fragments
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    
    // Get extension
    if let Some(ext) = path.rsplit('.').next() {
        match ext.to_lowercase().as_str() {
            "png" => return "image/png".to_string(),
            "jpg" | "jpeg" => return "image/jpeg".to_string(),
            "gif" => return "image/gif".to_string(),
            "webp" => return "image/webp".to_string(),
            "svg" => return "image/svg+xml".to_string(),
            "bmp" => return "image/bmp".to_string(),
            "ico" => return "image/x-icon".to_string(),
            "tiff" | "tif" => return "image/tiff".to_string(),
            "pdf" => return "application/pdf".to_string(),
            _ => {}
        }
    }
    
    // Default to octet-stream if unknown
    "application/octet-stream".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_image_from_data_uri() {
        let data_uri = "data:image/png;base64,iVBORw0KGgo";
        let result = extract_image_from_url(data_uri);
        
        assert!(result.is_some());
        let source = result.unwrap();
        assert_eq!(source.type_, "base64");
        assert_eq!(source.media_type, "image/png");
        assert_eq!(source.data, "iVBORw0KGgo");
    }

    #[test]
    fn test_extract_image_from_http_url() {
        let url = "https://example.com/image.png";
        let result = extract_image_from_url(url);
        
        assert!(result.is_some());
        let source = result.unwrap();
        assert_eq!(source.type_, "url");
        assert_eq!(source.media_type, "image/png");
        assert_eq!(source.data, url);
    }

    #[test]
    fn test_infer_media_type() {
        assert_eq!(infer_media_type_from_url("https://example.com/image.png"), "image/png");
        assert_eq!(infer_media_type_from_url("https://example.com/photo.jpg"), "image/jpeg");
        assert_eq!(infer_media_type_from_url("https://example.com/photo.jpeg?size=large"), "image/jpeg");
        assert_eq!(infer_media_type_from_url("https://example.com/doc.pdf#page=1"), "application/pdf");
        assert_eq!(infer_media_type_from_url("https://example.com/file"), "application/octet-stream");
    }

    #[test]
    fn test_invalid_url() {
        assert!(extract_image_from_url("not-a-url").is_none());
        assert!(extract_image_from_url("ftp://example.com/file").is_none());
    }
}
