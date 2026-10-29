//! formatVersion 2 tests: originals (`sources[]`, page `source`) and the v1/v2 schema split.

use super::*;
use serde_json::Value;

const SHA: &str = "b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c";

fn schema(version: u32) -> jsonschema::Validator {
    let path = format!(
        "{}/../docs/book-format.v{version}.schema.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let value: Value = serde_json::from_str(&text).expect("schema is JSON");
    jsonschema::validator_for(&value).expect("schema compiles")
}

fn page(index: u32, source: Option<PageSource>) -> PageEntry {
    PageEntry {
        index,
        file: format!("pages/{index:04}.jpg"),
        width: 1200,
        height: 1600,
        byte_size: 1000,
        sha256: SHA.into(),
        page_label: None,
        storage: StorageMode::Copied,
        source,
    }
}

fn src_ref(id: &str, frame: Option<u32>) -> Option<PageSource> {
    Some(PageSource {
        source_ref: id.into(),
        frame,
    })
}

/// Page 0 is an ordinary JPEG; pages 1–3 are the frames of one 3-frame TIFF.
fn sample_v2() -> BookMeta {
    let mut book = BookMeta::sample();
    book.format_version = 2;
    book.pages = vec![
        page(0, None),
        page(1, src_ref("scan-2", Some(0))),
        page(2, src_ref("scan-2", Some(1))),
        page(3, src_ref("scan-2", Some(2))),
    ];
    book.sources = Some(vec![SourceEntry {
        id: "scan-2".into(),
        file: "originals/2.tif".into(),
        format: SourceFormat::Tiff,
        frames: 3,
        byte_size: 9000,
        sha256: SHA.into(),
    }]);
    book
}

fn expect_source_err(book: &BookMeta, want: &str) {
    match book.validate() {
        Err(BookMetaError::InvalidSource { reason, .. })
        | Err(BookMetaError::InvalidPageSource { reason, .. })
            if reason.contains(want) => {}
        other => panic!("expected an error containing {want:?}, got {other:?}"),
    }
}

#[test]
fn v2_sample_validates_in_rust_and_schema() {
    let book = sample_v2();
    book.validate().expect("v2 sample must validate");
    let json = serde_json::to_value(&book).unwrap();
    assert_eq!(
        json["pages"][1]["source"]["ref"], "scan-2",
        "ref serializes as `ref`"
    );
    assert!(
        json["pages"][0].get("source").is_none(),
        "None source is omitted"
    );
    if let Err(e) = schema(2).validate(&json) {
        panic!("v2 sample failed the v2 schema: {e:?}");
    }
    assert!(
        !schema(1).is_valid(&json),
        "v1 schema must reject a v2 document"
    );
}

#[test]
fn v2_without_sources_is_valid_and_round_trips() {
    let mut book = BookMeta::sample();
    book.format_version = 2;
    book.validate().expect("v2 without originals is valid");
    let json = serde_json::to_value(&book).unwrap();
    assert!(json.get("sources").is_none(), "None sources is omitted");
    assert!(schema(2).is_valid(&json));
    let back: BookMeta = serde_json::from_value(json).unwrap();
    assert_eq!(back, book);
}

#[test]
fn v1_package_still_reads() {
    let book = BookMeta::sample();
    assert_eq!(book.format_version, 1);
    book.validate().expect("v1 stays readable");
    assert!(schema(1).is_valid(&serde_json::to_value(&book).unwrap()));
}

#[test]
fn v1_rejects_v2_fields() {
    let mut book = sample_v2();
    book.format_version = 1;
    assert_eq!(
        book.validate(),
        Err(BookMetaError::FieldRequiresVersion {
            field: "sources",
            min_version: 2
        })
    );
    let mut only_page_ref = BookMeta::sample();
    only_page_ref.pages[0].source = src_ref("x", None);
    assert_eq!(
        only_page_ref.validate(),
        Err(BookMetaError::FieldRequiresVersion {
            field: "pages[].source",
            min_version: 2
        })
    );
}

#[test]
fn page_refs_must_resolve_and_respect_frames() {
    let mut unknown = sample_v2();
    unknown.pages[1].source = src_ref("nope", Some(0));
    expect_source_err(&unknown, "unknown source");

    let mut missing_frame = sample_v2();
    missing_frame.pages[1].source = src_ref("scan-2", None);
    expect_source_err(&missing_frame, "frame required");

    let mut out_of_range = sample_v2();
    out_of_range.pages[3].source = src_ref("scan-2", Some(3));
    expect_source_err(&out_of_range, "out of range");

    let mut dup = sample_v2();
    dup.pages[2].source = src_ref("scan-2", Some(0));
    expect_source_err(&dup, "already uses");

    let mut single = sample_v2();
    if let Some(s) = single.sources.as_mut() {
        s[0].frames = 1;
    }
    single.pages.truncate(2);
    expect_source_err(&single, "single-frame");
}

#[test]
fn sources_entries_are_checked() {
    let mut unreferenced = sample_v2();
    for p in &mut unreferenced.pages {
        p.source = None;
    }
    expect_source_err(&unreferenced, "not referenced");

    let mut empty = sample_v2();
    empty.sources = Some(vec![]);
    for p in &mut empty.pages {
        p.source = None;
    }
    expect_source_err(&empty, "omitted rather than empty");

    for (file, want) in [
        ("pages/2.tif", "under originals/"),
        ("originals/../2.tif", ".."),
        ("pages/0000.JPG", "under originals/"),
    ] {
        let mut bad = sample_v2();
        bad.sources.as_mut().unwrap()[0].file = file.into();
        expect_source_err(&bad, want);
    }

    let mut dup_id = sample_v2();
    let mut second = dup_id.sources.as_ref().unwrap()[0].clone();
    second.file = "originals/other.tif".into();
    dup_id.sources.as_mut().unwrap().push(second);
    expect_source_err(&dup_id, "duplicate id");

    let mut bad_id = sample_v2();
    bad_id.sources.as_mut().unwrap()[0].id = "Scan 2".into();
    expect_source_err(&bad_id, "lowercase slug");

    let mut zero = sample_v2();
    zero.sources.as_mut().unwrap()[0].byte_size = 0;
    expect_source_err(&zero, "byteSize");
}

#[test]
fn source_files_collide_case_insensitively() {
    let mut book = sample_v2();
    let mut second = book.sources.as_ref().unwrap()[0].clone();
    second.id = "scan-3".into();
    second.file = "originals/2.TIF".into();
    second.frames = 1;
    book.sources.as_mut().unwrap().push(second);
    book.pages.push(page(4, src_ref("scan-3", None)));
    expect_source_err(&book, "collides");
}

#[test]
fn v2_schema_rejects_bad_sources() {
    let v2 = schema(2);
    let mut json = serde_json::to_value(sample_v2()).unwrap();
    json["sources"][0]["file"] = Value::from("pages/2.tif");
    assert!(!v2.is_valid(&json), "source file must be under originals/");

    let mut json = serde_json::to_value(sample_v2()).unwrap();
    json["sources"][0]["format"] = Value::from("heic");
    assert!(!v2.is_valid(&json), "unknown format rejected");

    let mut json = serde_json::to_value(sample_v2()).unwrap();
    json["pages"][1]["source"]["extra"] = Value::from(1);
    assert!(!v2.is_valid(&json), "unknown page source key rejected");

    let mut json = serde_json::to_value(sample_v2()).unwrap();
    json["sources"][0]["frames"] = Value::from(0);
    assert!(!v2.is_valid(&json), "frames 0 rejected");
}

#[test]
fn deserialize_rejects_unknown_source_keys() {
    let mut json = serde_json::to_value(sample_v2()).unwrap();
    json["sources"][0]["extra"] = Value::from(true);
    assert!(serde_json::from_value::<BookMeta>(json).is_err());
}

#[test]
fn source_file_collides_with_a_page_file() {
    let mut book = sample_v2();
    book.pages[0].file = "originals/2.TIF".into();
    expect_source_err(&book, "collides");
}

#[test]
fn single_frame_source_takes_one_page() {
    let mut book = sample_v2();
    book.sources.as_mut().unwrap()[0].frames = 1;
    book.pages[1].source = src_ref("scan-2", None);
    book.pages[2].source = src_ref("scan-2", None);
    book.pages.truncate(3);
    expect_source_err(&book, "already uses");
}

#[test]
fn v1_and_v2_schemas_share_id_and_file_patterns() {
    let read = |v: u32| -> Value {
        let path = format!(
            "{}/../docs/book-format.v{v}.schema.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let (v1, v2) = (read(1), read(2));
    let id = |s: &Value| s["properties"]["id"]["pattern"].clone();
    let file = |s: &Value| s["$defs"]["pageEntry"]["properties"]["file"]["pattern"].clone();
    assert_eq!(id(&v1), id(&v2), "id pattern drifted between v1 and v2");
    assert_eq!(
        id(&v2),
        Value::from(BOOK_ID_PATTERN),
        "Rust BOOK_ID_PATTERN must match v2"
    );
    assert_eq!(
        file(&v1),
        file(&v2),
        "page file pattern drifted between v1 and v2"
    );
}
