//! Book package `meta.json` types (formatVersion 1 and 2).
//!
//! Mirrors `docs/book-format.v1.schema.json` and `docs/book-format.v2.schema.json`. The app
//! reads both versions and writes v2. v2 adds optional originals (`sources[]` and a page
//! `source`); their rules live in [`sources`]. Serde handles wire shape; call
//! [`BookMeta::validate`] after deserialize so cross-field rules the schema
//! cannot express (e.g. `lastReadPage` vs `pages.length`) are enforced.
//!
//! Unknown JSON keys are rejected at deserialize time (`deny_unknown_fields`).

use serde::{Deserialize, Serialize};

mod sources;
pub use sources::{PageSource, SourceEntry, SourceFormat};

/// Oldest `formatVersion` this build reads.
pub const MIN_FORMAT_VERSION: u32 = 1;
/// `formatVersion` this build writes, and the newest it reads.
pub const CURRENT_FORMAT_VERSION: u32 = 2;

/// Book `id` slug pattern — must stay in lockstep with
/// `docs/book-format.v{1,2}.schema.json` `properties.id.pattern` and Python `BOOK_ID_RE`.
pub const BOOK_ID_PATTERN: &str = r"^(?!(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.[a-z0-9._-]*)?$)(?!.*\.$)[a-z0-9][a-z0-9._-]{0,63}$";

/// Windows-reserved device stems (matched case-insensitively on the component stem).
const WINDOWS_RESERVED_STEMS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Max character length for `title` (matches schema `maxLength`; Unicode scalar count).
pub const TITLE_MAX_LEN: usize = 4096;
/// Max character length for `rights` / `attribution`.
pub const TEXT_FIELD_MAX_LEN: usize = 8192;
/// Max number of pages in a package (matches schema `maxItems`).
pub const PAGES_MAX_ITEMS: usize = 100_000;

/// How a book renders. Default is `Scan`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    #[default]
    Scan,
    Text,
    Hybrid,
}

/// How a page is stored. Default is `Copied`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    #[default]
    Copied,
    Referenced,
}

/// Validation failure for a book package `meta.json` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookMetaError {
    /// `formatVersion` is not the supported major version.
    UnsupportedFormatVersion(u32),
    /// `id` is empty, unsafe as a directory name, or fails the slug pattern.
    InvalidBookId { id: String, reason: String },
    /// `pages` must contain at least one entry.
    EmptyPages,
    /// `lastReadPage` is not a valid 0-based index into `pages`.
    LastReadPageOutOfRange {
        last_read_page: u32,
        page_count: usize,
    },
    /// `pages[i].index` does not equal the array position `i`.
    PageIndexMismatch { position: usize, index: u32 },
    /// A page `file` path is absolute, traverses, or uses disallowed characters.
    InvalidPageFile {
        index: u32,
        file: String,
        reason: String,
    },
    /// Image dimensions must be at least 1×1.
    DegenerateImageSize { index: u32, width: u32, height: u32 },
    /// `byteSize` must be greater than zero.
    ZeroByteSize { index: u32 },
    /// `sha256` must be 64 lowercase hex characters.
    InvalidSha256 { index: u32, sha256: String },
    /// `createdAt` / `updatedAt` must be RFC 3339 (ISO 8601 date-time).
    InvalidTimestamp { field: &'static str, value: String },
    /// Two page `file` paths collide when compared case-insensitively.
    CaseInsensitivePathCollision {
        index: u32,
        file: String,
        earlier_file: String,
    },
    /// `title` exceeds the schema max length.
    TitleTooLong { len: usize, max: usize },
    /// `rights` or `attribution` exceeds the schema max length.
    TextFieldTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    /// `pages` exceeds the schema maxItems bound.
    TooManyPages { count: usize, max: usize },
    /// A field appears in a package whose `formatVersion` predates it.
    FieldRequiresVersion {
        field: &'static str,
        min_version: u32,
    },
    /// A `sources[]` entry breaks a v2 rule (id, path, shape, or is unreferenced).
    InvalidSource { id: String, reason: String },
    /// A page `source` reference breaks a v2 rule (unknown ref, frame rules).
    InvalidPageSource { index: u32, reason: String },
}

impl std::fmt::Display for BookMetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormatVersion(v) => {
                write!(
                    f,
                    "unsupported formatVersion {v} (expected {MIN_FORMAT_VERSION}..={CURRENT_FORMAT_VERSION})"
                )
            }
            Self::InvalidBookId { id, reason } => {
                write!(f, "id {id:?} invalid: {reason}")
            }
            Self::EmptyPages => write!(f, "pages must be non-empty"),
            Self::LastReadPageOutOfRange {
                last_read_page,
                page_count,
            } => write!(
                f,
                "lastReadPage {last_read_page} out of range for {page_count} page(s)"
            ),
            Self::PageIndexMismatch { position, index } => {
                write!(f, "pages[{position}].index is {index}, expected {position}")
            }
            Self::InvalidPageFile {
                index,
                file,
                reason,
            } => {
                write!(f, "pages[{index}].file {file:?} invalid: {reason}")
            }
            Self::DegenerateImageSize {
                index,
                width,
                height,
            } => write!(f, "pages[{index}] has degenerate size {width}×{height}"),
            Self::ZeroByteSize { index } => {
                write!(f, "pages[{index}].byteSize must be > 0")
            }
            Self::InvalidSha256 { index, sha256 } => {
                write!(
                    f,
                    "pages[{index}].sha256 {sha256:?} is not 64 lowercase hex"
                )
            }
            Self::InvalidTimestamp { field, value } => {
                write!(f, "{field} {value:?} is not a valid RFC 3339 date-time")
            }
            Self::CaseInsensitivePathCollision {
                index,
                file,
                earlier_file,
            } => write!(
                f,
                "pages[{index}].file {file:?} collides case-insensitively with {earlier_file:?}"
            ),
            Self::TitleTooLong { len, max } => {
                write!(f, "title length {len} exceeds max {max}")
            }
            Self::TextFieldTooLong { field, len, max } => {
                write!(f, "{field} length {len} exceeds max {max}")
            }
            Self::TooManyPages { count, max } => {
                write!(f, "pages has {count} entries; max is {max}")
            }
            Self::FieldRequiresVersion { field, min_version } => {
                write!(f, "{field} requires formatVersion {min_version} or later")
            }
            Self::InvalidSource { id, reason } => {
                write!(f, "sources entry {id:?} invalid: {reason}")
            }
            Self::InvalidPageSource { index, reason } => {
                write!(f, "pages[{index}].source invalid: {reason}")
            }
        }
    }
}

impl std::error::Error for BookMetaError {}

/// One page entry in `meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageEntry {
    pub index: u32,
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_label: Option<String>,
    #[serde(default)]
    pub storage: StorageMode,
    /// v2: the original this display image was converted from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PageSource>,
}

impl PageEntry {
    /// Validate path safety, dimensions, byte size, and checksum shape for this page.
    pub fn validate(&self) -> Result<(), BookMetaError> {
        validate_page_file(self.index, &self.file)?;
        if self.width < 1 || self.height < 1 {
            return Err(BookMetaError::DegenerateImageSize {
                index: self.index,
                width: self.width,
                height: self.height,
            });
        }
        if self.byte_size == 0 {
            return Err(BookMetaError::ZeroByteSize { index: self.index });
        }
        if !is_lowercase_hex_sha256(&self.sha256) {
            return Err(BookMetaError::InvalidSha256 {
                index: self.index,
                sha256: self.sha256.clone(),
            });
        }
        Ok(())
    }
}

/// Top-level `meta.json` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookMeta {
    pub format_version: u32,
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub render_mode: RenderMode,
    pub last_read_page: u32,
    pub rights: String,
    pub attribution: String,
    pub pages: Vec<PageEntry>,
    /// v2: originals kept under `originals/` for converted pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<SourceEntry>>,
}

impl BookMeta {
    /// A minimal valid sample for tests and fixtures.
    pub fn sample() -> Self {
        Self {
            format_version: 1,
            id: "sample-book-001".into(),
            title: "Sample Book".into(),
            created_at: "2026-08-24T00:00:00Z".into(),
            updated_at: "2026-08-24T00:00:00Z".into(),
            render_mode: RenderMode::Scan,
            last_read_page: 0,
            rights: "".into(),
            attribution: "".into(),
            pages: vec![PageEntry {
                index: 0,
                file: "pages/000.jpg".into(),
                width: 1200,
                height: 1600,
                byte_size: 245_760,
                // Shape-only digest for structural tests — not the hash of a real 1200×1600 image.
                sha256: "b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c".into(),
                page_label: None,
                storage: StorageMode::Copied,
                source: None,
            }],
            sources: None,
        }
    }

    /// Enforce rules the JSON Schema cannot express alone, plus RFC 3339 timestamps
    /// so Rust-only loaders match the schema `date-time` contract.
    ///
    /// Callers that load packages MUST run this (or clamp `last_read_page`)
    /// before trusting the document. Out-of-range `lastReadPage` is rejected
    /// here; UI loaders may alternatively clamp to `pages.len() - 1`.
    pub fn validate(&self) -> Result<(), BookMetaError> {
        if !(MIN_FORMAT_VERSION..=CURRENT_FORMAT_VERSION).contains(&self.format_version) {
            return Err(BookMetaError::UnsupportedFormatVersion(self.format_version));
        }
        validate_book_id(&self.id)?;
        validate_rfc3339("createdAt", &self.created_at)?;
        validate_rfc3339("updatedAt", &self.updated_at)?;
        if self.title.chars().count() > TITLE_MAX_LEN {
            return Err(BookMetaError::TitleTooLong {
                len: self.title.chars().count(),
                max: TITLE_MAX_LEN,
            });
        }
        if self.rights.chars().count() > TEXT_FIELD_MAX_LEN {
            return Err(BookMetaError::TextFieldTooLong {
                field: "rights",
                len: self.rights.chars().count(),
                max: TEXT_FIELD_MAX_LEN,
            });
        }
        if self.attribution.chars().count() > TEXT_FIELD_MAX_LEN {
            return Err(BookMetaError::TextFieldTooLong {
                field: "attribution",
                len: self.attribution.chars().count(),
                max: TEXT_FIELD_MAX_LEN,
            });
        }
        if self.pages.is_empty() {
            return Err(BookMetaError::EmptyPages);
        }
        if self.pages.len() > PAGES_MAX_ITEMS {
            return Err(BookMetaError::TooManyPages {
                count: self.pages.len(),
                max: PAGES_MAX_ITEMS,
            });
        }
        let page_count = self.pages.len();
        if (self.last_read_page as usize) >= page_count {
            return Err(BookMetaError::LastReadPageOutOfRange {
                last_read_page: self.last_read_page,
                page_count,
            });
        }
        let mut seen_files: std::collections::HashMap<String, String> =
            std::collections::HashMap::with_capacity(page_count);
        for (position, page) in self.pages.iter().enumerate() {
            if page.index as usize != position {
                return Err(BookMetaError::PageIndexMismatch {
                    position,
                    index: page.index,
                });
            }
            page.validate()?;
            let folded = page.file.to_ascii_lowercase();
            if let Some(earlier_file) = seen_files.get(&folded) {
                return Err(BookMetaError::CaseInsensitivePathCollision {
                    index: page.index,
                    file: page.file.clone(),
                    earlier_file: earlier_file.clone(),
                });
            }
            seen_files.insert(folded, page.file.clone());
        }
        sources::validate(self, &seen_files)
    }
}

/// Book `id` must be a safe library directory slug (matches schema pattern).
fn validate_book_id(id: &str) -> Result<(), BookMetaError> {
    let reason = if id.is_empty() {
        Some("empty id")
    } else if id.len() > 64 {
        Some("longer than 64 characters")
    } else if id.ends_with('.') {
        Some("must not end with a period")
    } else if is_windows_reserved_component(id) {
        Some("Windows-reserved device name")
    } else if !id
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        Some("must start with a lowercase letter or digit")
    } else if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        Some("only lowercase alnum, '.', '_', '-' allowed")
    } else {
        None
    };

    match reason {
        Some(reason) => Err(BookMetaError::InvalidBookId {
            id: id.to_string(),
            reason: reason.to_string(),
        }),
        None => Ok(()),
    }
}

/// Reject absolute paths, `.` / `..` segments, trailing periods, Windows-reserved
/// components, backslashes, and null bytes.
fn validate_page_file(index: u32, file: &str) -> Result<(), BookMetaError> {
    match package_path_problem(file) {
        Some(reason) => Err(BookMetaError::InvalidPageFile {
            index,
            file: file.to_string(),
            reason: reason.to_string(),
        }),
        None => Ok(()),
    }
}

/// The first rule a package-relative path breaks, or `None` when it is safe. Shared by
/// `pages[].file` and `sources[].file`.
fn package_path_problem(file: &str) -> Option<&'static str> {
    if file.is_empty() {
        Some("empty path")
    } else if file.contains('\0') {
        Some("contains null byte")
    } else if file.starts_with('/') || file.starts_with('\\') {
        Some("absolute path")
    } else if file.contains('\\') {
        Some("backslash not allowed; use forward slashes")
    } else if file
        .split('/')
        .any(|seg| seg == "." || seg == ".." || seg.is_empty())
    {
        Some("empty, ., or .. path segment")
    } else if file.split('/').any(|seg| seg.ends_with('.')) {
        Some("path segment must not end with a period")
    } else if file.split('/').any(is_windows_reserved_component) {
        Some("Windows-reserved device name in path")
    } else if !file
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        Some("disallowed characters")
    } else {
        None
    }
}

/// True when `name` is a Windows-reserved device component (`CON`, `aux.txt`, …).
fn is_windows_reserved_component(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    if stem.is_empty() {
        return false;
    }
    let stem_lower = stem.to_ascii_lowercase();
    WINDOWS_RESERVED_STEMS.contains(&stem_lower.as_str())
}

fn is_lowercase_hex_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn validate_rfc3339(field: &'static str, value: &str) -> Result<(), BookMetaError> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;

    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| BookMetaError::InvalidTimestamp {
        field,
        value: value.to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_v2;
