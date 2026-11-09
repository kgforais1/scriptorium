//! Page text written by PDF ingest: `ocr/<NNNN>.json` and `ocr/index.json`.
//!
//! The layout is v1 (see `docs/book-format.md`). Page files are written atomically as
//! pages are rendered. `finish` writes the index, then reads every file back and validates it
//! against `docs/ocr.v1.schema.json`, so what is checked is what is on disk.

use super::{io_error, BookError};
use crate::ocr::store::{self, Entry, OcrIndex};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// The `engine` recorded in an index written by ingest, before any OCR run.
const ENGINE: &str = "pdfium";

/// Which PDF pages written so far have text, keyed by book page position.
#[derive(Default)]
pub(super) struct OcrLog {
    pages: BTreeMap<usize, bool>,
}

impl OcrLog {
    /// Record PDF page `index`. A page with text gets `ocr/<NNNN>.json` in `tmp` with a `pdf`
    /// layer, named with the book's page count so it lines up with `pages/`.
    pub(super) fn add_page(
        &mut self,
        tmp: &Path,
        index: usize,
        page_count: usize,
        text: &str,
    ) -> Result<(), BookError> {
        let has_text = !text.trim().is_empty();
        if has_text {
            let page = json!({ "schema": 1, "layers": { "pdf": { "text": text } } });
            store::write_json(&store::page_path(tmp, index, page_count), &page)?;
        }
        self.pages.insert(index, has_text);
        Ok(())
    }

    /// Write `ocr/index.json`, listing all `page_count` pages as `pending`, and validate
    /// everything under `ocr/`. A book with no PDF pages writes nothing and keeps its empty
    /// `ocr/` directory: a missing index already reads as every page `pending`.
    pub(super) fn finish(&self, tmp: &Path, page_count: usize) -> Result<(), BookError> {
        if self.pages.is_empty() {
            return Ok(());
        }
        let with_text = self.pages.values().filter(|&&t| t).count();
        let pages = (0..page_count).map(|i| (i, Entry::Pending)).collect();
        let index = OcrIndex::new(ENGINE, pages, with_text);
        store::write_index(tmp, &index)?;
        validate_dir(&tmp.join("ocr"))
    }
}

fn invalid(reason: String) -> BookError {
    BookError::InvalidPackage { reason }
}

/// Check one `ocr/` document against the schema.
fn validate_text(name: &str, text: &str) -> Result<(), BookError> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| invalid(format!("{name} is not JSON ({e})")))?;
    store::validate_v1(&value).map_err(|err| invalid(format!("{name} fails the ocr schema: {err}")))
}

/// Check every `.json` file in an `ocr/` directory.
fn validate_dir(dir: &Path) -> Result<(), BookError> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_error(dir, &e))? {
        let path = entry.map_err(|e| io_error(dir, &e))?.path();
        // Only ingest's own files. A stray file such as `.DS_Store` is not the book's.
        // `forced.json` is the OCR job's, not ingest's, and never exists in a new package.
        if path.extension().is_none_or(|ext| ext != "json")
            || path.file_name().is_some_and(|n| n == "forced.json")
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| io_error(&path, &e))?;
        validate_text(&path.display().to_string(), &text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_writes_v1_and_lists_every_page() {
        let dir = std::env::temp_dir().join(format!("scriptorium-ocr-v1-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ocr")).unwrap();
        let mut log = OcrLog::default();
        log.add_page(&dir, 1, 3, "hello").unwrap();
        log.add_page(&dir, 2, 3, "  ").unwrap();
        log.finish(&dir, 3).unwrap();
        let index = store::read_index(&dir).expect("a valid v1 index");
        assert_eq!(index.pages.len(), 3);
        assert!(index.pages.values().all(|e| *e == Entry::Pending));
        assert_eq!(
            (
                index.summary.pages,
                index.summary.pending,
                index.summary.with_text
            ),
            (3, 3, 1)
        );
        assert!(!dir.join("ocr/0002.json").exists());
        assert!(validate_text("x", "not json").is_err());
        std::fs::write(dir.join("ocr/forced.json"), "not ingest's").unwrap();
        assert!(
            validate_dir(&dir.join("ocr")).is_ok(),
            "forced.json is skipped"
        );
        assert!(validate_text("x", "{}").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_book_with_no_pdf_pages_writes_nothing() {
        let dir =
            std::env::temp_dir().join(format!("scriptorium-ocr-empty-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ocr")).unwrap();
        OcrLog::default().finish(&dir, 4).unwrap();
        assert_eq!(std::fs::read_dir(dir.join("ocr")).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
