//! The Tesseract engine: locate the bundled sidecar, run it on one image, and parse its TSV.
//!
//! The child runs in the image's directory with bare file names, so a non-ASCII temp path
//! never reaches Tesseract's argument handling on Windows. Its environment is cleared, and on
//! Windows it gets `CREATE_NO_WINDOW` so no console flashes. See decisions 7 to 10 in
//! `plans/2026-11-19-m2a-phase3-tesseract.md`.

use super::engine::{EngineId, Lang, Line, OcrEngine, OcrError, PageOcr, PixelBox, Word};
use super::OcrUnavailable;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Tesseract's 12 TSV column names, in order.
const TSV_COLUMNS: [&str; 12] = [
    "level",
    "page_num",
    "block_num",
    "par_num",
    "line_num",
    "word_num",
    "left",
    "top",
    "width",
    "height",
    "conf",
    "text",
];

/// The pinned Tesseract release, which `scripts/fetch_tesseract.sh` installs.
const TESSERACT_RELEASE: &str = "tesseract-5.5.3-r1";
/// The Tesseract version, the first part of [`TesseractEngine::id`].
const TESSERACT_VERSION: &str = "5.5.3";
/// The language data variant, the last part of [`TesseractEngine::id`].
const TESSDATA_VARIANT: &str = "eng-fast";

/// How often the running child is polled.
const POLL: Duration = Duration::from_millis(100);
/// How much of the child's stderr a failure message keeps.
const STDERR_TAIL: usize = 2048;
/// Windows' `CREATE_NO_WINDOW` process creation flag.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The bundled Tesseract binary and the directory holding its language data.
pub struct TesseractEngine {
    /// The sidecar binary.
    binary: PathBuf,
    /// The directory passed as `--tessdata-dir`.
    tessdata: PathBuf,
}

impl TesseractEngine {
    /// Find the binary and language data, in the order the plan sets.
    ///
    /// The debug-only `SCRIPTORIUM_TESSERACT` and `SCRIPTORIUM_TESSDATA` overrides come
    /// first, then the `scriptorium-tesseract` sidecar beside `current_exe()`, then, in debug
    /// builds, `src-tauri/binaries/scriptorium-tesseract-<triple>` and `src-tauri/tessdata/`.
    /// Both paths must exist; the binary is not run.
    pub fn locate(resource_dir: Option<&Path>) -> Result<Self, OcrUnavailable> {
        let binary = first_file(binary_candidates(), "Tesseract binary")?;
        let tessdata = first_dir(tessdata_candidates(resource_dir), "Tesseract language data")?;
        Ok(Self { binary, tessdata })
    }

    /// Run the sidecar once. `files` holds the output names and removes them when dropped.
    fn run(
        &self,
        files: &TempFiles,
        image_name: &OsStr,
        langs: &[Lang],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<PageOcr, OcrError> {
        let mut child = self
            .command(files, image_name, langs)?
            .spawn()
            .map_err(|e| {
                OcrError::Engine(format!("cannot start {}: {e}", self.binary.display()))
            })?;
        let status = wait(&mut child, stop, deadline)?;
        if !status.success() {
            let tail = tail(&files.stderr, STDERR_TAIL);
            let detail = if tail.is_empty() {
                String::new()
            } else {
                format!(": {tail}")
            };
            return Err(OcrError::Engine(format!(
                "tesseract exited with {status}{detail}"
            )));
        }
        let tsv = std::fs::read_to_string(&files.tsv)
            .map_err(|e| OcrError::Engine(format!("cannot read {}: {e}", files.tsv.display())))?;
        parse_tsv(&tsv)
    }

    /// Build the child command: bare image and base names, a cleared environment, and the
    /// fixed TSV options.
    fn command(
        &self,
        files: &TempFiles,
        image_name: &OsStr,
        langs: &[Lang],
    ) -> Result<Command, OcrError> {
        let stderr = File::create(&files.stderr).map_err(|e| {
            OcrError::Engine(format!("cannot create {}: {e}", files.stderr.display()))
        })?;
        let joined = langs
            .iter()
            .map(|lang| lang.0.as_str())
            .collect::<Vec<_>>()
            .join("+");
        let mut cmd = Command::new(&self.binary);
        cmd.current_dir(&files.dir)
            .arg(image_name)
            .arg(&files.base)
            .arg("--tessdata-dir")
            .arg(&self.tessdata)
            .arg("-l")
            .arg(joined)
            .arg("--psm")
            .arg("3")
            .arg("--oem")
            .arg("1")
            .arg("-c")
            .arg("tessedit_create_tsv=1")
            .arg("-c")
            .arg("tessedit_create_txt=0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr));
        // Nothing from the parent's environment, so a system Tesseract's settings and a
        // user's TESSDATA_PREFIX can never be picked up.
        cmd.env_clear();
        cmd.env("TESSDATA_PREFIX", &self.tessdata);
        cmd.env("OMP_THREAD_LIMIT", "1");
        #[cfg(windows)]
        constrain_on_windows(&mut cmd);
        Ok(cmd)
    }
}

impl OcrEngine for TesseractEngine {
    fn id(&self) -> EngineId {
        EngineId(format!("tesseract {TESSERACT_VERSION} {TESSDATA_VARIANT}"))
    }

    fn recognize(
        &self,
        image: &Path,
        langs: &[Lang],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<PageOcr, OcrError> {
        if langs.is_empty() {
            return Err(OcrError::NoLanguage);
        }
        let image_name = image.file_name().ok_or_else(|| {
            OcrError::Engine(format!("image path {} has no file name", image.display()))
        })?;
        let dir = match image.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        let files = TempFiles::new(dir);
        self.run(&files, image_name, langs, stop, deadline)
    }
}

/// The binary paths to try, in order.
fn binary_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("SCRIPTORIUM_TESSERACT") {
        out.push(PathBuf::from(path));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Tauri strips the triple and puts the sidecar beside the app executable.
            out.push(dir.join(format!(
                "scriptorium-tesseract{}",
                std::env::consts::EXE_SUFFIX
            )));
        }
    }
    #[cfg(debug_assertions)]
    out.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!(
                "scriptorium-tesseract-{}{}",
                env!("SCRIPTORIUM_TARGET_TRIPLE"),
                std::env::consts::EXE_SUFFIX
            )),
    );
    out
}

/// The language data directories to try, in order.
fn tessdata_candidates(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("SCRIPTORIUM_TESSDATA") {
        out.push(PathBuf::from(path));
    }
    if let Some(dir) = resource_dir {
        out.push(dir.join("tessdata"));
    }
    #[cfg(debug_assertions)]
    out.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tessdata"));
    out
}

/// The first candidate that is a file, or an error naming every path tried.
fn first_file(candidates: Vec<PathBuf>, what: &str) -> Result<PathBuf, OcrUnavailable> {
    if let Some(path) = candidates.iter().find(|path| path.is_file()) {
        return Ok(path.clone());
    }
    Err(OcrUnavailable::new(not_found(what, &candidates)))
}

/// The first candidate that is a directory, or an error naming every path tried.
fn first_dir(candidates: Vec<PathBuf>, what: &str) -> Result<PathBuf, OcrUnavailable> {
    if let Some(path) = candidates.iter().find(|path| path.is_dir()) {
        return Ok(path.clone());
    }
    Err(OcrUnavailable::new(not_found(what, &candidates)))
}

/// The "not found" message for `what`, listing `candidates`.
fn not_found(what: &str, candidates: &[PathBuf]) -> String {
    if candidates.is_empty() {
        return format!("{what} not found: no candidate paths");
    }
    let tried = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{what} not found; tried {tried}")
}

/// On Windows, start the child without a console window, and pass `SystemRoot` because some
/// Windows APIs need it to start a process.
#[cfg(windows)]
fn constrain_on_windows(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
    if let Some(root) = std::env::var_os("SystemRoot") {
        cmd.env("SystemRoot", root);
    }
}

/// Poll `child` until it exits, the stop flag is set, or the deadline passes. A stop or a
/// deadline kills and reaps the child first.
fn wait(child: &mut Child, stop: &AtomicBool, deadline: Instant) -> Result<ExitStatus, OcrError> {
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(stop_child(child, OcrError::Cancelled));
        }
        if Instant::now() >= deadline {
            return Err(stop_child(child, OcrError::Timeout));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => std::thread::sleep(POLL),
            Err(e) => {
                return Err(stop_child(
                    child,
                    OcrError::Engine(format!("cannot poll tesseract: {e}")),
                ))
            }
        }
    }
}

/// Kill and reap `child`, then return `error`.
fn stop_child(child: &mut Child, error: OcrError) -> OcrError {
    let _ = child.kill();
    let _ = child.wait();
    error
}

/// The last `max` bytes of `path`, lossily decoded and trimmed. A missing or unreadable file
/// reads as empty.
fn tail(path: &Path, max: usize) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    let start = bytes.len().saturating_sub(max);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
}

/// The names one call writes. Both are removed when this is dropped, on every path.
struct TempFiles {
    /// The directory the child runs in.
    dir: PathBuf,
    /// The output base, without its `.tsv` suffix.
    base: String,
    /// `<base>.tsv`, the engine's output.
    tsv: PathBuf,
    /// `<base>.err`, the engine's stderr.
    stderr: PathBuf,
}

impl TempFiles {
    /// Pick names unique to this call, so concurrent calls in one directory cannot collide.
    fn new(dir: &Path) -> Self {
        static CALL: AtomicU64 = AtomicU64::new(0);
        let n = CALL.fetch_add(1, Ordering::Relaxed);
        let base = format!("scriptorium-ocr-{}-{n}", std::process::id());
        Self {
            dir: dir.to_path_buf(),
            tsv: dir.join(format!("{base}.tsv")),
            stderr: dir.join(format!("{base}.err")),
            base,
        }
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tsv);
        let _ = std::fs::remove_file(&self.stderr);
    }
}

/// One TSV row the parser uses.
struct Row<'a> {
    /// 1 is the page, 4 a line, 5 a word.
    level: u32,
    /// The block the row belongs to.
    block: u32,
    /// The paragraph within the block; it restarts in each block.
    par: u32,
    /// Left edge in pixels.
    left: u32,
    /// Top edge in pixels.
    top: u32,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// Confidence, 0 to 100.
    conf: f32,
    /// The word's text; empty on the rows above a word.
    text: &'a str,
}

impl Row<'_> {
    /// The row's pixel box.
    fn bbox(&self) -> PixelBox {
        PixelBox {
            x: self.left,
            y: self.top,
            w: self.width,
            h: self.height,
        }
    }
}

/// Parse Tesseract's TSV output into a page.
///
/// The header must name the 12 expected columns. Level 1 gives the image size, level 4 starts
/// a line, and level 5 is a word. A word with empty trimmed text or a confidence below 0 is
/// dropped. Confidence is divided by 100 and clamped to 0..1. Paragraphs are numbered from 1
/// in first-appearance order of `(block_num, par_num)`. A bad row is an `Engine` error naming
/// it; a file with no level 1 row is an error; a file with no words is a valid empty page.
pub(crate) fn parse_tsv(tsv: &str) -> Result<PageOcr, OcrError> {
    let mut lines = tsv.lines();
    check_header(lines.next())?;
    let mut size: Option<(u32, u32)> = None;
    let mut out: Vec<Line> = Vec::new();
    let mut paragraphs: HashMap<(u32, u32), u32> = HashMap::new();
    let mut next_paragraph = 1;
    for (offset, text) in lines.enumerate() {
        let number = offset + 2;
        let row = parse_row(text, number)?;
        match row.level {
            1 => size = Some((row.width, row.height)),
            4 => {
                let key = (row.block, row.par);
                let paragraph = *paragraphs.entry(key).or_insert_with(|| {
                    let assigned = next_paragraph;
                    next_paragraph += 1;
                    assigned
                });
                out.push(Line {
                    bbox: row.bbox(),
                    paragraph,
                    words: Vec::new(),
                });
            }
            5 => add_word(&mut out, &row, number)?,
            _ => {}
        }
    }
    let (width, height) =
        size.ok_or_else(|| OcrError::Engine("TSV has no page row (level 1)".into()))?;
    Ok(PageOcr {
        width,
        height,
        lines: out,
    })
}

/// Check that the first line names the 12 expected columns.
fn check_header(first: Option<&str>) -> Result<(), OcrError> {
    let Some(first) = first else {
        return Err(OcrError::Engine("TSV is empty".into()));
    };
    let columns = first.split('\t').collect::<Vec<_>>();
    if columns.as_slice() != &TSV_COLUMNS[..] {
        return Err(row_error(
            1,
            format!("is not the expected 12-column header: {first:?}"),
        ));
    }
    Ok(())
}

/// Parse one row, checking every numeric column so a bad number names the row.
fn parse_row(line: &str, number: usize) -> Result<Row<'_>, OcrError> {
    let columns = line.split('\t').collect::<Vec<_>>();
    if columns.len() != TSV_COLUMNS.len() {
        return Err(row_error(
            number,
            format!(
                "has {} columns, expected {}",
                columns.len(),
                TSV_COLUMNS.len()
            ),
        ));
    }
    // Parsed even though unused, so a truncated or garbled row cannot slip through.
    let _: u32 = number_at(columns[1], number, "page_num")?;
    let _: u32 = number_at(columns[4], number, "line_num")?;
    let _: u32 = number_at(columns[5], number, "word_num")?;
    Ok(Row {
        level: number_at(columns[0], number, "level")?,
        block: number_at(columns[2], number, "block_num")?,
        par: number_at(columns[3], number, "par_num")?,
        left: number_at(columns[6], number, "left")?,
        top: number_at(columns[7], number, "top")?,
        width: number_at(columns[8], number, "width")?,
        height: number_at(columns[9], number, "height")?,
        conf: number_at(columns[10], number, "conf")?,
        text: columns[11],
    })
}

/// Parse one numeric column, naming the row and column when it will not parse.
fn number_at<T: std::str::FromStr>(text: &str, row: usize, name: &str) -> Result<T, OcrError> {
    text.parse()
        .map_err(|_| row_error(row, format!("has a bad {name}: {text:?}")))
}

/// An error naming row `number` with `detail`.
fn row_error(number: usize, detail: impl std::fmt::Display) -> OcrError {
    OcrError::Engine(format!("TSV row {number} {detail}"))
}

/// Attach a level 5 row to the line just started, dropping empty and negative-confidence
/// words.
fn add_word(out: &mut [Line], row: &Row<'_>, number: usize) -> Result<(), OcrError> {
    if row.text.trim().is_empty() || row.conf < 0.0 {
        return Ok(());
    }
    let Some(line) = out.last_mut() else {
        return Err(row_error(number, "is a word with no line row before it"));
    };
    line.words.push(Word {
        text: row.text.to_string(),
        bbox: row.bbox(),
        conf: (row.conf / 100.0).clamp(0.0, 1.0),
    });
    Ok(())
}

#[cfg(test)]
mod integration;
#[cfg(test)]
mod tests;
