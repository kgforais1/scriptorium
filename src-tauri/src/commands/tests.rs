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
    // The view drops Windows' verbatim prefix; elsewhere `simplified` changes nothing.
    let canonical_root = dunce::simplified(&root.canonicalize().unwrap()).to_path_buf();
    assert_eq!(view.root, canonical_root.to_str().unwrap());
    assert_eq!((view.id.as_str(), view.title.as_str()), ("bk", "A Book"));
    assert_eq!(view.last_read_page, 0);
    assert_eq!(view.pages.len(), 3);
    let pages_dir = canonical_root.join("pages");
    for (page, (w, h)) in view.pages.iter().zip([(30, 40), (31, 41), (32, 42)]) {
        let path = Path::new(&page.path);
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.starts_with(&pages_dir), "{path:?}");
        assert_eq!(dunce::simplified(&path.canonicalize().unwrap()), path);
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
    assert_eq!(
        keys,
        ["id", "lastReadPage", "pages", "root", "saveSeq", "title"]
    );
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

#[test]
fn save_progress_path_records_writes_and_drops_stale_pages() {
    use super::{save_in, SaveOutcome};
    use std::sync::Mutex;
    let (_fx, root) = Fixture::new("save-in");
    let t0 = Instant::now();
    let session = Mutex::new(Session::new(SAVE_INTERVAL));
    session
        .lock()
        .unwrap()
        .register(canonical_key(&root).unwrap(), 0, 3, t0);
    let write = |r: &Path, p: u32| save_last_read_page(r, p).map(|_| ());
    // Inside the debounce window the page is held; after it, the newest page is written.
    assert_eq!(
        save_in(&session, &root, 1, 1, t0, write),
        Ok(SaveOutcome::Pending)
    );
    let later = t0 + SAVE_INTERVAL;
    assert_eq!(
        save_in(&session, &root, 2, 3, later, write),
        Ok(SaveOutcome::Saved)
    );
    assert_eq!(view_of(&root).last_read_page, 2);
    // A save numbered before the one just written arrives late and is ignored.
    let late = later + SAVE_INTERVAL;
    assert_eq!(
        save_in(&session, &root, 0, 2, late, write),
        Ok(SaveOutcome::Pending)
    );
    assert_eq!(view_of(&root).last_read_page, 2);
    // A page past the end is refused.
    assert!(save_in(&session, &root, 3, 4, late, write).is_err());
}

#[test]
fn a_held_page_after_a_failed_write_is_reported() {
    use super::{save_in, SaveOutcome};
    use std::sync::Mutex;
    let (_fx, root) = Fixture::new("save-held");
    let t0 = Instant::now();
    let session = Mutex::new(Session::new(SAVE_INTERVAL));
    session
        .lock()
        .unwrap()
        .register(canonical_key(&root).unwrap(), 0, 3, t0);
    let fail = |_: &Path, _: u32| {
        Err(crate::library::BookError::PermissionDenied {
            path: PathBuf::from("/ro"),
        })
    };
    let later = t0 + SAVE_INTERVAL;
    assert!(
        save_in(&session, &root, 1, 1, later, fail).is_err(),
        "the write fails"
    );
    // The next turn is held by the backoff, and still says the place is not saved.
    let held = save_in(&session, &root, 2, 2, later, fail).unwrap_err();
    assert_eq!(held.kind, "SaveHeld");
    let ok = |r: &Path, p: u32| save_last_read_page(r, p).map(|_| ());
    let retry = later + SAVE_INTERVAL * super::session::RETRY_FACTOR;
    assert_eq!(
        save_in(&session, &root, 2, 3, retry, ok),
        Ok(SaveOutcome::Saved)
    );
}

#[test]
fn a_reopened_view_carries_the_save_seq_and_the_unsaved_page() {
    use super::register_in_session;
    let (_fx, root) = Fixture::new("reopen");
    let key = canonical_key(&root).unwrap();
    let t0 = Instant::now();
    let mut session = Session::new(SAVE_INTERVAL);
    let open = |session: &mut Session| {
        let book = load_book(&key).unwrap();
        let mut view = book_view(&book).unwrap();
        register_in_session(session, &book, &mut view, t0);
        view
    };
    let first = open(&mut session);
    assert_eq!((first.save_seq, first.last_read_page), (0, 0));
    session.record_seq(&key, 2, 7, t0).unwrap();
    // The page was never written, so the reopened view shows it, numbered above save 7.
    let second = open(&mut session);
    assert_eq!((second.save_seq, second.last_read_page), (7, 2));
    // After the page is written the sequence still carries over, and the disk page wins.
    let failures = session.flush(None, t0, |r, p| save_last_read_page(r, p).map(|_| ()));
    assert!(failures.is_empty(), "{failures:?}");
    let third = open(&mut session);
    assert_eq!((third.save_seq, third.last_read_page), (7, 2));
}

#[test]
fn a_held_or_failed_save_claims_exactly_one_worker() {
    use super::{claim_after_save, save_in, SaveOutcome};
    use std::sync::Mutex;
    let (_fx, root) = Fixture::new("claim");
    let key = canonical_key(&root).unwrap();
    let t0 = Instant::now();
    let session = Mutex::new(Session::new(SAVE_INTERVAL));
    session.lock().unwrap().register(key.clone(), 0, 3, t0);
    let ok = |r: &Path, p: u32| save_last_read_page(r, p).map(|_| ());
    // A page held inside the debounce window needs a worker, once.
    let held = save_in(&session, &root, 1, 1, t0, ok);
    assert_eq!(held, Ok(SaveOutcome::Pending));
    let claim = claim_after_save(&session, &root, &held);
    let (claimed_key, token) = claim.expect("a held page starts a worker");
    assert_eq!(claimed_key, key);
    assert_eq!(claim_after_save(&session, &root, &held), None, "one worker");
    // The same holds after a failed write, and a held page after that failure.
    let fail = |_: &Path, _: u32| {
        Err(crate::library::BookError::PermissionDenied {
            path: PathBuf::from("/ro"),
        })
    };
    let failed = save_in(&session, &root, 2, 2, t0 + SAVE_INTERVAL, fail);
    assert!(failed.is_err());
    assert_eq!(
        claim_after_save(&session, &root, &failed),
        None,
        "still owned"
    );
    let held_again = save_in(&session, &root, 2, 3, t0 + SAVE_INTERVAL, fail);
    assert_eq!(claim_after_save(&session, &root, &held_again), None);
    // A saved page needs none, and neither does a book nobody opened.
    assert_eq!(
        claim_after_save(&session, &root, &Ok(SaveOutcome::Saved)),
        None
    );
    let gone = root.join("nowhere");
    assert_eq!(
        claim_after_save(&session, &gone, &Ok(SaveOutcome::Pending)),
        None
    );
    // The worker's write ends it, which frees the claim for the next held page.
    let step = session
        .lock()
        .unwrap()
        .worker_step(&key, token, t0 + SAVE_INTERVAL * 20, ok);
    assert_eq!(step, super::WorkerStep::Saved(3));
    assert_eq!(view_of(&root).last_read_page, 2);
    let next = save_in(&session, &root, 0, 4, t0 + SAVE_INTERVAL * 20, ok);
    assert_eq!(next, Ok(SaveOutcome::Pending));
    assert!(claim_after_save(&session, &root, &next).is_some());
}

#[test]
fn a_worker_retries_after_a_failure_then_announces_once() {
    use super::run_worker;
    use std::cell::Cell;
    use std::sync::Mutex;
    use std::time::Duration;
    let key = PathBuf::from("/book");
    let tick = Duration::from_millis(5);
    let session = Mutex::new(Session::new(tick));
    let t0 = Instant::now();
    {
        let mut s = session.lock().unwrap();
        s.register(key.clone(), 0, 3, t0);
        s.record_seq(&key, 2, 1, t0).unwrap();
    }
    let token = session.lock().unwrap().claim_worker(&key).unwrap();
    let attempts = Cell::new(0);
    let announced = Cell::new(0);
    let slept = Cell::new(0);
    let announced_seq = Cell::new(0);
    run_worker(
        &session,
        &key,
        token,
        |_, page| {
            attempts.set(attempts.get() + 1);
            assert_eq!(page, 2);
            if attempts.get() < 3 {
                return Err(crate::library::BookError::PermissionDenied {
                    path: PathBuf::from("/ro"),
                });
            }
            Ok(())
        },
        |d| {
            slept.set(slept.get() + 1);
            std::thread::sleep(d);
        },
        |seq| {
            announced.set(announced.get() + 1);
            announced_seq.set(seq);
        },
    );
    assert_eq!(attempts.get(), 3, "two failures, then a success");
    assert_eq!(announced.get(), 1);
    assert_eq!(announced_seq.get(), 1, "the event carries the seq it wrote");
    assert!(
        slept.get() >= 3,
        "it waited for the interval and each backoff"
    );
    // A worker whose book is already clean exits and announces nothing.
    let none = Cell::new(0);
    run_worker(
        &session,
        &key,
        token,
        |_, _| Ok(()),
        |_| {},
        |_| none.set(1),
    );
    assert_eq!(none.get(), 0);
}

#[test]
fn a_worker_announces_only_after_meta_json_holds_the_page() {
    use super::run_worker;
    use std::cell::Cell;
    use std::sync::Mutex;
    use std::time::Duration;
    let (_fx, root) = Fixture::new("announce");
    let key = canonical_key(&root).unwrap();
    let tick = Duration::from_millis(5);
    let session = Mutex::new(Session::new(tick));
    let t0 = Instant::now();
    {
        let mut s = session.lock().unwrap();
        s.register(key.clone(), 0, 3, t0);
        s.record_seq(&key, 2, 1, t0).unwrap();
    }
    let token = session.lock().unwrap().claim_worker(&key).unwrap();
    assert_eq!(view_of(&root).last_read_page, 0, "not written yet");
    let announced = Cell::new(false);
    run_worker(
        &session,
        &key,
        token,
        |r, p| save_last_read_page(r, p).map(|_| ()),
        std::thread::sleep,
        |seq| {
            assert_eq!(seq, 1);
            assert_eq!(
                view_of(&root).last_read_page,
                2,
                "the event goes out after the write"
            );
            announced.set(true);
        },
    );
    assert!(announced.get());
}

#[test]
fn event_payloads_serialize_camel_case() {
    use super::{IngestProgress, ProgressSaved};
    let tick = IngestProgress {
        import_id: "i1".into(),
        done: 2,
        total: 5,
    };
    assert_eq!(
        serde_json::to_value(tick).unwrap(),
        serde_json::json!({ "importId": "i1", "done": 2, "total": 5 })
    );
    let saved = ProgressSaved::new(std::path::Path::new("/r"), 9);
    assert_eq!(
        serde_json::to_value(saved).unwrap(),
        serde_json::json!({ "root": "/r", "seq": 9 })
    );
}
