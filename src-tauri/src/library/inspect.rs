//! Looking at a folder the user picked: an existing book, or a folder of scans to import.

use super::create::{inspect_all, ordered_inputs, total_frames};
use super::load::load_book;
use super::slug::suggest_book_id;
use super::BookError;
use serde::Serialize;
use std::path::Path;

/// What a picked folder holds. Serializes as `{ "kind": "book", ... }` or
/// `{ "kind": "scans", ... }` with camelCase fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FolderInfo {
    /// A Scriptorium book package that loaded cleanly.
    Book {
        /// The book id from `meta.json`.
        id: String,
        /// The book title from `meta.json`.
        title: String,
        /// The number of pages.
        pages: usize,
    },
    /// A folder of scans that can become a book.
    Scans {
        /// The number of supported image files.
        files: usize,
        /// The number of pages the book would have: all frames of all files.
        frames: u32,
        /// Bytes the import will write, for the confirm dialog.
        estimated_bytes: u64,
        /// An id that passes `validate_book_id` and is free in the library.
        suggested_id: String,
        /// The folder name.
        suggested_title: String,
    },
}

/// The folder's own name, for the suggested title.
fn folder_name(path: &Path) -> String {
    let named = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    named(path)
        .or_else(|| path.canonicalize().ok().and_then(|p| named(&p)))
        .unwrap_or_default()
}

/// Inspect `path`. A folder with a `meta.json` is a book, and a `meta.json` that fails to
/// load is that load's error, never a fall-through to the scans branch. Any other folder is
/// listed exactly as create lists it, and `library` is used to pick a free id.
pub fn inspect_folder(path: &Path, library: &Path) -> Result<FolderInfo, BookError> {
    // `symlink_metadata`, so a dangling or symlinked `meta.json` still goes to `load_book`.
    if path.join("meta.json").symlink_metadata().is_ok() {
        let book = load_book(path)?;
        return Ok(FolderInfo::Book {
            id: book.meta.id.clone(),
            title: book.meta.title.clone(),
            pages: book.meta.pages.len(),
        });
    }
    let inputs = ordered_inputs(path)?;
    let (opened, estimated_bytes) = inspect_all(&inputs)?;
    let name = folder_name(path);
    Ok(FolderInfo::Scans {
        files: opened.len(),
        frames: total_frames(&opened),
        estimated_bytes,
        suggested_id: suggest_book_id(&name, library),
        suggested_title: name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::create::create_book_from_folder;
    use crate::library::tests::{mixed_folder, opts, Tmp};

    #[test]
    fn scans_folder_reports_files_frames_and_suggestions() {
        let t = Tmp::new("inspect-scans");
        let lib = t.dir("lib");
        let src = mixed_folder(&t);
        let info = inspect_folder(&src, &lib).unwrap();
        let FolderInfo::Scans {
            files,
            frames,
            estimated_bytes,
            suggested_id,
            suggested_title,
        } = info
        else {
            panic!("expected scans, got {info:?}");
        };
        assert_eq!((files, frames), (4, 6));
        assert!(estimated_bytes > 0);
        assert_eq!(
            (suggested_id.as_str(), suggested_title.as_str()),
            ("src", "src")
        );
    }

    #[test]
    fn suggested_id_avoids_existing_books() {
        let t = Tmp::new("inspect-taken");
        let lib = t.dir("lib");
        std::fs::create_dir(lib.join("src")).unwrap();
        let info = inspect_folder(&mixed_folder(&t), &lib).unwrap();
        assert!(matches!(
            &info,
            FolderInfo::Scans { suggested_id, .. } if suggested_id == "src-2"
        ));
    }

    #[test]
    fn scans_json_matches_the_plan() {
        let t = Tmp::new("inspect-scans-json");
        let lib = t.dir("lib");
        let info = inspect_folder(&mixed_folder(&t), &lib).unwrap();
        let v = serde_json::to_value(&info).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "estimatedBytes",
                "files",
                "frames",
                "kind",
                "suggestedId",
                "suggestedTitle"
            ]
        );
        assert_eq!(v["kind"], "scans");
        assert_eq!(v["frames"], 6);
    }

    #[test]
    fn book_folder_reports_id_title_and_pages() {
        let t = Tmp::new("inspect-book");
        let lib = t.dir("lib");
        let book = create_book_from_folder(&mixed_folder(&t), &lib, &opts("bk")).unwrap();
        let info = inspect_folder(&book, &lib).unwrap();
        assert_eq!(
            info,
            FolderInfo::Book {
                id: "bk".into(),
                title: "Test Book".into(),
                pages: 6
            }
        );
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({ "kind": "book", "id": "bk", "title": "Test Book", "pages": 6 })
        );
    }

    #[test]
    fn corrupt_meta_json_is_an_error_not_a_scans_folder() {
        let t = Tmp::new("inspect-corrupt");
        let lib = t.dir("lib");
        let book = create_book_from_folder(&mixed_folder(&t), &lib, &opts("bk")).unwrap();
        std::fs::write(book.join("meta.json"), "{ not json").unwrap();
        assert!(matches!(
            inspect_folder(&book, &lib),
            Err(BookError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn folder_errors_pass_through() {
        let t = Tmp::new("inspect-errors");
        let lib = t.dir("lib");
        assert!(matches!(
            inspect_folder(&t.dir("empty"), &lib),
            Err(BookError::EmptyFolder { .. })
        ));
        let text_only = t.dir("text");
        std::fs::write(text_only.join("a.txt"), "x").unwrap();
        assert!(matches!(
            inspect_folder(&text_only, &lib),
            Err(BookError::NoSupportedImages { .. })
        ));
    }
}
