//! The engine-input step: turn a display image into a capped PNG that Tesseract can read.
//!
//! A display image is already 8-bit sRGB and orientation-applied by ingest, so the step only
//! has to bound its size and make it readable. Pages within the PDF caps are decoded as they
//! are; larger JPEGs decode at a reduced DCT scale and then resize down to the caps. A page is
//! refused before decoding when its header claims more than 150 MP, a whole decode would
//! allocate more than 200 MB, or the buffers a JPEG holds at once would pass the 264 MB
//! large-page budget. Large pages share one permit, so their peaks add up to the memory
//! budget (Phase 4 decisions 2, 7, and 12).

use crate::ingest::pdf::{MAX_LONG_EDGE, MAX_PIXELS};
use crate::ingest::MAX_PAGE_PIXELS;
use crate::library::BookError;
use crate::ocr::run_dir::RunDir;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

/// Largest decode buffer a small page may allocate: 48 MB. Such a page needs no permit.
pub(crate) const SMALL_PAGE_BYTES: u64 = 48_000_000;
/// Largest buffer a whole decode may allocate: 200 MB. Any larger page is refused.
pub(crate) const MAX_WHOLE_DECODE_BYTES: u64 = 200_000_000;
/// Largest peak one large page may reach: 264 MB. A page past this is refused.
///
/// The figure is a 200 MB decode, the capped resize, and the encoder. It is not raised
/// for a progressive JPEG, so the pool total stays 600 MB.
pub(crate) const LARGE_PAGE_BUDGET: u64 = 264_000_000;

/// Why the input step produced no page image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputError {
    /// The stop flag was set before the step finished.
    Cancelled,
    /// The page deadline passed before the step finished.
    Timeout,
    /// The page could not be read, decoded, or written. A refusal is an `UnreadableFile`
    /// whose reason starts with "too large".
    Failed(BookError),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "the OCR input step was cancelled"),
            Self::Timeout => write!(f, "the OCR input step passed its deadline"),
            Self::Failed(error) => error.fmt(f),
        }
    }
}

/// A page image the engine can read, with the size it will see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prepared {
    /// The PNG written into the run directory.
    pub path: PathBuf,
    /// Width of that PNG in pixels.
    pub width: u32,
    /// Height of that PNG in pixels.
    pub height: u32,
}

/// The one large-page permit. Pages whose decode buffer exceeds [`SMALL_PAGE_BYTES`] take it,
/// so only one such buffer exists at a time across every book.
static LARGE_PAGE: Mutex<()> = Mutex::new(());

/// A held large-page permit. Dropping it releases the slot, including on an error or cancel.
pub(crate) struct LargePagePermit {
    _guard: MutexGuard<'static, ()>,
}

impl LargePagePermit {
    /// Wait for the one large-page slot.
    pub(crate) fn acquire() -> Self {
        let guard = LARGE_PAGE
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        #[cfg(test)]
        record_holder();
        Self { _guard: guard }
    }

    /// Take the slot only if it is free, for tests that prove it is shared.
    #[cfg(test)]
    pub(crate) fn try_acquire() -> Option<Self> {
        let guard = LARGE_PAGE.try_lock().ok()?;
        record_holder();
        Some(Self { _guard: guard })
    }
}

impl Drop for LargePagePermit {
    fn drop(&mut self) {
        #[cfg(test)]
        release_holder();
    }
}

#[cfg(test)]
static HOLDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static MAX_HOLDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
fn record_holder() {
    use std::sync::atomic::Ordering;
    let now = HOLDERS.fetch_add(1, Ordering::SeqCst) + 1;
    MAX_HOLDERS.fetch_max(now, Ordering::SeqCst);
}

#[cfg(test)]
fn release_holder() {
    HOLDERS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
impl LargePagePermit {
    /// The number of pages holding the permit now.
    pub(crate) fn holders() -> usize {
        HOLDERS.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The most pages that held the permit at once since the last reset.
    pub(crate) fn max_holders() -> usize {
        MAX_HOLDERS.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Forget the peak holder count.
    pub(crate) fn reset_holders() {
        MAX_HOLDERS.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Set while a decode is running, so a test can set the stop flag mid-decode.
#[cfg(test)]
pub(crate) static DECODING: AtomicBool = AtomicBool::new(false);

/// Read the display image at `source`, bound it to the PDF caps, and write a PNG into `run`.
///
/// The stop flag and deadline are checked before decoding, after decoding, and after resizing.
/// A decode already in progress cannot be interrupted, so a cancel waits for that one call.
pub(crate) fn prepare(
    source: &Path,
    run: &RunDir,
    page: u32,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<Prepared, InputError> {
    check(stop, deadline)?;
    let header = read_header(source)?;
    let plan = plan_page(source, &header)?;
    let _permit = if plan.large {
        Some(LargePagePermit::acquire())
    } else {
        None
    };
    check(stop, deadline)?;
    let image = decode(source, plan)?;
    check(stop, deadline)?;
    let (width, height) = fit_caps(image.width(), image.height());
    let path = write_png(&image, width, height, run, page)?;
    check(stop, deadline)?;
    Ok(Prepared {
        path,
        width,
        height,
    })
}

/// The header facts the step needs before allocating for pixels.
struct Header {
    width: u32,
    height: u32,
    bytes_per_pixel: u64,
    format: Option<ImageFormat>,
}

/// What to do with a page, decided from its header alone.
#[derive(Debug, Clone, Copy)]
struct Plan {
    target_width: u32,
    target_height: u32,
    jpeg: bool,
    large: bool,
}

/// Read the size and colour type without decoding pixels.
fn read_header(source: &Path) -> Result<Header, InputError> {
    let reader = ImageReader::open(source)
        .map_err(|error| unreadable(source, error))?
        .with_guessed_format()
        .map_err(|error| unreadable(source, error))?;
    let format = reader.format();
    let decoder = reader
        .into_decoder()
        .map_err(|error| unreadable(source, error))?;
    let (width, height) = decoder.dimensions();
    Ok(Header {
        width,
        height,
        bytes_per_pixel: u64::from(decoder.color_type().bytes_per_pixel()),
        format,
    })
}

/// Refuse a page that is too large, then choose the decode path and the output caps.
fn plan_page(source: &Path, header: &Header) -> Result<Plan, InputError> {
    let pixels = u64::from(header.width) * u64::from(header.height);
    if header.width == 0 || header.height == 0 {
        return Err(unreadable(source, "image has zero width or height"));
    }
    if pixels > MAX_PAGE_PIXELS {
        return Err(too_large(
            source,
            header,
            pixels,
            "over the 150 MP page limit",
        ));
    }
    // A scaled baseline JPEG never allocates its full pixel buffer. A progressive JPEG still
    // does: `jpeg-decoder` sizes the coefficient buffer from the frame's blocks, and the DCT
    // scale does not shrink that buffer. Two bytes a sample, so the check runs for every page.
    let coefficients = progressive_coefficient_bytes(source, header)?;
    let (target_width, target_height) = fit_caps(header.width, header.height);
    refuse_decode_budget(
        source,
        header,
        pixels,
        coefficients,
        target_width,
        target_height,
    )?;
    let bytes = pixels.saturating_mul(header.bytes_per_pixel);
    Ok(Plan {
        target_width,
        target_height,
        jpeg: header.format == Some(ImageFormat::Jpeg),
        // The permit follows the larger single buffer. The 264 MB check above is what
        // refuses a page whose buffers, added together, would pass the large-page budget.
        large: bytes > SMALL_PAGE_BYTES || coefficients > SMALL_PAGE_BYTES,
    })
}

/// Refuse a page whose single buffer passes 200 MB, or whose JPEG buffers would pass 264 MB.
///
/// Exactly either limit is allowed. A JPEG that will be DCT-scaled is not refused for its
/// full pixel buffer, because that buffer is not allocated. The combined check still runs:
/// the coefficient buffer and the scaled output are live together.
fn refuse_decode_budget(
    source: &Path,
    header: &Header,
    pixels: u64,
    coefficients: u64,
    target_width: u32,
    target_height: u32,
) -> Result<(), InputError> {
    let scaled_jpeg =
        header.format == Some(ImageFormat::Jpeg) && over_caps(header.width, header.height);
    let bytes = pixels.saturating_mul(header.bytes_per_pixel);
    if coefficients > MAX_WHOLE_DECODE_BYTES || (!scaled_jpeg && bytes > MAX_WHOLE_DECODE_BYTES) {
        return Err(too_large(
            source,
            header,
            pixels,
            "over the 200 MB decode limit",
        ));
    }
    if simultaneous_jpeg_bytes(header, coefficients, target_width, target_height)
        > LARGE_PAGE_BUDGET
    {
        return Err(too_large(
            source,
            header,
            pixels,
            "over the 264 MB page budget",
        ));
    }
    Ok(())
}

/// Bytes a JPEG decode holds at once, or 0 for any other format.
///
/// Progressive coefficients stay allocated until decode returns. The sample planes are
/// then built at the selected scale. Greyscale reuses its one plane as the output.
/// Colour keeps those planes and allocates the interleaved image as well, so the output
/// is counted twice. The scale is the one `jpeg-decoder` would pick for the capped size,
/// which stays 1/1 when a half-size image would fall below the caps.
fn simultaneous_jpeg_bytes(
    header: &Header,
    coefficients: u64,
    target_width: u32,
    target_height: u32,
) -> u64 {
    if header.format != Some(ImageFormat::Jpeg) {
        return 0;
    }
    let (out_width, out_height) =
        jpeg_output_size(header.width, header.height, target_width, target_height);
    let output = u64::from(out_width)
        .saturating_mul(u64::from(out_height))
        .saturating_mul(header.bytes_per_pixel);
    let copies = if header.bytes_per_pixel > 1 { 2 } else { 1 };
    coefficients.saturating_add(output.saturating_mul(copies))
}

/// The size `jpeg-decoder::Decoder::scale` would return for this request.
///
/// `choose_idct_size` tries 1/8, 1/4, and 1/2, then 1/1. A dimension that does not fit
/// in `u16` stays at full size, which is the larger buffer.
fn jpeg_output_size(
    width: u32,
    height: u32,
    requested_width: u32,
    requested_height: u32,
) -> (u32, u32) {
    let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
        return (width, height);
    };
    let requested_width = u16::try_from(requested_width).unwrap_or(u16::MAX);
    let requested_height = u16::try_from(requested_height).unwrap_or(u16::MAX);
    let idct = jpeg_idct_size(width, height, requested_width, requested_height);
    (
        jpeg_scaled_axis(width, idct),
        jpeg_scaled_axis(height, idct),
    )
}

/// `jpeg-decoder`'s `choose_idct_size`: 1, 2, 4, or 8 samples per block.
fn jpeg_idct_size(width: u16, height: u16, requested_width: u16, requested_height: u16) -> u32 {
    for scale in [1u32, 2, 4] {
        if jpeg_scaled_len(width, scale) >= requested_width
            || jpeg_scaled_len(height, scale) >= requested_height
        {
            return scale;
        }
    }
    8
}

fn jpeg_scaled_len(len: u16, scale: u32) -> u16 {
    ((u32::from(len) * scale - 1) / 8 + 1) as u16
}

fn jpeg_scaled_axis(len: u16, idct: u32) -> u32 {
    u32::from((f32::from(len) * idct as f32 / 8.0).ceil() as u16)
}

/// Bytes of DCT coefficients a progressive JPEG will hold, or 0 for any other page.
///
/// `jpeg-decoder` stores one `i16` per coefficient per component for the whole frame.
fn progressive_coefficient_bytes(source: &Path, header: &Header) -> Result<u64, InputError> {
    if header.format != Some(ImageFormat::Jpeg) {
        return Ok(0);
    }
    let file = File::open(source).map_err(|error| unreadable(source, error))?;
    let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    decoder
        .read_info()
        .map_err(|error| unreadable(source, error))?;
    let info = decoder
        .info()
        .ok_or_else(|| unreadable(source, "JPEG header has no size"))?;
    if info.coding_process != jpeg_decoder::CodingProcess::DctProgressive {
        return Ok(0);
    }
    let pixels = u64::from(header.width).saturating_mul(u64::from(header.height));
    Ok(pixels
        .saturating_mul(header.bytes_per_pixel)
        .saturating_mul(2))
}

/// True when the page is above either PDF cap.
fn over_caps(width: u32, height: u32) -> bool {
    width.max(height) > MAX_LONG_EDGE || u64::from(width) * u64::from(height) > MAX_PIXELS
}

/// The largest size at or below both PDF caps. Never enlarges a page.
fn fit_caps(width: u32, height: u32) -> (u32, u32) {
    if !over_caps(width, height) {
        return (width, height);
    }
    let edge = f64::from(MAX_LONG_EDGE) / f64::from(width.max(height));
    let area = (MAX_PIXELS as f64 / (u64::from(width) * u64::from(height)) as f64).sqrt();
    let factor = edge.min(area).min(1.0);
    let scaled = |value: u32| -> u32 {
        ((f64::from(value) * factor).floor() as u32)
            .max(1)
            .min(value)
    };
    (scaled(width), scaled(height))
}

/// Decode the page, either whole or, for a large JPEG, at a reduced DCT scale.
fn decode(source: &Path, plan: Plan) -> Result<DynamicImage, InputError> {
    #[cfg(test)]
    DECODING.store(true, Ordering::SeqCst);
    let result = if plan.jpeg {
        decode_jpeg(source, plan.target_width, plan.target_height)
    } else {
        decode_whole(source)
    };
    #[cfg(test)]
    DECODING.store(false, Ordering::SeqCst);
    result
}

/// Decode a non-JPEG at full size. JPEGs always take [`decode_jpeg`], which avoids `image`'s
/// two full-size buffers.
fn decode_whole(source: &Path) -> Result<DynamicImage, InputError> {
    ImageReader::open(source)
        .map_err(|error| unreadable(source, error))?
        .with_guessed_format()
        .map_err(|error| unreadable(source, error))?
        .decode()
        .map_err(|error| unreadable(source, error))
}

/// Decode a JPEG through `jpeg-decoder` at the smallest DCT scale whose output is still at or
/// above the requested capped size. The output is at least the requested size in one axis; the
/// resize that follows never enlarges the other. `image`'s JPEG path decodes twice into two
/// full buffers, so even a page within the caps takes this route.
fn decode_jpeg(source: &Path, width: u32, height: u32) -> Result<DynamicImage, InputError> {
    use image::{ImageBuffer, Luma};
    use jpeg_decoder::PixelFormat;

    let file = File::open(source).map_err(|error| unreadable(source, error))?;
    let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    let (out_width, out_height) = decoder
        .scale(
            u16::try_from(width).map_err(|error| unreadable(source, error))?,
            u16::try_from(height).map_err(|error| unreadable(source, error))?,
        )
        .map_err(|error| unreadable(source, error))?;
    let format = decoder
        .info()
        .ok_or_else(|| unreadable(source, "JPEG header has no size"))?
        .pixel_format;
    let pixels = decoder
        .decode()
        .map_err(|error| unreadable(source, error))?;
    let width = u32::from(out_width);
    let height = u32::from(out_height);
    let image = match format {
        PixelFormat::L8 => ImageBuffer::<Luma<u8>, _>::from_raw(width, height, pixels)
            .map(DynamicImage::ImageLuma8),
        PixelFormat::RGB24 => ImageBuffer::<image::Rgb<u8>, _>::from_raw(width, height, pixels)
            .map(DynamicImage::ImageRgb8),
        other => {
            return Err(unreadable(
                source,
                format!("unsupported display JPEG pixel format {other:?}"),
            ))
        }
    };
    image.ok_or_else(|| unreadable(source, "JPEG pixel buffer has the wrong size"))
}

/// The output size `jpeg-decoder` would choose for this JPEG, without decoding pixels. Tests
/// use it to prove a large page really is reduced: 16 MP stays 1/1 and 100 MP uses 1/2.
#[cfg(test)]
pub(crate) fn scaled_jpeg_output(source: &Path) -> Result<(u32, u32), InputError> {
    let header = read_header(source)?;
    let plan = plan_page(source, &header)?;
    let file = File::open(source).map_err(|error| unreadable(source, error))?;
    let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    let (width, height) = decoder
        .scale(
            u16::try_from(plan.target_width).map_err(|error| unreadable(source, error))?,
            u16::try_from(plan.target_height).map_err(|error| unreadable(source, error))?,
        )
        .map_err(|error| unreadable(source, error))?;
    Ok((u32::from(width), u32::from(height)))
}

/// Resize the decoded buffer into RGBA rows and write them as a PNG in one pass.
///
/// Sampling is nearest-neighbour: the output is at most the PDF caps, so text stays legible,
/// and the step never allocates a second copy of the source in another layout. Rows stream to
/// the encoder, so the resize itself adds only one row, not a full RGBA copy.
fn write_png(
    image: &DynamicImage,
    width: u32,
    height: u32,
    run: &RunDir,
    page: u32,
) -> Result<PathBuf, InputError> {
    let (source_width, source_height) = image.dimensions();
    let path = run.path().join(format!("page-{page:04}.png"));
    let file = File::create(&path).map_err(|error| unreadable(&path, error))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .and_then(|header| header.into_stream_writer())
        .map_err(|error| unreadable(&path, error))?;
    let mut row = vec![0u8; width as usize * 4];
    for y in 0..height {
        let source_y = (u64::from(y) * u64::from(source_height) / u64::from(height)) as u32;
        for x in 0..width {
            let source_x = (u64::from(x) * u64::from(source_width) / u64::from(width)) as u32;
            let pixel = image.get_pixel(source_x, source_y);
            let start = x as usize * 4;
            row[start..start + 4].copy_from_slice(&pixel.0);
        }
        writer
            .write_all(&row)
            .map_err(|error| unreadable(&path, error))?;
    }
    writer.finish().map_err(|error| unreadable(&path, error))?;
    Ok(path)
}

/// Stop when the flag is set or the deadline has passed. Cancellation wins over a timeout.
fn check(stop: &AtomicBool, deadline: Instant) -> Result<(), InputError> {
    if stop.load(Ordering::Relaxed) {
        return Err(InputError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(InputError::Timeout);
    }
    Ok(())
}

/// A read or write failure, as the library's existing unreadable-file kind.
fn unreadable(path: &Path, reason: impl std::fmt::Display) -> InputError {
    InputError::Failed(BookError::UnreadableFile {
        file: path.to_path_buf(),
        reason: reason.to_string(),
    })
}

/// A refusal the job records as "too large" on the failed page.
fn too_large(source: &Path, header: &Header, pixels: u64, why: &str) -> InputError {
    unreadable(
        source,
        format!(
            "too large: {}×{} ({} MP, {} bytes a pixel) is {why}",
            header.width,
            header.height,
            pixels / 1_000_000,
            header.bytes_per_pixel
        ),
    )
}

#[cfg(test)]
mod scale_estimate {
    use super::{fit_caps, jpeg_output_size};

    #[test]
    fn a_100mp_request_uses_half_scale() {
        assert_eq!(
            jpeg_output_size(10_000, 10_000, 4_000, 4_000),
            (5_000, 5_000)
        );
    }

    #[test]
    fn a_page_just_over_the_caps_stays_full_scale() {
        let (width, height) = fit_caps(4200, 3200);
        assert_eq!(jpeg_output_size(4200, 3200, width, height), (4200, 3200));
    }
}
