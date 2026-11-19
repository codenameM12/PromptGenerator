//! Rust port of `src/lib/errors.ts`.
//!
//! Behaviour-preserving: a real `Error` wins on its message, a non-blank string
//! is returned verbatim, structured objects unwrap `userMessage` → `message` →
//! `data` (each level recursive through the same reader), and anything else is
//! JSON-serialized with `{}`/`null` mapped to "Unknown error". Fixes belong in
//! `PORTING.md`, not here.

use serde_json::Value;

fn read(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Object(_) => read_object(value),
        _ => None,
    }
}

fn read_optional(value: Option<&Value>) -> Option<String> {
    value.and_then(read)
}

fn read_object(object: &Value) -> Option<String> {
    let map = object.as_object()?;
    read_optional(map.get("userMessage")).or_else(|| read_optional(map.get("message")))
}

/// Mirrors the object/string/serialization tail of the TypeScript `errorText`.
/// A real JS `Error` cannot be represented as JSON; callers holding an error
/// message should use it directly (that branch returns `cause.message`).
pub fn error_text_json(value: &Value) -> String {
    if let Some(text) = read(value) {
        return text;
    }
    if let Value::Object(map) = value {
        if let Some(direct) =
            read_optional(map.get("userMessage")).or_else(|| read_optional(map.get("message")))
        {
            return direct;
        }
        if let Some(nested) = read_optional(map.get("data")) {
            return nested;
        }
    }
    match serde_json::to_string(value) {
        Ok(serialized) if !serialized.is_empty() && serialized != "{}" && serialized != "null" => {
            serialized
        }
        _ => "Unknown error".into(),
    }
}

/// A JS `Error` instance: the source returns `cause.message` even when blank.
pub fn error_text_message(message: &str) -> String {
    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    enum ErrorInput {
        Error {
            message: String,
        },
        #[serde(rename = "string")]
        Str {
            value: String,
        },
        Value {
            value: Value,
        },
        Undefined,
        Circular,
    }

    fn error_text(input: &ErrorInput) -> String {
        match input {
            ErrorInput::Error { message } => error_text_message(message),
            ErrorInput::Str { value } => error_text_json(&Value::String(value.clone())),
            ErrorInput::Value { value } => error_text_json(value),
            // JSON.stringify(undefined) is undefined (falsy) and a circular
            // value makes it throw; both fall through to the final fallback.
            ErrorInput::Undefined | ErrorInput::Circular => "Unknown error".into(),
        }
    }

    #[test]
    fn prefers_user_message_over_message() {
        let value = json!({
            "code": "validation",
            "message": "raw reason",
            "userMessage": "friendly reason",
        });
        assert_eq!(error_text_json(&value), "friendly reason");
    }

    #[test]
    fn falls_back_to_message_then_nested_data() {
        assert_eq!(
            error_text_json(&json!({ "message": "model missing" })),
            "model missing"
        );
        assert_eq!(
            error_text_json(&json!({ "data": { "message": "nested reason" } })),
            "nested reason"
        );
    }

    #[test]
    fn serializes_unknown_objects_but_not_empty_ones() {
        assert_eq!(error_text_json(&json!({})), "Unknown error");
        assert_eq!(error_text_json(&json!({ "a": 1 })), "{\"a\":1}");
        assert_eq!(error_text_json(&json!(null)), "Unknown error");
    }

    #[test]
    fn reads_a_real_error_message_even_when_blank() {
        assert_eq!(error_text_message("boom"), "boom");
        assert_eq!(error_text_message(""), "");
    }

    #[derive(Debug, Deserialize)]
    struct Case {
        label: String,
        input: ErrorInput,
        output: String,
    }

    #[derive(Debug, Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }

    #[test]
    fn matches_the_recorded_typescript_behaviour() {
        let corpus: Corpus = serde_json::from_str(include_str!("../../../port/corpus/errors.json"))
            .expect("parse errors corpus");

        let mut compared = 0usize;
        let mut differences: Vec<String> = Vec::new();
        for case in &corpus.cases {
            compared += 1;
            let actual = error_text(&case.input);
            if actual != case.output {
                differences.push(format!(
                    "{}: expected {:?}, got {:?}",
                    case.label, case.output, actual
                ));
            }
        }

        assert!(
            differences.is_empty(),
            "errors: {compared} inputs compared, {} differences\n{differences:#?}",
            differences.len()
        );
        println!(
            "errors: {compared} inputs compared, {} differences",
            differences.len()
        );
    }
}
