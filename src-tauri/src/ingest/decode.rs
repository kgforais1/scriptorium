//! Decoding one frame of each input format into an `image::DynamicImage`, with the ICC
//! profile and orientation the conversion rules need.

use super::color;
use super::convert::{unreadable, ConvertError, InputKind};
use image::metadata::Orientation;
use image::{DynamicImage, ImageBuffer, ImageDecoder, ImageReader};
use std::io::Cursor;

/// A decoded frame before colour conversion and rotation.
pub(super) struct Decoded {
    pub image: DynamicImage,
    pub icc: Option<Vec<u8>>,
    pub orientation: Orientation,
}

pub(super) struct HeaderInfo {
    /// More than 8 bits per sample.
    pub deep: bool,
    pub icc: Option<Vec<u8>>,
    pub orientation: Orientation,
}

/// The marker segments of a JPEG header, in order, up to the start of scan. Skips `0xFF`
/// fill bytes and the standalone markers (TEM, RSTn) that carry no length. Stops quietly on
/// a truncated or malformed header, so callers see fewer segments, never a panic.
pub(super) fn jpeg_segments(bytes: &[u8]) -> Vec<(u8, &[u8])> {
    walk_jpeg(bytes).0
}

/// The header segments, plus the offset where entropy-coded scan data starts (after the
/// first SOS segment), or `None` when the header never reaches a scan.
fn walk_jpeg(bytes: &[u8]) -> (Vec<(u8, &[u8])>, Option<usize>) {
    let mut out = Vec::new();
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return (out, None);
    }
    let mut i = 2;
    loop {
        if bytes.get(i) != Some(&0xFF) {
            return (out, None);
        }
        while bytes.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        let Some(&marker) = bytes.get(i + 1) else {
            return (out, None);
        };
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xD9 {
            return (out, None);
        }
        let Some(len) = bytes
            .get(i + 2..i + 4)
            .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
        else {
            return (out, None);
        };
        let Some(payload) = len.checked_sub(2).and_then(|n| bytes.get(i + 4..i + 4 + n)) else {
            return (out, None);
        };
        if marker == 0xDA {
            return (out, Some(i + 2 + len));
        }
        out.push((marker, payload));
        i += 2 + len;
    }
}

/// Component count of a JPEG's first frame header (any SOFn).
fn jpeg_components(bytes: &[u8]) -> Option<u8> {
    jpeg_segments(bytes)
        .into_iter()
        .find(|(m, _)| (0xC0..=0xCF).contains(m) && !matches!(m, 0xC4 | 0xC8 | 0xCC))
        .and_then(|(_, p)| p.get(5).copied())
}

/// True when the JPEG has an Adobe APP14 segment; such CMYK JPEGs store inverted samples.
pub(super) fn jpeg_has_adobe_marker(bytes: &[u8]) -> bool {
    jpeg_segments(bytes)
        .iter()
        .any(|(m, p)| *m == 0xEE && p.starts_with(b"Adobe"))
}

/// True for a four-component (CMYK or YCCK) JPEG. `image` would silently convert these to
/// RGB without colour management, so they take their own path.
pub(super) fn jpeg_is_cmyk(bytes: &[u8]) -> bool {
    jpeg_components(bytes) == Some(4)
}

fn reader(bytes: &[u8]) -> Result<impl ImageDecoder + '_, ConvertError> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(unreadable)?
        .into_decoder()
        .map_err(unreadable)
}

pub(super) fn header_info(bytes: &[u8]) -> Result<HeaderInfo, ConvertError> {
    let mut dec = reader(bytes)?;
    let icc = dec.icc_profile().map_err(unreadable)?;
    let orientation = dec.orientation().map_err(unreadable)?;
    let deep = dec.color_type().bytes_per_pixel() / dec.color_type().channel_count() > 1;
    Ok(HeaderInfo {
        deep,
        icc,
        orientation,
    })
}

pub(super) fn tiff_frame_count(bytes: &[u8]) -> Result<u32, ConvertError> {
    let mut dec = tiff::decoder::Decoder::new(Cursor::new(bytes)).map_err(unreadable)?;
    let mut frames = 1;
    while dec.more_images() {
        dec.next_image().map_err(unreadable)?;
        frames += 1;
    }
    Ok(frames)
}

/// A complete JPEG has an end-of-image marker after its scan data. The JPEG decoders fill a
/// truncated scan with grey instead of failing, so a missing EOI is the truncation signal.
/// The search starts at the scan: header payloads (an EXIF thumbnail with its own EOI, ICC
/// bytes) must not count. Entropy-coded data never contains `FF D9`, because a data `FF` is
/// stuffed as `FF 00`.
fn jpeg_is_complete(bytes: &[u8]) -> bool {
    walk_jpeg(bytes).1.is_some_and(|start| {
        bytes[start.min(bytes.len())..]
            .windows(2)
            .any(|w| w == [0xFF, 0xD9])
    })
}

pub(super) fn decode_frame(
    kind: InputKind,
    bytes: &[u8],
    frame: u32,
) -> Result<Decoded, ConvertError> {
    if kind == InputKind::Jpeg && !jpeg_is_complete(bytes) {
        return Err(unreadable("JPEG is truncated (no end-of-image marker)"));
    }
    match kind {
        InputKind::Tiff => decode_tiff(bytes, frame),
        InputKind::Jp2 => decode_jp2(bytes),
        InputKind::Jpeg if jpeg_is_cmyk(bytes) => {
            let orientation = reader(bytes)?.orientation().map_err(unreadable)?;
            Ok(Decoded {
                image: color::cmyk_jpeg_to_srgb(bytes)?,
                icc: None,
                orientation,
            })
        }
        _ => {
            let mut dec = reader(bytes)?;
            let icc = dec.icc_profile().map_err(unreadable)?;
            let orientation = dec.orientation().map_err(unreadable)?;
            let image = DynamicImage::from_decoder(dec).map_err(unreadable)?;
            Ok(Decoded {
                image,
                icc,
                orientation,
            })
        }
    }
}

const TIFF_ICC_TAG: u16 = 34675;

fn decode_tiff(bytes: &[u8], frame: u32) -> Result<Decoded, ConvertError> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    use tiff::ColorType;

    let mut dec = Decoder::new(Cursor::new(bytes)).map_err(unreadable)?;
    dec.seek_to_image(frame as usize).map_err(unreadable)?;
    let (w, h) = dec.dimensions().map_err(unreadable)?;
    let color = dec.colortype().map_err(unreadable)?;
    let orientation = dec
        .find_tag(Tag::Orientation)
        .map_err(unreadable)?
        .and_then(|v| v.into_u16().ok())
        .and_then(|v| Orientation::from_exif(v as u8))
        .unwrap_or(Orientation::NoTransforms);
    let icc = dec
        .find_tag(Tag::Unknown(TIFF_ICC_TAG))
        .map_err(unreadable)?
        .and_then(|v| v.into_u8_vec().ok());
    let data = dec.read_image().map_err(unreadable)?;
    let bad = || unreadable(format!("unsupported TIFF layout {color:?}"));
    let image = match (color, data) {
        (ColorType::RGB(8), DecodingResult::U8(v)) => {
            DynamicImage::ImageRgb8(ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
        }
        (ColorType::RGBA(8), DecodingResult::U8(v)) => {
            DynamicImage::ImageRgba8(ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
        }
        (ColorType::Gray(8), DecodingResult::U8(v)) => {
            DynamicImage::ImageLuma8(ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
        }
        (ColorType::RGB(16), DecodingResult::U16(v)) => {
            DynamicImage::ImageRgb16(ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
        }
        (ColorType::RGBA(16), DecodingResult::U16(v)) => {
            DynamicImage::ImageRgba16(ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
        }
        (ColorType::Gray(16), DecodingResult::U16(v)) => {
            DynamicImage::ImageLuma16(ImageBuffer::from_raw(w, h, v).ok_or_else(bad)?)
        }
        (ColorType::CMYK(8), DecodingResult::U8(v)) => {
            color::cmyk_pixels_to_srgb(w, h, &v, icc.as_deref(), false)?
        }
        _ => return Err(bad()),
    };
    // A CMYK frame is already sRGB after its own transform.
    let icc = if matches!(color, ColorType::CMYK(_)) {
        None
    } else {
        icc
    };
    Ok(Decoded {
        image,
        icc,
        orientation,
    })
}

fn decode_jp2(bytes: &[u8]) -> Result<Decoded, ConvertError> {
    use jpeg2k::ColorSpace;
    let img = jpeg2k::Image::from_bytes(bytes).map_err(unreadable)?;
    let comps = img.components();
    let (w, h) = (img.width(), img.height());
    if comps.is_empty() || comps.iter().any(|c| c.width() != w || c.height() != h) {
        return Err(unreadable(
            "JPEG 2000 with subsampled or missing components",
        ));
    }
    if comps
        .iter()
        .any(|c| c.is_signed() || c.precision() == 0 || c.precision() > 16)
    {
        return Err(unreadable(
            "JPEG 2000 with signed or unusual-precision samples",
        ));
    }
    // Only RGB and grey are drawn; YCC and CMYK colour spaces are rejected, and a 4-component
    // image is taken as RGBA only when the file says sRGB.
    let space = img.color_space();
    let rgb_like = matches!(
        space,
        ColorSpace::SRGB | ColorSpace::Unspecified | ColorSpace::Unknown
    );
    let grey_like = matches!(
        space,
        ColorSpace::Gray | ColorSpace::Unspecified | ColorSpace::Unknown
    );
    let n = usize::try_from(u64::from(w) * u64::from(h)).map_err(unreadable)?;
    // Scale every sample to 8 bits from the component's own precision.
    let to8 = |c: &jpeg2k::ImageComponent| -> Vec<u8> {
        let max = ((1u32 << c.precision()) - 1) as f32;
        c.data()
            .iter()
            .map(|&s| ((s.max(0) as f32) * 255.0 / max).round().clamp(0.0, 255.0) as u8)
            .collect()
    };
    let planes: Vec<Vec<u8>> = comps.iter().map(to8).collect();
    let interleave = |k: usize| -> Vec<u8> {
        let mut out = Vec::with_capacity(n * k);
        for i in 0..n {
            for p in planes.iter().take(k) {
                out.push(p[i]);
            }
        }
        out
    };
    let bad = || unreadable("JPEG 2000 sample buffer has the wrong size");
    let image = match planes.len() {
        1 | 2 if grey_like => {
            DynamicImage::ImageLuma8(ImageBuffer::from_raw(w, h, interleave(1)).ok_or_else(bad)?)
        }
        3 if rgb_like => {
            DynamicImage::ImageRgb8(ImageBuffer::from_raw(w, h, interleave(3)).ok_or_else(bad)?)
        }
        4 if matches!(space, ColorSpace::SRGB) => {
            DynamicImage::ImageRgba8(ImageBuffer::from_raw(w, h, interleave(4)).ok_or_else(bad)?)
        }
        k => {
            return Err(unreadable(format!(
                "JPEG 2000 with {k} components in colour space {space:?}"
            )))
        }
    };
    Ok(Decoded {
        image,
        icc: None,
        orientation: Orientation::NoTransforms,
    })
}
