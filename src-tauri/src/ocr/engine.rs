//! The engine interface: what an OCR engine returns, and how its pixel boxes become the
//! normalised boxes stored in `ocr/<NNNN>.json`.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Stable engine id and version, recorded with every result, e.g. `tesseract 5.5.1 eng-fast`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineId(pub String);

/// A language the engine reads, by its engine code, e.g. `eng`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lang(pub String);

/// A box in pixels of the image the engine read, origin top-left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelBox {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// One recognised word.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// The word's text.
    pub text: String,
    /// Its box.
    pub bbox: PixelBox,
    /// Confidence from 0 to 1.
    pub conf: f32,
}

/// One line of words.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The line's box.
    pub bbox: PixelBox,
    /// The paragraph the line belongs to. A change between lines starts a new paragraph.
    pub paragraph: u32,
    /// Its words, in reading order.
    pub words: Vec<Word>,
}

/// What an engine read from one image.
#[derive(Debug, Clone, PartialEq)]
pub struct PageOcr {
    /// Pixel width of the image the engine read.
    pub width: u32,
    /// Pixel height of the image the engine read.
    pub height: u32,
    /// Lines in reading order.
    pub lines: Vec<Line>,
}

/// Why an engine returned no result for a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// `langs` was empty. A caller bug, never shown to a reader.
    NoLanguage,
    /// The stop flag was set.
    Cancelled,
    /// The deadline passed.
    Timeout,
    /// The engine failed or its output could not be read.
    Engine(String),
}

/// An OCR engine. Implementations must be safe to call from several workers at once.
pub trait OcrEngine: Send + Sync {
    /// Stable id and version, recorded with every result, e.g. `tesseract 5.5.1 eng-fast`.
    fn id(&self) -> EngineId;
    /// Returns `OcrError::Cancelled` soon after `stop` is set, and `OcrError::Timeout`
    /// once `deadline` passes.
    fn recognize(
        &self,
        image: &Path,
        langs: &[Lang],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<PageOcr, OcrError>;
}

/// A word as stored: normalised box and confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredWord {
    /// The word's text.
    pub text: String,
    /// `[x, y, w, h]` from 0 to 1.
    pub bbox: [f64; 4],
    /// Confidence from 0 to 1.
    pub conf: f64,
}

/// A line as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredLine {
    /// `[x, y, w, h]` from 0 to 1.
    pub bbox: [f64; 4],
    /// Its words.
    pub words: Vec<StoredWord>,
}

/// Round to 5 decimal places, under a tenth of a pixel at 4000 px.
fn round5(v: f64) -> f64 {
    (v * 100_000.0).round() / 100_000.0
}

/// Clamp a confidence to 0..1 and round it. `NaN` becomes 0, because JSON has no `NaN` and the
/// schema would refuse the `null` serde writes for it.
fn normalise_conf(conf: f32) -> f64 {
    let c = f64::from(conf);
    if c.is_nan() {
        0.0
    } else {
        round5(c.clamp(0.0, 1.0))
    }
}

/// Normalise a pixel box by the image size to `[x, y, w, h]` in 0..1. A box that runs past an
/// edge is clamped to it, so `x + w` and `y + h` never exceed 1.
pub fn normalise_box(b: PixelBox, width: u32, height: u32) -> [f64; 4] {
    let axis = |start: u32, len: u32, size: u32| {
        let size = f64::from(size.max(1));
        let a = round5((f64::from(start) / size).clamp(0.0, 1.0));
        let b = round5(((f64::from(start) + f64::from(len)) / size).clamp(0.0, 1.0));
        (a, round5(b - a))
    };
    let (x, w) = axis(b.x, b.w, width);
    let (y, h) = axis(b.y, b.h, height);
    [x, y, w, h]
}

/// Normalise a page's lines, and build its text: lines joined with `\n`, paragraphs with
/// `\n\n`.
pub fn normalise(page: &PageOcr) -> (String, Vec<StoredLine>) {
    let mut text = String::new();
    let mut prev: Option<u32> = None;
    let lines = page
        .lines
        .iter()
        .filter_map(|line| {
            // The schema refuses a word with empty text, so drop those first. A line left
            // with no words has nothing to show and would leave a stray separator.
            let words: Vec<StoredWord> = line
                .words
                .iter()
                .filter(|w| !w.text.trim().is_empty())
                .map(|w| StoredWord {
                    text: w.text.clone(),
                    bbox: normalise_box(w.bbox, page.width, page.height),
                    conf: normalise_conf(w.conf),
                })
                .collect();
            if words.is_empty() {
                return None;
            }
            if let Some(p) = prev {
                text.push_str(if p == line.paragraph { "\n" } else { "\n\n" });
            }
            prev = Some(line.paragraph);
            let joined: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
            text.push_str(&joined.join(" "));
            Some(StoredLine {
                bbox: normalise_box(line.bbox, page.width, page.height),
                words,
            })
        })
        .collect();
    (text, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(x: u32, y: u32, w: u32, h: u32) -> PixelBox {
        PixelBox { x, y, w, h }
    }

    #[test]
    fn boxes_are_normalised_and_rounded() {
        assert_eq!(
            normalise_box(px(100, 200, 600, 30), 1000, 1000),
            [0.1, 0.2, 0.6, 0.03]
        );
        assert_eq!(
            normalise_box(px(1, 1, 1, 1), 3, 3),
            [0.33333, 0.33333, 0.33334, 0.33334]
        );
    }

    #[test]
    fn boxes_are_clamped_at_both_edges() {
        // At the origin, a zero box stays at zero.
        assert_eq!(
            normalise_box(px(0, 0, 0, 0), 800, 600),
            [0.0, 0.0, 0.0, 0.0]
        );
        // A box that fills the image is exactly 0..1.
        assert_eq!(
            normalise_box(px(0, 0, 800, 600), 800, 600),
            [0.0, 0.0, 1.0, 1.0]
        );
        // A box running past the far edge is cut at 1.
        assert_eq!(
            normalise_box(px(700, 500, 300, 300), 800, 600),
            [0.875, 0.83333, 0.125, 0.16667]
        );
        // A box starting past the edge collapses onto it.
        assert_eq!(
            normalise_box(px(900, 700, 10, 10), 800, 600),
            [1.0, 1.0, 0.0, 0.0]
        );
        // A zero-sized image never divides by zero.
        assert_eq!(normalise_box(px(0, 0, 5, 5), 0, 0), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn text_joins_lines_and_paragraphs() {
        let word = |t: &str| Word {
            text: t.into(),
            bbox: px(0, 0, 1, 1),
            conf: 1.5,
        };
        let line = |paragraph, words: &[&str]| Line {
            bbox: px(0, 0, 10, 1),
            paragraph,
            words: words.iter().map(|w| word(w)).collect(),
        };
        let page = PageOcr {
            width: 10,
            height: 10,
            lines: vec![
                line(1, &["Once", "upon"]),
                line(1, &["a"]),
                line(2, &["time"]),
            ],
        };
        let (text, lines) = normalise(&page);
        assert_eq!(text, "Once upon\na\n\ntime");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].words[0].conf, 1.0, "confidence is clamped");
        assert_eq!(
            normalise(&PageOcr {
                width: 1,
                height: 1,
                lines: vec![]
            })
            .0,
            ""
        );
    }

    #[test]
    fn lines_without_words_are_dropped() {
        let line = |words: Vec<Word>| Line {
            bbox: px(0, 0, 10, 1),
            paragraph: 1,
            words,
        };
        let a = Word {
            text: "a".into(),
            bbox: px(0, 0, 1, 1),
            conf: 0.5,
        };
        let page = PageOcr {
            width: 10,
            height: 10,
            lines: vec![line(vec![]), line(vec![a])],
        };
        let (text, lines) = normalise(&page);
        assert_eq!(text, "a");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn empty_words_are_dropped_and_nan_confidence_is_zero() {
        let word = |text: &str, conf: f32| Word {
            text: text.into(),
            bbox: px(0, 0, 1, 1),
            conf,
        };
        let line = |words: Vec<Word>| Line {
            bbox: px(0, 0, 10, 1),
            paragraph: 1,
            words,
        };
        let page = PageOcr {
            width: 10,
            height: 10,
            lines: vec![
                // A line whose only word is empty goes, so no stray separator is left.
                line(vec![word("", 0.9)]),
                line(vec![word("a", f32::NAN), word(" ", 0.9), word("b", 0.5)]),
            ],
        };
        let (text, lines) = normalise(&page);
        assert_eq!(text, "a b");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].words.len(), 2);
        assert_eq!(lines[0].words[0].conf, 0.0);
    }
}
