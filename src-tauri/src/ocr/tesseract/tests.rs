//! The `tesseract` engine tests: parser fixtures and the spawn paths.

use super::*;
use std::sync::Arc;

const SMOKE: &str = include_str!("../testdata/smoke.tsv");
const PROSE: &str = include_str!("../testdata/page-prose.tsv");
const EMPTY_ROWS: &str = include_str!("../testdata/empty-rows.tsv");
const PAR_RESTART: &str = include_str!("../testdata/par-restart.tsv");
const PAGE_ONLY: &str = include_str!("../testdata/page-only.tsv");
const TRUNCATED: &str = include_str!("../testdata/truncated-row.tsv");
const BAD_NUMBER: &str = include_str!("../testdata/bad-number.tsv");
const CRLF: &str = include_str!("../testdata/crlf.tsv");

fn eng() -> [Lang; 1] {
    [Lang("eng".into())]
}

fn text_of(page: &PageOcr) -> Vec<&str> {
    page.lines
        .iter()
        .flat_map(|line| line.words.iter())
        .map(|word| word.text.as_str())
        .collect()
}

#[test]
fn id_matches_the_fetch_script_and_the_trait() {
    let script = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scripts/fetch_tesseract.sh"
    ))
    .expect("read fetch_tesseract.sh");
    assert!(
        script.contains(&format!("TESSERACT_RELEASE=\"{TESSERACT_RELEASE}\"")),
        "fetch_tesseract.sh no longer pins {TESSERACT_RELEASE}"
    );
    assert!(
        TESSERACT_RELEASE.starts_with(&format!("tesseract-{TESSERACT_VERSION}-r")),
        "the id version must come from the pinned release"
    );
    let engine = TesseractEngine {
        binary: PathBuf::from("tesseract"),
        tessdata: PathBuf::from("tessdata"),
    };
    assert_eq!(engine.id().0, "tesseract 5.5.4 eng-fast");
}

#[test]
fn a_relative_path_becomes_absolute_and_an_absolute_one_is_kept() {
    let relative = absolutise(PathBuf::from("bin/tesseract"));
    assert!(relative.is_absolute(), "{}", relative.display());
    assert!(
        relative.ends_with(Path::new("bin").join("tesseract")),
        "{}",
        relative.display()
    );
    let absolute = std::env::temp_dir().join("tessdata");
    assert_eq!(absolutise(absolute.clone()), absolute);
}

#[test]
fn a_real_smoke_page_parses() {
    let page = parse_tsv(SMOKE).unwrap();
    assert_eq!((page.width, page.height), (900, 160));
    assert_eq!(page.lines.len(), 1);
    let line = &page.lines[0];
    assert_eq!(line.paragraph, 1);
    assert_eq!(
        line.bbox,
        PixelBox {
            x: 22,
            y: 53,
            w: 719,
            h: 35
        }
    );
    assert_eq!(
        text_of(&page),
        vec!["THE", "PRINCESS", "AND", "THE", "GOBLIN"]
    );
    assert!((line.words[1].conf - 0.956_421_43).abs() < 1e-6);
}

#[test]
fn a_real_multi_block_page_numbers_paragraphs_in_order() {
    let page = parse_tsv(PROSE).unwrap();
    assert_eq!((page.width, page.height), (2808, 3906));
    assert_eq!(page.lines.len(), 36);
    assert_eq!(page.lines[0].paragraph, 1);
    // Block 2 starts its own par_num at 1, so it is the second paragraph overall.
    assert_eq!(page.lines[1].paragraph, 2);
    assert_eq!(page.lines[7].paragraph, 3);
    assert_eq!(page.lines[35].paragraph, 7);
    assert_eq!(page.lines[0].words[0].text, "LAYLA");
}

#[test]
fn empty_text_and_negative_confidence_words_are_dropped() {
    let page = parse_tsv(EMPTY_ROWS).unwrap();
    assert_eq!(text_of(&page), vec!["HELLO", "WORLD"]);
    assert_eq!(page.lines[0].words[0].conf, 1.0, "250 is clamped");
    assert!((page.lines[0].words[1].conf - 0.425).abs() < 1e-6);
}

#[test]
fn a_nan_confidence_is_stored_as_zero() {
    let tsv = format!(
        "{}\n1\t1\t0\t0\t0\t0\t0\t0\t10\t20\t-1\t\n\
         4\t1\t1\t1\t1\t0\t0\t0\t10\t10\t95\t\n\
         5\t1\t1\t1\t1\t1\t0\t0\t10\t10\tNaN\tWORD\n",
        TSV_COLUMNS.join("\t")
    );
    let page = parse_tsv(&tsv).unwrap();
    assert_eq!(page.lines[0].words[0].conf, 0.0);
}

#[test]
fn paragraph_numbers_restart_with_each_block() {
    let page = parse_tsv(PAR_RESTART).unwrap();
    let paragraphs = page
        .lines
        .iter()
        .map(|line| line.paragraph)
        .collect::<Vec<_>>();
    assert_eq!(paragraphs, vec![1, 2, 3, 4]);
    assert_eq!(
        text_of(&page),
        vec!["one", "two", "three", "four"],
        "each line keeps its own words"
    );
}

#[test]
fn a_page_with_no_words_is_valid() {
    let page = parse_tsv(PAGE_ONLY).unwrap();
    assert_eq!((page.width, page.height), (900, 160));
    assert!(page.lines.is_empty());
}

#[test]
fn a_truncated_row_names_the_row() {
    let error = parse_tsv(TRUNCATED).unwrap_err();
    let OcrError::Engine(message) = error else {
        panic!("a truncated row is an engine error");
    };
    assert!(message.contains("row 3"), "{message}");
    assert!(message.contains("columns"), "{message}");
}

#[test]
fn a_bad_number_names_the_row() {
    let error = parse_tsv(BAD_NUMBER).unwrap_err();
    let OcrError::Engine(message) = error else {
        panic!("a bad number is an engine error");
    };
    assert!(message.contains("row 3"), "{message}");
    assert!(message.contains("conf"), "{message}");
}

#[test]
fn crlf_is_read_like_lf() {
    let page = parse_tsv(CRLF).unwrap();
    assert_eq!((page.width, page.height), (10, 20));
    assert_eq!(text_of(&page), vec!["CRLF"]);
}

#[test]
fn a_missing_page_row_or_a_bad_header_is_an_error() {
    let no_page = format!("{}\n", TSV_COLUMNS.join("\t"));
    assert!(matches!(parse_tsv(&no_page), Err(OcrError::Engine(m)) if m.contains("page row")));
    let bad = String::from("not\tthe\theader\n");
    assert!(matches!(parse_tsv(&bad), Err(OcrError::Engine(m)) if m.contains("row 1")));
    assert!(matches!(parse_tsv(""), Err(OcrError::Engine(m)) if m.contains("empty")));
}

#[test]
fn a_word_without_a_line_row_names_the_row() {
    let tsv = format!(
        "{}\n1\t1\t0\t0\t0\t0\t0\t0\t10\t20\t-1\t\n\
         5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t90\tOrphan\n",
        TSV_COLUMNS.join("\t")
    );
    assert!(
        matches!(parse_tsv(&tsv), Err(OcrError::Engine(m)) if m.contains("row 3") && m.contains("no line"))
    );
}

/// A temporary directory under the OS temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        static CALL: AtomicU64 = AtomicU64::new(0);
        let n = CALL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "scriptorium-ocr-test-{name}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The stand-in engine `examples/ocr_test_child.rs`, which `cargo test` builds with the tests.
/// `cargo llvm-cov` builds no examples, so the coverage job builds it itself and names it in
/// `SCRIPTORIUM_TEST_CHILD`.
fn stand_in() -> PathBuf {
    if let Some(path) = std::env::var_os("SCRIPTORIUM_TEST_CHILD") {
        return PathBuf::from(path);
    }
    let exe = std::env::current_exe().unwrap();
    let debug = exe.parent().unwrap().parent().unwrap();
    let path = debug
        .join("examples")
        .join(format!("ocr_test_child{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} is missing; `cargo test` builds the ocr_test_child example",
        path.display()
    );
    path
}

fn stand_in_engine(tessdata: &Path) -> TesseractEngine {
    TesseractEngine {
        binary: stand_in(),
        tessdata: tessdata.to_path_buf(),
    }
}

/// Create the empty image the stand-in is told to recognise.
fn image(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"").unwrap();
    path
}

/// No temporary output or stderr file survives a call.
fn assert_clean(dir: &Path) {
    let left = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("scriptorium-ocr-"))
        .collect::<Vec<_>>();
    assert!(left.is_empty(), "temporary files left: {left:?}");
}

#[test]
fn a_successful_run_parses_and_cleans_up() {
    let dir = TempDir::new("ok");
    let image = image(dir.path(), "ok.png");
    let page = stand_in_engine(dir.path())
        .recognize(
            &image,
            &eng(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!((page.width, page.height), (10, 20));
    assert_eq!(text_of(&page), vec!["Hi"]);
    assert!((page.lines[0].words[0].conf - 0.9).abs() < 1e-6);
    assert_clean(dir.path());
}

#[test]
fn cancel_stops_the_child_within_two_seconds() {
    let dir = TempDir::new("cancel");
    let image = image(dir.path(), "sleep.png");
    let stop = Arc::new(AtomicBool::new(false));
    let setter = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            stop.store(true, Ordering::Relaxed);
        })
    };
    let start = Instant::now();
    let result = stand_in_engine(dir.path()).recognize(
        &image,
        &eng(),
        &stop,
        Instant::now() + Duration::from_secs(30),
    );
    let elapsed = start.elapsed();
    setter.join().unwrap();
    assert_eq!(result, Err(OcrError::Cancelled));
    assert!(elapsed < Duration::from_secs(2), "cancel took {elapsed:?}");
    assert_clean(dir.path());
}

#[test]
fn a_passed_deadline_returns_timeout() {
    let dir = TempDir::new("timeout");
    let image = image(dir.path(), "sleep.png");
    let start = Instant::now();
    let result = stand_in_engine(dir.path()).recognize(
        &image,
        &eng(),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_millis(300),
    );
    let elapsed = start.elapsed();
    assert_eq!(result, Err(OcrError::Timeout));
    assert!(elapsed < Duration::from_secs(5), "timeout took {elapsed:?}");
    assert_clean(dir.path());
}

#[test]
fn a_non_zero_exit_puts_stderr_in_the_error() {
    let dir = TempDir::new("fail");
    let image = image(dir.path(), "fail.png");
    let error = stand_in_engine(dir.path())
        .recognize(
            &image,
            &eng(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap_err();
    let OcrError::Engine(message) = error else {
        panic!("a failed child is an engine error");
    };
    assert!(message.contains("did not converge"), "{message}");
    assert!(message.contains('3'), "the exit status is named: {message}");
    assert_clean(dir.path());
}

#[test]
fn a_missing_tsv_is_an_engine_error_and_cleans_up() {
    let dir = TempDir::new("no-tsv");
    let image = image(dir.path(), "crash.png");
    let error = stand_in_engine(dir.path())
        .recognize(
            &image,
            &eng(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap_err();
    assert!(
        matches!(&error, OcrError::Engine(m) if m.contains("cannot read")),
        "{error:?}"
    );
    assert_clean(dir.path());
}

#[test]
fn no_language_is_refused_before_spawning() {
    let dir = TempDir::new("no-lang");
    let image = image(dir.path(), "ok.png");
    let result = stand_in_engine(dir.path()).recognize(
        &image,
        &[],
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(30),
    );
    assert_eq!(result, Err(OcrError::NoLanguage));
    assert_clean(dir.path());
}

#[test]
fn tail_keeps_only_the_last_bytes() {
    let dir = TempDir::new("tail");
    let path = dir.path().join("err.txt");
    std::fs::write(&path, format!("{}end", "x".repeat(5000))).unwrap();
    let kept = tail(&path, 10);
    assert_eq!(kept, "xxxxxxxend");
    assert_eq!(tail(&dir.path().join("missing"), 10), "");
}
