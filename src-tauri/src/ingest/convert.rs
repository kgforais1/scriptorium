//! Input formats and display-image conversion (M1 plan, "Input formats (normative for M1)").
//!
//! Every page's display image is an 8-bit sRGB JPEG, PNG, or WebP, stored upright. An input
//! that already meets that is copied as-is. Anything else is converted, and the caller keeps
//! the untouched original under `originals/`.

use super::{color, decode};
use crate::book_format::SourceFormat;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder};
use std::path::Path;

/// JPEG quality for converted copies (plan: at least 92).
pub const JPEG_QUALITY: u8 = 92;

/// A supported input format, chosen by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Jpeg,
    Png,
    Webp,
    Bmp,
    Gif,
    Tiff,
    Jp2,
}

impl InputKind {
    /// The kind for a file extension (case-insensitive), or `None` when ingest skips it.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "webp" => Self::Webp,
            "bmp" => Self::Bmp,
            "gif" => Self::Gif,
            "tif" | "tiff" => Self::Tiff,
            "jp2" | "j2k" | "jpf" => Self::Jp2,
            _ => return None,
        })
    }

    /// The `sources[].format` value recorded when an original is kept.
    pub fn source_format(self) -> SourceFormat {
        match self {
            Self::Jpeg => SourceFormat::Jpeg,
            Self::Png => SourceFormat::Png,
            Self::Webp => SourceFormat::Webp,
            Self::Bmp => SourceFormat::Bmp,
            Self::Gif => SourceFormat::Gif,
            Self::Tiff => SourceFormat::Tiff,
            Self::Jp2 => SourceFormat::Jp2,
        }
    }

    /// The webview can show this container directly (subject to the conversion rules).
    fn is_display_format(self) -> bool {
        matches!(self, Self::Jpeg | Self::Png | Self::Webp)
    }

    /// Display extension for a converted copy: JPEG for photographic sources, else PNG.
    fn converted_ext(self) -> &'static str {
        match self {
            Self::Jpeg | Self::Tiff | Self::Jp2 => "jpg",
            Self::Png | Self::Webp | Self::Bmp | Self::Gif => "png",
        }
    }

    /// Display extension when the input is copied as-is.
    fn passthrough_ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            _ => "webp",
        }
    }
}

/// A file that cannot become a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// The extension is not an M1 input format.
    UnsupportedFormat,
    /// The file could not be decoded or breaks a conversion rule (for example CMYK with no
    /// ICC profile). Maps to the ingest error `UnreadableFile`.
    UnreadableFile(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat => write!(f, "not a supported input format"),
            Self::UnreadableFile(reason) => write!(f, "unreadable file: {reason}"),
        }
    }
}

impl std::error::Error for ConvertError {}

pub(super) fn unreadable(reason: impl std::fmt::Display) -> ConvertError {
    ConvertError::UnreadableFile(reason.to_string())
}

/// What ingest needs to know about one input file before it writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub kind: InputKind,
    /// Number of pages this file yields (TIFF directories; 1 otherwise, GIF included).
    pub frames: u32,
    /// True when the display images are converted copies, so the original must be kept.
    pub needs_conversion: bool,
    /// Display extension of every page from this file.
    pub display_ext: &'static str,
}

/// One page's display image.
#[derive(Debug, Clone)]
pub struct DisplayImage {
    pub ext: &'static str,
    pub width: u32,
    pub height: u32,
    /// Encoded bytes, or `None` when the input file itself is the display image.
    pub bytes: Option<Vec<u8>>,
}

/// An input file read once, with its inspection. Ingest opens each file once and asks for
/// every frame from the same buffer, so a multi-page TIFF is not re-read per page.
pub struct Source {
    pub info: Inspection,
    bytes: Vec<u8>,
}

/// Read and classify a file, deciding whether it must be converted. Decodes only what the
/// rules need (a JPEG's component count, bit depth, ICC profile, orientation).
pub fn open(path: &Path) -> Result<Source, ConvertError> {
    let kind = InputKind::from_path(path).ok_or(ConvertError::UnsupportedFormat)?;
    let bytes = std::fs::read(path).map_err(unreadable)?;
    let frames = match kind {
        InputKind::Tiff => decode::tiff_frame_count(&bytes)?,
        _ => 1,
    };
    let needs_conversion = !kind.is_display_format() || needs_display_conversion(kind, &bytes)?;
    let display_ext = if needs_conversion {
        kind.converted_ext()
    } else {
        kind.passthrough_ext()
    };
    let info = Inspection {
        kind,
        frames,
        needs_conversion,
        display_ext,
    };
    Ok(Source { info, bytes })
}

/// Classify a file without keeping its bytes.
pub fn inspect(path: &Path) -> Result<Inspection, ConvertError> {
    open(path).map(|s| s.info)
}

/// The display image for frame `frame` (0-based) of `path`. Convenience for one-off use;
/// ingest should `open` once and call [`Source::frame`] per page.
pub fn display_frame(path: &Path, frame: u32) -> Result<DisplayImage, ConvertError> {
    open(path)?.frame(frame)
}

/// A JPEG, PNG, or WebP is converted when it is CMYK, deeper than 8 bits, carries a
/// non-sRGB ICC profile, or has an EXIF orientation other than 1.
fn needs_display_conversion(kind: InputKind, bytes: &[u8]) -> Result<bool, ConvertError> {
    if kind == InputKind::Jpeg && decode::jpeg_is_cmyk(bytes) {
        return Ok(true);
    }
    let info = decode::header_info(bytes)?;
    let icc_ok = match &info.icc {
        None => true,
        Some(icc) => color::is_srgb_equivalent(icc)?,
    };
    Ok(info.deep || !icc_ok || info.orientation != image::metadata::Orientation::NoTransforms)
}

impl Source {
    /// Produce the display image for frame `frame` (0-based).
    pub fn frame(&self, frame: u32) -> Result<DisplayImage, ConvertError> {
        let info = &self.info;
        if frame >= info.frames {
            return Err(unreadable(format!("frame {frame} of {}", info.frames)));
        }
        if !info.needs_conversion {
            // The file is its own display image; only its size is needed.
            let (width, height) = decode::header_info(&self.bytes)?.dimensions;
            return Ok(DisplayImage {
                ext: info.display_ext,
                width,
                height,
                bytes: None,
            });
        }
        let decoded = decode::decode_frame(info.kind, &self.bytes, frame)?;
        let mut img = color::to_srgb8(decoded.image, decoded.icc.as_deref())?;
        img.apply_orientation(decoded.orientation);
        let encoded = encode(&img, info.display_ext)?;
        Ok(DisplayImage {
            ext: info.display_ext,
            width: img.width(),
            height: img.height(),
            bytes: Some(encoded),
        })
    }
}

fn encode(img: &DynamicImage, ext: &str) -> Result<Vec<u8>, ConvertError> {
    let mut out = Vec::new();
    if ext == "jpg" {
        let mut enc = JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
        // Grey scans stay one channel, a third of the size of an RGB copy.
        if img.color() == image::ColorType::L8 {
            enc.encode_image(&img.to_luma8())
        } else {
            enc.encode_image(&img.to_rgb8())
        }
        .map_err(unreadable)?;
    } else {
        let (w, h) = (img.width(), img.height());
        let enc = PngEncoder::new(&mut out);
        match img.color() {
            image::ColorType::L8 => {
                enc.write_image(img.as_bytes(), w, h, image::ExtendedColorType::L8)
            }
            c if c.has_alpha() => enc.write_image(
                img.to_rgba8().as_raw(),
                w,
                h,
                image::ExtendedColorType::Rgba8,
            ),
            _ => enc.write_image(img.to_rgb8().as_raw(), w, h, image::ExtendedColorType::Rgb8),
        }
        .map_err(unreadable)?;
    }
    Ok(out)
}
