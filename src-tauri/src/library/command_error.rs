//! The error shape commands return to the UI.

use super::BookError;
use serde::Serialize;

/// Serializes as `{ "kind": "<BookError variant>", "message": "<Display text>" }`.
/// `kind` is `"Internal"` when a background task panicked, and `"SaveHeld"` when a reading
/// position is held in memory after an earlier save failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    /// The `BookError` variant name, or `Internal`.
    pub kind: String,
    /// Text a reader can act on.
    pub message: String,
}

/// The variant name of `err`. The match is exhaustive, so a new variant fails to compile
/// until it is named here.
fn variant_name(err: &BookError) -> &'static str {
    match err {
        BookError::EmptyFolder { .. } => "EmptyFolder",
        BookError::NoSupportedImages { .. } => "NoSupportedImages",
        BookError::UnreadableFile { .. } => "UnreadableFile",
        BookError::DestinationExists { .. } => "DestinationExists",
        BookError::DuplicatePageStem { .. } => "DuplicatePageStem",
        BookError::InsufficientDiskSpace { .. } => "InsufficientDiskSpace",
        BookError::UnsupportedFormatVersion { .. } => "UnsupportedFormatVersion",
        BookError::InvalidPackage { .. } => "InvalidPackage",
        BookError::ChecksumMismatch { .. } => "ChecksumMismatch",
        BookError::PermissionDenied { .. } => "PermissionDenied",
    }
}

impl CommandError {
    /// An error of kind `Internal`, for a task that panicked or could not be joined.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: "Internal".into(),
            message: message.into(),
        }
    }

    /// An error of kind `SaveHeld`: the page is held in memory because an earlier save failed,
    /// and a retry is scheduled.
    pub fn save_held() -> Self {
        Self {
            kind: "SaveHeld".into(),
            message: "The last save failed. It will be retried.".into(),
        }
    }
}

impl From<BookError> for CommandError {
    fn from(err: BookError) -> Self {
        Self {
            kind: variant_name(&err).into(),
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// One value of every `BookError` variant.
    fn every_variant() -> Vec<BookError> {
        let p = PathBuf::from("/x/page.jpg");
        vec![
            BookError::EmptyFolder { folder: p.clone() },
            BookError::NoSupportedImages { folder: p.clone() },
            BookError::UnreadableFile {
                file: p.clone(),
                reason: "bad".into(),
            },
            BookError::DestinationExists { id: "b".into() },
            BookError::DuplicatePageStem {
                first: p.clone(),
                second: p.clone(),
            },
            BookError::InsufficientDiskSpace {
                needed: 10 << 20,
                available: 1 << 20,
            },
            BookError::UnsupportedFormatVersion { found: 9 },
            BookError::InvalidPackage { reason: "r".into() },
            BookError::ChecksumMismatch {
                file: "pages/0001.jpg".into(),
            },
            BookError::PermissionDenied { path: p },
        ]
    }

    #[test]
    fn json_shape_is_kind_and_message() {
        let err = CommandError::from(BookError::ChecksumMismatch {
            file: "pages/0001.jpg".into(),
        });
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            serde_json::json!({
                "kind": "ChecksumMismatch",
                "message": "The book's files are corrupted or were modified: pages/0001.jpg no longer matches its checksum."
            })
        );
        let err = CommandError::from(BookError::DestinationExists { id: "b".into() });
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            serde_json::json!({
                "kind": "DestinationExists",
                "message": "A book named \"b\" already exists. It was not overwritten."
            })
        );
    }

    #[test]
    fn every_variant_maps_to_its_own_name() {
        let all = every_variant();
        let mut seen = std::collections::HashSet::new();
        for err in all {
            // The derived Debug output starts with the variant name.
            let debug = format!("{err:?}");
            let expected = debug.split([' ', '{']).next().unwrap().to_string();
            let mapped = CommandError::from(err.clone());
            assert_eq!(mapped.kind, expected);
            assert_eq!(mapped.message, err.to_string());
            assert!(seen.insert(mapped.kind), "duplicate kind");
        }
        assert_eq!(seen.len(), 10);
    }

    #[test]
    fn internal_has_kind_internal() {
        let err = CommandError::internal("task panicked");
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            serde_json::json!({ "kind": "Internal", "message": "task panicked" })
        );
    }
}
