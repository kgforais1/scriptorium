//! Opening an existing book package (`load_book`).
//!
//! Order: read `meta.json`, read `formatVersion` and pick that version's JSON Schema, validate,
//! deserialize, run `BookMeta::validate`, then check every page and original on disk: the
//! path stays inside the package (A12), and the bytes match their size and sha256 (A10).

use super::{io_error, sha256_file, BookError};
use crate::book_format::{BookMeta, BookMetaError, CURRENT_FORMAT_VERSION, MIN_FORMAT_VERSION};
use crate::ingest::convert;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SCHEMA_V1: &str = include_str!("../../../docs/book-format.v1.schema.json");
const SCHEMA_V2: &str = include_str!("../../../docs/book-format.v2.schema.json");

/// A loaded, verified book.
#[derive(Debug, Clone)]
pub struct LoadedBook {
    /// Canonical package directory.
    pub root: PathBuf,
    pub meta: BookMeta,
    /// Canonical path of each page's display image, verified inside `root` at load.
    page_paths: Vec<PathBuf>,
}

impl LoadedBook {
    /// The verified canonical path of page `index`'s display image, as checked at load (A12).
    /// Hand this to the asset protocol, never a fresh join of `meta.pages[i].file`.
    pub fn page_path(&self, index: usize) -> Option<&Path> {
        self.page_paths.get(index).map(PathBuf::as_path)
    }
}

/// Largest `meta.json` the loader reads. A 100,000-page book is about 30 MB, so this leaves
/// room while refusing a file that would exhaust memory.
pub(crate) const META_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Read a book's `meta.json`, refusing a missing or oversized file before loading it.
pub(crate) fn read_meta_text(meta_path: &Path) -> Result<String, BookError> {
    use std::io::Read;
    // A linked meta.json would be read through, then replaced by the next atomic save.
    if meta_path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(invalid("meta.json is a symlink"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // Open nonblocking, so a FIFO named meta.json cannot hang the open.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "macos")]
        const O_NONBLOCK: i32 = 0x4;
        #[cfg(target_os = "linux")]
        const O_NONBLOCK: i32 = 0o4000;
        options.custom_flags(O_NONBLOCK);
    }
    let file = options.open(meta_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => invalid("meta.json is missing"),
        _ => io_error(meta_path, &e),
    })?;
    // Check the opened handle, not the path, so the file cannot be swapped in between.
    if !file
        .metadata()
        .map_err(|e| io_error(meta_path, &e))?
        .is_file()
    {
        return Err(invalid("meta.json is not a file"));
    }
    let mut bytes = Vec::new();
    // Read one byte past the cap, so a file that grows after a size check is still caught.
    file.take(META_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(meta_path, &e))?;
    // Check the size before the encoding, so an oversized file reports its size.
    if bytes.len() as u64 > META_MAX_BYTES {
        return Err(invalid(format!(
            "meta.json is larger than {} MB",
            META_MAX_BYTES >> 20
        )));
    }
    String::from_utf8(bytes).map_err(|_| invalid("meta.json is not UTF-8 text"))
}

fn invalid(reason: impl Into<String>) -> BookError {
    BookError::InvalidPackage {
        reason: reason.into(),
    }
}

fn validator(version: u64) -> Result<&'static jsonschema::Validator, BookError> {
    static V1: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    static V2: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    let (cell, text) = match version {
        1 => (&V1, SCHEMA_V1),
        2 => (&V2, SCHEMA_V2),
        found => return Err(BookError::UnsupportedFormatVersion { found }),
    };
    cell.get_or_init(|| {
        let schema: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(|e| invalid(format!("built-in schema failed to compile: {e}")))
}

/// Parse and validate a `meta.json` document without touching page files.
pub fn parse_meta(text: &str) -> Result<BookMeta, BookError> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| invalid(format!("meta.json is not JSON ({e})")))?;
    let version = value
        .get("formatVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("meta.json has no numeric formatVersion"))?;
    if !(u64::from(MIN_FORMAT_VERSION)..=u64::from(CURRENT_FORMAT_VERSION)).contains(&version) {
        return Err(BookError::UnsupportedFormatVersion { found: version });
    }
    if let Some(err) = validator(version)?.iter_errors(&value).next() {
        return Err(invalid(format!(
            "meta.json fails the v{version} schema at {}: {err}",
            err.instance_path()
        )));
    }
    let meta: BookMeta =
        serde_json::from_value(value).map_err(|e| invalid(format!("meta.json: {e}")))?;
    meta.validate().map_err(|e| match e {
        BookMetaError::UnsupportedFormatVersion(found) => BookError::UnsupportedFormatVersion {
            found: found.into(),
        },
        other => invalid(other.to_string()),
    })?;
    Ok(meta)
}

/// Resolve a package-relative path and require that it stays inside `root` after symlinks
/// are followed (A12). `root` must already be canonical.
pub(crate) fn resolve_inside(root: &Path, rel: &str) -> Result<PathBuf, BookError> {
    let joined = root.join(rel);
    let real = joined.canonicalize().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => invalid(format!("{rel} is missing")),
        _ => io_error(&joined, &e),
    })?;
    if !real.starts_with(root) {
        return Err(invalid(format!("{rel} resolves outside the book folder")));
    }
    if !real.is_file() {
        return Err(invalid(format!("{rel} is not a file")));
    }
    Ok(real)
}

fn verify_file(root: &Path, rel: &str, size: u64, sha256: &str) -> Result<PathBuf, BookError> {
    let path = resolve_inside(root, rel)?;
    // A symlink inside the package could point one entry at another's bytes, so refuse it.
    let link = root
        .join(rel)
        .symlink_metadata()
        .map_err(|e| io_error(&root.join(rel), &e))?;
    if link.file_type().is_symlink() {
        return Err(invalid(format!("{rel} is a symlink")));
    }
    let (digest, bytes) = sha256_file(&path)?;
    if bytes != size || digest != sha256 {
        return Err(BookError::ChecksumMismatch {
            file: rel.to_string(),
        });
    }
    Ok(path)
}

/// Open and fully verify the book package at `dir`.
pub fn load_book(dir: &Path) -> Result<LoadedBook, BookError> {
    let root = dir.canonicalize().map_err(|e| io_error(dir, &e))?;
    let meta_path = root.join("meta.json");
    let text = read_meta_text(&meta_path)?;
    let meta = parse_meta(&text)?;
    let mut page_paths = Vec::with_capacity(meta.pages.len());
    for page in &meta.pages {
        page_paths.push(verify_file(
            &root,
            &page.file,
            page.byte_size,
            &page.sha256,
        )?);
    }
    for src in meta.sources.iter().flatten() {
        let path = verify_file(&root, &src.file, src.byte_size, &src.sha256)?;
        // `frames` must match what the file itself reports. Formats ingest cannot decode yet
        // (PDF before M1.5) are checked by checksum only.
        if convert::InputKind::from_path(&path).is_none() {
            continue;
        }
        let found = convert::inspect(&path)
            .map_err(|e| invalid(format!("original {} cannot be read: {e}", src.file)))?
            .frames;
        if found != src.frames {
            return Err(invalid(format!(
                "original {} records {} frame(s) but has {found}",
                src.file, src.frames
            )));
        }
    }
    Ok(LoadedBook {
        root,
        meta,
        page_paths,
    })
}
