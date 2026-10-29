//! Creating a book package from a folder of scans (`create_book_from_folder`).
//!
//! The package is assembled in a temp directory under the library root, validated, and
//! renamed into place, so a crash never leaves a half-written book under its real name.

use super::load::parse_meta;
use super::{io_error, now_rfc3339, sha256_bytes, sha256_file, sort, sweep, BookError};
use crate::book_format::{
    validate_book_id, BookMeta, PageEntry, PageSource, RenderMode, SourceEntry, StorageMode,
    CURRENT_FORMAT_VERSION,
};
use crate::ingest::convert::{self, InputKind};
use std::path::{Path, PathBuf};

/// Headroom the free-space check keeps on top of the estimate (plan: 50 MB).
const SPACE_HEADROOM: u64 = 50 * 1024 * 1024;

/// What to create.
#[derive(Debug, Clone)]
pub struct CreateOptions {
    /// Book id and package directory name: a lowercase slug (see `docs/book-format.md`).
    pub id: String,
    pub title: String,
    pub rights: String,
    pub attribution: String,
}

/// List the supported image files in `folder`. Hidden files and subfolders are skipped;
/// a symbolic link is an error, because ingest copies real files only.
fn list_inputs(folder: &Path) -> Result<Vec<PathBuf>, BookError> {
    let entries = std::fs::read_dir(folder).map_err(|e| io_error(folder, &e))?;
    let mut any_file = false;
    let mut inputs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_error(folder, &e))?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type().map_err(|e| io_error(&path, &e))?;
        if kind.is_symlink() {
            return Err(BookError::UnreadableFile {
                file: path,
                reason: "symbolic links are not copied; use the real file".into(),
            });
        }
        if !kind.is_file() {
            continue;
        }
        any_file = true;
        if InputKind::from_path(&path).is_some() {
            inputs.push(path);
        }
    }
    if !any_file {
        return Err(BookError::EmptyFolder {
            folder: folder.to_path_buf(),
        });
    }
    if inputs.is_empty() {
        return Err(BookError::NoSupportedImages {
            folder: folder.to_path_buf(),
        });
    }
    Ok(inputs)
}

/// Two inputs whose stems match case-insensitively (`1.jpg` and `1.JPG`, or `1.jpg` and
/// `1.png`) are ambiguous page names.
fn check_duplicate_stems(inputs: &[PathBuf]) -> Result<(), BookError> {
    let mut seen = std::collections::HashMap::new();
    for path in inputs {
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if let Some(first) = seen.insert(stem, path.clone()) {
            return Err(BookError::DuplicatePageStem {
                first,
                second: path.clone(),
            });
        }
    }
    Ok(())
}

/// Refuse an id that matches an existing entry in the library case-insensitively (A11).
fn check_destination(library: &Path, id: &str) -> Result<(), BookError> {
    let folded = id.to_lowercase();
    for entry in std::fs::read_dir(library).map_err(|e| io_error(library, &e))? {
        let entry = entry.map_err(|e| io_error(library, &e))?;
        if entry.file_name().to_string_lossy().to_lowercase() == folded {
            return Err(BookError::DestinationExists { id: id.to_string() });
        }
    }
    Ok(())
}

pub(crate) fn check_space(library: &Path, estimated: u64) -> Result<(), BookError> {
    let available = fs4::available_space(library).map_err(|e| io_error(library, &e))?;
    let needed = estimated
        .saturating_add(estimated / 10)
        .saturating_add(SPACE_HEADROOM);
    if available < needed {
        return Err(BookError::InsufficientDiskSpace { needed, available });
    }
    Ok(())
}

fn unreadable(file: &Path, err: convert::ConvertError) -> BookError {
    match err {
        convert::ConvertError::UnreadableFile(reason) => BookError::UnreadableFile {
            file: file.to_path_buf(),
            reason,
        },
        convert::ConvertError::UnsupportedFormat => BookError::UnreadableFile {
            file: file.to_path_buf(),
            reason: "not a supported input format".into(),
        },
    }
}

/// Zero-padded page or source number, at least four digits.
fn numbered(i: usize, count: usize) -> String {
    let width = count.saturating_sub(1).to_string().len().max(4);
    format!("{i:0width$}")
}

/// Removes the temp directory unless the build succeeds.
struct TempDir(Option<PathBuf>);
impl Drop for TempDir {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

/// Build a new book in `library` from the images in `folder`. Returns the package path.
pub fn create_book_from_folder(
    folder: &Path,
    library: &Path,
    opts: &CreateOptions,
) -> Result<PathBuf, BookError> {
    validate_book_id(&opts.id).map_err(|e| BookError::InvalidPackage {
        reason: e.to_string(),
    })?;
    std::fs::create_dir_all(library).map_err(|e| io_error(library, &e))?;
    sweep::sweep_orphans(library, sweep::ORPHAN_AGE);
    check_destination(library, &opts.id)?;

    let mut inputs = list_inputs(folder)?;
    check_duplicate_stems(&inputs)?;
    sort::sort_pages(&mut inputs);

    // Inspect everything before writing anything, so a bad file fails fast. Only the
    // inspection is kept: each file is read again when its pages are written, so memory
    // holds one input at a time, not the whole folder.
    let mut opened = Vec::with_capacity(inputs.len());
    let mut estimated = 0u64;
    for path in &inputs {
        // Open first so an OS refusal is reported as PermissionDenied, not a decode failure.
        std::fs::File::open(path).map_err(|e| io_error(path, &e))?;
        let source = convert::open(path).map_err(|e| unreadable(path, e))?;
        let size = std::fs::metadata(path)
            .map_err(|e| io_error(path, &e))?
            .len();
        // A converted file keeps its original and adds a display copy of similar size.
        estimated += if source.info.needs_conversion {
            size * 2
        } else {
            size
        };
        opened.push((path, source.info));
    }
    check_space(library, estimated)?;

    let tmp_path = library.join(format!(
        "{}{}-{}-{}",
        sweep::TMP_PREFIX,
        opts.id,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir(&tmp_path).map_err(|e| io_error(&tmp_path, &e))?;
    let guard = TempDir(Some(tmp_path.clone()));
    for dir in ["pages", "ocr"] {
        let d = tmp_path.join(dir);
        std::fs::create_dir(&d).map_err(|e| io_error(&d, &e))?;
    }

    let page_count: usize = opened.iter().map(|(_, i)| i.frames as usize).sum();
    let originals_count = opened.iter().filter(|(_, i)| i.needs_conversion).count();
    let mut pages = Vec::with_capacity(page_count);
    let mut sources = Vec::new();
    for (path, info) in &opened {
        let source = convert::open(path).map_err(|e| unreadable(path, e))?;
        if source.info != *info {
            return Err(BookError::UnreadableFile {
                file: path.to_path_buf(),
                reason: "the file changed while the book was being created".into(),
            });
        }
        let source_id = if info.needs_conversion {
            let n = numbered(sources.len(), originals_count);
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            let rel = format!("originals/{n}.{ext}");
            let dest = tmp_path.join(&rel);
            std::fs::create_dir_all(tmp_path.join("originals")).map_err(|e| io_error(&dest, &e))?;
            std::fs::copy(path, &dest).map_err(|e| io_error(path, &e))?;
            let (sha256, byte_size) = sha256_file(&dest)?;
            let id = format!("src-{n}");
            sources.push(SourceEntry {
                id: id.clone(),
                file: rel,
                format: info.kind.source_format(),
                frames: info.frames,
                byte_size,
                sha256,
            });
            Some(id)
        } else {
            None
        };
        for frame in 0..info.frames {
            let display = source.frame(frame).map_err(|e| unreadable(path, e))?;
            let index = pages.len();
            let rel = format!("pages/{}.{}", numbered(index, page_count), display.ext);
            let dest = tmp_path.join(&rel);
            let (sha256, byte_size) = match &display.bytes {
                Some(bytes) => {
                    std::fs::write(&dest, bytes).map_err(|e| io_error(&dest, &e))?;
                    (sha256_bytes(bytes), bytes.len() as u64)
                }
                None => {
                    std::fs::copy(path, &dest).map_err(|e| io_error(path, &e))?;
                    sha256_file(&dest)?
                }
            };
            pages.push(PageEntry {
                index: index as u32,
                file: rel,
                width: display.width,
                height: display.height,
                byte_size,
                sha256,
                page_label: None,
                storage: StorageMode::Copied,
                source: source_id.as_ref().map(|id| PageSource {
                    source_ref: id.clone(),
                    frame: (info.frames > 1).then_some(frame),
                }),
            });
        }
    }

    let now = now_rfc3339();
    let meta = BookMeta {
        format_version: CURRENT_FORMAT_VERSION,
        id: opts.id.clone(),
        title: opts.title.clone(),
        created_at: now.clone(),
        updated_at: now,
        render_mode: RenderMode::Scan,
        last_read_page: 0,
        rights: opts.rights.clone(),
        attribution: opts.attribution.clone(),
        pages,
        sources: (!sources.is_empty()).then_some(sources),
    };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| BookError::InvalidPackage {
        reason: e.to_string(),
    })?;
    // Validate exactly what will be written, schema included (A1).
    parse_meta(&text)?;
    let write = |name: &str, body: &str| {
        let p = tmp_path.join(name);
        std::fs::write(&p, body).map_err(|e| io_error(&p, &e))
    };
    write("meta.json", &(text + "\n"))?;
    write("annotations.json", "[]\n")?;

    let dest = library.join(&opts.id);
    // Re-check right before the rename: on POSIX, rename silently replaces an empty
    // directory, so the name must still be free.
    check_destination(library, &opts.id)?;
    std::fs::rename(&tmp_path, &dest).map_err(|e| {
        // Another book took the name between the check and the rename.
        if dest.exists() {
            BookError::DestinationExists {
                id: opts.id.clone(),
            }
        } else {
            io_error(&dest, &e)
        }
    })?;
    std::mem::forget(guard);
    Ok(dest)
}
