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
// Phase 4 Chunk 3's job creates the run directory; startup sweeps stale ones.
#[allow(dead_code)]
pub(crate) mod run_dir;

#[cfg(test)]
mod tests;
