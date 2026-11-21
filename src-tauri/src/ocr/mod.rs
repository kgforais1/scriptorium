//! OCR: the engine interface and the `ocr/` store (M2a, `docs/book-format.md`).
//!
//! Phase 3 adds the Tesseract engine and Phase 4 the job and commands, which are the main
//! consumers of what is here.

// Phase 3's Tesseract engine and Phase 4's job are the consumers of the engine interface.
#[allow(dead_code)]
pub(crate) mod engine;
#[cfg(test)]
pub(crate) mod fake;
pub(crate) mod store;
// Phase 4's job is the consumer.
#[allow(dead_code)]
pub(crate) mod tesseract;

/// The engine binary or its language data is missing or will not start.
///
/// The parent plan's `OcrUnavailable` becomes a `BookError` kind in Phase 4, next to
/// `PdfiumUnavailable`. This is the minimal form `TesseractEngine::locate` returns until then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcrUnavailable {
    /// What could not be found, and where it was looked for.
    pub(crate) reason: String,
}

impl OcrUnavailable {
    /// Build the error from `reason`.
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for OcrUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for OcrUnavailable {}

#[cfg(test)]
mod tests;
