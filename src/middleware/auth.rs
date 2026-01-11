use axum::extract::FromRequestParts;
use axum_auth::AuthBearer;
use tracing::warn;

use crate::{config::CLEWDR_CONFIG, error::ClewdrError};

/// Extractor for the X-API-Key header used in Claude API compatibility
///
/// This struct extracts the API key from the "x-api-key" header and makes it
/// available to handlers that need to verify Claude-style authentication.
struct XApiKey(pub String);

impl<S> FromRequestParts<S> for XApiKey
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or(ClewdrError::InvalidAuth)?;
        Ok(Self(key.to_string()))
    }
}

/// Middleware guard that ensures requests have valid admin authentication
///
/// This extractor checks for a valid admin authorization token in the Bearer Auth header.
/// It can be used on routes that should only be accessible to administrators.
///
/// # Example
///
/// ```ignore
/// async fn admin_only_handler(
///     _: RequireAdminAuth,
///     // other extractors...
/// ) -> impl IntoResponse {
///     // This handler only executes if admin authentication succeeds
///     // ...
/// }
/// ```ignore
pub struct RequireAdminAuth;
impl<S> FromRequestParts<S> for RequireAdminAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let AuthBearer(key) = AuthBearer::from_request_parts(parts, &())
            .await
            .map_err(|_| ClewdrError::InvalidAuth)?;
        if !CLEWDR_CONFIG.load().admin_auth(&key) {
            warn!("Invalid admin key");
            return Err(ClewdrError::InvalidAuth);
        }
        Ok(Self)
    }
}

/// Middleware guard that ensures requests have valid OpenAI API authentication
///
/// This extractor validates either the Bearer token or X-API-Key header against the configured API keys.
/// It supports both OpenAI-style (Bearer) and Claude-style (x-api-key) authentication.
/// It's used to protect OpenAI-compatible API endpoints.
///
/// # Example
///
/// ```ignore
/// async fn openai_handler(
///     _: RequireOaiAuth,
///     // other extractors...
/// ) -> impl IntoResponse {
///     // This handler only executes if OpenAI authentication succeeds
///     // ...
/// }
/// ```ignore
pub struct RequireBearerAuth;
impl<S> FromRequestParts<S> for RequireBearerAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        // Try Bearer token first
        if let Ok(AuthBearer(key)) = AuthBearer::from_request_parts(parts, &()).await {
            if CLEWDR_CONFIG.load().user_auth(&key) {
                return Ok(Self);
            }
            warn!("Invalid Bearer key: {}", key);
            return Err(ClewdrError::InvalidAuth);
        }
        
        // Fall back to X-API-Key (for flexibility)
        if let Ok(XApiKey(key)) = XApiKey::from_request_parts(parts, &()).await {
            if CLEWDR_CONFIG.load().user_auth(&key) {
                return Ok(Self);
            }
            warn!("Invalid x-api-key: {}", key);
            return Err(ClewdrError::InvalidAuth);
        }
        
        // Neither auth method provided
        Err(ClewdrError::InvalidAuth)
    }
}

/// Middleware guard that ensures requests have valid Claude API authentication
///
/// This extractor validates either the X-API-Key header or Bearer token against the configured API keys.
/// It supports both Claude-style (x-api-key) and OpenAI-style (Bearer) authentication.
/// This is required because Claude Code CLI uses Bearer token but sends to /v1/messages endpoint.
pub struct RequireXApiKeyAuth;
impl<S> FromRequestParts<S> for RequireXApiKeyAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        // Try X-API-Key first
        if let Ok(XApiKey(key)) = XApiKey::from_request_parts(parts, &()).await {
            if CLEWDR_CONFIG.load().user_auth(&key) {
                return Ok(Self);
            }
            warn!("Invalid x-api-key: {}", key);
            return Err(ClewdrError::InvalidAuth);
        }
        
        // Fall back to Bearer token (for Claude Code CLI compatibility)
        if let Ok(AuthBearer(key)) = AuthBearer::from_request_parts(parts, &()).await {
            if CLEWDR_CONFIG.load().user_auth(&key) {
                return Ok(Self);
            }
            warn!("Invalid Bearer key: {}", key);
            return Err(ClewdrError::InvalidAuth);
        }
        
        // Neither auth method provided
        Err(ClewdrError::InvalidAuth)
    }
}
