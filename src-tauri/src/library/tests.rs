//! Library tests: create, load, verify, progress, sweep, and every error variant (A1, A3,
//! A8, A10–A12). Fixtures are generated in temp directories at test time.

use super::create::{check_space, create_book_from_folder, CreateOptions};
use super::load::{load_book, parse_meta};
use super::progress::save_last_read_page;
use super::sweep::{sweep_orphans, ORPHAN_AGE, TMP_PREFIX};
use super::BookError;
use crate::book_format::{BookMeta, CURRENT_FORMAT_VERSION};
use image::codecs::jpeg::JpegEncoder;
use image::{ImageBuffer, ImageEncoder, Rgb, RgbImage};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "scriptorium-lib-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
    fn dir(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn picture(shade: u8) -> RgbImage {
    ImageBuffer::from_fn(30, 40, |x, y| Rgb([shade, (x * 6) as u8, (y * 5) as u8]))
}

fn jpeg(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(img)
        .unwrap();
    out
}

fn png(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    out
}

fn bmp(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    out
}

fn tiff(frames: &[RgbImage]) -> Vec<u8> {
    use tiff::encoder::{colortype, TiffEncoder};
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = TiffEncoder::new(&mut out).unwrap();
    for f in frames {
        enc.write_image::<colortype::RGB8>(f.width(), f.height(), f.as_raw())
            .unwrap();
    }
    out.into_inner()
}

fn opts(id: &str) -> CreateOptions {
    CreateOptions {
        id: id.into(),
        title: "Test Book".into(),
        rights: "Public domain".into(),
        attribution: "".into(),
    }
}

/// `1.jpg`, `2.TIF` (three frames), `3.png`, `cover (front).bmp`, plus a stray text file. The
/// uppercase extension and the spaced name must never reach the package.
fn mixed_folder(t: &Tmp) -> PathBuf {
    let src = t.dir("src");
    std::fs::write(src.join("3.png"), png(&picture(30))).unwrap();
    std::fs::write(src.join("cover (front).bmp"), bmp(&picture(40))).unwrap();
    std::fs::write(src.join("1.jpg"), jpeg(&picture(10))).unwrap();
    std::fs::write(
        src.join("2.TIF"),
        tiff(&[picture(21), picture(22), picture(23)]),
    )
    .unwrap();
    std::fs::write(src.join("notes.txt"), "not a page").unwrap();
    std::fs::write(src.join(".DS_Store"), "hidden").unwrap();
    src
}

fn read_meta(book: &Path) -> BookMeta {
    serde_json::from_str(&std::fs::read_to_string(book.join("meta.json")).unwrap()).unwrap()
}

fn write_meta(book: &Path, meta: &BookMeta) {
    std::fs::write(
        book.join("meta.json"),
        serde_json::to_string_pretty(meta).unwrap(),
    )
    .unwrap();
}

#[test]
fn creates_a_v2_package_in_natural_order_with_originals() {
    let t = Tmp::new("create");
    let lib = t.dir("library");
    let book = create_book_from_folder(&mixed_folder(&t), &lib, &opts("mixed-book")).unwrap();
    assert_eq!(book, lib.join("mixed-book"));
    let meta = read_meta(&book);
    assert_eq!(meta.format_version, CURRENT_FORMAT_VERSION);
    let files: Vec<&str> = meta.pages.iter().map(|p| p.file.as_str()).collect();
    assert_eq!(
        files,
        [
            "pages/0000.jpg",
            "pages/0001.jpg",
            "pages/0002.jpg",
            "pages/0003.jpg",
            "pages/0004.png",
            "pages/0005.png"
        ],
        "1.jpg, the three TIFF frames, 3.png, then cover.bmp"
    );
    // 1.jpg and 3.png are copied as-is; the TIFF and BMP keep originals.
    assert!(meta.pages[0].source.is_none() && meta.pages[4].source.is_none());
    let sources = meta.sources.as_ref().unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(
        (sources[0].file.as_str(), sources[0].frames),
        ("originals/0000.tif", 3)
    );
    assert_eq!(
        (sources[1].file.as_str(), sources[1].frames),
        ("originals/0001.bmp", 1)
    );
    let frames: Vec<Option<u32>> = meta.pages[1..4]
        .iter()
        .map(|p| p.source.as_ref().unwrap().frame)
        .collect();
    assert_eq!(frames, [Some(0), Some(1), Some(2)]);
    assert_eq!(
        meta.pages[5].source.as_ref().unwrap().frame,
        None,
        "single-frame source has no frame"
    );
    assert_eq!(
        std::fs::read(book.join("pages/0000.jpg")).unwrap(),
        jpeg(&picture(10)),
        "copied byte-for-byte"
    );
    assert_eq!(
        std::fs::read_to_string(book.join("annotations.json"))
            .unwrap()
            .trim(),
        "[]"
    );
    assert!(book.join("ocr").is_dir());
    // A1: the written meta.json passes the v2 schema and the loader verifies every file.
    parse_meta(&std::fs::read_to_string(book.join("meta.json")).unwrap()).unwrap();
    let loaded = load_book(&book).unwrap();
    assert_eq!(loaded.meta, meta);
    let page = loaded.page_path(5).unwrap();
    assert!(
        page.is_file() && page.starts_with(&loaded.root),
        "verified canonical path"
    );
    assert!(!lib.read_dir().unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(TMP_PREFIX)));
}

#[test]
fn folder_errors() {
    let t = Tmp::new("folders");
    let lib = t.dir("library");
    let empty = t.dir("empty");
    std::fs::write(empty.join(".hidden"), "x").unwrap();
    assert!(matches!(
        create_book_from_folder(&empty, &lib, &opts("a")),
        Err(BookError::EmptyFolder { .. })
    ));

    let docs = t.dir("docs");
    std::fs::write(docs.join("readme.txt"), "x").unwrap();
    assert!(matches!(
        create_book_from_folder(&docs, &lib, &opts("a")),
        Err(BookError::NoSupportedImages { .. })
    ));

    let dup = t.dir("dup");
    std::fs::write(dup.join("1.jpg"), jpeg(&picture(1))).unwrap();
    std::fs::write(dup.join("1.PNG"), png(&picture(2))).unwrap();
    assert!(matches!(
        create_book_from_folder(&dup, &lib, &opts("a")),
        Err(BookError::DuplicatePageStem { .. })
    ));

    let bad = t.dir("bad");
    std::fs::write(bad.join("1.jpg"), jpeg(&picture(1))).unwrap();
    std::fs::write(bad.join("2.jpg"), b"not really a jpeg").unwrap();
    match create_book_from_folder(&bad, &lib, &opts("a")) {
        Err(BookError::UnreadableFile { file, .. }) => assert!(file.ends_with("2.jpg"), "{file:?}"),
        other => panic!("expected UnreadableFile, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_dir(&lib).unwrap().count(),
        0,
        "nothing left behind after a failure"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_and_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let t = Tmp::new("perm");
    let lib = t.dir("library");
    let linked = t.dir("linked");
    std::fs::write(t.0.join("real.jpg"), jpeg(&picture(1))).unwrap();
    std::os::unix::fs::symlink(t.0.join("real.jpg"), linked.join("1.jpg")).unwrap();
    assert!(matches!(
        create_book_from_folder(&linked, &lib, &opts("a")),
        Err(BookError::UnreadableFile { .. })
    ));

    let locked = t.dir("locked");
    let page = locked.join("1.jpg");
    std::fs::write(&page, jpeg(&picture(1))).unwrap();
    std::fs::set_permissions(&page, std::fs::Permissions::from_mode(0o000)).unwrap();
    let res = create_book_from_folder(&locked, &lib, &opts("a"));
    std::fs::set_permissions(&page, std::fs::Permissions::from_mode(0o644)).unwrap();
    // Root can read a 0o000 file, so only assert when the OS actually refused.
    if res.is_err() {
        assert!(
            matches!(res, Err(BookError::PermissionDenied { .. })),
            "{res:?}"
        );
    }
}

#[test]
fn destination_rules() {
    let t = Tmp::new("dest");
    let lib = t.dir("library");
    let src = t.dir("src");
    std::fs::write(src.join("1.jpg"), jpeg(&picture(1))).unwrap();
    create_book_from_folder(&src, &lib, &opts("book")).unwrap();
    assert_eq!(
        create_book_from_folder(&src, &lib, &opts("book")),
        Err(BookError::DestinationExists { id: "book".into() })
    );
    // A11: an existing directory that differs only in case blocks the id.
    std::fs::create_dir(lib.join("Other-Book")).unwrap();
    assert!(matches!(
        create_book_from_folder(&src, &lib, &opts("other-book")),
        Err(BookError::DestinationExists { .. })
    ));
    assert!(matches!(
        create_book_from_folder(&src, &lib, &opts("Bad Id")),
        Err(BookError::InvalidPackage { .. })
    ));
}

#[test]
fn free_space_preflight() {
    let t = Tmp::new("space");
    assert!(check_space(&t.0, 1024).is_ok());
    match check_space(&t.0, u64::MAX / 2) {
        Err(BookError::InsufficientDiskSpace { needed, available }) => assert!(needed > available),
        other => panic!("expected InsufficientDiskSpace, got {other:?}"),
    }
}

#[test]
fn load_rejects_bad_packages() {
    let t = Tmp::new("load");
    let lib = t.dir("library");
    let book = create_book_from_folder(&mixed_folder(&t), &lib, &opts("bk")).unwrap();

    // A8: an unsupported version is named as such, before any schema check.
    let mut v = serde_json::to_value(read_meta(&book)).unwrap();
    v["formatVersion"] = 3.into();
    assert_eq!(
        parse_meta(&v.to_string()),
        Err(BookError::UnsupportedFormatVersion { found: 3 })
    );

    assert!(matches!(
        parse_meta("{not json"),
        Err(BookError::InvalidPackage { .. })
    ));
    let mut v = serde_json::to_value(read_meta(&book)).unwrap();
    v["surprise"] = true.into();
    assert!(
        matches!(
            parse_meta(&v.to_string()),
            Err(BookError::InvalidPackage { .. })
        ),
        "schema rejects unknown keys"
    );

    let empty = t.dir("not-a-book");
    assert!(matches!(
        load_book(&empty),
        Err(BookError::InvalidPackage { .. })
    ));
}

fn flip_last_byte(path: &Path) {
    let mut bytes = std::fs::read(path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn checksums_and_frames_are_verified() {
    let t = Tmp::new("verify");
    let lib = t.dir("library");
    let src = mixed_folder(&t);

    // A10: one changed byte in a page, then in an original.
    let book = create_book_from_folder(&src, &lib, &opts("page")).unwrap();
    flip_last_byte(&book.join("pages/0004.png"));
    assert_eq!(
        load_book(&book).err(),
        Some(BookError::ChecksumMismatch {
            file: "pages/0004.png".into()
        })
    );

    let book = create_book_from_folder(&src, &lib, &opts("orig")).unwrap();
    flip_last_byte(&book.join("originals/0001.bmp"));
    assert_eq!(
        load_book(&book).err(),
        Some(BookError::ChecksumMismatch {
            file: "originals/0001.bmp".into()
        })
    );

    // A missing page is an invalid package, not a crash.
    let book = create_book_from_folder(&src, &lib, &opts("gone")).unwrap();
    std::fs::remove_file(book.join("pages/0000.jpg")).unwrap();
    assert!(matches!(
        load_book(&book),
        Err(BookError::InvalidPackage { .. })
    ));

    // `frames` must match the original: swap in a one-frame TIFF with a matching checksum.
    let book = create_book_from_folder(&src, &lib, &opts("frames")).unwrap();
    let one = tiff(&[picture(9)]);
    std::fs::write(book.join("originals/0000.tif"), &one).unwrap();
    let mut meta = read_meta(&book);
    let s = &mut meta.sources.as_mut().unwrap()[0];
    s.sha256 = super::sha256_bytes(&one);
    s.byte_size = one.len() as u64;
    write_meta(&book, &meta);
    match load_book(&book) {
        Err(BookError::InvalidPackage { reason }) => assert!(reason.contains("frame"), "{reason}"),
        other => panic!("expected a frame-count error, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn page_paths_stay_inside_the_package() {
    // A12: a page that is a symlink to a file outside the book is refused.
    let t = Tmp::new("escape");
    let lib = t.dir("library");
    let book = create_book_from_folder(&mixed_folder(&t), &lib, &opts("bk")).unwrap();
    let page = book.join("pages/0000.jpg");
    let outside = t.0.join("outside.jpg");
    std::fs::copy(&page, &outside).unwrap();
    std::fs::remove_file(&page).unwrap();
    std::os::unix::fs::symlink(&outside, &page).unwrap();
    match load_book(&book) {
        Err(BookError::InvalidPackage { reason }) => {
            assert!(reason.contains("outside"), "{reason}")
        }
        other => panic!("expected an outside-path error, got {other:?}"),
    }
}

#[test]
fn progress_saves_atomically_and_upgrades_v1() {
    let t = Tmp::new("progress");
    let lib = t.dir("library");
    let book = create_book_from_folder(&mixed_folder(&t), &lib, &opts("bk")).unwrap();
    let saved = save_last_read_page(&book, 4).unwrap();
    assert_eq!(saved.last_read_page, 4);
    assert_eq!(
        load_book(&book).unwrap().meta.last_read_page,
        4,
        "persists across a reload"
    );
    assert!(matches!(
        save_last_read_page(&book, 99),
        Err(BookError::InvalidPackage { .. })
    ));
    assert_eq!(
        read_meta(&book).last_read_page,
        4,
        "a refused save leaves the file alone"
    );
    let leftovers = book
        .read_dir()
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".scriptorium-tmp")
        })
        .count();
    assert_eq!(leftovers, 0, "no temp file left behind");

    // A v1 package (no originals) is upgraded to v2 on its first save.
    let v1 = t.dir("v1book");
    std::fs::create_dir(v1.join("pages")).unwrap();
    let bytes = jpeg(&picture(5));
    std::fs::write(v1.join("pages/000.jpg"), &bytes).unwrap();
    let mut meta = BookMeta::sample_v1();
    meta.pages[0].sha256 = super::sha256_bytes(&bytes);
    meta.pages[0].byte_size = bytes.len() as u64;
    write_meta(&v1, &meta);
    assert_eq!(
        load_book(&v1).unwrap().meta.format_version,
        1,
        "v1 loads as-is"
    );
    assert_eq!(save_last_read_page(&v1, 0).unwrap().format_version, 2);
    assert_eq!(read_meta(&v1).format_version, 2);
}

#[test]
fn sweep_removes_only_old_temp_entries() {
    let t = Tmp::new("sweep");
    let lib = t.dir("library");
    std::fs::create_dir(lib.join(format!("{TMP_PREFIX}crashed"))).unwrap();
    // A crash after meta.json was written but before the rename still leaves litter.
    std::fs::create_dir(lib.join(format!("{TMP_PREFIX}late-crash"))).unwrap();
    std::fs::write(lib.join(format!("{TMP_PREFIX}late-crash/meta.json")), "{}").unwrap();
    std::fs::write(lib.join("meta.json.scriptorium-tmp"), "x").unwrap();
    std::fs::create_dir(lib.join("a-real-book")).unwrap();
    std::fs::write(lib.join("a-real-book/meta.json"), "{}").unwrap();
    std::fs::write(lib.join("a-real-book/meta.json.1-0.scriptorium-tmp"), "x").unwrap();
    // A valid book id may end in the temp suffix; a finished book is still never swept.
    std::fs::create_dir(lib.join("odd.scriptorium-tmp")).unwrap();
    std::fs::write(lib.join("odd.scriptorium-tmp/meta.json"), "{}").unwrap();
    std::fs::write(
        lib.join("odd.scriptorium-tmp/meta.json.1-1.scriptorium-tmp"),
        "x",
    )
    .unwrap();
    assert_eq!(
        sweep_orphans(&lib, ORPHAN_AGE),
        0,
        "fresh temp entries may belong to a live ingest"
    );
    assert_eq!(
        sweep_orphans(&lib, Duration::ZERO),
        5,
        "three at the top, one inside each book"
    );
    assert!(
        lib.join("a-real-book/meta.json").exists(),
        "the book itself is untouched"
    );
    let mut left: Vec<_> = lib
        .read_dir()
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    left.sort();
    assert_eq!(left, ["a-real-book", "odd.scriptorium-tmp"]);
}

#[test]
fn every_error_has_a_user_message() {
    // A3: each variant renders a message that names what went wrong.
    let p = PathBuf::from("/x/page.jpg");
    let cases = [
        (BookError::EmptyFolder { folder: p.clone() }, "no files"),
        (
            BookError::NoSupportedImages { folder: p.clone() },
            "No supported images",
        ),
        (
            BookError::UnreadableFile {
                file: p.clone(),
                reason: "bad".into(),
            },
            "page.jpg",
        ),
        (
            BookError::DestinationExists { id: "b".into() },
            "already exists",
        ),
        (
            BookError::DuplicatePageStem {
                first: p.clone(),
                second: p.clone(),
            },
            "Duplicate page names",
        ),
        (
            BookError::InsufficientDiskSpace {
                needed: 10 << 20,
                available: 1 << 20,
            },
            "Not enough free space",
        ),
        (
            BookError::UnsupportedFormatVersion { found: 9 },
            "format version 9",
        ),
        (
            BookError::InvalidPackage { reason: "r".into() },
            "not a valid Scriptorium book",
        ),
        (
            BookError::ChecksumMismatch {
                file: "pages/0001.jpg".into(),
            },
            "corrupted",
        ),
        (
            BookError::PermissionDenied { path: p.clone() },
            "Permission denied",
        ),
    ];
    for (err, want) in cases {
        let msg = err.to_string();
        assert!(msg.contains(want), "{err:?} -> {msg:?}");
    }
}
