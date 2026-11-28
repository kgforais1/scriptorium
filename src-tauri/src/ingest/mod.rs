//! Ingest: turning a folder of scans into a book package. This chunk holds input-format
//! conversion; folder sorting, atomic package creation, and loading land in later M1 chunks.

mod color;
pub mod convert;
mod decode;
pub mod pdf;

// The OCR input step reuses ingest's 150 MP page limit. `decode` stays private; only the
// constant is shared.
pub(crate) use decode::JP2_MAX_PIXELS as MAX_PAGE_PIXELS;

#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod pdf_tests;
