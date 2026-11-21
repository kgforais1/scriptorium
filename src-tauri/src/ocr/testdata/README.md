# Tesseract TSV fixtures

The parser tests in `../tesseract.rs` read these files with `include_str!`.

`smoke.tsv` and `page-prose.tsv` are real output of the fetched binary
(`src-tauri/binaries/scriptorium-tesseract-<triple>`) with the engine's exact arguments.
Resolve the paths before changing directory, because the binary name and `--tessdata-dir`
would otherwise be resolved from the image's directory:

```sh
repo_root="$(git rev-parse --show-toplevel)"
bin="${repo_root}/src-tauri/binaries/scriptorium-tesseract-<triple>"
tessdata="${repo_root}/src-tauri/tessdata"
image="$(cd "${repo_root}/tests/fixtures/ocr" && pwd)/smoke.png"

cd "$(dirname "${image}")"
"${bin}" "$(basename "${image}")" smoke --tessdata-dir "${tessdata}" -l eng \
  --psm 3 --oem 1 -c tessedit_create_tsv=1 -c tessedit_create_txt=0
```

- `smoke.tsv` — `tests/fixtures/ocr/smoke.png`, one line of text (the command above writes
  `smoke.tsv`, which moves here).
- `page-prose.tsv` — page `09.jpg` of the M2a spike corpus
  (`tmp/fixtures/spike-pages/`, rebuilt per `tests/fixtures/README.md`), 36 lines in
  two blocks, with `par_num` restarting in block 2. Substitute that image path and base.

The rest are hand-built to pin edge cases:

- `empty-rows.tsv` — empty-text words, a `-1` word confidence, and confidences over 100.
- `par-restart.tsv` — `(block_num, par_num)` restarts in each block.
- `page-only.tsv` — header and the level 1 page row only, no words.
- `truncated-row.tsv` — a line row cut off after three columns.
- `bad-number.tsv` — a column that will not parse.
- `crlf.tsv` — the same small page with CRLF line endings.
