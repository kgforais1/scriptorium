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
use crate::ingest::convert::{self, DisplayImage, InputKind, Inspection};
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

/// Display copies are re-encoded, so their size is not the input's. Each converted frame is
/// budgeted at least 6 MB: a q92 JPEG of an 11 MP scan is 2–4 MB, with room for detailed or
/// heavily compressed sources whose re-encode grows.
const DISPLAY_FRAME_ESTIMATE: u64 = 6 * 1024 * 1024;

/// Bytes ingest will write for one input: the file itself when copied as-is, or the kept
/// original plus one display copy per frame when converted.
pub(crate) fn estimate_bytes(size: u64, info: &Inspection) -> u64 {
    if !info.needs_conversion {
        return size;
    }
    let frames = u64::from(info.frames.max(1));
    size.saturating_add(frames.saturating_mul((size / frames).max(DISPLAY_FRAME_ESTIMATE)))
}

/// Each input path with its inspection, in page order.
type Inspected<'a> = Vec<(&'a Path, Inspection)>;

/// Inspect every input before writing anything, so a bad file fails fast. Only the
/// inspection is kept: each file is read again when its pages are written, so memory holds
/// one input at a time, not the whole folder. Returns the inspections and a size estimate.
fn inspect_all(inputs: &[PathBuf]) -> Result<(Inspected<'_>, u64), BookError> {
    let mut opened = Vec::with_capacity(inputs.len());
    let mut estimated = 0u64;
    for path in inputs {
        // Open first so an OS refusal is reported as PermissionDenied, not a decode failure.
        std::fs::File::open(path).map_err(|e| io_error(path, &e))?;
        let info = convert::open(path).map_err(|e| unreadable(path, e))?.info;
        let size = std::fs::metadata(path)
            .map_err(|e| io_error(path, &e))?
            .len();
        estimated += estimate_bytes(size, &info);
        opened.push((path.as_path(), info));
    }
    Ok((opened, estimated))
}

/// Pages and originals written so far into the temp package.
struct Builder {
    tmp: PathBuf,
    page_count: usize,
    originals_count: usize,
    pages: Vec<PageEntry>,
    sources: Vec<SourceEntry>,
}

impl Builder {
    fn new(
        library: &Path,
        id: &str,
        opened: &[(&Path, Inspection)],
    ) -> Result<(Self, TempDir), BookError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = library.join(format!(
            "{}{id}-{}-{nanos}",
            sweep::TMP_PREFIX,
            std::process::id()
        ));
        std::fs::create_dir(&tmp).map_err(|e| io_error(&tmp, &e))?;
        let guard = TempDir(Some(tmp.clone()));
        for dir in ["pages", "ocr"] {
            let d = tmp.join(dir);
            std::fs::create_dir(&d).map_err(|e| io_error(&d, &e))?;
        }
        let builder = Self {
            tmp,
            page_count: opened.iter().map(|(_, i)| i.frames as usize).sum(),
            originals_count: opened.iter().filter(|(_, i)| i.needs_conversion).count(),
            pages: Vec::new(),
            sources: Vec::new(),
        };
        Ok((builder, guard))
    }

    /// Copy `path` into `originals/` and record it. Returns the new source id.
    fn add_original(&mut self, path: &Path, info: &Inspection) -> Result<String, BookError> {
        let n = numbered(self.sources.len(), self.originals_count);
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let rel = format!("originals/{n}.{ext}");
        let dest = self.tmp.join(&rel);
        std::fs::create_dir_all(self.tmp.join("originals")).map_err(|e| io_error(&dest, &e))?;
        std::fs::copy(path, &dest).map_err(|e| io_error(path, &e))?;
        let (sha256, byte_size) = sha256_file(&dest)?;
        let id = format!("src-{n}");
        self.sources.push(SourceEntry {
            id: id.clone(),
            file: rel,
            format: info.kind.source_format(),
            frames: info.frames,
            byte_size,
            sha256,
        });
        Ok(id)
    }

    /// Write one display page and record it.
    fn add_page(
        &mut self,
        path: &Path,
        display: &DisplayImage,
        source: Option<PageSource>,
    ) -> Result<(), BookError> {
        let index = self.pages.len();
        let rel = format!("pages/{}.{}", numbered(index, self.page_count), display.ext);
        let dest = self.tmp.join(&rel);
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
        self.pages.push(PageEntry {
            index: index as u32,
            file: rel,
            width: display.width,
            height: display.height,
            byte_size,
            sha256,
            page_label: None,
            storage: StorageMode::Copied,
            source,
        });
        Ok(())
    }

    /// Write every page of one input, plus its original when it was converted.
    fn add_input(&mut self, path: &Path, info: &Inspection) -> Result<(), BookError> {
        let source = convert::open(path).map_err(|e| unreadable(path, e))?;
        if source.info != *info {
            return Err(BookError::UnreadableFile {
                file: path.to_path_buf(),
                reason: "the file changed while the book was being created".into(),
            });
        }
        let source_id = if info.needs_conversion {
            Some(self.add_original(path, info)?)
        } else {
            None
        };
        for frame in 0..info.frames {
            let display = source.frame(frame).map_err(|e| unreadable(path, e))?;
            let page_source = source_id.as_ref().map(|id| PageSource {
                source_ref: id.clone(),
                frame: (info.frames > 1).then_some(frame),
            });
            self.add_page(path, &display, page_source)?;
        }
        Ok(())
    }

    /// Write `meta.json` and `annotations.json`, validating exactly what is written (A1).
    fn finish(self, opts: &CreateOptions) -> Result<PathBuf, BookError> {
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
            pages: self.pages,
            sources: (!self.sources.is_empty()).then_some(self.sources),
        };
        let text = serde_json::to_string_pretty(&meta).map_err(|e| BookError::InvalidPackage {
            reason: e.to_string(),
        })?;
        parse_meta(&text)?;
        for (name, body) in [
            ("meta.json", text + "\n"),
            ("annotations.json", "[]\n".to_string()),
        ] {
            let p = self.tmp.join(name);
            std::fs::write(&p, body).map_err(|e| io_error(&p, &e))?;
        }
        Ok(self.tmp)
    }
}

/// Rename the finished temp package to its real name.
fn commit(tmp: &Path, library: &Path, id: &str) -> Result<PathBuf, BookError> {
    let dest = library.join(id);
    // Re-check right before the rename: on POSIX, rename silently replaces an empty
    // directory, so the name must still be free.
    check_destination(library, id)?;
    std::fs::rename(tmp, &dest).map_err(|e| {
        // Another book took the name between the check and the rename. A rename that fails
        // for another reason while the name is taken is reported the same way; the re-check
        // just above makes the race the likely cause.
        if dest.exists() {
            BookError::DestinationExists { id: id.to_string() }
        } else {
            io_error(&dest, &e)
        }
    })?;
    Ok(dest)
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
    let (opened, estimated) = inspect_all(&inputs)?;
    check_space(library, estimated)?;

    let (mut builder, guard) = Builder::new(library, &opts.id, &opened)?;
    for (path, info) in &opened {
        builder.add_input(path, info)?;
    }
    let tmp = builder.finish(opts)?;
    let dest = commit(&tmp, library, &opts.id)?;
    std::mem::forget(guard);
    Ok(dest)
}
