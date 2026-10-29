//! The book library: creating packages from folders, loading them, and saving progress.
//!
//! Every failure is a [`BookError`] from the M1 error taxonomy, each with a message a reader
//! can act on. Nothing here panics on bad input or aborts the process (acceptance A3).

pub mod create;
pub mod load;
pub mod progress;
pub mod sort;
pub mod sweep;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

/// Library errors (M1 plan, "Error taxonomy").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookError {
    /// The source folder has no files.
    EmptyFolder { folder: PathBuf },
    /// The source folder has files, but none in a supported input format.
    NoSupportedImages { folder: PathBuf },
    /// A file could not be read or decoded.
    UnreadableFile { file: PathBuf, reason: String },
    /// A book with this id already exists (case-insensitively).
    DestinationExists { id: String },
    /// Two input files share a name stem (case-insensitively).
    DuplicatePageStem { first: PathBuf, second: PathBuf },
    /// The library volume lacks room for the copy.
    InsufficientDiskSpace { needed: u64, available: u64 },
    /// The package's `formatVersion` is not one this build reads.
    UnsupportedFormatVersion { found: u64 },
    /// `meta.json` is missing or fails the schema or validation rules.
    InvalidPackage { reason: String },
    /// A page or original no longer matches its recorded checksum or size.
    ChecksumMismatch { file: String },
    /// The operating system refused access.
    PermissionDenied { path: PathBuf },
}

impl std::fmt::Display for BookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mb = |b: &u64| b.div_ceil(1_048_576);
        match self {
            Self::EmptyFolder { folder } => write!(f, "The folder {} has no files.", folder.display()),
            Self::NoSupportedImages { folder } => write!(
                f,
                "No supported images found in {}. Use JPEG, PNG, WebP, BMP, GIF, TIFF, or JPEG 2000.",
                folder.display()
            ),
            Self::UnreadableFile { file, reason } => write!(f, "Could not read {}: {reason}.", file.display()),
            Self::DestinationExists { id } => {
                write!(f, "A book named \"{id}\" already exists. It was not overwritten.")
            }
            Self::DuplicatePageStem { first, second } => write!(
                f,
                "Duplicate page names: {} and {} have the same name. Rename one and try again.",
                first.display(),
                second.display()
            ),
            Self::InsufficientDiskSpace { needed: 0, .. } => write!(
                f,
                "Not enough free space: the disk filled up while the book was being written."
            ),
            Self::InsufficientDiskSpace { needed, available } => write!(
                f,
                "Not enough free space: the book needs about {} MB and {} MB is free.",
                mb(needed),
                mb(available)
            ),
            Self::UnsupportedFormatVersion { found } => write!(
                f,
                "This book uses format version {found}, which this version of Scriptorium cannot open."
            ),
            Self::InvalidPackage { reason } => write!(f, "This is not a valid Scriptorium book: {reason}."),
            Self::ChecksumMismatch { file } => write!(
                f,
                "The book's files are corrupted or were modified: {file} no longer matches its checksum."
            ),
            Self::PermissionDenied { path } => write!(f, "Permission denied: {}.", path.display()),
        }
    }
}

impl std::error::Error for BookError {}

/// Map an I/O error on `path`: permission problems become `PermissionDenied`, the rest
/// `UnreadableFile`.
pub(crate) fn io_error(path: &Path, err: &std::io::Error) -> BookError {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        BookError::PermissionDenied {
            path: path.to_path_buf(),
        }
    } else if err.kind() == std::io::ErrorKind::StorageFull {
        // The volume filled up mid-copy despite the preflight estimate.
        BookError::InsufficientDiskSpace {
            needed: 0,
            available: 0,
        }
    } else {
        BookError::UnreadableFile {
            file: path.to_path_buf(),
            reason: err.to_string(),
        }
    }
}

/// Lowercase hex SHA-256 of a file, streamed so large pages are not held in memory.
pub(crate) fn sha256_file(path: &Path) -> Result<(String, u64), BookError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| io_error(path, &e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| io_error(path, &e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hex(&hasher.finalize()), total))
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The current time as RFC 3339, for `createdAt` and `updatedAt`.
pub(crate) fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

/// Write `bytes` to `path` atomically: a uniquely named sibling temp file, fsync, rename
/// over, then fsync the directory.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BookError> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    // A unique name keeps two overlapping saves (a page turn and the close flush) from
    // sharing one temp file.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(
        "{name}.{}-{}{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
        sweep::TMP_SUFFIX
    ));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(bytes)?;
        f.sync_all()
    });
    if let Err(e) = written {
        // Never leave a half-written temp file behind.
        let _ = std::fs::remove_file(&tmp);
        return Err(io_error(&tmp, &e));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_error(path, &e)
    })?;
    // Sync the directory too, so the rename itself survives a crash.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        std::fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| io_error(dir, &e))?;
    }
    Ok(())
}
