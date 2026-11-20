//! A stand-in Tesseract for the spawn tests in `src/ocr/tesseract.rs`.
//!
//! `recognize` passes the image name as the first argument and the output base as the second,
//! so the mode comes from the image's file name. The image is never read. An exit 0 without a
//! TSV stands in for a child that dies before writing its output.

use std::time::Duration;

/// A minimal valid TSV: a page row, a line row, and one word.
const OK_TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
1\t1\t0\t0\t0\t0\t0\t0\t10\t20\t-1\t\n\
4\t1\t1\t1\t1\t0\t0\t0\t10\t20\t-1\t\n\
5\t1\t1\t1\t1\t1\t0\t0\t10\t20\t90\tHi\n";

fn main() {
    let mut args = std::env::args_os().skip(1);
    let image = args.next().expect("image name");
    let base = args
        .next()
        .expect("output base")
        .to_string_lossy()
        .into_owned();
    let name = std::path::Path::new(&image)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match name {
        // Long enough for a cancel or a deadline to win.
        "sleep.png" => std::thread::sleep(Duration::from_secs(30)),
        "fail.png" => {
            eprintln!("ocr_test_child: did not converge");
            std::process::exit(3);
        }
        "ok.png" => std::fs::write(format!("{base}.tsv"), OK_TSV).expect("write TSV"),
        // Exit 0 without an output file.
        "crash.png" => {}
        other => {
            eprintln!("ocr_test_child: unknown image name {other:?}");
            std::process::exit(2);
        }
    }
}
