//! formatVersion 2 originals: book-level `sources[]` and the per-page `source` reference.
//!
//! A page whose display image was converted keeps its untouched original under
//! `originals/`. Several pages may share one multi-frame source, such as a multi-page TIFF.

use super::{
    is_lowercase_hex_sha256, package_path_problem, BookMeta, BookMetaError, SOURCES_MAX_ITEMS,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Container format of an original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    Jpeg,
    Png,
    Webp,
    Bmp,
    Gif,
    Tiff,
    Jp2,
    Pdf,
}

/// One original kept under `originals/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEntry {
    pub id: String,
    pub file: String,
    pub format: SourceFormat,
    /// Number of frames (pages) in the file; 1 for a single image.
    pub frames: u32,
    pub byte_size: u64,
    pub sha256: String,
}

/// A page's link to the original it was converted from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSource {
    /// A `sources[].id` in the same book.
    #[serde(rename = "ref")]
    pub source_ref: String,
    /// 0-based frame; present exactly when the source has more than one frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<u32>,
}

const ORIGINALS_DIR: &str = "originals/";

fn source_err(id: &str, reason: impl Into<String>) -> BookMetaError {
    BookMetaError::InvalidSource {
        id: id.to_string(),
        reason: reason.into(),
    }
}

fn page_err(index: u32, reason: impl Into<String>) -> BookMetaError {
    BookMetaError::InvalidPageSource {
        index,
        reason: reason.into(),
    }
}

/// True when `id` is a lowercase slug of 1–64 characters (schema `sourceEntry.id`).
fn is_source_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// Run the v2 rules. `page_files` maps each lowercased page `file` to its original spelling,
/// so a source path cannot collide with a page path.
pub(super) fn validate(
    meta: &BookMeta,
    page_files: &HashMap<String, String>,
) -> Result<(), BookMetaError> {
    if meta.format_version < 2 {
        // Name the v2 field that is actually present, so the message matches the document.
        let field = if meta.sources.is_some() {
            Some("sources")
        } else if meta.pages.iter().any(|p| p.source.is_some()) {
            Some("pages[].source")
        } else {
            None
        };
        return match field {
            Some(field) => Err(BookMetaError::FieldRequiresVersion {
                field,
                min_version: 2,
            }),
            None => Ok(()),
        };
    }
    let sources = meta.sources.as_deref().unwrap_or(&[]);
    if meta.sources.is_some() && sources.is_empty() {
        return Err(source_err("", "sources must be omitted rather than empty"));
    }
    if sources.len() > SOURCES_MAX_ITEMS {
        return Err(source_err(
            "",
            format!("{} entries; max is {SOURCES_MAX_ITEMS}", sources.len()),
        ));
    }
    let by_id = validate_entries(sources, page_files)?;
    validate_page_refs(meta, &by_id)
}

fn validate_entries<'a>(
    sources: &'a [super::SourceEntry],
    page_files: &HashMap<String, String>,
) -> Result<HashMap<&'a str, &'a SourceEntry>, BookMetaError> {
    let mut by_id = HashMap::with_capacity(sources.len());
    let mut files = HashSet::with_capacity(sources.len());
    for src in sources {
        if !is_source_id(&src.id) {
            return Err(source_err(
                &src.id,
                "id must be a lowercase slug of 1–64 characters",
            ));
        }
        if by_id.insert(src.id.as_str(), src).is_some() {
            return Err(source_err(&src.id, "duplicate id"));
        }
        if let Some(reason) = package_path_problem(&src.file) {
            return Err(source_err(
                &src.id,
                format!("file {:?}: {reason}", src.file),
            ));
        }
        if !src.file.starts_with(ORIGINALS_DIR) {
            return Err(source_err(
                &src.id,
                format!("file {:?} must be under originals/", src.file),
            ));
        }
        let folded = src.file.to_ascii_lowercase();
        if page_files.contains_key(&folded) || !files.insert(folded) {
            return Err(source_err(
                &src.id,
                format!(
                    "file {:?} collides case-insensitively with another file",
                    src.file
                ),
            ));
        }
        if src.frames < 1 {
            return Err(source_err(&src.id, "frames must be at least 1"));
        }
        if src.byte_size == 0 {
            return Err(source_err(&src.id, "byteSize must be > 0"));
        }
        if !is_lowercase_hex_sha256(&src.sha256) {
            return Err(source_err(
                &src.id,
                "sha256 must be 64 lowercase hex characters",
            ));
        }
    }
    Ok(by_id)
}

fn validate_page_refs(
    meta: &BookMeta,
    by_id: &HashMap<&str, &SourceEntry>,
) -> Result<(), BookMetaError> {
    let mut used: HashSet<(&str, Option<u32>)> = HashSet::new();
    for page in &meta.pages {
        let Some(ps) = &page.source else { continue };
        let Some(src) = by_id.get(ps.source_ref.as_str()) else {
            return Err(page_err(
                page.index,
                format!("unknown source {:?}", ps.source_ref),
            ));
        };
        match (src.frames, ps.frame) {
            (1, Some(_)) => {
                return Err(page_err(
                    page.index,
                    "frame given for a single-frame source",
                ))
            }
            (n, None) if n > 1 => {
                return Err(page_err(
                    page.index,
                    "frame required for a multi-frame source",
                ))
            }
            (n, Some(f)) if f >= n => {
                return Err(page_err(
                    page.index,
                    format!("frame {f} out of range for {n} frame(s)"),
                ))
            }
            _ => {}
        }
        if !used.insert((src.id.as_str(), ps.frame)) {
            return Err(page_err(
                page.index,
                "another page already uses this source frame",
            ));
        }
    }
    let referenced: HashSet<&str> = used.iter().map(|(id, _)| *id).collect();
    if let Some(id) = by_id.keys().find(|id| !referenced.contains(*id)) {
        return Err(source_err(id, "not referenced by any page"));
    }
    Ok(())
}
