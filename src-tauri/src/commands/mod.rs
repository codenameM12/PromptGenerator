pub mod assets;
pub mod datasets;
pub mod diagnostics;
pub mod drag;
pub mod edits;
pub mod jobs;
pub mod library;
pub mod models;
pub mod pipeline;
pub mod presets;
pub mod settings;
pub mod status;
pub mod workspace;

use serde::{Deserialize, Serialize};

/// Guidance merged in increasing priority: global, preset, run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guidance {
    pub global: Option<String>,
    pub preset: Option<String>,
    pub run: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptWarning {
    pub code: String,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCategory {
    Validation,
    NotFound,
    ModelMissing,
    ContextOverflow,
    InvalidResponse,
    Conflict,
    Unauthorized,
    Forbidden,
    Io,
    Media,
    Runtime,
    Network,
    Timeout,
    Cancelled,
    Internal,
    NotImplemented,
}

impl ErrorCategory {
    /// Whether retrying the same request may succeed without user changes.
    pub fn retryable(self) -> bool {
        matches!(self, Self::Network | Self::Timeout | Self::Runtime)
    }
}

/// Error shape returned by every IPC command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub category: ErrorCategory,
    pub message: String,
    pub user_message: String,
    pub retryable: bool,
    pub details: Option<String>,
}

impl CommandError {
    fn base(category: ErrorCategory, code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: code.to_string(),
            category,
            user_message: message.clone(),
            message,
            retryable: category.retryable(),
            details: None,
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Validation, "validation", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::NotFound, "not_found", message)
    }

    pub fn model_missing(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::ModelMissing, "model_missing", message)
    }

    pub fn context_overflow(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::ContextOverflow, "context_overflow", message)
    }

    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::InvalidResponse, "invalid_response", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Unauthorized, "unauthorized", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Conflict, "conflict", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Forbidden, "forbidden", message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Io, "io", message)
    }

    pub fn media(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Media, "media", message)
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        let mut error = Self::base(ErrorCategory::Runtime, "runtime", message);
        error.user_message = "The model runtime returned an error. Try again.".into();
        error
    }

    pub fn network(message: impl Into<String>) -> Self {
        let mut error = Self::base(ErrorCategory::Network, "network", message);
        error.user_message =
            "Cannot reach LM Studio. Check that the local server is running.".into();
        error
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        let mut error = Self::base(ErrorCategory::Timeout, "timeout", message);
        error.user_message = "The local model did not respond in time. Try again.".into();
        error
    }

    pub fn cancelled() -> Self {
        let mut error = Self::base(ErrorCategory::Cancelled, "cancelled", "Operation cancelled");
        error.user_message = "The operation was cancelled.".into();
        error
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::base(ErrorCategory::Internal, "internal", message)
    }

    pub fn not_implemented(command: &str) -> Self {
        let mut error = Self::base(
            ErrorCategory::NotImplemented,
            "not_implemented",
            format!("{command} is not implemented yet"),
        );
        error.user_message = "This action is not available yet.".into();
        error
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Maps an I/O failure while writing into `folder`. Permission failures name
    /// the folder and the way out instead of a bare OS error.
    pub fn write_io(error: &std::io::Error, folder: &std::path::Path, what: &str) -> Self {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            return Self::base(
                ErrorCategory::Forbidden,
                "permission_denied",
                format!(
                    "Access denied while writing {what} in {}.\n\nChoose a different output folder in Settings, or allow Prompt Studio in Windows Security > Ransomware protection if this folder is protected.",
                    folder.display()
                ),
            )
            .with_details(error.to_string());
        }
        Self::from_io(error).with_details(format!("{}: {error}", folder.display()))
    }

    /// Maps a filesystem error onto the most specific category we can infer.
    pub fn from_io(error: &std::io::Error) -> Self {
        use std::io::ErrorKind;

        let message = error.to_string();
        match error.kind() {
            ErrorKind::NotFound => Self::base(ErrorCategory::NotFound, "file_not_found", message),
            ErrorKind::PermissionDenied => {
                Self::base(ErrorCategory::Forbidden, "permission_denied", message)
            }
            ErrorKind::TimedOut => Self::base(ErrorCategory::Timeout, "io_timeout", message),
            ErrorKind::AlreadyExists => {
                Self::base(ErrorCategory::Conflict, "already_exists", message)
            }
            _ => Self::base(ErrorCategory::Io, "io", message),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

/// One page of a paginated result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryability_matches_category() {
        for category in [
            ErrorCategory::Network,
            ErrorCategory::Timeout,
            ErrorCategory::Runtime,
        ] {
            assert!(category.retryable(), "{category:?} should be retryable");
        }

        for category in [
            ErrorCategory::Validation,
            ErrorCategory::NotFound,
            ErrorCategory::ModelMissing,
            ErrorCategory::ContextOverflow,
            ErrorCategory::InvalidResponse,
            ErrorCategory::Conflict,
            ErrorCategory::Unauthorized,
            ErrorCategory::Forbidden,
            ErrorCategory::Io,
            ErrorCategory::Media,
            ErrorCategory::Cancelled,
            ErrorCategory::Internal,
            ErrorCategory::NotImplemented,
        ] {
            assert!(
                !category.retryable(),
                "{category:?} should not be retryable"
            );
        }
    }

    #[test]
    fn io_errors_map_to_specific_categories() {
        let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert_eq!(
            CommandError::from_io(&not_found).category,
            ErrorCategory::NotFound
        );

        let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let mapped = CommandError::from_io(&denied);
        assert_eq!(mapped.category, ErrorCategory::Forbidden);
        assert!(!mapped.retryable);

        let timeout = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let mapped = CommandError::from_io(&timeout);
        assert_eq!(mapped.category, ErrorCategory::Timeout);
        assert!(mapped.retryable);
    }

    #[test]
    fn permission_write_errors_name_the_folder_and_the_way_out() {
        let denied = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Access is denied. (os error 5)",
        );
        let folder = std::path::Path::new("C:\\Users\\admin\\Pictures\\edited");
        let mapped = CommandError::write_io(&denied, folder, "the edited output");
        assert_eq!(mapped.category, ErrorCategory::Forbidden);
        assert!(
            mapped
                .message
                .contains("C:\\Users\\admin\\Pictures\\edited")
        );
        assert!(mapped.message.contains("output folder"));
        assert!(mapped.message.contains("Ransomware protection"));
        assert_eq!(
            mapped.details.as_deref(),
            Some("Access is denied. (os error 5)")
        );
    }

    #[test]
    fn serializes_with_camel_case_and_category() {
        let error = CommandError::network("connection refused").with_details("127.0.0.1:1234");
        let value = serde_json::to_value(&error).expect("serialize");

        assert_eq!(value["category"], "network");
        assert_eq!(value["retryable"], true);
        assert_eq!(value["userMessage"], error.user_message);
        assert_eq!(value["details"], "127.0.0.1:1234");
    }

    #[test]
    fn round_trips_through_json() {
        let error = CommandError::validation("endpoint must be a URL");
        let json = serde_json::to_string(&error).expect("serialize");
        let parsed: CommandError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, error);
    }

    #[test]
    fn not_implemented_is_permanent_with_friendly_message() {
        let error = CommandError::not_implemented("assets_list");
        assert_eq!(error.category, ErrorCategory::NotImplemented);
        assert!(!error.retryable);
        assert_eq!(error.user_message, "This action is not available yet.");
        assert!(error.message.contains("assets_list"));
    }
}
