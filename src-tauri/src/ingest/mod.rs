//! Ingest: turning a folder of scans into a book package. This chunk holds input-format
//! conversion; folder sorting, atomic package creation, and loading land in later M1 chunks.

mod color;
pub mod convert;
mod decode;

#[cfg(test)]
mod convert_tests;
