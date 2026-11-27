//! Tests for the engine-input step: caps, refusals, budgets, the large-page permit, and
//! cancellation. Fixtures are generated into a temp folder; no large image is committed.
//!
//! policy:file-size allow=780 reason=the progressive budget refusal stays with these tests

use super::input::*;
use crate::library::BookError;
use crate::ocr::run_dir::RunDir;
use crate::test_alloc::{self, Budget};
use image::{DynamicImage, ImageBuffer, ImageFormat, RgbImage, Rgba};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant, SystemTime};

const SMALL_BUDGET: usize = 112_000_000;
const LARGE_BUDGET: usize = LARGE_PAGE_BUDGET as usize;
const COMBINED_BUDGET: usize = 600_000_000;
/// Sample plane of the 100 MP greyscale fixture at the 1/2 DCT scale.
const SCALED_100MP_PLANE: usize = 5_000 * 5_000;
/// Sample plane of that fixture decoded at 1/1. A baseline peak this large skipped the scale.
const FULL_100MP_PLANE: usize = 10_000 * 10_000;
const PAGE_DEADLINE: Duration = Duration::from_secs(120);

/// The shared fixture folder for this test run. It is removed when the last test drops it.
struct Fixtures {
    dir: PathBuf,
    jpeg_100mp_baseline: OnceLock<PathBuf>,
    jpeg_100mp_progressive: OnceLock<PathBuf>,
    png_60mp_rgb: OnceLock<PathBuf>,
}

impl Fixtures {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "scriptorium-ocr-input-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            jpeg_100mp_baseline: OnceLock::new(),
            jpeg_100mp_progressive: OnceLock::new(),
            png_60mp_rgb: OnceLock::new(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// A 100 MP greyscale JPEG, built once per test run under the budget lock.
    fn jpeg_100mp(&self, progressive: bool) -> PathBuf {
        let cell = if progressive {
            &self.jpeg_100mp_progressive
        } else {
            &self.jpeg_100mp_baseline
        };
        cell.get_or_init(|| {
            let _heavy = test_alloc::lock_budget();
            let name = if progressive {
                "jpeg-100mp-progressive.jpg"
            } else {
                "jpeg-100mp-baseline.jpg"
            };
            let path = self.path(name);
            if !path.is_file() {
                // Greyscale needs 100 MB, not 300 MB, to build.
                write_jpeg(&path, 10_000, 10_000, false, progressive);
            }
            path
        })
        .clone()
    }

    /// A 60 MP RGB8 PNG (180 MB), built once per test run under the budget lock.
    fn png_60mp_rgb(&self) -> PathBuf {
        self.png_60mp_rgb
            .get_or_init(|| {
                let _heavy = test_alloc::lock_budget();
                let path = self.path("png-60mp-rgb.png");
                if !path.is_file() {
                    write_png_rgb(&path, 8000, 7500);
                }
                path
            })
            .clone()
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

static FIXTURES: Mutex<Option<Weak<Fixtures>>> = Mutex::new(None);

/// The shared fixtures for this run. The last test to finish removes the folder.
fn fixtures() -> Arc<Fixtures> {
    let mut slot = FIXTURES.lock().unwrap();
    if let Some(fixtures) = slot.as_ref().and_then(Weak::upgrade) {
        return fixtures;
    }
    let fixtures = Arc::new(Fixtures::new());
    *slot = Some(Arc::downgrade(&fixtures));
    fixtures
}

/// Write a greyscale or RGB baseline/progressive JPEG.
fn write_jpeg(path: &Path, width: u32, height: u32, rgb: bool, progressive: bool) {
    let channels = if rgb { 3 } else { 1 };
    let mut pixels = vec![0u8; width as usize * height as usize * channels];
    // A high-frequency checkerboard gives the encoder real coefficients to write, so decoding
    // costs what a scanned page costs instead of a flat fill.
    for y in 0..height as usize {
        for x in 0..width as usize {
            let value = if ((x / 2) ^ (y / 2)) & 1 == 0 {
                32
            } else {
                224
            };
            let start = (y * width as usize + x) * channels;
            for channel in 0..channels {
                pixels[start + channel] = value;
            }
        }
    }
    let mut encoder = jpeg_encoder::Encoder::new_file(path, 70).unwrap();
    encoder.set_optimized_huffman_tables(false);
    // Quality 70 otherwise subsamples chroma, which hides the full-frame coefficient buffer.
    if rgb {
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    }
    if progressive {
        encoder.set_progressive(true);
        encoder.set_progressive_scans(2);
    }
    let color = if rgb {
        jpeg_encoder::ColorType::Rgb
    } else {
        jpeg_encoder::ColorType::Luma
    };
    encoder
        .encode(&pixels, width as u16, height as u16, color)
        .unwrap();
}

/// Write an RGB8 PNG.
fn write_png_rgb(path: &Path, width: u32, height: u32) {
    let data = vec![128u8; width as usize * height as usize * 3];
    let image = RgbImage::from_raw(width, height, data).unwrap();
    DynamicImage::ImageRgb8(image)
        .save_with_format(path, ImageFormat::Png)
        .unwrap();
}

/// Write a 16-bit RGBA PNG.
fn write_png_rgba16(path: &Path, width: u32, height: u32) {
    let data = vec![0x8080u16; width as usize * height as usize * 4];
    let image = ImageBuffer::<Rgba<u16>, _>::from_raw(width, height, data).unwrap();
    DynamicImage::ImageRgba16(image)
        .save_with_format(path, ImageFormat::Png)
        .unwrap();
}

/// Write a small lossless WebP page.
fn write_webp(path: &Path, width: u32, height: u32) {
    let data = vec![96u8; width as usize * height as usize * 3];
    let image = RgbImage::from_raw(width, height, data).unwrap();
    DynamicImage::ImageRgb8(image)
        .save_with_format(path, ImageFormat::WebP)
        .unwrap();
}

/// Write a PNG with a real IHDR and no pixel data, so only the header is read.
fn write_png_header(path: &Path, width: u32, height: u32, color_type: u8, bit_depth: u8) {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[bit_depth, color_type, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png_chunk(&mut out, b"IHDR", &ihdr);
    // A valid empty zlib stream: the decoder insists an IDAT exists before it will report the
    // header, but the body is never decompressed because the step refuses first.
    png_chunk(
        &mut out,
        b"IDAT",
        &[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    );
    png_chunk(&mut out, b"IEND", &[]);
    fs::write(path, out).unwrap();
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(body);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn prepare_ok(path: &Path, run: &RunDir, page: u32) -> Prepared {
    prepare(
        path,
        run,
        page,
        &AtomicBool::new(false),
        Instant::now() + PAGE_DEADLINE,
    )
    .unwrap()
}

fn assert_within_caps(width: u32, height: u32) {
    assert!(width.max(height) <= 4000, "{width}×{height} edge");
    assert!(
        u64::from(width) * u64::from(height) <= 12_000_000,
        "{width}×{height} area"
    );
}

/// The scaled greyscale plane is 25 MB. The worker-thread test is what proves that plane is
/// charged; this floor only checks the measured decode is at least that large.
fn assert_counted_scaled_plane(budget: &Budget, label: &str) {
    let peak = budget.peak();
    assert!(
        peak >= SCALED_100MP_PLANE,
        "{label}: peak {peak} is below the 25 MB scaled sample plane"
    );
}

/// A full-size baseline decode holds a 100 MB plane. Staying under that, while still covering
/// the scaled plane, fails if `decode_jpeg` stops scaling.
fn assert_scaled_baseline_peak(budget: &Budget) {
    assert_counted_scaled_plane(budget, "100 MP baseline JPEG");
    let peak = budget.peak();
    assert!(
        peak < FULL_100MP_PLANE,
        "100 MP baseline JPEG: peak {peak} is at least a full-size 100 MB plane"
    );
}

/// An allocation on `jpeg-decoder`'s worker thread counts. One on any other thread does not.
#[test]
fn a_jpeg_worker_allocation_counts_against_the_armed_meter() {
    const WORKER_BYTES: usize = 32_000_000;
    const OTHER_BYTES: usize = 48_000_000;
    let budget = Budget::arm();

    std::thread::Builder::new()
        .name("not a jpeg worker".into())
        .spawn(move || {
            let buffer = vec![1u8; OTHER_BYTES];
            std::hint::black_box(buffer.len());
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        budget.peak() < OTHER_BYTES,
        "an unrelated thread was charged: peak {}",
        budget.peak()
    );

    std::thread::Builder::new()
        .name(format!("{}0", crate::test_alloc::JPEG_WORKER_PREFIX))
        .spawn(move || {
            let buffer = vec![1u8; WORKER_BYTES];
            std::hint::black_box(buffer.len());
        })
        .unwrap()
        .join()
        .unwrap();
    let peak = budget.peak();
    assert!(
        peak >= WORKER_BYTES,
        "the worker plane was not charged: peak {peak}"
    );
    assert!(
        peak < OTHER_BYTES,
        "the worker charge also picked up another thread: peak {peak}"
    );
}

fn assert_too_large(result: Result<Prepared, InputError>) {
    assert_too_large_mentioning(result, "too large");
}

fn assert_too_large_mentioning(result: Result<Prepared, InputError>, fragment: &str) {
    match result {
        Err(InputError::Failed(BookError::UnreadableFile { reason, .. })) => {
            assert!(reason.starts_with("too large"), "reason was {reason:?}");
            assert!(
                reason.contains(fragment),
                "reason {reason:?} did not mention {fragment}"
            );
        }
        other => panic!("expected a too-large refusal, got {other:?}"),
    }
}

#[test]
fn a_16mp_baseline_jpeg_is_a_small_page() {
    let fixtures = fixtures();
    let path = fixtures.path("jpeg-16mp-baseline.jpg");
    {
        let _heavy = test_alloc::lock_budget();
        write_jpeg(&path, 4000, 4000, true, false);
    }
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(SMALL_BUDGET, "16 MP baseline JPEG");
    assert_within_caps(prepared.width, prepared.height);
    assert!(prepared.width <= 4000 && prepared.height <= 4000);
    assert_eq!(
        scaled_jpeg_output(&path).unwrap(),
        (4000, 4000),
        "16 MP stays 1/1"
    );
}

#[test]
fn a_16mp_progressive_jpeg_takes_the_large_page_permit() {
    let fixtures = fixtures();
    let path = fixtures.path("jpeg-16mp-progressive.jpg");
    {
        let _heavy = test_alloc::lock_budget();
        write_jpeg(&path, 4000, 4000, true, true);
    }
    let run = RunDir::new().unwrap();
    LargePagePermit::reset_holders();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    // 16 MP RGB progressive keeps a 96 MB coefficient buffer plus the planes and the
    // interleaved output, about 192 MB, so it takes the large-page permit.
    budget.assert_within(LARGE_BUDGET, "16 MP progressive JPEG");
    assert_eq!(
        LargePagePermit::max_holders(),
        1,
        "a progressive RGB page did not take the large-page permit"
    );
    assert_within_caps(prepared.width, prepared.height);
    assert_eq!(
        scaled_jpeg_output(&path).unwrap(),
        (4000, 4000),
        "16 MP stays 1/1"
    );
}

#[test]
fn a_16mp_png_is_a_small_page() {
    let fixtures = fixtures();
    let path = fixtures.path("png-16mp.png");
    {
        let _heavy = test_alloc::lock_budget();
        write_png_rgb(&path, 4000, 4000);
    }
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(SMALL_BUDGET, "16 MP PNG");
    assert_within_caps(prepared.width, prepared.height);
}

#[test]
fn a_webp_page_is_prepared() {
    let fixtures = fixtures();
    let path = fixtures.path("page.webp");
    write_webp(&path, 1200, 900);
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(SMALL_BUDGET, "WebP page");
    assert_eq!((prepared.width, prepared.height), (1200, 900));
    assert!(prepared.path.is_file());
}

#[test]
fn a_100mp_baseline_jpeg_is_decoded_at_a_reduced_scale() {
    let fixtures = fixtures();
    let path = fixtures.jpeg_100mp(false);
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(LARGE_BUDGET, "100 MP baseline JPEG");
    assert_scaled_baseline_peak(&budget);
    assert_within_caps(prepared.width, prepared.height);
    assert_eq!(
        scaled_jpeg_output(&path).unwrap(),
        (5000, 5000),
        "100 MP uses 1/2"
    );
}

#[test]
fn a_100mp_progressive_jpeg_is_decoded_at_a_reduced_scale() {
    let fixtures = fixtures();
    let path = fixtures.jpeg_100mp(true);
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(LARGE_BUDGET, "100 MP progressive JPEG");
    assert_within_caps(prepared.width, prepared.height);
    assert_eq!(
        scaled_jpeg_output(&path).unwrap(),
        (5000, 5000),
        "100 MP uses 1/2"
    );
}

/// A JPEG frame header and a start-of-scan marker, with no entropy data.
///
/// `marker` is SOF0 (`0xC0`) or SOF2 (`0xC2`). The header reader stops at the scan marker,
/// so the frame size is known before any coefficient is stored.
fn write_jpeg_rgb_header(path: &Path, marker: u8, width: u32, height: u32) {
    let mut sof = vec![8];
    sof.extend_from_slice(&(height as u16).to_be_bytes());
    sof.extend_from_slice(&(width as u16).to_be_bytes());
    sof.push(3);
    for id in 1..=3u8 {
        sof.extend_from_slice(&[id, 0x11, 0]);
    }
    // Three components, spectral selection 0–63, approximation 0. Length is 6 + 2 per component.
    let mut sos = vec![3];
    for id in 1..=3u8 {
        sos.extend_from_slice(&[id, 0]);
    }
    sos.extend_from_slice(&[0, 63, 0]);
    let mut out = vec![0xFF, 0xD8];
    push_marker(&mut out, marker, &sof);
    push_marker(&mut out, 0xDA, &sos);
    fs::write(path, out).unwrap();
}

fn push_marker(out: &mut Vec<u8>, marker: u8, body: &[u8]) {
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(body);
}

#[test]
fn a_progressive_rgb_jpeg_over_33mp_is_refused_before_decoding() {
    let fixtures = fixtures();
    let path = fixtures.path("jpeg-34mp-progressive-header.jpg");
    // 6000×5600 is 33.6 MP. Three components at two bytes a sample is 201.6 MB.
    // The page is over the PDF caps, so only the coefficient check can refuse it.
    write_jpeg_rgb_header(&path, 0xC2, 6000, 5600);
    let run = RunDir::new().unwrap();
    assert_too_large_mentioning(
        prepare(
            &path,
            &run,
            1,
            &AtomicBool::new(false),
            Instant::now() + PAGE_DEADLINE,
        ),
        "200 MB",
    );
}

#[test]
fn a_progressive_rgb_jpeg_over_the_caps_stays_within_the_large_budget() {
    let fixtures = fixtures();
    let path = fixtures.path("jpeg-13mp-progressive.jpg");
    {
        let _heavy = test_alloc::lock_budget();
        write_jpeg(&path, 4200, 3200, true, true);
    }
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    // 4200×3200 is 13.44 MP and over both PDF caps. 4:4:4 coefficients are 80.6 MB and
    // stay live with the planes and the interleaved output. Half scale falls below the caps.
    budget.assert_within(LARGE_BUDGET, "13 MP progressive RGB JPEG");
    assert!(
        budget.peak() >= 80_640_000,
        "coefficient buffer was not in the peak: {}",
        budget.peak()
    );
    assert_within_caps(prepared.width, prepared.height);
    assert_eq!(
        scaled_jpeg_output(&path).unwrap(),
        (4200, 3200),
        "a page just over the caps stays 1/1"
    );
}

#[test]
fn a_progressive_rgb_jpeg_over_the_page_budget_is_refused_before_decoding() {
    let fixtures = fixtures();
    let path = fixtures.path("jpeg-25mp-progressive-header.jpg");
    // 5000×5000 is 25 MP. Coefficients are 150 MB, under the 200 MB buffer limit.
    // At 1/1 the planes and the interleaved output add another 150 MB, past 264 MB.
    write_jpeg_rgb_header(&path, 0xC2, 5000, 5000);
    let run = RunDir::new().unwrap();
    assert_too_large_mentioning(
        prepare(
            &path,
            &run,
            1,
            &AtomicBool::new(false),
            Instant::now() + PAGE_DEADLINE,
        ),
        "264 MB",
    );
}

#[test]
fn a_baseline_rgb_jpeg_over_the_page_budget_is_refused_before_decoding() {
    let fixtures = fixtures();
    let path = fixtures.path("jpeg-45mp-baseline-header.jpg");
    // 6688×6688 is 44.7 MP. No coefficient buffer. The planes and the interleaved
    // output at 1/1 are 268 MB, and half scale is still below the caps.
    write_jpeg_rgb_header(&path, 0xC0, 6688, 6688);
    let run = RunDir::new().unwrap();
    assert_too_large_mentioning(
        prepare(
            &path,
            &run,
            1,
            &AtomicBool::new(false),
            Instant::now() + PAGE_DEADLINE,
        ),
        "264 MB",
    );
}

#[test]
fn a_60mp_rgba_png_is_refused_before_decoding() {
    let fixtures = fixtures();
    let path = fixtures.path("png-60mp-rgba-header.png");
    write_png_header(&path, 8000, 7500, 6, 8);
    let run = RunDir::new().unwrap();
    assert_too_large(prepare(
        &path,
        &run,
        1,
        &AtomicBool::new(false),
        Instant::now() + PAGE_DEADLINE,
    ));
}

#[test]
fn a_60mp_rgb8_png_is_accepted_under_the_large_page_permit() {
    let fixtures = fixtures();
    let path = fixtures.png_60mp_rgb();
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(LARGE_BUDGET, "60 MP RGB8 PNG");
    assert_within_caps(prepared.width, prepared.height);
}

#[test]
fn a_50mp_16bit_rgba_png_is_refused_before_decoding() {
    let fixtures = fixtures();
    let path = fixtures.path("png-50mp-rgba16-header.png");
    write_png_header(&path, 8000, 6250, 6, 16);
    let run = RunDir::new().unwrap();
    assert_too_large(prepare(
        &path,
        &run,
        1,
        &AtomicBool::new(false),
        Instant::now() + PAGE_DEADLINE,
    ));
}

#[test]
fn a_20mp_16bit_rgba_png_is_accepted_under_the_large_page_permit() {
    let fixtures = fixtures();
    let path = fixtures.path("png-20mp-rgba16.png");
    {
        let _heavy = test_alloc::lock_budget();
        write_png_rgba16(&path, 5000, 4000);
    }
    let run = RunDir::new().unwrap();
    let budget = Budget::arm();
    let prepared = prepare_ok(&path, &run, 1);
    budget.assert_within(LARGE_BUDGET, "20 MP 16-bit RGBA PNG");
    assert_within_caps(prepared.width, prepared.height);
}

#[test]
fn a_200mp_header_is_refused_before_decoding() {
    let fixtures = fixtures();
    let path = fixtures.path("png-200mp-header.png");
    write_png_header(&path, 20_000, 10_000, 2, 8);
    let run = RunDir::new().unwrap();
    assert_too_large(prepare(
        &path,
        &run,
        1,
        &AtomicBool::new(false),
        Instant::now() + PAGE_DEADLINE,
    ));
}

#[test]
fn only_one_large_page_holds_the_permit() {
    let fixtures = fixtures();
    let path = fixtures.png_60mp_rgb();
    let run = RunDir::new().unwrap();
    let _heavy = test_alloc::lock_budget();
    test_alloc::reset_combined();
    LargePagePermit::reset_holders();
    // One page can take it and a second attempt fails while that page holds it.
    let probe = LargePagePermit::try_acquire().expect("the permit starts free");
    assert!(
        LargePagePermit::try_acquire().is_none(),
        "two pages took the one permit"
    );
    drop(probe);
    LargePagePermit::reset_holders();
    let barrier = Barrier::new(2);

    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for page in 0..2 {
            let path = &path;
            let run = &run;
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                let budget = Budget::arm_unlocked();
                barrier.wait();
                let prepared = prepare(
                    path,
                    run,
                    page,
                    &AtomicBool::new(false),
                    Instant::now() + PAGE_DEADLINE,
                )
                .unwrap();
                (prepared.width, prepared.height, budget.peak())
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    for (width, height, peak) in results {
        assert_within_caps(width, height);
        assert!(peak <= LARGE_BUDGET, "page peak {peak} over budget");
    }
    assert_eq!(
        LargePagePermit::max_holders(),
        1,
        "two pages held the one large-page permit"
    );
    assert_eq!(
        LargePagePermit::holders(),
        0,
        "a page kept the permit after it finished"
    );
    let combined = test_alloc::combined_peak();
    assert!(
        combined <= COMBINED_BUDGET,
        "combined peak {combined} over {COMBINED_BUDGET}"
    );
}

#[test]
fn cancelling_during_a_100mp_decode_returns_within_the_measured_time() {
    let fixtures = fixtures();
    let path = fixtures.jpeg_100mp(false);
    let run = RunDir::new().unwrap();

    let full = {
        let _heavy = test_alloc::lock_budget();
        let start = Instant::now();
        prepare(
            &path,
            &run,
            1,
            &AtomicBool::new(false),
            Instant::now() + PAGE_DEADLINE,
        )
        .unwrap();
        start.elapsed()
    };

    let _heavy = test_alloc::lock_budget();
    DECODING.store(false, Ordering::SeqCst);
    let stop = AtomicBool::new(false);
    let wait = std::thread::scope(|scope| {
        let handle = scope.spawn(|| prepare(&path, &run, 2, &stop, Instant::now() + PAGE_DEADLINE));
        let start = Instant::now();
        while !DECODING.load(Ordering::SeqCst) {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "the decode never started"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let stopped_at = Instant::now();
        stop.store(true, Ordering::SeqCst);
        let result = handle.join().unwrap();
        let wait = stopped_at.elapsed();
        assert_eq!(result, Err(InputError::Cancelled));
        wait
    });

    // A decode cannot be interrupted, so the wait is the rest of that one decode. Bound it by
    // the measured full prepare rather than a tighter invented number.
    assert!(
        wait <= full + Duration::from_secs(2),
        "cancel took {wait:?}; a full prepare took {full:?}"
    );
}
