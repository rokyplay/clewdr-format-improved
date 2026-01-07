//! Image format conversion utilities
//!
//! This module provides utilities for converting between different image formats
//! used by Claude API and OpenAI API.
//!
//! Supported formats:
//! - Claude native: `{ "type": "image", "source": { "type": "base64", "media_type": "...", "data": "..." } }`
//! - OpenAI format: `{ "type": "image_url", "image_url": { "url": "data:..." or "https://..." } }`
//! - Document format: `{ "type": "document", "source": { "type": "base64", ... } }`

use crate::types::claude::{ContentBlock, DocumentSource, ImageSource, ImageUrl};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde_json::Value;

/// Supported image media types
pub const SUPPORTED_IMAGE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
    "image/bmp",
    "image/tiff",
];

/// Supported document media types
pub const SUPPORTED_DOCUMENT_TYPES: &[&str] = &[
    "application/pdf",
    "text/plain",
    "text/html",
    "text/markdown",
    "application/json",
];

/// Convert OpenAI image_url format to Claude native image format
///
/// # Arguments
/// * `image_url` - The OpenAI ImageUrl struct
///
/// # Returns
/// * `Option<ContentBlock>` - Claude Image content block, or None if conversion fails
pub fn oai_image_url_to_claude(image_url: &ImageUrl) -> Option<ContentBlock> {
    let url = &image_url.url;

    // Handle data URI
    if url.starts_with("data:") {
        let source = extract_image_from_data_uri(url)?;
        return Some(ContentBlock::Image {
            source,
            cache_control: None,
        });
    }

    // Handle HTTP/HTTPS URLs - keep as ImageUrl for now
    // A full implementation would download and convert to base64
    if url.starts_with("http://") || url.starts_with("https://") {
        // Return as-is, the API might support URL references
        return Some(ContentBlock::ImageUrl {
            image_url: image_url.clone(),
        });
    }

    None
}

/// Convert Claude native image to OpenAI image_url format
///
/// # Arguments
/// * `source` - The Claude ImageSource struct
///
/// # Returns
/// * `ContentBlock` - OpenAI ImageUrl content block
pub fn claude_image_to_oai(source: &ImageSource) -> ContentBlock {
    let data_uri = format!(
        "data:{};{},{}",
        source.media_type, source.type_, source.data
    );

    ContentBlock::ImageUrl {
        image_url: ImageUrl { url: data_uri },
    }
}

/// Convert document to image source for upload
///
/// Claude's document format can be converted to a generic upload format.
///
/// # Arguments
/// * `source` - The document source
///
/// # Returns
/// * `Option<ImageSource>` - Image source for upload, or None if conversion fails
pub fn document_to_image_source(source: &DocumentSource) -> Option<ImageSource> {
    if source.type_ != "base64" {
        return None;
    }

    let data = source.data.as_ref()?;
    let media_type = source
        .media_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Some(ImageSource {
        type_: "base64".to_string(),
        media_type,
        data: data.clone(),
    })
}

/// Extract image from data URI
///
/// Parses a data URI and extracts the base64 data and media type.
///
/// # Arguments
/// * `url` - The data URI string
///
/// # Returns
/// * `Option<ImageSource>` - Extracted image source, or None if parsing fails
pub fn extract_image_from_data_uri(url: &str) -> Option<ImageSource> {
    if !url.starts_with("data:") {
        return None;
    }

    let (metadata, base64_data) = url.split_once(',')?;
    let rest = metadata.strip_prefix("data:")?;

    // Handle optional encoding specification
    // Format: data:[<mediatype>][;base64],<data>
    let (media_type, encoding) = if let Some((mt, enc)) = rest.split_once(';') {
        (mt, enc)
    } else {
        // No encoding specified, assume base64
        (rest, "base64")
    };

    Some(ImageSource {
        type_: encoding.to_string(),
        media_type: media_type.to_string(),
        data: base64_data.to_owned(),
    })
}

/// Infer media type from file extension in URL
///
/// # Arguments
/// * `url` - The URL string
///
/// # Returns
/// * `String` - Inferred media type
pub fn infer_media_type_from_url(url: &str) -> String {
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
            "txt" => return "text/plain".to_string(),
            "html" | "htm" => return "text/html".to_string(),
            "md" | "markdown" => return "text/markdown".to_string(),
            "json" => return "application/json".to_string(),
            _ => {}
        }
    }

    // Default to octet-stream if unknown
    "application/octet-stream".to_string()
}

/// Check if a media type is a supported image type
pub fn is_supported_image_type(media_type: &str) -> bool {
    SUPPORTED_IMAGE_TYPES
        .iter()
        .any(|&t| media_type.starts_with(t))
}

/// Check if a media type is a supported document type
pub fn is_supported_document_type(media_type: &str) -> bool {
    SUPPORTED_DOCUMENT_TYPES
        .iter()
        .any(|&t| media_type.starts_with(t))
}

/// Validate base64 data
///
/// Checks if the provided string is valid base64 encoded data.
///
/// # Arguments
/// * `data` - The base64 string to validate
///
/// # Returns
/// * `bool` - True if valid base64, false otherwise
pub fn is_valid_base64(data: &str) -> bool {
    BASE64_STANDARD.decode(data).is_ok()
}

/// Convert raw bytes to base64 ImageSource
///
/// # Arguments
/// * `bytes` - The raw image bytes
/// * `media_type` - The media type of the image
///
/// # Returns
/// * `ImageSource` - The image source with base64 encoded data
pub fn bytes_to_image_source(bytes: &[u8], media_type: &str) -> ImageSource {
    ImageSource {
        type_: "base64".to_string(),
        media_type: media_type.to_string(),
        data: BASE64_STANDARD.encode(bytes),
    }
}

/// Process content blocks and extract/convert images
///
/// This function processes a vector of content blocks and:
/// - Converts ImageUrl blocks to native Image format where possible
/// - Extracts Document blocks as images
/// - Returns the processed blocks
///
/// # Arguments
/// * `blocks` - The content blocks to process
///
/// # Returns
/// * `Vec<ContentBlock>` - Processed content blocks
pub fn process_image_blocks(blocks: Vec<ContentBlock>) -> Vec<ContentBlock> {
    blocks
        .into_iter()
        .map(|block| {
            match block {
                ContentBlock::ImageUrl { image_url } => {
                    // Try to convert to native format
                    if let Some(converted) = oai_image_url_to_claude(&image_url) {
                        converted
                    } else {
                        // Keep as-is if conversion fails
                        ContentBlock::ImageUrl { image_url }
                    }
                }
                other => other,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_image_from_data_uri() {
        let uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
        let result = extract_image_from_data_uri(uri);

        assert!(result.is_some());
        let source = result.unwrap();
        assert_eq!(source.type_, "base64");
        assert_eq!(source.media_type, "image/png");
        assert_eq!(source.data, "iVBORw0KGgoAAAANSUhEUg==");
    }

    #[test]
    fn test_infer_media_type() {
        assert_eq!(
            infer_media_type_from_url("https://example.com/image.png"),
            "image/png"
        );
        assert_eq!(
            infer_media_type_from_url("https://example.com/photo.jpg"),
            "image/jpeg"
        );
        assert_eq!(
            infer_media_type_from_url("https://example.com/photo.jpeg?size=large"),
            "image/jpeg"
        );
        assert_eq!(
            infer_media_type_from_url("https://example.com/doc.pdf#page=1"),
            "application/pdf"
        );
        assert_eq!(
            infer_media_type_from_url("https://example.com/file"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_is_supported_image_type() {
        assert!(is_supported_image_type("image/png"));
        assert!(is_supported_image_type("image/jpeg"));
        assert!(is_supported_image_type("image/gif"));
        assert!(!is_supported_image_type("application/pdf"));
        assert!(!is_supported_image_type("text/plain"));
    }

    #[test]
    fn test_is_supported_document_type() {
        assert!(is_supported_document_type("application/pdf"));
        assert!(is_supported_document_type("text/plain"));
        assert!(!is_supported_document_type("image/png"));
    }

    #[test]
    fn test_bytes_to_image_source() {
        let bytes = b"test image data";
        let source = bytes_to_image_source(bytes, "image/png");

        assert_eq!(source.type_, "base64");
        assert_eq!(source.media_type, "image/png");
        assert!(is_valid_base64(&source.data));
    }

    #[test]
    fn test_oai_to_claude_data_uri() {
        let image_url = ImageUrl {
            url: "data:image/png;base64,iVBORw0KGgo=".to_string(),
        };
        let result = oai_image_url_to_claude(&image_url);

        assert!(result.is_some());
        if let Some(ContentBlock::Image { source, .. }) = result {
            assert_eq!(source.media_type, "image/png");
        } else {
            panic!("Expected Image block");
        }
    }

    #[test]
    fn test_oai_to_claude_http_url() {
        let image_url = ImageUrl {
            url: "https://example.com/image.png".to_string(),
        };
        let result = oai_image_url_to_claude(&image_url);

        assert!(result.is_some());
        if let Some(ContentBlock::ImageUrl { .. }) = result {
            // HTTP URLs are kept as ImageUrl
        } else {
            panic!("Expected ImageUrl block");
        }
    }

    #[test]
    fn test_claude_image_to_oai() {
        let source = ImageSource {
            type_: "base64".to_string(),
            media_type: "image/png".to_string(),
            data: "iVBORw0KGgo=".to_string(),
        };
        let result = claude_image_to_oai(&source);

        if let ContentBlock::ImageUrl { image_url } = result {
            assert!(image_url.url.starts_with("data:image/png;base64,"));
        } else {
            panic!("Expected ImageUrl block");
        }
    }

    #[test]
    fn test_document_to_image_source() {
        let doc = DocumentSource {
            type_: "base64".to_string(),
            media_type: Some("application/pdf".to_string()),
            data: Some("JVBERi0xLjQ=".to_string()),
            url: None,
        };
        let result = document_to_image_source(&doc);

        assert!(result.is_some());
        let source = result.unwrap();
        assert_eq!(source.media_type, "application/pdf");
    }
}