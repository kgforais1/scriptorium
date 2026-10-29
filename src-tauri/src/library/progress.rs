//! Recording reading progress in `meta.json`.

use super::load::parse_meta;
use super::{io_error, now_rfc3339, write_atomic, BookError};
use crate::book_format::{BookMeta, CURRENT_FORMAT_VERSION};
use std::path::Path;

/// Set `lastReadPage` for the book at `dir` and save it atomically. Saving writes the
/// current `formatVersion`, which upgrades a v1 package to v2 (see the migration note).
/// Page files are not re-verified, so this stays fast enough to call on every page turn and
/// on window close.
pub fn save_last_read_page(dir: &Path, page: u32) -> Result<BookMeta, BookError> {
    let meta_path = dir.join("meta.json");
    let text = std::fs::read_to_string(&meta_path).map_err(|e| io_error(&meta_path, &e))?;
    let mut meta = parse_meta(&text)?;
    if page as usize >= meta.pages.len() {
        return Err(BookError::InvalidPackage {
            reason: format!(
                "page {page} is past the last page ({})",
                meta.pages.len() - 1
            ),
        });
    }
    meta.last_read_page = page;
    meta.format_version = CURRENT_FORMAT_VERSION;
    meta.updated_at = now_rfc3339();
    let out = serde_json::to_string_pretty(&meta).map_err(|e| BookError::InvalidPackage {
        reason: e.to_string(),
    })?;
    parse_meta(&out)?;
    write_atomic(&meta_path, (out + "\n").as_bytes())?;
    Ok(meta)
}
