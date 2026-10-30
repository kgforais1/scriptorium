//! `state.json`: the one thing the app remembers between runs, the last opened book.

use crate::library::{write_atomic, BookError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file's contents: `{ "lastBook": "<absolute package path>" }`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    last_book: String,
}

/// Record `book` as the last opened book in the state file at `file`, atomically.
pub fn write_last_book(file: &Path, book: &Path) -> Result<(), BookError> {
    let last_book = book
        .to_str()
        .ok_or_else(|| BookError::InvalidPackage {
            reason: "the book path is not valid UTF-8".into(),
        })?
        .to_owned();
    let text = serde_json::to_string_pretty(&State { last_book }).map_err(|e| {
        BookError::InvalidPackage {
            reason: e.to_string(),
        }
    })?;
    write_atomic(file, (text + "\n").as_bytes())
}

/// The last opened book recorded in `file`, or `None` when the file is missing, unreadable,
/// or not what [`write_last_book`] writes. Never an error: a bad state file means a home screen.
pub fn read_last_book(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let state: State = serde_json::from_str(&text).ok()?;
    (!state.last_book.is_empty()).then(|| PathBuf::from(state.last_book))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("scriptorium-state-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_and_has_the_documented_shape() {
        let dir = tmp("round");
        let file = dir.join("state.json");
        let book = dir.join("books").join("my book");
        write_last_book(&file, &book).unwrap();
        assert_eq!(read_last_book(&file), Some(book.clone()));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "lastBook": book.to_str().unwrap() })
        );
        // A second write replaces the first.
        write_last_book(&file, &dir).unwrap();
        assert_eq!(read_last_book(&file), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupt_files_read_as_none() {
        let dir = tmp("bad");
        let file = dir.join("state.json");
        assert_eq!(read_last_book(&file), None, "missing");
        for text in [
            "",
            "not json",
            "[]",
            "{}",
            r#"{"lastBook": 3}"#,
            r#"{"lastBook": ""}"#,
        ] {
            std::fs::write(&file, text).unwrap();
            assert_eq!(read_last_book(&file), None, "{text:?}");
        }
        std::fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_last_book(&file), None, "not UTF-8");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
