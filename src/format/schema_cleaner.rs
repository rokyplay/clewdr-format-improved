//! JSON Schema cleaning utilities
//!
//! This module provides utilities for cleaning JSON schemas to ensure
//! compatibility with different API providers (Claude, Gemini, OpenAI).
//! Some providers don't support all JSON Schema features, so we need
//! to clean/transform schemas for compatibility.
//!
//! Reference: 
//! - antigravity-claude-proxy/src/format/schema-sanitizer.js
//! - claude-code-router/packages/core/src/utils/gemini.util.ts

use serde_json::{Value, json};

/// Keywords not supported by some API providers (especially Gemini)
const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "default",
    "$schema",
    "$defs",
    "definitions",
    "$ref",
    "$id",
    "$comment",
    "title",
    "minLength",
    "maxLength",
    "pattern",
    "format",
    "minItems",
    "maxItems",
    "examples",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "if",
    "then",
    "else",
    "dependentSchemas",
    "dependentRequired",
    "unevaluatedProperties",
    "unevaluatedItems",
    "contentMediaType",
    "contentEncoding",
    "const",
];

/// Valid fields that should be preserved
/// Reference: claude-code-router validFields
#[allow(dead_code)]
const VALID_FIELDS: &[&str] = &[
    "type",
    "format",  // Some providers support this
    "description",
    "nullable",
    "enum",
    "maxItems",
    "minItems",
    "properties",
    "required",
    "minProperties",
    "maxProperties",
    "minLength",
    "maxLength",
    "pattern",
    "example",
    "anyOf",
    "propertyOrdering",
    "default",
    "items",
    "minimum",
    "maximum",
];

/// Clean a JSON Schema for compatibility with target API
///
/// This function recursively processes a JSON schema and removes
/// unsupported keywords while preserving the essential structure.
///
/// # Arguments
/// * `schema` - The schema to clean (modified in place)
///
/// # Example
/// ```rust
/// use serde_json::json;
/// use clewdr::format::clean_json_schema;
///
/// let mut schema = json!({
///     "type": "object",
///     "$schema": "http://json-schema.org/draft-07/schema#",
///     "additionalProperties": false,
///     "properties": {
///         "name": { "type": "string", "minLength": 1 }
///     }
/// });
///
/// clean_json_schema(&mut schema);
/// // $schema and additionalProperties are removed
/// // minLength is kept in description if preserve_constraints is implemented
/// ```
pub fn clean_json_schema(schema: &mut Value) {
    clean_json_schema_recursive(schema);
}

fn clean_json_schema_recursive(schema: &mut Value) {
    if !schema.is_object() {
        return;
    }

    let obj = schema.as_object_mut().unwrap();

    // Remove unsupported keywords
    for keyword in UNSUPPORTED_KEYWORDS {
        obj.remove(*keyword);
    }

    // Handle type arrays: ["string", "null"] -> "string" with nullable: true
    if let Some(type_val) = obj.get("type").cloned() {
        if let Some(arr) = type_val.as_array() {
            let has_null = arr.iter().any(|v| v.as_str() == Some("null"));
            let non_null: Vec<_> = arr
                .iter()
                .filter(|v| v.as_str() != Some("null"))
                .cloned()
                .collect();
            
            if has_null {
                obj.insert("nullable".to_string(), json!(true));
            }
            
            if non_null.len() == 1 {
                obj.insert("type".to_string(), non_null[0].clone());
            } else if non_null.len() > 1 {
                // Convert to anyOf format
                let any_of: Vec<Value> = non_null
                    .iter()
                    .map(|t| json!({ "type": t }))
                    .collect();
                obj.remove("type");
                obj.insert("anyOf".to_string(), json!(any_of));
            }
        }
    }

    // Recursively process nested schemas
    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_, prop_schema) in props_obj.iter_mut() {
                clean_json_schema_recursive(prop_schema);
            }
        }
    }

    // Process items (for array types)
    if let Some(items) = obj.get_mut("items") {
        if items.is_object() {
            clean_json_schema_recursive(items);
        } else if items.is_array() {
            for item in items.as_array_mut().unwrap() {
                clean_json_schema_recursive(item);
            }
        }
    }

    // Process anyOf/oneOf/allOf if they weren't removed
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(arr) = obj.get_mut(key) {
            if let Some(arr) = arr.as_array_mut() {
                for item in arr.iter_mut() {
                    clean_json_schema_recursive(item);
                }
            }
        }
    }
}

/// Ensure a schema is valid and has required fields
///
/// If the schema is empty or missing required fields, this function
/// adds placeholder properties to make it valid.
///
/// # Arguments
/// * `schema` - The schema to validate and fix (modified in place)
pub fn ensure_valid_schema(schema: &mut Value) {
    if !schema.is_object() {
        *schema = create_placeholder_schema();
        return;
    }

    let obj = schema.as_object_mut().unwrap();

    // Ensure type exists
    if !obj.contains_key("type") {
        obj.insert("type".to_string(), json!("object"));
    }

    // If object type but no properties, add placeholder
    if obj.get("type").and_then(|v| v.as_str()) == Some("object") {
        let has_properties = obj
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|o| !o.is_empty())
            .unwrap_or(false);

        if !has_properties {
            obj.insert(
                "properties".to_string(),
                json!({
                    "reason": {
                        "type": "string",
                        "description": "Reason for calling this tool"
                    }
                }),
            );
            obj.insert("required".to_string(), json!(["reason"]));
        }
    }
}

/// Create a placeholder schema for tools without input schema
fn create_placeholder_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "reason": {
                "type": "string",
                "description": "Reason for calling this tool"
            }
        },
        "required": ["reason"]
    })
}

/// Move constraints to description (for APIs that don't support constraint keywords)
///
/// This function extracts constraint keywords and adds them to the description
/// field, then removes the original constraint keywords.
///
/// # Arguments
/// * `schema` - The schema to process (modified in place)
pub fn move_constraints_to_description(schema: &mut Value) {
    move_constraints_recursive(schema);
}

fn move_constraints_recursive(schema: &mut Value) {
    if !schema.is_object() {
        return;
    }

    let obj = schema.as_object_mut().unwrap();
    let mut constraint_notes: Vec<String> = Vec::new();

    // Collect constraint information
    if let Some(min_length) = obj.get("minLength").and_then(|v| v.as_u64()) {
        constraint_notes.push(format!("Minimum length: {}", min_length));
    }
    if let Some(max_length) = obj.get("maxLength").and_then(|v| v.as_u64()) {
        constraint_notes.push(format!("Maximum length: {}", max_length));
    }
    if let Some(pattern) = obj.get("pattern").and_then(|v| v.as_str()) {
        constraint_notes.push(format!("Pattern: {}", pattern));
    }
    if let Some(minimum) = obj.get("minimum").and_then(|v| v.as_f64()) {
        constraint_notes.push(format!("Minimum: {}", minimum));
    }
    if let Some(maximum) = obj.get("maximum").and_then(|v| v.as_f64()) {
        constraint_notes.push(format!("Maximum: {}", maximum));
    }
    if let Some(min_items) = obj.get("minItems").and_then(|v| v.as_u64()) {
        constraint_notes.push(format!("Minimum items: {}", min_items));
    }
    if let Some(max_items) = obj.get("maxItems").and_then(|v| v.as_u64()) {
        constraint_notes.push(format!("Maximum items: {}", max_items));
    }

    // Add constraints to description
    if !constraint_notes.is_empty() {
        let existing_desc = obj
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        let new_desc = if existing_desc.is_empty() {
            constraint_notes.join(". ")
        } else {
            format!("{}. Constraints: {}", existing_desc, constraint_notes.join(", "))
        };
        
        obj.insert("description".to_string(), json!(new_desc));
    }

    // Recursively process nested schemas
    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_, prop_schema) in props_obj.iter_mut() {
                move_constraints_recursive(prop_schema);
            }
        }
    }

    if let Some(items) = obj.get_mut("items") {
        move_constraints_recursive(items);
    }
}

/// Expand $ref references inline
///
/// This function resolves $ref references within the schema and
/// replaces them with the actual definition content.
///
/// # Arguments
/// * `schema` - The schema with definitions
///
/// # Returns
/// A new schema with all $refs expanded
pub fn expand_refs(schema: &Value) -> Value {
    let definitions = schema
        .get("$defs")
        .or_else(|| schema.get("definitions"))
        .cloned()
        .unwrap_or(json!({}));

    let mut result = schema.clone();
    expand_refs_recursive(&mut result, &definitions);
    
    // Remove definition keys from result
    if let Some(obj) = result.as_object_mut() {
        obj.remove("$defs");
        obj.remove("definitions");
    }
    
    result
}

fn expand_refs_recursive(schema: &mut Value, definitions: &Value) {
    if !schema.is_object() {
        return;
    }

    let obj = schema.as_object_mut().unwrap();

    // Check for $ref and expand it
    if let Some(ref_path) = obj.remove("$ref") {
        if let Some(ref_str) = ref_path.as_str() {
            // Parse ref path like "#/$defs/MyType" or "#/definitions/MyType"
            let parts: Vec<&str> = ref_str.split('/').collect();
            if parts.len() >= 3 && parts[0] == "#" {
                let def_name = parts.last().unwrap();
                if let Some(definition) = definitions.get(*def_name) {
                    // Merge definition into current schema
                    if let Some(def_obj) = definition.as_object() {
                        for (key, value) in def_obj {
                            if !obj.contains_key(key) {
                                obj.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    // Recursively process nested schemas
    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_, prop_schema) in props_obj.iter_mut() {
                expand_refs_recursive(prop_schema, definitions);
            }
        }
    }

    if let Some(items) = obj.get_mut("items") {
        expand_refs_recursive(items, definitions);
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(arr) = obj.get_mut(key) {
            if let Some(arr) = arr.as_array_mut() {
                for item in arr.iter_mut() {
                    expand_refs_recursive(item, definitions);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_clean_removes_unsupported_keywords() {
        let mut schema = json!({
            "type": "object",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$id": "test",
            "additionalProperties": false,
            "properties": {
                "name": { "type": "string" }
            }
        });

        clean_json_schema(&mut schema);

        assert!(schema.get("$schema").is_none());
        assert!(schema.get("$id").is_none());
        assert!(schema.get("additionalProperties").is_none());
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_clean_handles_type_arrays() {
        let mut schema = json!({
            "type": ["string", "null"]
        });

        clean_json_schema(&mut schema);

        assert_eq!(schema["type"], "string");
        assert_eq!(schema["nullable"], true);
    }

    #[test]
    fn test_clean_handles_multiple_types() {
        let mut schema = json!({
            "type": ["string", "number"]
        });

        clean_json_schema(&mut schema);

        assert!(schema.get("type").is_none());
        assert!(schema.get("anyOf").is_some());
    }

    #[test]
    fn test_ensure_valid_schema_empty() {
        let mut schema = json!({});

        ensure_valid_schema(&mut schema);

        assert_eq!(schema["type"], "object");
        assert!(schema.get("properties").is_some());
        assert!(schema.get("required").is_some());
    }

    #[test]
    fn test_ensure_valid_schema_non_object() {
        let mut schema = json!("not an object");

        ensure_valid_schema(&mut schema);

        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_move_constraints_to_description() {
        let mut schema = json!({
            "type": "string",
            "minLength": 1,
            "maxLength": 100,
            "description": "A name"
        });

        move_constraints_to_description(&mut schema);

        let desc = schema["description"].as_str().unwrap();
        assert!(desc.contains("Minimum length: 1"));
        assert!(desc.contains("Maximum length: 100"));
    }

    #[test]
    fn test_expand_refs() {
        let schema = json!({
            "$defs": {
                "Address": {
                    "type": "object",
                    "properties": {
                        "street": { "type": "string" }
                    }
                }
            },
            "type": "object",
            "properties": {
                "home": { "$ref": "#/$defs/Address" }
            }
        });

        let expanded = expand_refs(&schema);

        assert!(expanded.get("$defs").is_none());
        assert_eq!(
            expanded["properties"]["home"]["type"],
            "object"
        );
    }

    #[test]
    fn test_recursive_cleaning() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "inner": {
                    "type": "object",
                    "$comment": "should be removed",
                    "properties": {
                        "deep": {
                            "type": ["string", "null"]
                        }
                    }
                }
            }
        });

        clean_json_schema(&mut schema);

        assert!(schema["properties"]["inner"].get("$comment").is_none());
        assert_eq!(schema["properties"]["inner"]["properties"]["deep"]["type"], "string");
        assert_eq!(schema["properties"]["inner"]["properties"]["deep"]["nullable"], true);
    }
}