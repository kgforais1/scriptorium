//! Natural sort of page files (M1 plan, "Natural-sort specification (normative for M1)").
//!
//! Stems split into alternating numeric and text chunks, compared pairwise left to right:
//! numbers by value, a number before text, text case-insensitively, and a stem that runs out
//! of chunks first sorts first. Ties go to the longer zero-padded number, then the stem's
//! UTF-8 bytes, then the full path's bytes. The extension never affects order.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum Chunk<'a> {
    /// Digits as written, so padding survives for the tie-break.
    Num(&'a str),
    Text(&'a str),
}

fn chunks(stem: &str) -> Vec<Chunk<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut digit = None;
    for (i, c) in stem.char_indices() {
        let d = c.is_ascii_digit();
        if digit.is_some_and(|prev| prev != d) {
            out.push(chunk(&stem[start..i], digit == Some(true)));
            start = i;
        }
        digit = Some(d);
    }
    if start < stem.len() {
        out.push(chunk(&stem[start..], digit == Some(true)));
    }
    out
}

fn chunk(s: &str, is_num: bool) -> Chunk<'_> {
    if is_num {
        Chunk::Num(s)
    } else {
        Chunk::Text(s)
    }
}

/// Compare two digit strings by integer value without parsing, so any length works.
fn num_value_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(&cb) {
        let ord = match (x, y) {
            (Chunk::Num(p), Chunk::Num(q)) => num_value_cmp(p, q),
            (Chunk::Num(_), Chunk::Text(_)) => Ordering::Less,
            (Chunk::Text(_), Chunk::Num(_)) => Ordering::Greater,
            (Chunk::Text(p), Chunk::Text(q)) => {
                p.to_lowercase().as_bytes().cmp(q.to_lowercase().as_bytes())
            }
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ca.len().cmp(&cb.len())
}

/// Tie-break 1: at the first numeric chunk pair with equal value but different digits, the
/// longer (more zero-padded) one sorts first.
fn padding_cmp(a: &str, b: &str) -> Ordering {
    for (x, y) in chunks(a).iter().zip(&chunks(b)) {
        if let (Chunk::Num(p), Chunk::Num(q)) = (x, y) {
            if p != q {
                return q.len().cmp(&p.len());
            }
        }
    }
    Ordering::Equal
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Compare two page files by the normative order.
pub fn compare(a: &Path, b: &Path) -> Ordering {
    let (sa, sb) = (stem(a), stem(b));
    natural_cmp(&sa, &sb)
        .then_with(|| padding_cmp(&sa, &sb))
        .then_with(|| sa.as_bytes().cmp(sb.as_bytes()))
        .then_with(|| {
            a.as_os_str()
                .as_encoded_bytes()
                .cmp(b.as_os_str().as_encoded_bytes())
        })
}

/// Sort page files in place.
pub fn sort_pages(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| compare(a, b));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(names: &[&str]) -> Vec<String> {
        let mut paths: Vec<PathBuf> = names.iter().map(PathBuf::from).collect();
        sort_pages(&mut paths);
        paths.iter().map(|p| stem(p)).collect()
    }

    #[test]
    fn plan_table_cases() {
        let cases: &[(&[&str], &[&str])] = &[
            (&["2.jpg", "10.jpg", "1.jpg"], &["1", "2", "10"]),
            (
                &["page-2.PNG", "page-10.png", "page-1.png"],
                &["page-1", "page-2", "page-10"],
            ),
            (&["img01.jpg", "img001.jpg"], &["img001", "img01"]),
            (&["cover.jpg", "1.jpg", "2.jpg"], &["1", "2", "cover"]),
            (&["1.jpg", "1b.jpg", "1a.jpg"], &["1", "1a", "1b"]),
            (&["1.jpg", "0a.jpg"], &["0a", "1"]),
        ];
        for (input, want) in cases {
            assert_eq!(order(input), *want, "input {input:?}");
        }
    }

    #[test]
    fn more_cases() {
        let cases: &[(&[&str], &[&str])] = &[
            // Text compares case-insensitively.
            (&["B.jpg", "a.jpg", "C.jpg"], &["a", "B", "C"]),
            // Numbers far beyond u64 still compare by value.
            (
                &[
                    "99999999999999999999999.jpg",
                    "100000000000000000000000.jpg",
                ],
                &["99999999999999999999999", "100000000000000000000000"],
            ),
            // A number sorts before text at the same position.
            (&["a.jpg", "9.jpg"], &["9", "a"]),
            // A shorter chunk list sorts first.
            (&["page.jpg", "page1.jpg"], &["page", "page1"]),
            // Multi-chunk stems compare chunk by chunk.
            (
                &["v2-p10.jpg", "v2-p9.jpg", "v10-p1.jpg"],
                &["v2-p9", "v2-p10", "v10-p1"],
            ),
            // Padding tie-break only applies at equal value: 007 before 07 before 7.
            (&["7.jpg", "007.jpg", "07.jpg"], &["007", "07", "7"]),
            // Equal stems fall back to the full path, so extension order is stable.
            (&["1.png", "1.jpg"], &["1", "1"]),
            // Text compares by lowercase UTF-8 bytes, so non-ASCII letters follow ASCII ones.
            (
                &["Été.jpg", "été2.jpg", "zebra.jpg"],
                &["zebra", "Été", "été2"],
            ),
        ];
        for (input, want) in cases {
            assert_eq!(order(input), *want, "input {input:?}");
        }
        let mut paths = vec![PathBuf::from("1.png"), PathBuf::from("1.jpg")];
        sort_pages(&mut paths);
        assert_eq!(paths, [PathBuf::from("1.jpg"), PathBuf::from("1.png")]);
    }

    #[test]
    fn order_is_total_and_stable_under_shuffles() {
        let names = [
            "10.jpg",
            "2.jpg",
            "02.jpg",
            "a.jpg",
            "A.jpg",
            "1a.jpg",
            "1.jpg",
            "cover.png",
        ];
        let want = order(&names);
        let mut rev = names;
        rev.reverse();
        assert_eq!(order(&rev), want);
        assert_eq!(want, ["1", "1a", "02", "2", "10", "A", "a", "cover"]);
    }
}
