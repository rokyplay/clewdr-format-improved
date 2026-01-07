//! Global thought signature storage
//!
//! This module provides a thread-safe global storage for thought signatures
//! used in Claude's thinking mode. Signatures are used to maintain context
//! across multi-turn conversations with tool calls.
//!
//! Reference: Antigravity-Manager/src-tauri/src/proxy/mappers/signature_store.rs

use std::sync::{Mutex, OnceLock};

/// Global storage for thought signature
/// Uses OnceLock<Mutex<Option<String>>> pattern for thread-safe lazy initialization
static GLOBAL_THOUGHT_SIG: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Get the global thought signature storage
fn get_thought_sig_storage() -> &'static Mutex<Option<String>> {
    GLOBAL_THOUGHT_SIG.get_or_init(|| Mutex::new(None))
}

/// Store a thought signature (only stores if it's longer than existing)
///
/// This strategy ensures we keep the most complete signature available,
/// as longer signatures typically contain more context.
///
/// # Arguments
/// * `sig` - The signature string to store
pub fn store_thought_signature(sig: &str) {
    if sig.is_empty() {
        return;
    }
    
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        let should_store = match &*guard {
            None => true,
            Some(existing) => sig.len() > existing.len(),
        };
        if should_store {
            tracing::debug!(
                "[ThoughtSig] Storing new signature (length: {})",
                sig.len()
            );
            *guard = Some(sig.to_string());
        }
    }
}

/// Get the stored thought signature
///
/// # Returns
/// The stored signature if present, None otherwise
pub fn get_thought_signature() -> Option<String> {
    get_thought_sig_storage().lock().ok()?.clone()
}

/// Clear the stored thought signature
///
/// Useful when starting a new conversation or when the signature
/// is no longer valid.
pub fn clear_thought_signature() {
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        *guard = None;
        tracing::debug!("[ThoughtSig] Cleared signature");
    }
}

/// Check if a valid signature is stored
///
/// # Arguments
/// * `min_length` - Minimum length required to consider the signature valid
///
/// # Returns
/// true if a valid signature exists, false otherwise
pub fn has_valid_signature(min_length: usize) -> bool {
    get_thought_sig_storage()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|s| s.len() >= min_length))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_get_signature() {
        clear_thought_signature();
        
        assert!(get_thought_signature().is_none());
        
        store_thought_signature("test_signature_12345");
        assert_eq!(get_thought_signature(), Some("test_signature_12345".to_string()));
    }

    #[test]
    fn test_store_longer_signature() {
        clear_thought_signature();
        
        store_thought_signature("short");
        store_thought_signature("longer_signature");
        
        assert_eq!(get_thought_signature(), Some("longer_signature".to_string()));
    }

    #[test]
    fn test_does_not_store_shorter_signature() {
        clear_thought_signature();
        
        store_thought_signature("longer_signature");
        store_thought_signature("short");
        
        // Should still have the longer one
        assert_eq!(get_thought_signature(), Some("longer_signature".to_string()));
    }

    #[test]
    fn test_clear_signature() {
        store_thought_signature("test");
        clear_thought_signature();
        
        assert!(get_thought_signature().is_none());
    }

    #[test]
    fn test_has_valid_signature() {
        clear_thought_signature();
        
        assert!(!has_valid_signature(10));
        
        store_thought_signature("short");
        assert!(!has_valid_signature(10));
        assert!(has_valid_signature(5));
        
        store_thought_signature("longer_signature_12345");
        assert!(has_valid_signature(10));
    }

    #[test]
    fn test_empty_signature_ignored() {
        clear_thought_signature();
        store_thought_signature("valid");
        store_thought_signature("");
        
        assert_eq!(get_thought_signature(), Some("valid".to_string()));
    }
}