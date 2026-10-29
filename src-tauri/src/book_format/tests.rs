//! Schema-bind and validation tests for formatVersion 1 packages.

use super::*;
use serde_json::Value;

fn load_schema() -> Value {
    let schema_path = format!(
        "{}/../docs/book-format.v1.schema.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let schema_str = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("failed to read schema at {schema_path}: {e}"));
    serde_json::from_str(&schema_str).expect("schema is not valid JSON")
}

fn compile_schema(schema: &Value) -> jsonschema::Validator {
    jsonschema::validator_for(schema).expect("schema is not valid")
}

#[test]
fn sample_serialization_matches_schema() {
    let book = BookMeta::sample();
    book.validate().expect("sample must validate");
    let json = serde_json::to_value(&book).unwrap();
    let schema = load_schema();
    let validator = compile_schema(&schema);
    if let Err(error) = validator.validate(&json) {
        panic!("sample did not validate against schema: {error:?}");
    }
}

#[test]
fn page_label_variants_validate() {
    let mut with_label = BookMeta::sample();
    with_label.pages[0].page_label = Some("Cover".into());

    let schema = load_schema();
    let validator = compile_schema(&schema);

    let labeled = serde_json::to_value(&with_label).unwrap();
    assert_eq!(
        labeled["pages"][0]["pageLabel"], "Cover",
        "key must serialize as pageLabel"
    );
    assert!(
        validator.is_valid(&labeled),
        "Some(page_label) variant must validate"
    );

    let unlabeled = serde_json::to_value(BookMeta::sample()).unwrap();
    assert!(
        unlabeled["pages"][0].get("pageLabel").is_none(),
        "None must be omitted"
    );
    assert!(validator.is_valid(&unlabeled), "None variant must validate");
}

#[test]
fn schema_rejects_missing_required_field() {
    let book = BookMeta::sample();
    let mut json = serde_json::to_value(&book).unwrap();
    json.as_object_mut().unwrap().remove("title");
    let schema = load_schema();
    let validator = compile_schema(&schema);
    assert!(
        !validator.is_valid(&json),
        "schema should reject a missing required field"
    );
}

#[test]
fn schema_rejects_zero_dimensions() {
    let mut book = BookMeta::sample();
    book.pages[0].width = 0;
    let json = serde_json::to_value(&book).unwrap();
    let validator = compile_schema(&load_schema());
    assert!(!validator.is_valid(&json), "width 0 must fail schema");
}

#[test]
fn schema_rejects_path_traversal_file() {
    let mut book = BookMeta::sample();
    book.pages[0].file = "../etc/passwd".into();
    let json = serde_json::to_value(&book).unwrap();
    let validator = compile_schema(&load_schema());
    assert!(!validator.is_valid(&json), "../ path must fail schema");
    assert!(book.pages[0].validate().is_err());
}

#[test]
fn schema_rejects_terminal_parent_segment() {
    let validator = compile_schema(&load_schema());
    for bad in ["..", "pages/.."] {
        let mut book = BookMeta::sample();
        book.pages[0].file = bad.into();
        let json = serde_json::to_value(&book).unwrap();
        assert!(
            !validator.is_valid(&json),
            "schema must reject terminal .. path {bad:?}"
        );
        assert!(
            book.pages[0].validate().is_err(),
            "Rust must reject terminal .. path {bad:?}"
        );
    }
}

#[test]
fn schema_rejects_integer_above_u32() {
    let book = BookMeta::sample();
    let mut json = serde_json::to_value(&book).unwrap();
    json["pages"][0]["width"] = serde_json::json!(4_294_967_296u64);
    let validator = compile_schema(&load_schema());
    assert!(
        !validator.is_valid(&json),
        "width above u32::MAX must fail schema"
    );
}

#[test]
fn validate_rejects_last_read_page_out_of_range() {
    let mut book = BookMeta::sample();
    book.last_read_page = 999;
    match book.validate() {
        Err(BookMetaError::LastReadPageOutOfRange { .. }) => {}
        other => panic!("expected LastReadPageOutOfRange, got {other:?}"),
    }
}

#[test]
fn validate_rejects_invalid_book_id() {
    let validator = compile_schema(&load_schema());
    for bad in [
        "", "../etc", "Has Caps", ".", "bad/id", "con", "aux", "nul", "com1", "lpt9", "con.txt",
        "prn.png", "book.", "ok\n",
    ] {
        let mut book = BookMeta::sample();
        book.id = bad.into();
        assert!(
            matches!(book.validate(), Err(BookMetaError::InvalidBookId { .. })),
            "Rust must reject id {bad:?}"
        );
        let json = serde_json::to_value(&book).unwrap();
        assert!(!validator.is_valid(&json), "schema must reject id {bad:?}");
    }
}

#[test]
fn validate_rejects_windows_reserved_page_paths() {
    let cases = [
        "con",
        "AUX",
        "pages/nul.png",
        "pages/COM1.jpg",
        "pages/lpt9",
        "pages/page.",
        "pages/foo./bar.png",
    ];
    for file in cases {
        match validate_page_file(0, file) {
            Err(BookMetaError::InvalidPageFile { .. }) => {}
            other => panic!("expected InvalidPageFile for {file:?}, got {other:?}"),
        }
    }
}

#[test]
fn validate_rejects_case_insensitive_path_collision() {
    let mut book = BookMeta::sample();
    book.pages[0].file = "pages/Cover.png".into();
    book.pages.push(PageEntry {
        index: 1,
        file: "pages/cover.png".into(),
        width: 100,
        height: 100,
        byte_size: 10,
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        page_label: None,
        storage: StorageMode::Copied,
        source: None,
    });
    match book.validate() {
        Err(BookMetaError::CaseInsensitivePathCollision {
            index: 1,
            file,
            earlier_file,
        }) => {
            assert_eq!(file, "pages/cover.png");
            assert_eq!(earlier_file, "pages/Cover.png");
        }
        other => panic!("expected CaseInsensitivePathCollision, got {other:?}"),
    }
}

#[test]
fn validate_rejects_title_too_long() {
    let mut book = BookMeta::sample();
    book.title = "x".repeat(TITLE_MAX_LEN + 1);
    match book.validate() {
        Err(BookMetaError::TitleTooLong { len, max }) => {
            assert_eq!(len, TITLE_MAX_LEN + 1);
            assert_eq!(max, TITLE_MAX_LEN);
        }
        other => panic!("expected TitleTooLong, got {other:?}"),
    }
    let json = serde_json::to_value(&book).unwrap();
    let validator = compile_schema(&load_schema());
    assert!(
        !validator.is_valid(&json),
        "schema must reject overlong title"
    );
}

#[test]
fn validate_rejects_too_many_pages_bound_documented() {
    // Building 100_001 real pages is expensive; assert the constant matches schema
    // and that a synthetic oversize list is rejected by validate().
    let schema = load_schema();
    let max_items = schema["properties"]["pages"]["maxItems"]
        .as_u64()
        .expect("pages.maxItems") as usize;
    assert_eq!(max_items, PAGES_MAX_ITEMS);
    let mut book = BookMeta::sample();
    book.pages = (0..=PAGES_MAX_ITEMS)
        .map(|i| PageEntry {
            index: i as u32,
            file: format!("pages/{i:05}.png"),
            width: 1,
            height: 1,
            byte_size: 1,
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            page_label: None,
            storage: StorageMode::Copied,
            source: None,
        })
        .collect();
    // Cap last_read so we hit TooManyPages before LastReadPageOutOfRange.
    book.last_read_page = 0;
    match book.validate() {
        Err(BookMetaError::TooManyPages { count, max }) => {
            assert_eq!(count, PAGES_MAX_ITEMS + 1);
            assert_eq!(max, PAGES_MAX_ITEMS);
        }
        other => panic!("expected TooManyPages, got {other:?}"),
    }
}

#[test]
fn validate_rejects_zero_byte_size() {
    let mut book = BookMeta::sample();
    book.pages[0].byte_size = 0;
    match book.validate() {
        Err(BookMetaError::ZeroByteSize { .. }) => {}
        other => panic!("expected ZeroByteSize, got {other:?}"),
    }
    let json = serde_json::to_value(&book).unwrap();
    let validator = compile_schema(&load_schema());
    assert!(!validator.is_valid(&json), "schema must reject byteSize 0");
}

#[test]
fn book_id_pattern_matches_schema() {
    let schema = load_schema();
    let pattern = schema["properties"]["id"]["pattern"]
        .as_str()
        .expect("schema id.pattern must be a string");
    assert_eq!(
        pattern, BOOK_ID_PATTERN,
        "Rust BOOK_ID_PATTERN must match docs/book-format.schema.json"
    );
}

#[test]
fn deserialize_rejects_unknown_top_level_field() {
    let mut json = serde_json::to_value(BookMeta::sample()).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("unknownField".into(), serde_json::json!(1));
    assert!(
        serde_json::from_value::<BookMeta>(json).is_err(),
        "unknown top-level keys must fail deserialize"
    );
}

#[test]
fn deserialize_rejects_unknown_page_field() {
    let mut json = serde_json::to_value(BookMeta::sample()).unwrap();
    json["pages"][0]["extra"] = serde_json::json!("nope");
    assert!(
        serde_json::from_value::<BookMeta>(json).is_err(),
        "unknown page keys must fail deserialize"
    );
}

#[test]
fn validate_rejects_unsupported_format_version() {
    let mut book = BookMeta::sample();
    book.format_version = 3;
    match book.validate() {
        Err(BookMetaError::UnsupportedFormatVersion(3)) => {}
        other => panic!("expected UnsupportedFormatVersion(3), got {other:?}"),
    }
}

#[test]
fn validate_rejects_empty_pages() {
    let mut book = BookMeta::sample();
    book.pages.clear();
    match book.validate() {
        Err(BookMetaError::EmptyPages) => {}
        other => panic!("expected EmptyPages, got {other:?}"),
    }
}

#[test]
fn validate_page_file_rejects_unsafe_paths() {
    let cases = [
        ("", "empty path"),
        ("/etc/passwd", "absolute path"),
        (r"pages\000.jpg", "backslash"),
        ("a//b", "empty segment"),
        (".", "dot segment"),
        ("pages/.", "dot segment under pages"),
        ("pages/..", "parent segment"),
        ("pages/foo bar.jpg", "space"),
        ("pages/foo:bar.jpg", "colon"),
        ("pages/über.jpg", "non-ascii"),
    ];
    let validator = compile_schema(&load_schema());
    for (file, label) in cases {
        match validate_page_file(0, file) {
            Err(BookMetaError::InvalidPageFile { .. }) => {}
            other => panic!("{label} path {file:?} should fail, got {other:?}"),
        }
        if file.is_empty() || file.contains('\0') {
            continue;
        }
        let mut book = BookMeta::sample();
        book.pages[0].file = file.into();
        let json = serde_json::to_value(&book).unwrap();
        assert!(
            !validator.is_valid(&json),
            "schema must also reject {label} path {file:?}"
        );
    }
}

#[test]
fn validate_page_file_rejects_null_byte() {
    let file = format!("pages/{}\0.jpg", 'a');
    match validate_page_file(0, &file) {
        Err(BookMetaError::InvalidPageFile { reason, .. }) => {
            assert!(reason.contains("null byte"), "reason: {reason}");
        }
        other => panic!("expected InvalidPageFile for null byte, got {other:?}"),
    }
}

#[test]
fn validate_accepts_wrong_sha256_shape_only() {
    let mut book = BookMeta::sample();
    book.pages[0].sha256 =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    book.pages[0]
        .validate()
        .expect("PageEntry only checks sha256 shape, not file bytes");
    book.validate()
        .expect("BookMeta::validate does not verify sha256 against files (M1 load_book must)");
}

#[test]
fn validate_rejects_page_index_mismatch_cases() {
    let good_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let mut ok = BookMeta::sample();
    ok.pages.push(PageEntry {
        index: 1,
        file: "pages/001.jpg".into(),
        width: 100,
        height: 100,
        byte_size: 10,
        sha256: good_sha.into(),
        page_label: None,
        storage: StorageMode::Copied,
        source: None,
    });
    ok.validate().expect("contiguous indexes 0,1 must pass");

    let mut dupes = BookMeta::sample();
    dupes.pages.push(PageEntry {
        index: 0,
        file: "pages/001.jpg".into(),
        width: 100,
        height: 100,
        byte_size: 10,
        sha256: good_sha.into(),
        page_label: None,
        storage: StorageMode::Copied,
        source: None,
    });
    match dupes.validate() {
        Err(BookMetaError::PageIndexMismatch {
            position: 1,
            index: 0,
        }) => {}
        other => panic!("duplicate indexes should fail at position 1, got {other:?}"),
    }

    let mut gap = BookMeta::sample();
    gap.pages.push(PageEntry {
        index: 2,
        file: "pages/002.jpg".into(),
        width: 100,
        height: 100,
        byte_size: 10,
        sha256: good_sha.into(),
        page_label: None,
        storage: StorageMode::Copied,
        source: None,
    });
    match gap.validate() {
        Err(BookMetaError::PageIndexMismatch {
            position: 1,
            index: 2,
        }) => {}
        other => panic!("gap at index 2 should fail at position 1, got {other:?}"),
    }

    let mut huge = BookMeta::sample();
    huge.pages.push(PageEntry {
        index: u32::MAX,
        file: "pages/huge.jpg".into(),
        width: 100,
        height: 100,
        byte_size: 10,
        sha256: good_sha.into(),
        page_label: None,
        storage: StorageMode::Copied,
        source: None,
    });
    match huge.validate() {
        Err(BookMetaError::PageIndexMismatch { position: 1, index }) if index == u32::MAX => {}
        other => panic!("huge index at position 1 should fail, got {other:?}"),
    }
}

#[test]
fn storage_referenced_deserializes() {
    let json = serde_json::json!({
        "formatVersion": 1,
        "id": "ref-book",
        "title": "Referenced",
        "createdAt": "2026-08-24T00:00:00Z",
        "updatedAt": "2026-08-24T00:00:00Z",
        "renderMode": "scan",
        "lastReadPage": 0,
        "rights": "",
        "attribution": "",
        "pages": [{
            "index": 0,
            "file": "pages/000.png",
            "width": 100,
            "height": 100,
            "byteSize": 12,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "storage": "referenced"
        }]
    });
    let book: BookMeta = serde_json::from_value(json).expect("deserialize");
    assert_eq!(book.pages[0].storage, StorageMode::Referenced);
    book.validate().expect("referenced storage is schema-valid");
    let validator = compile_schema(&load_schema());
    assert!(validator.is_valid(&serde_json::to_value(&book).unwrap()));
}

#[test]
fn validate_rejects_invalid_timestamps() {
    for (field, bad) in [
        ("createdAt", "not-a-date"),
        ("createdAt", "2026-08-24"),
        ("updatedAt", "2026/08/24 00:00:00"),
    ] {
        let mut book = BookMeta::sample();
        if field == "createdAt" {
            book.created_at = bad.into();
        } else {
            book.updated_at = bad.into();
        }
        match book.validate() {
            Err(BookMetaError::InvalidTimestamp { field: f, .. }) if f == field => {}
            other => panic!("expected InvalidTimestamp for {field}={bad:?}, got {other:?}"),
        }
    }
    let mut ok = BookMeta::sample();
    ok.created_at = "2026-08-24T12:34:56+00:00".into();
    ok.updated_at = "2026-08-24T12:34:56.123Z".into();
    ok.validate()
        .expect("RFC 3339 variants with offset/fraction must pass");
}

#[test]
fn fixture_generator_meta_round_trips_through_rust_types() {
    // Integration-style: needs a Python interpreter on PATH (`python3` or `python`).
    // Soft-skips when none is available so plain `cargo test` stays usable.
    let python = ["python3", "python"].into_iter().find(|cmd| {
        std::process::Command::new(cmd)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    });
    let Some(python) = python else {
        eprintln!("skipping fixture round-trip: no python3/python on PATH");
        return;
    };

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let out = std::env::temp_dir().join(format!(
        "scriptorium-rust-roundtrip-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("create out dir");

    let status = std::process::Command::new(python)
        .arg(root.join("scripts/generate-fixture-book.py"))
        .arg("--pages")
        .arg("2")
        .arg("--seed")
        .arg("7")
        .arg("--out")
        .arg(&out)
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("spawn {python}: {e}"));
    assert!(status.success(), "fixture generator failed: {status}");

    let meta_path = out.join("meta.json");
    let meta_str = std::fs::read_to_string(&meta_path).expect("read meta.json");
    let book: BookMeta = serde_json::from_str(&meta_str).expect("deserialize fixture meta");
    book.validate()
        .expect("fixture must pass BookMeta::validate");

    let re_serialized = serde_json::to_value(&book).expect("re-serialize");
    let original: Value = serde_json::from_str(&meta_str).expect("parse original JSON");
    assert_eq!(
        re_serialized, original,
        "Rust round-trip must preserve fixture meta.json"
    );

    // The generator writes the current version, so check against that version's schema.
    assert_eq!(book.format_version, CURRENT_FORMAT_VERSION);
    let schema_path = format!(
        "{}/../docs/book-format.v{}.schema.json",
        env!("CARGO_MANIFEST_DIR"),
        book.format_version
    );
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(&schema_path).expect("read schema"))
            .expect("schema is JSON");
    assert!(
        compile_schema(&schema).is_valid(&re_serialized),
        "round-tripped meta must match the schema for its formatVersion"
    );

    let _ = std::fs::remove_dir_all(&out);
}
