# OCR engine

Last reviewed: 2026-11-09

Scriptorium will bundle **Tesseract 5.5.3** with the `tessdata_fast` English model as its OCR
engine. The app does not bundle or run Tesseract yet. M2a Phase 3 adds it as a sidecar binary,
so a crash on a bad page cannot take down the app. This page records why Tesseract was chosen,
how the binary is built, and how to bump it. The plan is
[M2a OCR engine and pipeline](../plans/2026-11-07-m2a-ocr-pipeline.md).

## Decision (2026-11-09)

The M2a Phase 1 spike compared Tesseract with two English models and ocrs, a pure-Rust engine,
on 12 hand-checked pages from the two test books
([`tests/fixtures/ocr/`](../tests/fixtures/ocr/), listed in the
[fixtures README](../tests/fixtures/README.md#ocr-ground-truth-pages)).

| Engine | CER | WER | Plain-prose CER | Invented chars on blank pages | s/page |
|---|---|---|---|---|---|
| Tesseract, `tessdata_fast` | 5.06% | 13.68% | 1.13% | 4 | 0.96 |
| Tesseract, `tessdata_best` | 5.37% | 14.26% | 1.10% | 5 | 1.34 |
| ocrs 0.14.0 | 12.24% | 28.23% | 11.54% | 25 | 1.58 |

CER and WER are computed over the whole set: total edits over total ground-truth characters or
words, with whitespace runs collapsed. Timings are single-threaded on an Apple-silicon Mac. Most
of Tesseract's error comes from two hard pages: a small-caps contents page and a faint caption
under a plate, which every engine misses.

The plan's gate picks Tesseract unless its packaging fails on any platform, or ocrs comes within
1 point of its CER at under half its size. Tesseract packages on all three platforms (below), and
ocrs is 10.4 points worse on plain prose. Both Tesseract models clear the 3% plain-prose bar.
`tessdata_fast` is chosen over `tessdata_best`, because it is as accurate here, faster, and a
quarter of the size.

Neither book has a clearly foxed page, so foxing is not covered by the ground truth.

## The binary

[`scripts/build_tesseract.sh`](../scripts/build_tesseract.sh) builds Tesseract and Leptonica
with static zlib, libpng, libjpeg-turbo, libtiff, and libwebp, from pinned source archives
checked against pinned SHA-256 values. It builds no network features, no OpenMP, no OpenCL, and
no training tools. It writes `src-tauri/binaries/tesseract-<target-triple>[.exe]`, which is
gitignored.

| Platform | Runtime dependencies | Size (stripped) |
|---|---|---|
| macOS arm64 | `libSystem`, `libc++` | 6.5 MB |
| Linux x64 | glibc 2.35 or later, `libstdc++`, `libgcc_s`, `libm` | built in CI on Ubuntu 22.04 |
| Windows x64 (MSYS2 MinGW) | `KERNEL32.dll`, `msvcrt.dll` | built in CI |

[`.github/workflows/tesseract.yml`](../.github/workflows/tesseract.yml) builds it on all three
platforms, checks that no system Tesseract is on `PATH`, and OCRs
[`tests/fixtures/ocr/smoke.png`](../tests/fixtures/ocr/smoke.png). It runs on pull requests that
touch the build script, the workflow, or the smoke image, and on demand. The smoke run uses TSV
output, as the app will. On macOS the binary was also checked by hand on 2026-11-09: it ran
ad-hoc signed under the hardened runtime with a cleared environment, and matched Homebrew's
output byte for byte. CI does not repeat that check. Phase 3 adds it for the built app.

The English model is `eng.traineddata` from
[tessdata_fast](https://github.com/tesseract-ocr/tessdata_fast), 4.1 MB, SHA-256
`7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2`.

Build notes, kept in the script's comments:

- GNU `ld` reads static archives once. Leptonica's link line puts `libjpeg.a` before
  `libtiff.a`, so the script repeats libjpeg and zlib at the end of Tesseract's link line.
- On MinGW, zlib's static library is `libzlibstatic.a`, and the native linker needs Windows
  paths, so the script converts them with `cygpath -m`.
- The Windows build's triple is `x86_64-pc-windows-gnu`. The app builds for
  `x86_64-pc-windows-msvc`, so Phase 3 sets `TESSERACT_TARGET_TRIPLE=x86_64-pc-windows-msvc`
  to name the binary the way Tauri's sidecar convention expects.

## Bumping

1. Change the version and SHA-256 in `build_tesseract.sh`. Check the hash against the
   project's published checksum or signature where one exists. zlib's current hash matches
   zlib.net. Leptonica, libjpeg-turbo, libtiff, and libwebp publish checksums or signatures for
   their release archives, but the current hashes were taken on download over HTTPS and not yet
   checked against them. Do that at the next bump. Tesseract and libpng come from GitHub tag
   archives, which have no published checksum.
2. Run the script locally, and let the `Tesseract build` workflow pass on all three platforms.
3. Rerun the spike, `python3 scripts/ocr_spike.py` with the flags in its `--help`, and record any
   change in the table above. The page images must be rebuilt first; the fixtures README has the
   recipe.
4. A new engine id redoes OCR for every book, as the plan's skip rule requires, so note the bump
   in the changelog.
