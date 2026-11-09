//! Store and schema tests: the layer guarantees, the v0 shim, and the index rebuild rules.

use super::engine::{EngineId, Lang, Line, OcrEngine, PageOcr, PixelBox, Word};
use super::fake::FakeEngine;
use super::store::*;
use crate::book_format::BookMeta;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ENGINE: &str = "fake 1.0";

/// A book directory with an `ocr/` folder, removed on drop.
struct Book(PathBuf);

impl Book {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "scriptorium-ocr-store-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ocr")).unwrap();
        Self(dir)
    }

    fn page(&self, i: usize) -> PathBuf {
        page_path(&self.0, i, 4)
    }

    fn write(&self, rel: &str, body: &str) {
        std::fs::write(self.0.join(rel), body).unwrap();
    }
}

impl Drop for Book {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A four-page book whose pages all have checksum `SHA_A`.
fn meta() -> BookMeta {
    let mut m = BookMeta::sample();
    let page = m.pages[0].clone();
    m.pages = (0..4)
        .map(|i| {
            let mut p = page.clone();
            p.index = i;
            p.sha256 = SHA_A.into();
            p
        })
        .collect();
    m
}

fn engine() -> EngineId {
    EngineId(ENGINE.into())
}

fn sample_page() -> PageOcr {
    let b = PixelBox {
        x: 10,
        y: 20,
        w: 50,
        h: 10,
    };
    PageOcr {
        width: 100,
        height: 100,
        lines: vec![Line {
            bbox: b,
            paragraph: 1,
            words: vec![Word {
                text: "Once".into(),
                bbox: b,
                conf: 0.96,
            }],
        }],
    }
}

/// Run the fake engine and build the layer it would store.
fn layer(run: &str, sha: &str) -> OcrLayer {
    let fake = FakeEngine {
        id: ENGINE.into(),
        result: Ok(sample_page()),
    };
    let later = Instant::now() + Duration::from_secs(60);
    let page = fake
        .recognize(
            Path::new("p.png"),
            &[Lang("eng".into())],
            &AtomicBool::new(false),
            later,
        )
        .unwrap();
    OcrLayer::new(&page, &fake.id(), run, sha)
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn done(run: &str) -> Entry {
    Entry::Done {
        image_sha256: SHA_A.into(),
        run_id: run.into(),
    }
}

fn old_index(book: &Book, pages: Vec<(usize, Entry)>) {
    let index = OcrIndex::new(ENGINE, pages.into_iter().collect(), 0);
    write_index(&book.0, &index).unwrap();
}

#[test]
fn ocr_over_pdf_and_curator_layers_keeps_both() {
    let book = Book::new("layers");
    let curator = json!({ "text": "Fixed by hand", "history": [{ "by": "me", "n": 1 }] });
    let page =
        json!({ "schema": 1, "layers": { "pdf": { "text": "From PDF" }, "curator": curator } });
    std::fs::write(book.page(0), page.to_string()).unwrap();
    assert_eq!(
        write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap(),
        OcrWrite::Written
    );
    let after = read_json(&book.page(0));
    assert_eq!(after["layers"]["pdf"], page["layers"]["pdf"]);
    assert_eq!(after["layers"]["curator"], page["layers"]["curator"]);
    assert_eq!(after["layers"]["ocr"]["text"], "Once");
    assert_eq!(
        after["layers"]["ocr"]["lines"][0]["bbox"],
        json!([0.1, 0.2, 0.5, 0.1])
    );
    assert_eq!(published_text(&after), Some("Fixed by hand"));
}

#[test]
fn ocr_over_an_invalid_file_leaves_it_and_fails_the_page() {
    let book = Book::new("invalid");
    let body =
        r#"{ "schema": 1, "layers": { "curator": { "text": "keep" }, "pdf": { "text": "" } } }"#;
    std::fs::write(book.page(1), body).unwrap();
    let out = write_ocr_layer(&book.page(1), &layer("r1", SHA_A)).unwrap();
    assert!(matches!(out, OcrWrite::Refused(_)));
    assert_eq!(std::fs::read_to_string(book.page(1)).unwrap(), body);
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert!(
        matches!(&index.pages[&1], Entry::Failed { error, after_run_id: None, .. } if error.contains("invalid"))
    );
}

#[test]
fn a_missing_file_gets_an_ocr_only_page() {
    let book = Book::new("fresh");
    write_ocr_layer(&book.page(2), &layer("r1", SHA_A)).unwrap();
    let PageRead::Valid(v) = read_page(&book.page(2)).unwrap() else {
        panic!("written page must read back");
    };
    assert_eq!(
        ocr_layer(&v),
        Some(layer("r1", SHA_A)).map(|mut l| {
            l.generated_at = v["layers"]["ocr"]["generatedAt"].as_str().unwrap().into();
            l
        })
    );
    assert_eq!(read_page(&book.page(3)).unwrap(), PageRead::Missing);
}

#[test]
fn a_v0_page_reads_as_v1_with_a_pdf_layer() {
    let book = Book::new("v0");
    book.write(
        "ocr/0000.json",
        r#"{ "source": "pdf", "text": "Old text" }"#,
    );
    let read = read_page(&book.page(0)).unwrap();
    let expected = json!({ "schema": 1, "layers": { "pdf": { "text": "Old text" } } });
    assert_eq!(read, PageRead::Valid(expected.clone()));
    assert!(validate_v1(&expected).is_ok());
    // OCR then writes it as v1, keeping the PDF text.
    write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap();
    let after = read_json(&book.page(0));
    assert_eq!(after["schema"], 1);
    assert_eq!(after["layers"]["pdf"]["text"], "Old text");
}

#[test]
fn a_file_that_is_neither_v0_nor_v1_is_invalid() {
    let book = Book::new("neither");
    for body in [
        r#"{ "source": "pdf", "text": "" }"#,
        r#"{ "source": "pdf", "text": "x", "schema": 1 }"#,
        r#"{ "hello": 1 }"#,
        r#"[1, 2]"#,
        "not json",
    ] {
        book.write("ocr/0000.json", body);
        assert!(
            matches!(read_page(&book.page(0)).unwrap(), PageRead::Invalid(_)),
            "{body}"
        );
    }
}

#[test]
fn published_text_prefers_curator_then_ocr_then_pdf() {
    let pdf = json!({ "text": "pdf" });
    let ocr = json!({ "text": "ocr" });
    let page = |layers: Value| json!({ "schema": 1, "layers": layers });
    assert_eq!(published_text(&page(json!({ "pdf": pdf }))), Some("pdf"));
    assert_eq!(
        published_text(&page(json!({ "pdf": pdf, "ocr": ocr }))),
        Some("ocr")
    );
    let curator = json!({ "text": "cur" });
    assert_eq!(
        published_text(&page(json!({ "pdf": pdf, "ocr": ocr, "curator": curator }))),
        Some("cur")
    );
    // A curator layer with no text yet does not hide the OCR text.
    assert_eq!(
        published_text(&page(json!({ "ocr": ocr, "curator": {} }))),
        Some("ocr")
    );
    assert_eq!(published_text(&json!({})), None);
}

#[test]
fn stale_layers_are_pending_by_checksum_and_by_engine() {
    let book = Book::new("stale");
    write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap();
    write_ocr_layer(&book.page(1), &layer("r1", SHA_B)).unwrap();
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], done("r1"));
    assert_eq!(index.pages[&1], Entry::Pending, "checksum changed");
    let newer = rebuild_index(&book.0, &meta(), &EngineId("fake 2.0".into())).unwrap();
    assert_eq!(newer.pages[&0], Entry::Pending, "engine changed");
    assert!(layer("r1", SHA_A).is_current(&engine(), SHA_A));
}

#[test]
fn rebuild_after_a_crash_recovers_done_pages() {
    let book = Book::new("crash");
    book.write(
        "ocr/0003.json",
        r#"{ "source": "pdf", "text": "pdf only" }"#,
    );
    write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap();
    write_ocr_layer(&book.page(1), &layer("r1", SHA_A)).unwrap();
    // The index is missing: the page files alone say which pages are done.
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], done("r1"));
    assert_eq!(index.pages[&1], done("r1"));
    assert_eq!(index.pages[&2], Entry::Pending);
    assert_eq!(index.pages[&3], Entry::Pending);
    let s = index.summary;
    assert_eq!(
        (s.pages, s.done, s.pending, s.failed, s.skipped, s.with_text),
        (4, 2, 2, 0, 0, 3)
    );
    // A stale index that lags the page files is corrected the same way.
    old_index(&book, vec![(0, done("r1")), (1, Entry::Pending)]);
    let again = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(again.pages[&1], done("r1"));
    write_index(&book.0, &again).unwrap();
    assert_eq!(read_index(&book.0).unwrap().pages, again.pages);
}

#[test]
fn a_missing_or_invalid_old_index_brings_no_page_back_skipped() {
    let book = Book::new("noskip");
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.summary.skipped, 0);
    for body in [
        "not json",
        r#"{ "schema": 1, "pages": { "0": { "status": "skipped" } } }"#,
    ] {
        book.write("ocr/index.json", body);
        assert!(read_index(&book.0).is_none());
        let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
        assert_eq!(index.summary.pending, 4, "{body}");
    }
    // A valid old index keeps `skipped`, even over a current layer.
    write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap();
    old_index(&book, vec![(0, Entry::Skipped)]);
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], Entry::Skipped);
}

#[test]
fn a_failure_is_carried_until_a_new_layer_lands() {
    let book = Book::new("carried");
    write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap();
    let failed = |after: Option<&str>| Entry::Failed {
        error: "timeout".into(),
        failed_at: "2026-11-07T12:00:00Z".into(),
        after_run_id: after.map(Into::into),
    };
    // Page 0 failed over r1's layer, page 1 failed with no layer, page 2 failed with no
    // layer but has since been written.
    old_index(
        &book,
        vec![
            (0, failed(Some("r1"))),
            (1, failed(None)),
            (2, failed(None)),
        ],
    );
    write_ocr_layer(&book.page(2), &layer("r2", SHA_A)).unwrap();
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], failed(Some("r1")));
    assert_eq!(index.pages[&1], failed(None));
    assert_eq!(index.pages[&2], done("r2"));
    // A later success clears page 0's failure.
    write_ocr_layer(&book.page(0), &layer("r3", SHA_A)).unwrap();
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], done("r3"));
    assert_eq!(index.summary.failed, 1);
}

#[test]
fn a_forced_run_reopens_pages_it_has_not_written() {
    let book = Book::new("forced");
    write_ocr_layer(&book.page(0), &layer("old", SHA_A)).unwrap();
    write_ocr_layer(&book.page(1), &layer("f1", SHA_A)).unwrap();
    book.write("ocr/forced.json", r#"{ "runId": "f1" }"#);
    // The index is lost, but forced intent survives in its own file.
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], Entry::Pending);
    assert_eq!(index.pages[&1], done("f1"));
    assert_eq!(read_forced(&book.0).unwrap().as_deref(), Some("f1"));
}

#[test]
fn an_invalid_forced_file_is_deleted_and_the_run_is_not_forced() {
    let book = Book::new("forced-bad");
    write_ocr_layer(&book.page(0), &layer("old", SHA_A)).unwrap();
    for body in [
        "{",
        r#"{ "runId": "" }"#,
        r#"{ "runId": "a b" }"#,
        r#"{ "runId": "x", "more": 1 }"#,
    ] {
        book.write("ocr/forced.json", body);
        let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
        assert_eq!(index.pages[&0], done("old"), "{body}");
        assert!(!book.0.join("ocr/forced.json").exists(), "{body}");
    }
    assert_eq!(
        read_forced(&book.0).unwrap(),
        None,
        "missing means not forced"
    );
}

fn ok(doc: &Value) -> bool {
    validate_v1(doc).is_ok()
}

fn valid_page() -> Value {
    json!({
        "schema": 1,
        "layers": {
            "ocr": serde_json::to_value(layer("r1", SHA_A)).unwrap(),
            "pdf": { "text": "From PDF" },
            "curator": { "anything": [1, 2] },
        }
    })
}

fn valid_index() -> Value {
    json!({
        "schema": 1,
        "engine": ENGINE,
        "updatedAt": "2026-11-07T12:00:00Z",
        "pages": {
            "0": { "status": "done", "imageSha256": SHA_A, "runId": "r1" },
            "1": { "status": "failed", "error": "timeout", "failedAt": "2026-11-07T12:00:00Z", "afterRunId": "r1" },
            "2": { "status": "failed", "error": "timeout", "failedAt": "2026-11-07T12:00:00Z", "afterRunId": null },
            "3": { "status": "failed", "error": "timeout", "failedAt": "2026-11-07T12:00:00Z" },
            "4": { "status": "pending" },
            "5": { "status": "skipped" },
        },
        "summary": { "pages": 6, "done": 1, "failed": 3, "pending": 1, "skipped": 1, "withText": 1 },
    })
}

#[test]
fn schema_accepts_valid_pages_and_indexes() {
    assert!(ok(&valid_page()));
    let empty = json!({ "schema": 1, "layers": { "ocr": {
        "engine": ENGINE, "generatedAt": "2026-11-07T12:00:00Z", "runId": "r1",
        "imageSha256": SHA_A, "text": "", "lines": [] } } });
    assert!(ok(&empty), "a plate with no text is valid");
    assert!(ok(
        &json!({ "schema": 1, "layers": { "pdf": { "text": "x" } } })
    ));
    assert!(ok(&valid_index()));
    assert!(validate_v0(&json!({ "source": "pdf", "text": "x" })).is_ok());
}

/// Apply `edit` to a copy of `doc` at the JSON pointer `at`, and expect a refusal.
fn refuses(doc: Value, at: &str, edit: Option<Value>) {
    let mut doc = doc;
    let (parent, key) = at.rsplit_once('/').unwrap();
    let obj = doc.pointer_mut(parent).unwrap();
    match (obj, edit) {
        (Value::Object(m), Some(v)) => {
            m.insert(key.into(), v);
        }
        (Value::Object(m), None) => {
            m.remove(key);
        }
        (Value::Array(a), Some(v)) => a[key.parse::<usize>().unwrap()] = v,
        _ => unreachable!(),
    }
    assert!(!ok(&doc), "{at} should be refused: {doc}");
}

#[test]
fn schema_refuses_bad_pages() {
    let p = valid_page;
    refuses(p(), "/schema", Some(json!(2)));
    refuses(p(), "/schema", None);
    refuses(p(), "/layers", Some(json!({})));
    refuses(p(), "/layers/other", Some(json!({ "text": "x" })));
    refuses(p(), "/extra", Some(json!(1)));
    refuses(p(), "/layers/pdf/text", Some(json!("")));
    refuses(p(), "/layers/pdf/boxes", Some(json!([])));
    refuses(p(), "/layers/curator", Some(json!("text")));
    refuses(p(), "/layers/ocr/engine", Some(json!("")));
    refuses(p(), "/layers/ocr/generatedAt", Some(json!("yesterday")));
    refuses(p(), "/layers/ocr/runId", Some(json!("has space")));
    refuses(p(), "/layers/ocr/imageSha256", Some(json!("ABC")));
    refuses(p(), "/layers/ocr/lines", None);
    refuses(p(), "/layers/ocr/text", None);
    refuses(p(), "/layers/ocr/lines/0/bbox", Some(json!([0, 0, 1])));
    refuses(p(), "/layers/ocr/lines/0/bbox/2", Some(json!(1.5)));
    refuses(p(), "/layers/ocr/lines/0/bbox/0", Some(json!(-0.1)));
    refuses(p(), "/layers/ocr/lines/0/words/0/conf", Some(json!(1.2)));
    refuses(p(), "/layers/ocr/lines/0/words/0/text", Some(json!("")));
    refuses(p(), "/layers/ocr/lines/0/words/0/conf", None);
    refuses(p(), "/layers/ocr/lines/0/words", Some(json!([])));
}

#[test]
fn schema_refuses_bad_indexes() {
    let i = valid_index;
    refuses(i(), "/schema", Some(json!(0)));
    refuses(i(), "/engine", None);
    refuses(i(), "/updatedAt", Some(json!("yesterday")));
    refuses(i(), "/pages/0001", Some(json!({ "status": "pending" })));
    refuses(i(), "/pages/0/status", Some(json!("maybe")));
    refuses(i(), "/pages/0/imageSha256", None);
    refuses(i(), "/pages/0/runId", None);
    refuses(i(), "/pages/4/runId", Some(json!("r1")));
    refuses(i(), "/pages/1/error", None);
    refuses(i(), "/pages/1/failedAt", None);
    refuses(i(), "/pages/1/afterRunId", Some(json!(5)));
    refuses(i(), "/summary/withText", None);
    refuses(i(), "/summary/done", Some(json!(-1)));
    refuses(i(), "/summary/extra", Some(json!(0)));
}

#[test]
fn a_plate_with_an_empty_layer_is_done() {
    let book = Book::new("plate");
    let empty = PageOcr {
        width: 10,
        height: 10,
        lines: vec![],
    };
    let plate = OcrLayer::new(&empty, &engine(), "r1", SHA_A);
    write_ocr_layer(&book.page(0), &plate).unwrap();
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert_eq!(index.pages[&0], done("r1"));
    assert_eq!(index.summary.with_text, 0);
}

#[test]
fn a_v0_index_reads_as_missing() {
    let book = Book::new("v0-index");
    let v0 = json!({
        "engine": "pdfium",
        "generatedAt": "2026-11-07T12:00:00Z",
        "pages": { "0": { "status": "text" }, "1": { "status": "none" } },
        "summary": { "pdfPages": 2, "withText": 1 },
    });
    assert!(validate_v0(&v0).is_ok());
    book.write("ocr/index.json", &v0.to_string());
    assert!(read_index(&book.0).is_none());
}

#[test]
fn an_unreadable_page_file_fails_that_page_only() {
    let book = Book::new("unreadable");
    std::fs::create_dir(book.page(1)).unwrap();
    write_ocr_layer(&book.page(0), &layer("r1", SHA_A)).unwrap();
    let index = rebuild_index(&book.0, &meta(), &engine()).unwrap();
    assert!(
        matches!(&index.pages[&1], Entry::Failed { error, .. } if error.contains("cannot be read"))
    );
    assert_eq!(index.pages[&0], done("r1"));
}

#[test]
fn forced_files_follow_the_schemas_run_id_rule() {
    assert!(validate_v1(&json!({ "runId": "f-1_A" })).is_ok());
    assert!(validate_v1(&json!({ "runId": "x".repeat(65) })).is_err());
    let book = Book::new("forced-shape");
    // A valid index is valid v1, but it is not a forced file.
    book.write("ocr/forced.json", &valid_index().to_string());
    assert_eq!(read_forced(&book.0).unwrap(), None);
    assert!(!book.0.join("ocr/forced.json").exists());
}

#[test]
fn ocr_over_an_unreadable_file_is_refused() {
    let book = Book::new("unreadable-write");
    std::fs::create_dir(book.page(2)).unwrap();
    let out = write_ocr_layer(&book.page(2), &layer("r1", SHA_A)).unwrap();
    assert!(matches!(out, OcrWrite::Refused(r) if r.contains("cannot be read")));
    assert!(book.page(2).is_dir());
}

#[test]
fn a_v1_document_of_the_wrong_shape_is_not_a_page() {
    let book = Book::new("wrong-shape");
    book.write("ocr/0000.json", r#"{ "runId": "f1" }"#);
    let PageRead::Invalid(reason) = read_page(&book.page(0)).unwrap() else {
        panic!("a forced file is not a page");
    };
    assert!(reason.contains("not a page"), "{reason}");
}
