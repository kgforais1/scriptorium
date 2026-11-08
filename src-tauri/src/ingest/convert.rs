//! Input formats and display-image conversion (M1 plan, "Input formats (normative for M1)").
//!
//! Every page's display image is an 8-bit sRGB JPEG, PNG, or WebP, stored upright. An input
//! that already meets that is copied as-is. Anything else is converted, and the caller keeps
//! the untouched original under `originals/`.

use super::{color, decode, pdf};
use crate::book_format::SourceFormat;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder};
use std::path::Path;

/// How far into a file `%PDF-` may start.
const PDF_HEADER_WINDOW: usize = 1024;

/// JPEG quality for converted copies (plan: at least 92).
pub const JPEG_QUALITY: u8 = 92;

/// A supported input format, chosen by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// A JPEG image.
    Jpeg,
    /// A PNG image.
    Png,
    /// A WebP image.
    Webp,
    /// A BMP image.
    Bmp,
    /// A GIF image.
    Gif,
    /// A TIFF image.
    Tiff,
    /// A JPEG 2000 image.
    Jp2,
    /// A PDF document. Each page becomes one frame.
    Pdf,
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
            "pdf" => Self::Pdf,
            _ => return None,
        })
    }

    /// The format's name as a user reads it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
            Self::Bmp => "BMP",
            Self::Gif => "GIF",
            Self::Tiff => "TIFF",
            Self::Jp2 => "JPEG 2000",
            Self::Pdf => "PDF",
        }
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
            Self::Pdf => SourceFormat::Pdf,
        }
    }

    /// The webview can show this container directly (subject to the conversion rules).
    fn is_display_format(self) -> bool {
        matches!(self, Self::Jpeg | Self::Png | Self::Webp)
    }

    /// Display extension for a converted copy: JPEG for photographic sources, else PNG.
    fn converted_ext(self) -> &'static str {
        match self {
            Self::Jpeg | Self::Tiff | Self::Jp2 | Self::Pdf => "jpg",
            Self::Png | Self::Webp | Self::Bmp | Self::Gif => "png",
        }
    }

    /// Display extension when the input is copied as-is.
    fn passthrough_ext(self) -> &'static str {
        match self {
            Self::Jpeg | Self::Pdf => "jpg",
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
    /// A PDF needs a password. Maps to the ingest error `EncryptedPdf`.
    EncryptedPdf,
    /// The pdfium library is not loaded. Maps to the ingest error `PdfiumUnavailable`.
    PdfiumUnavailable(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat => write!(f, "not a supported input format"),
            Self::UnreadableFile(reason) => write!(f, "unreadable file: {reason}"),
            Self::EncryptedPdf => write!(f, "the PDF is password-protected"),
            Self::PdfiumUnavailable(reason) => write!(f, "pdfium is unavailable: {reason}"),
        }
    }
}

impl std::error::Error for ConvertError {}

pub(super) fn unreadable(reason: impl std::fmt::Display) -> ConvertError {
    ConvertError::UnreadableFile(reason.to_string())
}

impl From<pdf::PdfError> for ConvertError {
    fn from(err: pdf::PdfError) -> Self {
        match err {
            pdf::PdfError::LibraryUnavailable(reason) => Self::PdfiumUnavailable(reason),
            pdf::PdfError::Encrypted => Self::EncryptedPdf,
            pdf::PdfError::NoPages => unreadable("the PDF has no pages"),
            pdf::PdfError::Unreadable(reason) => Self::UnreadableFile(reason),
        }
    }
}

/// What ingest needs to know about one input file before it writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    /// The detected input format.
    pub kind: InputKind,
    /// Number of pages this file yields (TIFF directories or PDF pages; 1 otherwise, GIF
    /// included).
    pub frames: u32,
    /// True when the display images are converted copies, so the original must be kept.
    pub needs_conversion: bool,
    /// Display extension of every page from this file.
    pub display_ext: &'static str,
}

/// One page's display image.
#[derive(Debug, Clone)]
pub struct DisplayImage {
    /// File extension of the display image.
    pub ext: &'static str,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Encoded bytes, or `None` when the input file itself is the display image.
    pub bytes: Option<Vec<u8>>,
}

/// An input file read once, with its inspection. Ingest opens each file once and asks for
/// every frame from the same buffer, so a multi-page TIFF is not re-read per page.
pub struct Source {
    /// What inspection learned about the file.
    pub info: Inspection,
    bytes: Vec<u8>,
    /// The open document, for a PDF. Its pages are rendered from this, never from `bytes`.
    pdf: Option<pdf::PdfDoc>,
}

impl Source {
    /// The file's bytes as read by `open`. Ingest writes originals and copied pages from
    /// these, so every page and original comes from the same read that was inspected.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// The input format the bytes actually hold, from their signature.
fn sniff(bytes: &[u8]) -> Option<InputKind> {
    let at = |off: usize, sig: &[u8]| bytes.get(off..off + sig.len()) == Some(sig);
    Some(if at(0, &[0xFF, 0xD8, 0xFF]) {
        InputKind::Jpeg
    } else if at(0, b"\x89PNG\r\n\x1a\n") {
        InputKind::Png
    } else if at(0, b"RIFF") && at(8, b"WEBP") {
        InputKind::Webp
    } else if at(0, b"BM") {
        InputKind::Bmp
    } else if at(0, b"GIF87a") || at(0, b"GIF89a") {
        InputKind::Gif
    } else if at(0, b"II*\0") || at(0, b"MM\0*") || at(0, b"II+\0") || at(0, b"MM\0+") {
        InputKind::Tiff
    } else if at(0, b"\0\0\0\x0cjP  \r\n\x87\n") || at(0, &[0xFF, 0x4F, 0xFF, 0x51]) {
        InputKind::Jp2
    } else if has_pdf_header(bytes) {
        InputKind::Pdf
    } else {
        return None;
    })
}

/// Whether `%PDF-` starts within the first 1024 bytes. Readers such as Acrobat accept leading
/// junk there, so some valid PDFs do not start at byte 0. The slice runs four bytes past the
/// window, so a header starting at byte 1023 is still found.
fn has_pdf_header(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(PDF_HEADER_WINDOW + 4)]
        .windows(5)
        .any(|w| w == b"%PDF-")
}

/// Read and classify a file, deciding whether it must be converted. Decodes only what the
/// rules need (a JPEG's component count, bit depth, ICC profile, orientation).
pub fn open(path: &Path) -> Result<Source, ConvertError> {
    let kind = InputKind::from_path(path).ok_or(ConvertError::UnsupportedFormat)?;
    let bytes = std::fs::read(path).map_err(unreadable)?;
    // The extension decides the format, so the contents must agree: a TIFF renamed `.png`
    // would otherwise be copied as a "PNG" the webview cannot show.
    match sniff(&bytes) {
        Some(found) if found == kind => {}
        Some(found) => {
            return Err(unreadable(format!(
                "the file is {} but its extension says {}; rename it to match",
                found.label(),
                kind.label()
            )))
        }
        None => {
            return Err(unreadable(
                "the file's contents are not a recognized image format",
            ))
        }
    }
    if kind == InputKind::Pdf {
        return open_pdf(path, bytes);
    }
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
    Ok(Source {
        info,
        bytes,
        pdf: None,
    })
}

/// A PDF is opened once through pdfium. Every page is converted, so the PDF is always kept as
/// an original. It is opened from `path`, while `bytes` stay the source of the checksum.
fn open_pdf(path: &Path, bytes: Vec<u8>) -> Result<Source, ConvertError> {
    let doc = pdf::open(path)?;
    let info = Inspection {
        kind: InputKind::Pdf,
        frames: doc.page_count(),
        needs_conversion: true,
        display_ext: InputKind::Pdf.converted_ext(),
    };
    Ok(Source {
        info,
        bytes,
        pdf: Some(doc),
    })
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
    /// The plain text of PDF page `frame`, or `None` for an image. A PDF page without text
    /// gives an empty string. A page whose text pdfium cannot read counts as having none.
    pub fn page_text(&self, frame: u32) -> Option<String> {
        let doc = self.pdf.as_ref()?;
        Some(doc.page_text(frame).unwrap_or_default())
    }

    /// Produce the display image for frame `frame` (0-based).
    pub fn frame(&self, frame: u32) -> Result<DisplayImage, ConvertError> {
        let info = &self.info;
        if frame >= info.frames {
            return Err(unreadable(format!("frame {frame} of {}", info.frames)));
        }
        if let Some(doc) = &self.pdf {
            return render_pdf_frame(doc, frame, info.display_ext);
        }
        if !info.needs_conversion {
            // The file is its own display image. Decode it anyway, so a damaged file is
            // `UnreadableFile` at ingest rather than a blank page in the reader.
            let img = decode::decode_frame(info.kind, &self.bytes, frame)?.image;
            let (width, height) = (img.width(), img.height());
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

/// One PDF page as a JPEG. pdfium renders in sRGB, so no colour conversion follows.
fn render_pdf_frame(
    doc: &pdf::PdfDoc,
    frame: u32,
    ext: &'static str,
) -> Result<DisplayImage, ConvertError> {
    let img = DynamicImage::ImageRgb8(doc.render_page(frame)?);
    Ok(DisplayImage {
        ext,
        width: img.width(),
        height: img.height(),
        bytes: Some(encode(&img, ext)?),
    })
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
