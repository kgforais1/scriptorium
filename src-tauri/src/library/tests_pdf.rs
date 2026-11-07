//! PDF ingest and load tests. They need the pdfium library from `scripts/fetch_pdfium.sh`.

use super::create::{create_book_from_folder, estimate_bytes, total_frames};
use super::inspect::{inspect_folder, FolderInfo};
use super::load::{check_pdf_original, load_book};
use super::tests::{opts, Tmp};
use super::BookError;
use crate::book_format::{BookMeta, SourceFormat};
use crate::ingest::convert::{InputKind, Inspection};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/pdf"
    ))
    .join(name)
}

fn jpeg(shade: u8) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(20, 30, image::Rgb([shade, 90, 40]));
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode_image(&img)
        .unwrap();
    out
}

/// `1.jpg`, `2.pdf` (three pages with text), and `3.jpg`.
fn pdf_folder(t: &Tmp, name: &str, pdf: &str) -> PathBuf {
    let src = t.dir(name);
    std::fs::write(src.join("1.jpg"), jpeg(10)).unwrap();
    std::fs::copy(fixture(pdf), src.join("2.pdf")).unwrap();
    std::fs::write(src.join("3.jpg"), jpeg(30)).unwrap();
    src
}

fn read_meta(book: &Path) -> BookMeta {
    serde_json::from_str(&std::fs::read_to_string(book.join("meta.json")).unwrap()).unwrap()
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn a_pdf_between_two_images_gives_ordered_pages_sources_and_text() {
    let t = Tmp::new("pdf-book");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();
    let meta = read_meta(&book);

    assert_eq!(meta.pages.len(), 5);
    let sources = meta.sources.as_ref().unwrap();
    assert_eq!(sources.len(), 1, "only the PDF is kept as an original");
    assert_eq!(sources[0].format, SourceFormat::Pdf);
    assert_eq!(sources[0].frames, 3);
    assert_eq!(sources[0].file, "originals/0000.pdf");
    let original = std::fs::read(book.join("originals/0000.pdf")).unwrap();
    assert_eq!(original, std::fs::read(fixture("text3.pdf")).unwrap());

    let refs: Vec<_> = meta
        .pages
        .iter()
        .map(|p| p.source.as_ref().map(|s| (s.source_ref.as_str(), s.frame)))
        .collect();
    assert_eq!(
        refs,
        [
            None,
            Some(("src-0000", Some(0))),
            Some(("src-0000", Some(1))),
            Some(("src-0000", Some(2))),
            None,
        ]
    );
    assert!(meta.pages.iter().all(|p| p.file.ends_with(".jpg")));
    load_book(&book).expect("checksums and the PDF page count verify");
}

#[test]
fn pdf_text_sits_at_the_book_position_of_its_page() {
    let t = Tmp::new("pdf-text");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();

    // Not the PDF's own page number. at the book position of its page, not the PDF's page number.
    assert_eq!(
        names(&book.join("ocr")),
        ["0001.json", "0002.json", "0003.json", "index.json"]
    );
    let page = read_json(&book.join("ocr/0002.json"));
    assert_eq!(page["source"], "pdf");
    assert!(page["text"].as_str().unwrap().contains("Page two"));
    let index = read_json(&book.join("ocr/index.json"));
    assert_eq!(index["engine"], "pdfium");
    assert_eq!(
        index["summary"],
        serde_json::json!({ "pdfPages": 3, "withText": 3 })
    );
    assert_eq!(index["pages"]["1"]["status"], "text");
    assert!(
        index["pages"].get("0").is_none(),
        "image pages are not listed"
    );
}

#[test]
fn a_pdf_page_without_text_is_marked_none() {
    let t = Tmp::new("pdf-notext");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "transparent.pdf");
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();
    assert_eq!(names(&book.join("ocr")), ["index.json"]);
    let index = read_json(&book.join("ocr/index.json"));
    assert_eq!(index["pages"]["1"]["status"], "none");
    assert_eq!(
        index["summary"],
        serde_json::json!({ "pdfPages": 1, "withText": 0 })
    );
}

#[test]
fn a_book_without_pdfs_has_an_empty_ocr_directory() {
    let t = Tmp::new("pdf-none");
    let lib = t.dir("lib");
    let src = t.dir("src");
    std::fs::write(src.join("1.jpg"), jpeg(10)).unwrap();
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();
    assert!(names(&book.join("ocr")).is_empty());
}

#[test]
fn two_ingests_of_one_folder_give_identical_pages() {
    let t = Tmp::new("pdf-determinism");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let a = create_book_from_folder(&src, &lib, &opts("a")).unwrap();
    let b = create_book_from_folder(&src, &lib, &opts("b")).unwrap();
    let files = names(&a.join("pages"));
    assert_eq!(files.len(), 5);
    assert_eq!(files, names(&b.join("pages")));
    for f in files {
        let (x, y) = (a.join("pages").join(&f), b.join("pages").join(&f));
        assert_eq!(std::fs::read(x).unwrap(), std::fs::read(y).unwrap(), "{f}");
    }
}

#[test]
fn a_pdf_with_junk_before_the_header_imports() {
    let t = Tmp::new("pdf-junk");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "junk_prefix.pdf");
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();
    assert_eq!(read_meta(&book).pages.len(), 5);
}

#[test]
fn refused_pdfs_name_the_reason_and_leave_no_package() {
    let t = Tmp::new("pdf-refused");
    let lib = t.dir("lib");
    let cases = [
        ("encrypted.pdf", "enc"),
        ("zero_pages.pdf", "zero"),
        ("truncated.pdf", "cut"),
    ];
    for (name, id) in cases {
        let src = pdf_folder(&t, id, name);
        match create_book_from_folder(&src, &lib, &opts(id)) {
            Err(BookError::EncryptedPdf { file }) => {
                assert_eq!(name, "encrypted.pdf");
                assert!(file.ends_with("2.pdf"));
            }
            Err(BookError::UnreadableFile { file, .. }) => {
                assert_ne!(name, "encrypted.pdf");
                assert!(file.ends_with("2.pdf"));
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    assert!(
        names(&lib).is_empty(),
        "no package or temp directory is left"
    );
}

#[test]
fn a_pdf_renamed_to_png_is_refused() {
    let t = Tmp::new("pdf-renamed");
    let lib = t.dir("lib");
    let src = t.dir("src");
    std::fs::copy(fixture("text3.pdf"), src.join("1.png")).unwrap();
    match create_book_from_folder(&src, &lib, &opts("bk")) {
        Err(BookError::UnreadableFile { reason, .. }) => {
            assert!(reason.contains("PDF"), "{reason}")
        }
        other => panic!("expected a mismatch error, got {other:?}"),
    }
}

#[test]
fn inspecting_a_pdf_folder_counts_its_pages() {
    let t = Tmp::new("pdf-inspect");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let FolderInfo::Scans {
        files,
        frames,
        estimated_bytes,
        ..
    } = inspect_folder(&src, &lib).unwrap()
    else {
        panic!("expected scans");
    };
    assert_eq!((files, frames), (3, 5));
    assert!(estimated_bytes > 3 * 12 * 1024 * 1024);
}

fn inspection(kind: InputKind, frames: u32) -> Inspection {
    Inspection {
        kind,
        frames,
        needs_conversion: true,
        display_ext: "jpg",
    }
}

#[test]
fn pdf_pages_are_budgeted_at_12_mb_plus_text() {
    let size = 2 * 1024 * 1024;
    let want = size + 10 * (12 * 1024 * 1024 + 64 * 1024);
    assert_eq!(estimate_bytes(size, &inspection(InputKind::Pdf, 10)), want);
    // A huge PDF is still budgeted per page, plus the original.
    let big = 900 * 1024 * 1024;
    assert!(estimate_bytes(big, &inspection(InputKind::Pdf, 3)) > big + 3 * 12 * 1024 * 1024);
}

#[test]
fn page_limit_is_checked_without_overflow() {
    let p = Path::new("x.pdf");
    let at = |frames: &[u32]| {
        let opened: Vec<_> = frames
            .iter()
            .map(|&n| (p, inspection(InputKind::Pdf, n)))
            .collect();
        total_frames(&opened)
    };
    assert_eq!(at(&[99_999, 1]).unwrap(), 100_000);
    for frames in [&[100_000, 1][..], &[u32::MAX, u32::MAX]] {
        match at(frames) {
            Err(BookError::TooManyPages { pages, max }) => {
                assert!(pages > 100_000);
                assert_eq!(max, 100_000);
            }
            other => panic!("expected TooManyPages, got {other:?}"),
        }
    }
}

#[test]
fn load_checks_the_pdf_page_count_when_pdfium_is_loaded() {
    let t = Tmp::new("pdf-load");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();
    let mut meta = read_meta(&book);
    meta.sources.as_mut().unwrap()[0].frames = 4;
    std::fs::write(
        book.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();
    match load_book(&book) {
        Err(BookError::InvalidPackage { reason }) => {
            assert!(reason.contains("records 4 frame(s) but has 3"), "{reason}")
        }
        other => panic!("expected a frame count error, got {other:?}"),
    }
}

#[test]
fn load_falls_back_to_the_checksum_when_pdfium_is_unavailable() {
    let t = Tmp::new("pdf-fallback");
    let lib = t.dir("lib");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let book = create_book_from_folder(&src, &lib, &opts("bk")).unwrap();
    let mut meta = read_meta(&book);
    meta.sources.as_mut().unwrap()[0].frames = 99;
    let entry = &meta.sources.as_ref().unwrap()[0];
    let original = book.join(&entry.file);
    // With pdfium ready the wrong count is caught. Without it, it is not looked at.
    assert!(check_pdf_original(&original, entry, true).is_err());
    assert!(check_pdf_original(&original, entry, false).is_ok());
}

#[test]
fn only_pdf_pages_have_text() {
    let t = Tmp::new("pdf-source-text");
    let src = pdf_folder(&t, "src", "text3.pdf");
    let image = crate::ingest::convert::open(&src.join("1.jpg")).unwrap();
    assert_eq!(image.page_text(0), None);
    let doc = crate::ingest::convert::open(&src.join("2.pdf")).unwrap();
    assert!(doc.page_text(0).unwrap().contains("Page one"));
    assert_eq!(doc.info.frames, 3);
}
