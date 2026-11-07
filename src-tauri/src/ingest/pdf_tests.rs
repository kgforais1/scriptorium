//! PDF tests. They need the pdfium library from `scripts/fetch_pdfium.sh`, found through
//! `PDFIUM_DYNAMIC_LIB_PATH` or `src-tauri/pdfium/`. Fixtures are committed under
//! `tests/fixtures/pdf/` and come from `scripts/generate_pdf_fixtures.py`.

use super::pdf::{load_library, open, render_size, PdfDoc, PdfError, MAX_LONG_EDGE, MAX_PIXELS};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/pdf"
    ))
    .join(name)
}

fn open_fixture(name: &str) -> PdfDoc {
    open(&fixture(name)).unwrap_or_else(|err| panic!("{name}: {err}"))
}

fn within_caps(width: u32, height: u32) -> bool {
    width.max(height) <= MAX_LONG_EDGE && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

#[test]
fn letter_page_renders_at_300_dpi_without_scaling() {
    assert_eq!(render_size(612.0, 792.0).unwrap(), (2550, 3300));
}

#[test]
fn oversized_page_is_scaled_into_both_caps() {
    let (w, h) = render_size(2384.0, 3370.0).unwrap();
    assert!(within_caps(w, h), "{w}x{h}");
    assert_eq!(
        h, MAX_LONG_EDGE,
        "the long edge cap is the binding one for A0"
    );
    let ratio = f64::from(w) / f64::from(h);
    assert!((ratio - 2384.0 / 3370.0).abs() < 0.001, "aspect ratio kept");
}

#[test]
fn area_cap_binds_for_a_square_page() {
    // 300 DPI of 1000 pt is 4166 px a side, so the long edge cap alone would allow 16 MP.
    let (w, h) = render_size(1000.0, 1000.0).unwrap();
    assert!(within_caps(w, h), "{w}x{h}");
    assert!(w < MAX_LONG_EDGE, "the area cap binds before the edge cap");
}

#[test]
fn degenerate_page_sizes_are_unreadable() {
    for (w, h) in [(0.0, 100.0), (100.0, -1.0), (f32::NAN, 10.0), (0.1, 100.0)] {
        assert!(
            matches!(render_size(w, h), Err(PdfError::Unreadable(_))),
            "{w}x{h}"
        );
    }
}

#[test]
fn text_pdf_has_three_pages_with_text() {
    let doc = open_fixture("text3.pdf");
    assert_eq!(doc.page_count(), 3);
    assert!(doc.page_text(0).unwrap().contains("Page one"));
    assert!(doc.page_text(2).unwrap().contains("Page three"));
    let page = doc.render_page(1).unwrap();
    assert_eq!(page.dimensions(), (1250, 1250));
}

#[test]
fn out_of_range_page_is_an_error() {
    let doc = open_fixture("text3.pdf");
    assert!(matches!(doc.render_page(3), Err(PdfError::Unreadable(_))));
}

#[test]
fn rotated_page_renders_landscape() {
    let page = open_fixture("rotate90.pdf").render_page(0).unwrap();
    // A 200x300 pt page turned by 90 degrees is 300x200 pt, which is 1250x833 px.
    assert_eq!(page.dimensions(), (1250, 833));
}

#[test]
fn crop_box_sets_the_page_size() {
    let page = open_fixture("cropbox.pdf").render_page(0).unwrap();
    // The 200x100 pt crop box, not the 400x400 pt media box.
    assert_eq!(page.dimensions(), (833, 417));
}

#[test]
fn oversized_page_renders_within_both_caps() {
    let page = open_fixture("a0.pdf").render_page(0).unwrap();
    let (w, h) = page.dimensions();
    assert!(within_caps(w, h), "{w}x{h}");
    assert_eq!((w, h), render_size(2384.0, 3370.0).unwrap());
}

#[test]
fn transparent_area_renders_white() {
    let page = open_fixture("transparent.pdf").render_page(0).unwrap();
    let (w, h) = page.dimensions();
    assert_eq!(
        page.get_pixel(w / 8, h / 2).0,
        [255, 255, 255],
        "left half is paper"
    );
    let [r, g, b] = page.get_pixel(w * 7 / 8, h / 2).0;
    assert!(
        r > 180 && g < 80 && b < 80,
        "right half keeps the image: {r},{g},{b}"
    );
}

#[test]
fn encrypted_pdf_is_refused() {
    assert!(matches!(
        open(&fixture("encrypted.pdf")),
        Err(PdfError::Encrypted)
    ));
}

#[test]
fn zero_page_pdf_is_refused() {
    assert!(matches!(
        open(&fixture("zero_pages.pdf")),
        Err(PdfError::NoPages)
    ));
}

#[test]
fn truncated_pdf_is_refused() {
    assert!(matches!(
        open(&fixture("truncated.pdf")),
        Err(PdfError::Unreadable(_))
    ));
}

#[test]
fn missing_file_is_refused() {
    assert!(matches!(
        open(&fixture("no-such-file.pdf")),
        Err(PdfError::Unreadable(_))
    ));
}

#[test]
fn pdf_with_junk_before_the_header_opens() {
    assert_eq!(open_fixture("junk_prefix.pdf").page_count(), 3);
}

#[test]
fn missing_library_reports_the_path_it_tried() {
    let dir = std::env::temp_dir().join("scriptorium-no-pdfium-here");
    let Err(err) = load_library(&dir) else {
        panic!("no library should load from {}", dir.display());
    };
    assert!(err.contains("scriptorium-no-pdfium-here"), "{err}");
}
