//! PDF pages through pdfium (M1.5 plan, "Rasteriser" and "Rendering rule").
//!
//! The pdfium shared library is loaded at runtime from a directory, once per process. A
//! missing or broken library fails PDF work only. Nothing else in ingest depends on it.

use image::RgbImage;
use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
};
use std::path::Path;
use std::sync::OnceLock;

/// Render resolution before the caps apply.
const RENDER_DPI: f32 = 300.0;
/// PDF user-space units per inch.
const POINTS_PER_INCH: f32 = 72.0;
/// Longest allowed edge of a rendered page, in pixels.
pub const MAX_LONG_EDGE: u32 = 4000;
/// Largest allowed rendered page area, in pixels.
pub const MAX_PIXELS: u64 = 12_000_000;

/// Why a PDF could not be opened or rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// The pdfium library is not loaded, or it failed to load.
    LibraryUnavailable(String),
    /// The PDF needs a password, or uses a security handler pdfium cannot open.
    Encrypted,
    /// The PDF has no pages.
    NoPages,
    /// The file is damaged, or a page cannot be rendered.
    Unreadable(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LibraryUnavailable(reason) => write!(f, "pdfium is unavailable: {reason}"),
            Self::Encrypted => write!(f, "the PDF is password-protected"),
            Self::NoPages => write!(f, "the PDF has no pages"),
            Self::Unreadable(reason) => write!(f, "unreadable PDF: {reason}"),
        }
    }
}

impl std::error::Error for PdfError {}

fn unreadable(reason: impl std::fmt::Display) -> PdfError {
    PdfError::Unreadable(reason.to_string())
}

/// The one pdfium instance. It keeps the load error, so a failed load is reported the same
/// way on every later call.
static LIBRARY: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// Loads the pdfium library from `dir` and keeps it for the rest of the process.
///
/// Only the first call has an effect. A later call reports the first call's outcome.
pub fn bind(dir: &Path) -> Result<(), PdfError> {
    LIBRARY
        .get_or_init(|| load_library(dir))
        .as_ref()
        .map(|_| ())
        .map_err(|reason| PdfError::LibraryUnavailable(reason.clone()))
}

/// Loads the pdfium library from `dir`, which holds the platform's library file.
pub(super) fn load_library(dir: &Path) -> Result<Pdfium, String> {
    let path = Pdfium::pdfium_platform_library_name_at_path(dir);
    Pdfium::bind_to_library(&path)
        .map(Pdfium::new)
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// Whether the library is loaded. It does not try to load it.
pub fn is_available() -> bool {
    matches!(LIBRARY.get(), Some(Ok(_)))
}

/// Where tests look for the library: `PDFIUM_DYNAMIC_LIB_PATH`, else the fetch script's
/// directory.
#[cfg(test)]
fn default_dir() -> std::path::PathBuf {
    std::env::var_os("PDFIUM_DYNAMIC_LIB_PATH").map_or_else(
        || std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/pdfium")),
        std::path::PathBuf::from,
    )
}

/// The loaded library. Tests load it on first use. The app loads it at startup with `bind`.
fn library() -> Result<&'static Pdfium, PdfError> {
    #[cfg(test)]
    LIBRARY.get_or_init(|| load_library(&default_dir()));
    match LIBRARY.get() {
        Some(Ok(pdfium)) => Ok(pdfium),
        Some(Err(reason)) => Err(PdfError::LibraryUnavailable(reason.clone())),
        None => Err(PdfError::LibraryUnavailable("not loaded".into())),
    }
}

/// An open PDF. Pages are read by zero-based index.
pub struct PdfDoc {
    doc: PdfDocument<'static>,
}

/// Opens the PDF at `path`. It refuses an encrypted PDF, a PDF with no pages, and a file
/// pdfium cannot parse.
pub fn open(path: &Path) -> Result<PdfDoc, PdfError> {
    let doc = library()?
        .load_pdf_from_file(path, None)
        .map_err(|err| match err {
            PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::PasswordError | PdfiumInternalError::SecurityError,
            ) => PdfError::Encrypted,
            other => unreadable(format!("{other:?}")),
        })?;
    if doc.pages().is_empty() {
        return Err(PdfError::NoPages);
    }
    Ok(PdfDoc { doc })
}

impl PdfDoc {
    /// Number of pages in the document.
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.doc.pages().len()).unwrap_or(0)
    }

    /// Renders one page to an upright sRGB image, following the rendering rule: 300 DPI,
    /// scaled down to the caps, annotations off, flattened onto white.
    pub fn render_page(&self, index: u32) -> Result<RgbImage, PdfError> {
        let page = self.page(index)?;
        let (width, height) = render_size(page.width().value, page.height().value)?;
        let config = PdfRenderConfig::new()
            // Both targets are set, so pdfium-render scales each axis to exactly this size.
            .set_target_size(
                i32::try_from(width).map_err(unreadable)?,
                i32::try_from(height).map_err(unreadable)?,
            )
            .set_clear_color(PdfColor::WHITE)
            .render_annotations(false)
            .render_form_data(false);
        let bitmap = page.render_with_config(&config).map_err(unreadable)?;
        let rgba = bitmap.as_rgba_bytes();
        RgbImage::from_raw(width, height, flatten_on_white(&rgba))
            .ok_or_else(|| unreadable("pdfium returned a bitmap of an unexpected size"))
    }

    /// The plain text of one page. An empty string means the page has no text.
    pub fn page_text(&self, index: u32) -> Result<String, PdfError> {
        let page = self.page(index)?;
        let text = page.text().map_err(unreadable)?;
        Ok(text.all())
    }

    fn page(&self, index: u32) -> Result<pdfium_render::prelude::PdfPage<'static>, PdfError> {
        let index = i32::try_from(index).map_err(unreadable)?;
        self.doc.pages().get(index).map_err(unreadable)
    }
}

/// Pixel size for a page of `width_pt` by `height_pt` points: 300 DPI, scaled down (never up)
/// until the long edge is at most `MAX_LONG_EDGE` and the area at most `MAX_PIXELS`.
///
/// A page that rounds below 1 px on either side is unreadable.
pub fn render_size(width_pt: f32, height_pt: f32) -> Result<(u32, u32), PdfError> {
    if !(width_pt.is_finite() && height_pt.is_finite() && width_pt > 0.0 && height_pt > 0.0) {
        return Err(unreadable("the page has no size"));
    }
    let scale = f64::from(RENDER_DPI / POINTS_PER_INCH);
    let (w, h) = (f64::from(width_pt) * scale, f64::from(height_pt) * scale);
    let edge_factor = f64::from(MAX_LONG_EDGE) / w.max(h);
    let area_factor = (MAX_PIXELS as f64 / (w * h)).sqrt();
    let factor = edge_factor.min(area_factor).min(1.0);
    let (w, h) = (w * factor, h * factor);
    let (mut pw, mut ph) = (w.round(), h.round());
    // Rounding up can step over a cap by a pixel. Rounding down cannot.
    if pw.max(ph) > f64::from(MAX_LONG_EDGE) || pw * ph > MAX_PIXELS as f64 {
        (pw, ph) = (w.floor(), h.floor());
    }
    if pw < 1.0 || ph < 1.0 {
        return Err(unreadable("the page is smaller than one pixel"));
    }
    // Both values are within 1..=MAX_LONG_EDGE here.
    Ok((pw as u32, ph as u32))
}

/// Drops alpha from RGBA bytes, blending each pixel over white.
fn flatten_on_white(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.chunks_exact(4) {
        let alpha = u32::from(px[3]);
        for &channel in &px[..3] {
            let blended = (u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
            out.push(blended as u8);
        }
    }
    out
}
