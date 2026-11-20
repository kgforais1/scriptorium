//! The Phase 3 integration test: the fetched sidecar, the real smoke page, and a non-ASCII
//! `--tessdata-dir`.
//!
//! The plan files this as `src-tauri/tests/tesseract_engine.rs`, but `ocr` is a private module
//! of the lib crate, so an integration test could not name [`TesseractEngine`] without making
//! the whole engine surface public. It stays a unit test: `cargo test` still runs the real
//! binary through `locate` and `recognize`, and nothing is skipped when CI asks for it.

use super::*;

/// The temp directory name the plan fixes. It is not ASCII, so the absolute `--tessdata-dir`
/// path is checked outside the C locale on every platform.
const TEMP_NAME: &str = "scriptorium-ocr-tést";

/// The environment variable that makes a missing binary a failure instead of a skip. CI sets
/// it, so a CI run can never pass by skipping.
const REQUIRE: &str = "SCRIPTORIUM_REQUIRE_TESSERACT";

/// Every `tesseract` or `tesseract.exe` file on `PATH`.
fn tesseract_on_path() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Some(path) = std::env::var_os("PATH") else {
        return found;
    };
    for dir in std::env::split_paths(&path) {
        for name in ["tesseract", "tesseract.exe"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                found.push(candidate);
            }
        }
    }
    found
}

/// Whether `a` and `b` name the same file, following symlinks where both exist.
fn same_file(a: &Path, b: &Path) -> bool {
    let resolve = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    resolve(a) == resolve(b)
}

/// Copy `from` to `to`, naming both paths when it fails.
fn copy(from: &Path, to: &Path) {
    std::fs::copy(from, to)
        .unwrap_or_else(|e| panic!("cannot copy {} to {}: {e}", from.display(), to.display()));
}

/// The temp directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    /// Create the directory empty, replacing a leftover one. The process id keeps two test
    /// processes on one machine out of each other's way.
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("{TEMP_NAME}-{}", std::process::id()));
        assert!(
            dir.is_absolute(),
            "a temp directory must be absolute: {}",
            dir.display()
        );
        assert!(
            !dir.to_string_lossy().is_ascii(),
            "the temp path must be non-ASCII: {}",
            dir.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
        Self(dir)
    }

    /// The directory path.
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_fetched_sidecar_reads_the_smoke_page_from_a_non_ascii_directory() {
    let strict = std::env::var(REQUIRE).as_deref() == Ok("1");
    let on_path = tesseract_on_path();
    if strict {
        // The plan's first check: a PATH tesseract would make the test prove nothing.
        assert!(
            on_path.is_empty(),
            "{REQUIRE}=1, but tesseract is on PATH at {on_path:?}"
        );
    } else if let Some(first) = on_path.first() {
        eprintln!(
            "note: {} is on PATH; the test runs the fetched sidecar, which must be a different \
             file",
            first.display()
        );
    }

    let located = match TesseractEngine::locate(None) {
        Ok(engine) => engine,
        Err(error) if strict => {
            panic!("{REQUIRE}=1, but the Tesseract engine was not found: {error}")
        }
        Err(error) => {
            eprintln!("note: skipping the Tesseract integration test: {error}");
            return;
        }
    };
    for candidate in &on_path {
        assert!(
            !same_file(&located.binary, candidate),
            "locate chose the tesseract on PATH at {}; the bundle was not exercised",
            candidate.display()
        );
    }

    let dir = TempDir::new();
    copy(
        &located.tessdata.join("eng.traineddata"),
        &dir.path().join("eng.traineddata"),
    );
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/ocr/smoke.png");
    let image = dir.path().join("smoke.png");
    copy(&fixture, &image);

    // `--tessdata-dir` is the temp copy. Set the field rather than `SCRIPTORIUM_TESSDATA`:
    // `cargo test` runs tests as threads in one process, so changing the process environment
    // here would race every other test that reads one. The child still gets an absolute
    // non-ASCII path, and the image still reaches it as a bare name from its own directory.
    let engine = TesseractEngine {
        binary: located.binary,
        tessdata: dir.path().to_path_buf(),
    };
    eprintln!(
        "note: running {} from {}",
        engine.id().0,
        engine.binary.display()
    );
    let page = engine
        .recognize(
            &image,
            &[Lang("eng".into())],
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap_or_else(|e| panic!("recognize failed on {}: {e:?}", image.display()));
    assert_eq!((page.width, page.height), (900, 160));

    let word = page
        .lines
        .iter()
        .flat_map(|line| line.words.iter())
        .find(|word| word.text.to_ascii_uppercase().contains("PRINCESS"))
        .unwrap_or_else(|| panic!("no word contains \"PRINCESS\": {:?}", page.lines));
    let bbox = word.bbox;
    assert!(
        bbox.w > 0 && bbox.h > 0,
        "the word has an empty box: {bbox:?}"
    );
    assert!(
        bbox.x + bbox.w <= page.width && bbox.y + bbox.h <= page.height,
        "the word's box {bbox:?} is outside the {}x{} page",
        page.width,
        page.height
    );
}
