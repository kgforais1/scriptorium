//! Tests that tie the pure pieces to a real book on disk. No Tauri runtime is needed.

use super::session::{canonical_key, Session, SAVE_INTERVAL};
use super::view::{book_view, BookView};
use crate::library::create::{create_book_from_folder, CreateOptions};
use crate::library::load::load_book;
use crate::library::progress::save_last_read_page;
use image::codecs::jpeg::JpegEncoder;
use image::{ImageBuffer, Rgb};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Fixture(PathBuf);

impl Fixture {
    /// A library holding a three-page book with id `bk`. Returns the fixture and the book root.
    fn new(tag: &str) -> (Self, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "scriptorium-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let scans = base.join("scans");
        std::fs::create_dir_all(&scans).unwrap();
        for (i, (w, h)) in [(30u32, 40u32), (31, 41), (32, 42)].into_iter().enumerate() {
            let img = ImageBuffer::from_fn(w, h, |x, y| Rgb([i as u8 * 60, x as u8, y as u8]));
            let mut out = Vec::new();
            JpegEncoder::new_with_quality(&mut out, 90)
                .encode_image(&img)
                .unwrap();
            std::fs::write(scans.join(format!("{}.jpg", i + 1)), out).unwrap();
        }
        let opts = CreateOptions {
            id: "bk".into(),
            title: "A Book".into(),
            rights: "public domain".into(),
            attribution: "test".into(),
        };
        let root = create_book_from_folder(&scans, &base.join("lib"), &opts).unwrap();
        (Self(base), root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn view_of(root: &Path) -> BookView {
    book_view(&load_book(root).unwrap()).unwrap()
}

#[test]
fn book_view_paths_are_absolute_canonical_and_under_pages() {
    let (_fx, root) = Fixture::new("view");
    let view = view_of(&root);
    let canonical_root = root.canonicalize().unwrap();
    assert_eq!(view.root, canonical_root.to_str().unwrap());
    assert_eq!((view.id.as_str(), view.title.as_str()), ("bk", "A Book"));
    assert_eq!(view.last_read_page, 0);
    assert_eq!(view.pages.len(), 3);
    let pages_dir = canonical_root.join("pages");
    for (page, (w, h)) in view.pages.iter().zip([(30, 40), (31, 41), (32, 42)]) {
        let path = Path::new(&page.path);
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.starts_with(&pages_dir), "{path:?}");
        assert_eq!(path.canonicalize().unwrap(), path);
        assert_eq!((page.width, page.height), (w, h));
    }
}

#[test]
fn book_view_serializes_camel_case() {
    let (_fx, root) = Fixture::new("json");
    let json = serde_json::to_value(view_of(&root)).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["id", "lastReadPage", "pages", "root", "title"]);
    let mut page_keys: Vec<&str> = json["pages"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    page_keys.sort_unstable();
    assert_eq!(page_keys, ["height", "path", "width"]);
}

#[test]
fn flushed_position_reaches_meta_json_and_the_next_open() {
    let (_fx, root) = Fixture::new("flush");
    let key = canonical_key(&root).unwrap();
    let t0 = Instant::now();
    let mut session = Session::new(SAVE_INTERVAL);
    session.register(key.clone(), 0, 3, t0);
    // A turn inside the debounce window is not due, but is written by the close flush.
    assert_eq!(session.record(&key, 2, t0), Ok(false));
    assert_eq!(view_of(&root).last_read_page, 0, "not written yet");
    let failures = session.flush(None, t0, |r, p| save_last_read_page(r, p).map(|_| ()));
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(view_of(&root).last_read_page, 2);
}

#[test]
fn an_out_of_range_page_is_refused_before_it_can_be_written() {
    let (_fx, root) = Fixture::new("range");
    let key = canonical_key(&root).unwrap();
    let t0 = Instant::now();
    let mut session = Session::new(SAVE_INTERVAL);
    session.register(key.clone(), 0, 3, t0);
    assert!(session.record(&key, 3, t0).is_err());
    assert!(session.flush(None, t0, |_, _| Err::<(), ()>(())).is_empty());
}

#[test]
fn save_outcome_matches_the_ts_type() {
    use super::SaveOutcome;
    // `SaveOutcome` in src/lib/bookTypes.ts is "saved" | "pending".
    assert_eq!(serde_json::to_value(SaveOutcome::Saved).unwrap(), "saved");
    assert_eq!(
        serde_json::to_value(SaveOutcome::Pending).unwrap(),
        "pending"
    );
}
