//! The per-page progress callback of `create_book_from_folder_with`.

use super::create::create_book_from_folder_with;
use super::tests::{mixed_folder, opts, Tmp};

#[test]
fn callback_fires_once_per_page_and_ends_at_total() {
    let t = Tmp::new("callback");
    let lib = t.dir("lib");
    let mut calls: Vec<(u32, u32)> = Vec::new();
    create_book_from_folder_with(&mixed_folder(&t), &lib, &opts("bk"), |done, total| {
        calls.push((done, total));
    })
    .unwrap();
    // The mixed fixture has 6 pages: 1.jpg, 3 TIFF frames, 3.png, and the BMP.
    assert_eq!(calls.len(), 6);
    assert!(calls.iter().all(|&(_, total)| total == 6), "{calls:?}");
    let done: Vec<u32> = calls.iter().map(|&(d, _)| d).collect();
    assert_eq!(done, [1, 2, 3, 4, 5, 6]);
}

#[test]
fn callback_is_not_called_when_creation_fails_early() {
    let t = Tmp::new("callback-fail");
    let lib = t.dir("lib");
    let mut calls = 0;
    let res = create_book_from_folder_with(&t.dir("empty"), &lib, &opts("bk"), |_, _| calls += 1);
    assert!(res.is_err());
    assert_eq!(calls, 0);
}
