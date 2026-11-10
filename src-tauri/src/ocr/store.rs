//! Reading and writing `ocr/`: page files, the index, and `forced.json`.
//!
//! Page files are the truth; `index.json` is a cache rebuilt from them (see
//! `docs/book-format.md`). Every write goes through [`write_atomic`] and is validated against
//! `docs/ocr.v1.schema.json` first. A page file that cannot be parsed and validated is never
//! rewritten, so a damaged file that still holds curator text survives as it is.

use super::engine::{normalise, EngineId, PageOcr, StoredLine};
use crate::book_format::BookMeta;
use crate::library::create::numbered;
use crate::library::{io_error, now_rfc3339, write_atomic, BookError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SCHEMA_V0: &str = include_str!("../../../docs/ocr.v0.schema.json");
const SCHEMA_V1: &str = include_str!("../../../docs/ocr.v1.schema.json");

type Compiled = Result<jsonschema::Validator, String>;

/// Compile the schema in `src` once into `cell` and return the cached validator.
fn compile(
    cell: &'static OnceLock<Compiled>,
    src: &str,
) -> Result<&'static jsonschema::Validator, String> {
    cell.get_or_init(|| {
        let schema: Value = serde_json::from_str(src).map_err(|e| e.to_string())?;
        jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(|e| format!("built-in ocr schema failed to compile: {e}"))
}

/// Return the first schema violation in `value`, or `Ok(())` when it conforms.
fn check(v: &jsonschema::Validator, value: &Value) -> Result<(), String> {
    v.iter_errors(value)
        .next()
        .map_or(Ok(()), |e| Err(e.to_string()))
}

/// Check a document against `ocr.v1.schema.json`. The error is the first violation.
pub(crate) fn validate_v1(value: &Value) -> Result<(), String> {
    static V: OnceLock<Compiled> = OnceLock::new();
    check(compile(&V, SCHEMA_V1)?, value)
}

/// Check a document against `ocr.v0.schema.json`, which M1.5 wrote. Kept for the read shim.
pub(crate) fn validate_v0(value: &Value) -> Result<(), String> {
    static V: OnceLock<Compiled> = OnceLock::new();
    check(compile(&V, SCHEMA_V0)?, value)
}

/// Write `value` as pretty JSON, atomically, after checking it against the v1 schema.
pub(crate) fn write_json(path: &Path, value: &Value) -> Result<(), BookError> {
    validate_v1(value).map_err(|reason| BookError::InvalidPackage {
        reason: format!("{} would fail the ocr schema: {reason}", path.display()),
    })?;
    let text = serde_json::to_string_pretty(value).map_err(|e| BookError::InvalidPackage {
        reason: e.to_string(),
    })?;
    write_atomic(path, (text + "\n").as_bytes())
}

/// A page file as read from disk.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) enum PageRead {
    /// No file: nothing recorded yet.
    Missing,
    /// A valid v1 file, or a valid v0 file read as v1 with only a `pdf` layer.
    Valid(Value),
    /// A file that is neither valid v0 nor valid v1, with the reason.
    Invalid(String),
}

/// Read one page file. Only an I/O error other than "not found" is an error.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn read_page(path: &Path) -> Result<PageRead, BookError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(PageRead::Missing),
        Err(e) => return Err(io_error(path, &e)),
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return Ok(PageRead::Invalid(format!("not JSON ({e})"))),
    };
    if value.get("layers").is_some() {
        return Ok(match validate_v1(&value) {
            Ok(()) => PageRead::Valid(value),
            Err(e) => PageRead::Invalid(format!("fails the ocr v1 schema: {e}")),
        });
    }
    if value.get("source").is_some() && validate_v0(&value).is_ok() {
        return Ok(PageRead::Valid(
            json!({ "schema": 1, "layers": { "pdf": { "text": value["text"] } } }),
        ));
    }
    let reason = validate_v1(&value)
        .err()
        .unwrap_or_else(|| "it is a v1 index or forced file, not a page".into());
    Ok(PageRead::Invalid(format!(
        "is neither a v0 nor a v1 page file: {reason}"
    )))
}

/// The `ocr` layer of a page file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) struct OcrLayer {
    /// The engine id that produced it.
    pub engine: String,
    /// When it was written, RFC 3339.
    pub generated_at: String,
    /// The run that wrote it.
    pub run_id: String,
    /// The page's `sha256` from `meta.json` at the time of the run.
    pub image_sha256: String,
    /// Lines joined with `\n`, paragraphs with `\n\n`.
    pub text: String,
    /// Lines with normalised boxes.
    pub lines: Vec<StoredLine>,
}

#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
impl OcrLayer {
    /// Build the layer for `page` as read by `engine` in run `run_id`.
    pub(crate) fn new(page: &PageOcr, engine: &EngineId, run_id: &str, image_sha256: &str) -> Self {
        let (text, lines) = normalise(page);
        Self {
            engine: engine.0.clone(),
            generated_at: now_rfc3339(),
            run_id: run_id.into(),
            image_sha256: image_sha256.into(),
            text,
            lines,
        }
    }

    /// Whether this layer is still good: same engine, and the image has not changed.
    pub(crate) fn is_current(&self, engine: &EngineId, image_sha256: &str) -> bool {
        self.engine == engine.0 && self.image_sha256 == image_sha256
    }
}

/// The `ocr` layer of a valid page file, if it has one.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn ocr_layer(page: &Value) -> Option<OcrLayer> {
    serde_json::from_value(page.get("layers")?.get("ocr")?.clone()).ok()
}

/// The published text of a page: the first layer present in the order `curator`, `ocr`,
/// `pdf`. A `curator` layer counts once it has a string `text`, since M2c sets its shape.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn published_text(page: &Value) -> Option<&str> {
    let layers = page.get("layers")?;
    ["curator", "ocr", "pdf"]
        .iter()
        .find_map(|name| layers.get(name)?.get("text")?.as_str())
}

/// What happened to an OCR write.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) enum OcrWrite {
    /// The `ocr` layer is on disk.
    Written,
    /// The existing file is invalid, so it was left untouched. The page is `failed`.
    Refused(String),
}

/// Replace only `layers.ocr` of the page file at `path`, creating the file if needed. Every
/// other layer is kept as read. An invalid file is never rewritten.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn write_ocr_layer(path: &Path, layer: &OcrLayer) -> Result<OcrWrite, BookError> {
    let layer = serde_json::to_value(layer).map_err(|e| BookError::InvalidPackage {
        reason: e.to_string(),
    })?;
    // An unreadable file (a directory, no permission) is refused like an invalid one.
    let read =
        read_page(path).unwrap_or_else(|e| PageRead::Invalid(format!("cannot be read: {e}")));
    let page = match read {
        PageRead::Invalid(reason) => return Ok(OcrWrite::Refused(reason)),
        PageRead::Missing => json!({ "schema": 1, "layers": { "ocr": layer } }),
        PageRead::Valid(mut page) => {
            page["layers"]["ocr"] = layer;
            page
        }
    };
    write_json(path, &page)?;
    Ok(OcrWrite::Written)
}

/// One page's status in `index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub(crate) enum Entry {
    /// Not run yet, or stale.
    Pending,
    /// The page file holds a current `ocr` layer.
    Done {
        /// The page's checksum when it ran.
        #[serde(rename = "imageSha256")]
        image_sha256: String,
        /// The run that wrote the layer.
        #[serde(rename = "runId")]
        run_id: String,
    },
    /// The page failed.
    Failed {
        /// Why.
        error: String,
        /// When, for display only.
        #[serde(rename = "failedAt")]
        failed_at: String,
        /// The `runId` of the `ocr` layer on disk when it failed, if any.
        #[serde(
            rename = "afterRunId",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        after_run_id: Option<String>,
    },
    /// The curator said not to OCR this page (M2c).
    Skipped,
}

/// The book-level rollup in `index.json`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Summary {
    /// Every page in the book.
    pub pages: usize,
    /// Pages `done`.
    pub done: usize,
    /// Pages `failed`.
    pub failed: usize,
    /// Pages `pending`.
    pub pending: usize,
    /// Pages `skipped`.
    pub skipped: usize,
    /// Pages whose published text is not empty.
    pub with_text: usize,
}

impl Summary {
    /// Count the statuses of `pages`.
    pub(crate) fn of(pages: &BTreeMap<usize, Entry>, with_text: usize) -> Self {
        let mut s = Self {
            pages: pages.len(),
            with_text,
            ..Self::default()
        };
        for entry in pages.values() {
            match entry {
                Entry::Pending => s.pending += 1,
                Entry::Done { .. } => s.done += 1,
                Entry::Failed { .. } => s.failed += 1,
                Entry::Skipped => s.skipped += 1,
            }
        }
        s
    }
}

/// `ocr/index.json` v1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OcrIndex {
    /// Always 1.
    pub schema: u32,
    /// The engine id the index was built for.
    pub engine: String,
    /// When it was written, RFC 3339.
    pub updated_at: String,
    /// Every page of the book, by position.
    pub pages: BTreeMap<usize, Entry>,
    /// The rollup.
    pub summary: Summary,
}

impl OcrIndex {
    /// An index stamped now, with its summary computed.
    pub(crate) fn new(engine: &str, pages: BTreeMap<usize, Entry>, with_text: usize) -> Self {
        Self {
            schema: 1,
            engine: engine.into(),
            updated_at: now_rfc3339(),
            summary: Summary::of(&pages, with_text),
            pages,
        }
    }
}

/// The path of a book's `ocr/index.json`.
fn index_path(book: &Path) -> PathBuf {
    book.join("ocr/index.json")
}

/// The path of page `index`'s file in a book of `page_count` pages.
pub(crate) fn page_path(book: &Path, index: usize, page_count: usize) -> PathBuf {
    book.join(format!("ocr/{}.json", numbered(index, page_count)))
}

/// Read `ocr/index.json` if it is a valid v1 index. Missing, v0, and invalid all read as `None`.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn read_index(book: &Path) -> Option<OcrIndex> {
    let text = std::fs::read_to_string(index_path(book)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    validate_v1(&value).ok()?;
    serde_json::from_value(value).ok()
}

/// Write `ocr/index.json` atomically.
pub(crate) fn write_index(book: &Path, index: &OcrIndex) -> Result<(), BookError> {
    let value = serde_json::to_value(index).map_err(|e| BookError::InvalidPackage {
        reason: e.to_string(),
    })?;
    write_json(&index_path(book), &value)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Forced {
    run_id: String,
}

/// The id of the unfinished forced run named by `ocr/forced.json`, if any. A missing file
/// means no forced run. An invalid file is deleted and also means no forced run, since the
/// intent is unknown.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn read_forced(book: &Path) -> Result<Option<String>, BookError> {
    let path = book.join("ocr/forced.json");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(&path, &e)),
    };
    // Damaged bytes, including invalid UTF-8, take the same delete-and-continue path as bad
    // JSON, so one broken marker cannot block every later rebuild. The v1 schema's `forced`
    // shape checks the id; the struct rejects a page or an index.
    let parsed = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(|v| validate_v1(v).is_ok())
        .and_then(|v| serde_json::from_value::<Forced>(v).ok());
    match parsed {
        Some(f) => Ok(Some(f.run_id)),
        _ => {
            eprintln!(
                "{} is invalid; treating the run as not forced",
                path.display()
            );
            std::fs::remove_file(&path).map_err(|e| io_error(&path, &e))?;
            Ok(None)
        }
    }
}

/// The current state of one page, as the rebuild needs it.
struct PageState<'a> {
    read: &'a PageRead,
    image_sha256: &'a str,
}

/// Apply the index rules to one page, first match wins (see `docs/book-format.md`).
fn rebuild_entry(
    old: Option<&Entry>,
    page: &PageState,
    engine: &EngineId,
    forced: Option<&str>,
) -> Entry {
    if matches!(old, Some(Entry::Skipped)) {
        return Entry::Skipped;
    }
    let layer = match page.read {
        PageRead::Invalid(reason) => {
            return Entry::Failed {
                error: format!("invalid page file: {reason}"),
                failed_at: now_rfc3339(),
                after_run_id: None,
            }
        }
        PageRead::Valid(v) => ocr_layer(v),
        PageRead::Missing => None,
    };
    let run_id = layer.as_ref().map(|l| l.run_id.as_str());
    if let Some(Entry::Failed { after_run_id, .. }) = old {
        if after_run_id.as_deref() == run_id {
            return old.cloned().unwrap_or(Entry::Pending);
        }
    }
    match layer {
        Some(l)
            if l.is_current(engine, page.image_sha256) && forced.is_none_or(|f| f == l.run_id) =>
        {
            Entry::Done {
                image_sha256: l.image_sha256,
                run_id: l.run_id,
            }
        }
        _ => Entry::Pending,
    }
}

/// Rebuild the index for `engine` from the page files, carrying over what only the old index
/// knows (`skipped`, and failures). Does not write it.
#[cfg_attr(not(test), allow(dead_code))] // Phase 4's job is the consumer.
pub(crate) fn rebuild_index(
    book: &Path,
    meta: &BookMeta,
    engine: &EngineId,
) -> Result<OcrIndex, BookError> {
    let old = read_index(book).map(|i| i.pages).unwrap_or_default();
    let forced = read_forced(book)?;
    let count = meta.pages.len();
    let mut pages = BTreeMap::new();
    let mut with_text = 0;
    for (i, entry) in meta.pages.iter().enumerate() {
        // An unreadable file (a directory, no permission) fails that page, not the job.
        let read = read_page(&page_path(book, i, count))
            .unwrap_or_else(|e| PageRead::Invalid(format!("cannot be read: {e}")));
        if let PageRead::Valid(v) = &read {
            if published_text(v).is_some_and(|t| !t.trim().is_empty()) {
                with_text += 1;
            }
        }
        let state = PageState {
            read: &read,
            image_sha256: &entry.sha256,
        };
        pages.insert(
            i,
            rebuild_entry(old.get(&i), &state, engine, forced.as_deref()),
        );
    }
    Ok(OcrIndex::new(&engine.0, pages, with_text))
}
