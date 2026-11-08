//! Page text written by PDF ingest: `ocr/<NNNN>.json` and `ocr/index.json`.
//!
//! The layout is provisional until M2 (see `docs/book-format.md`). Page files are written as
//! pages are rendered. `finish` writes the index, then reads every file back and validates it
//! against `docs/ocr.v0.schema.json`, so what is checked is what is on disk.

use super::{io_error, now_rfc3339, BookError};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const SCHEMA: &str = include_str!("../../../docs/ocr.v0.schema.json");

/// The `engine` recorded for text that came out of a PDF.
const ENGINE: &str = "pdfium";

/// Text status of every PDF page written so far, keyed by book page position.
#[derive(Default)]
pub(super) struct OcrLog {
    pages: BTreeMap<usize, bool>,
}

impl OcrLog {
    /// Record PDF page `index`. A page with text gets `ocr/<NNNN>.json` in `tmp`, named with
    /// the book's page count so it lines up with `pages/`.
    pub(super) fn add_page(
        &mut self,
        tmp: &Path,
        index: usize,
        page_count: usize,
        text: &str,
    ) -> Result<(), BookError> {
        let has_text = !text.trim().is_empty();
        if has_text {
            let name = format!("ocr/{}.json", super::create::numbered(index, page_count));
            write_json(&tmp.join(name), &json!({ "source": "pdf", "text": text }))?;
        }
        self.pages.insert(index, has_text);
        Ok(())
    }

    /// Write `ocr/index.json` and validate everything under `ocr/`. A book with no PDF pages
    /// writes nothing and keeps its empty `ocr/` directory.
    pub(super) fn finish(&self, tmp: &Path) -> Result<(), BookError> {
        if self.pages.is_empty() {
            return Ok(());
        }
        let with_text = self.pages.values().filter(|&&t| t).count();
        let pages: serde_json::Map<String, Value> = self
            .pages
            .iter()
            .map(|(index, &t)| {
                let status = if t { "text" } else { "none" };
                (index.to_string(), json!({ "status": status }))
            })
            .collect();
        let index = json!({
            "engine": ENGINE,
            "generatedAt": now_rfc3339(),
            "pages": pages,
            "summary": { "pdfPages": self.pages.len(), "withText": with_text },
        });
        write_json(&tmp.join("ocr/index.json"), &index)?;
        validate_dir(&tmp.join("ocr"))
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), BookError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| invalid(e.to_string()))?;
    std::fs::write(path, text + "\n").map_err(|e| io_error(path, &e))
}

fn invalid(reason: String) -> BookError {
    BookError::InvalidPackage { reason }
}

fn validator() -> Result<&'static jsonschema::Validator, BookError> {
    static V: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    V.get_or_init(|| {
        let schema: Value = serde_json::from_str(SCHEMA).map_err(|e| e.to_string())?;
        jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(|e| invalid(format!("built-in ocr schema failed to compile: {e}")))
}

/// Check one `ocr/` document against the schema.
fn validate_text(name: &str, text: &str) -> Result<(), BookError> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| invalid(format!("{name} is not JSON ({e})")))?;
    let first = validator()?.iter_errors(&value).next();
    first.map_or(Ok(()), |err| {
        Err(invalid(format!("{name} fails the ocr schema: {err}")))
    })
}

/// Check every `.json` file in an `ocr/` directory.
fn validate_dir(dir: &Path) -> Result<(), BookError> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_error(dir, &e))? {
        let path = entry.map_err(|e| io_error(dir, &e))?.path();
        // Only ingest's own files. A stray file such as `.DS_Store` is not the book's.
        if path.extension().is_none_or(|ext| ext != "json") {
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

    fn ok(doc: &Value) -> bool {
        validate_text("doc", &doc.to_string()).is_ok()
    }

    #[test]
    fn schema_accepts_both_shapes() {
        assert!(ok(&json!({ "source": "pdf", "text": "hello" })));
        assert!(ok(&json!({
            "engine": "pdfium",
            "generatedAt": "2026-11-07T12:00:00Z",
            "pages": { "0": { "status": "text" }, "12": { "status": "none" } },
            "summary": { "pdfPages": 2, "withText": 1 },
        })));
    }

    #[test]
    fn schema_refuses_bad_documents() {
        assert!(!ok(&json!({ "source": "ocr", "text": "x" })));
        assert!(!ok(&json!({ "source": "pdf", "text": "" })));
        assert!(!ok(&json!({ "source": "pdf", "text": "x", "boxes": [] })));
        let index = |pages: Value, at: &str| {
            json!({
                "engine": "pdfium",
                "generatedAt": at,
                "pages": pages,
                "summary": { "pdfPages": 1, "withText": 0 },
            })
        };
        assert!(!ok(&index(
            json!({ "0": { "status": "maybe" } }),
            "2026-11-07T12:00:00Z"
        )));
        assert!(!ok(&index(
            json!({ "0001": { "status": "none" } }),
            "2026-11-07T12:00:00Z"
        )));
        assert!(!ok(&index(json!({}), "yesterday")));
        assert!(validate_text("doc", "not json").is_err());
    }

    #[test]
    fn a_book_with_no_pdf_pages_writes_nothing() {
        let dir =
            std::env::temp_dir().join(format!("scriptorium-ocr-empty-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ocr")).unwrap();
        OcrLog::default().finish(&dir).unwrap();
        assert_eq!(std::fs::read_dir(dir.join("ocr")).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
