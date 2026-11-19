//! Shared, user-facing error text (port of `src/lib/errors.ts`).
//!
//! Backend failures arrive as structured `CommandError`-shaped JSON. The UI
//! must show the real user message, never a debug type representation such as
//! `CommandError { code: ..., .. }`.

use serde::Serialize;
use serde_json::Value;

/// Extracts a readable message from a structured value, preferring
/// `userMessage`, then `message`, then a nested `data` payload (recursively).
pub fn error_text(value: &Value) -> String {
    if let Some(text) = read(value) {
        return text;
    }
    match value {
        Value::String(raw) if !raw.trim().is_empty() => raw.clone(),
        Value::Object(_) => {
            let serialized = value.to_string();
            if serialized != "{}" && serialized != "null" {
                serialized
            } else {
                "Unknown error".to_string()
            }
        }
        _ => "Unknown error".to_string(),
    }
}

/// Convenience for any serializable error (e.g. `BackendError`).
pub fn error_text_of<T: Serialize>(error: &T) -> String {
    match serde_json::to_value(error) {
        Ok(value) => error_text(&value),
        Err(_) => "Unknown error".to_string(),
    }
}

fn read(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) if !raw.trim().is_empty() => Some(raw.clone()),
        Value::Object(map) => {
            // Same priority as the source: userMessage first, then message.
            if let Some(direct) = map
                .get("userMessage")
                .and_then(read)
                .or_else(|| map.get("message").and_then(read))
            {
                return Some(direct);
            }
            map.get("data").and_then(read)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{error_text, error_text_of};
    use crate::backend::BackendError;
    use serde_json::{Value, json};

    #[test]
    fn prefers_user_message() {
        let value = json!({
            "code": "network",
            "message": "connection refused at 127.0.0.1:1234",
            "userMessage": "Cannot reach LM Studio. Check that the local server is running.",
            "retryable": true
        });
        assert_eq!(
            error_text(&value),
            "Cannot reach LM Studio. Check that the local server is running."
        );
    }

    #[test]
    fn falls_back_to_message() {
        let value = json!({ "message": "Crop height must be positive", "userMessage": "" });
        assert_eq!(error_text(&value), "Crop height must be positive");
    }

    #[test]
    fn unwraps_nested_data_message() {
        let value = json!({
            "userMessage": null,
            "message": null,
            "data": { "userMessage": "Nested validation text" }
        });
        assert_eq!(error_text(&value), "Nested validation text");
    }

    #[test]
    fn never_shows_a_debug_type_representation() {
        let value = json!({
            "code": "validation",
            "category": "validation",
            "message": "Mask radius is out of range",
            "userMessage": "Mask radius is out of range"
        });
        let text = error_text(&value);
        assert!(!text.contains("CommandError"));
        assert!(!text.contains("{"));
        assert_eq!(text, "Mask radius is out of range");
    }

    #[test]
    fn plain_string_and_unknown() {
        assert_eq!(error_text(&json!("boom")), "boom");
        assert_eq!(error_text(&json!({})), "Unknown error");
        assert_eq!(error_text(&Value::Null), "Unknown error");
    }

    #[test]
    fn accepts_a_backend_error_directly() {
        let error = BackendError::validation("endpoint must be a URL");
        assert_eq!(error_text_of(&error), "endpoint must be a URL");
    }
}
