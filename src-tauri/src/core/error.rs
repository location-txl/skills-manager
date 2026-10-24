use serde::Serialize;
use std::fmt;

/// Structured error type for Tauri commands.
///
/// Serialized as `{"kind": "io", "message": "..."}` so the frontend
/// can branch on `kind` while still showing a human-readable `message`.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Io,
    Network,
    NotFound,
    InvalidInput,
    Cancelled,
    Internal,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl AppError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotFound,
            message: msg.into(),
        }
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::InvalidInput,
            message: msg.into(),
        }
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Cancelled,
            message: msg.into(),
        }
    }

    /// Convert an error originating from network operations.
    pub fn network(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Network,
            message: e.to_string(),
        }
    }

    /// Convert an error originating from IO operations.
    pub fn io(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: e.to_string(),
        }
    }

    pub fn internal(e: impl fmt::Display) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: e.to_string(),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: e.to_string(),
        }
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: e.to_string(),
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_correct_kinds() {
        assert!(matches!(AppError::not_found("x").kind, ErrorKind::NotFound));
        assert!(matches!(
            AppError::invalid_input("x").kind,
            ErrorKind::InvalidInput
        ));
        assert!(matches!(AppError::cancelled("x").kind, ErrorKind::Cancelled));
        assert!(matches!(AppError::network("x").kind, ErrorKind::Network));
        assert!(matches!(AppError::io("x").kind, ErrorKind::Io));
        assert!(matches!(AppError::internal("x").kind, ErrorKind::Internal));
    }

    #[test]
    fn display_shows_message() {
        let err = AppError::not_found("Skill not found");
        assert_eq!(format!("{}", err), "Skill not found");
    }

    #[test]
    fn serializes_to_json_with_kind_and_message() {
        let err = AppError::not_found("missing");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "not_found");
        assert_eq!(json["message"], "missing");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err.kind, ErrorKind::Io));
        assert!(app_err.message.contains("file gone"));
    }
}
