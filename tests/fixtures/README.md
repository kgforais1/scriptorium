# Test fixtures

Large scan corpora and binary fixtures do **not** live in git. Instead, scripts
in `scripts/` regenerate them on demand into gitignored `tmp/`.

## Fixture book

`../scripts/generate-fixture-book.py` produces a deterministic, schema-valid
Scriptorium book package under `tmp/fixtures/fixture-book/`:

```
tmp/fixtures/fixture-book/
├── meta.json
├── annotations.json
└── pages/
    ├── 000.png
    ├── 001.png
    └── ...
```

It is stdlib-only (no pip deps) and hand-rolls minimal valid PNGs, so it runs
anywhere Python 3 is available.

### Usage

```sh
python3 scripts/generate-fixture-book.py                  # default 12-page book
python3 scripts/generate-fixture-book.py --pages 8        # fewer pages
python3 scripts/generate-fixture-book.py --seed 42 --title "My Book"
python3 scripts/generate-fixture-book.py --out tmp/fixtures/custom-book
```

### Determinism

The generator is fully deterministic: the same `--pages`, `--seed`, and `--title`
arguments produce byte-identical output across runs. The default seed is fixed,
so plain invocations regenerate the same fixture every time. This keeps tests
reproducible and means no large binaries need to be committed.

### Schema

The generated `meta.json` conforms to formatVersion 1 of the book package spec
(see [`docs/book-format.md`](../../docs/book-format.md)): real `sha256` and
`byteSize` are computed from the written PNGs, and page 0 carries an optional
`pageLabel` to exercise that field.

## PDF fixtures

`../scripts/generate_pdf_fixtures.py` writes nine tiny PDFs (a few KB each) into
`tests/fixtures/pdf/`. Unlike the fixture book, these **are committed**. The script is
stdlib-only and deterministic, so re-running it leaves `git status` clean. The Rust PDF tests
also need the pdfium library from `../scripts/fetch_pdfium.sh`.

## Public-domain test corpora

Real scans for load, aspect, and format testing. Download them into gitignored
`tmp/fixtures/corpus/`. Never commit them. Record the source and rights in the book's
`rights` and `attribution` when you ingest one.

| Book | Archive.org id | Pages | Why |
|---|---|---|---|
| *Edmund Dulac's Picture-Book* (1916) | `edmunddulacspict00dulauoft` | 202 | Colour plates; the M1 gate corpus |
| *The Princess and the Goblin* (1920) | `princessgoblin00macd` | 238 | Text with plates; second long book |
| *Alice's Adventures in Wonderland* (1911) | `alicesadventures00carr` | 348 | Larger book for scale tests |

Other good sources: e-codices (medieval manuscripts, irregular page sizes), the Library
of Congress, Gallica (BnF), and the Biodiversity Heritage Library (fold-out plates). Avoid
medical collections, per the repository data rule.

### Fastest download: the page-image zip

Archive.org stores pages as JPEG 2000. Its IIIF server converts each page on request and
often times out, so fetch the zip instead. The `.jp2` files also serve as JPEG 2000 ingest
fixtures, and the `.pdf` serves M1.5. This recipe is normative for the M1 gate corpus. It
needs macOS, because `sips` does the JPEG conversion. Rerunning it is safe.

```sh
set -euo pipefail
id=edmunddulacspict00dulauoft; out=tmp/fixtures/corpus/dulac-picture-book
mkdir -p "$out/source" "$out/jpg"
curl -fL --retry 3 -o "$out/source/jp2.zip" "https://archive.org/download/$id/${id}_jp2.zip"
curl -fL --retry 3 -o "$out/source/$id.pdf" "https://archive.org/download/$id/$id.pdf"
unzip -oq "$out/source/jp2.zip" -d "$out/source"
test -d "$out/source/${id}_jp2" || { echo "unexpected zip layout"; exit 1; }
for f in "$out/source/${id}_jp2"/*.jp2; do
  b=$(basename "$f" .jp2); sips -s format jpeg -s formatOptions 92 "$f" --out "$out/jpg/${b##*_}.jpg" >/dev/null
done
```

Verify the gate corpus before an A9 run. Expect 202 pages: 196 at 2808x3906, one each at
2924x4218 and 3048x4198, and four at 936x1292.

```sh
cd tmp/fixtures/corpus/dulac-picture-book/jpg
sips -g pixelWidth -g pixelHeight *.jpg | awk '/pixelWidth/{w=$2} /pixelHeight/{print w "x" $2}' | sort | uniq -c
```

### Any IIIF manifest

`scripts/fetch-iiif-corpus.py` downloads every page of a IIIF v2 or v3 manifest as
full-size JPEGs. It fetches only https URLs on the public IIIF hosts listed in its
`ALLOWED_HOSTS`, writes only under `tmp/`, and caps sizes and download time. To use another
library, add its host to `ALLOWED_HOSTS` in a reviewed change. It retries and resumes:
`.fetch-state.json` ties each page to its source URL, and a folder belongs to one manifest.
Pages are numbered from `0000`, like the zip recipe. Exit code 2 means some canvases had no
usable image. It is slow against archive.org, and its JPEG quality is
whatever the server chooses, so do not use it for the gate corpus.

```sh
python3 scripts/fetch-iiif-corpus.py <manifest-url> tmp/fixtures/corpus/<name> [--limit N]
```

## OCR ground-truth pages

`ocr/smoke.png` is a one-line image ("THE PRINCESS AND THE GOBLIN") that
`.github/workflows/tesseract.yml` OCRs to prove the built binary runs. It was drawn with
Pillow's default font.

`ocr/` holds hand-checked transcriptions of 12 corpus pages for the M2a OCR engine spike
(`../scripts/ocr_spike.py`). The page images are **not** committed. Rebuild them with the
two zip recipes above (Dulac) and below (*The Princess and the Goblin*), which write
`tmp/fixtures/corpus/<book>/jpg/NNNN.jpg`; pages are numbered from `0000`. File names are
`<book>-<NNNN>.txt`, where `<book>` is the corpus folder name.

Transcription rules: text exactly as printed, with printed line breaks, end-of-line hyphens,
punctuation (including the spaces before `!` `?` `;` `:` in the Dulac text), running heads and
page numbers; italics and small caps as plain text; a blank line between paragraphs. Decorative
display lettering is transcribed as the words it spells. Artists' signatures painted inside
an illustration and handwriting in the margin are not transcribed. A credit line typeset
outside the illustration, such as `© D.McK` under the plate in `princess-and-goblin-0021`, is
printed text and is transcribed. Textless pages have an
empty file.

| File | Category |
|---|---|
| `dulac-picture-book-0013.txt` | Contents page with dot leaders and small caps |
| `dulac-picture-book-0025.txt` | Verse with a display title (aged paper, show-through) |
| `dulac-picture-book-0031.txt` | Prose with a display title and italics (aged paper, show-through) |
| `dulac-picture-book-0049.txt` | Caption under a colour plate (faint print on the mount) |
| `dulac-picture-book-0109.txt` | Dense plain prose with a running head (plain-text page) |
| `dulac-picture-book-0154.txt` | Speckled tissue guard, no text (empty) |
| `princess-and-goblin-0003.txt` | Textless illustration, no text (empty) |
| `princess-and-goblin-0007.txt` | Title page |
| `princess-and-goblin-0015.txt` | Chapter opening with a drop cap and italic dialogue |
| `princess-and-goblin-0019.txt` | Chapter opening with a drop cap and prose |
| `princess-and-goblin-0021.txt` | Caption under a colour plate |
| `princess-and-goblin-0036.txt` | Plain prose with a running head (plain-text page) |

Neither book has a page with heavy foxing. The Dulac pages are the closest: yellowed, with
show-through and speckle (`0154` is the most speckled).

Rebuild the Princess and the Goblin images (same recipe as Dulac, other archive.org id):

```sh
set -euo pipefail
id=princessgoblin00macd; out=tmp/fixtures/corpus/princess-and-goblin
mkdir -p "$out/source" "$out/jpg"
curl -fL --retry 3 -o "$out/source/jp2.zip" "https://archive.org/download/$id/${id}_jp2.zip"
unzip -oq "$out/source/jp2.zip" -d "$out/source"
test -d "$out/source/${id}_jp2" || { echo "unexpected zip layout"; exit 1; }
for f in "$out/source/${id}_jp2"/*.jp2; do
  b=$(basename "$f" .jp2); sips -s format jpeg -s formatOptions 92 "$f" --out "$out/jpg/${b##*_}.jpg" >/dev/null
done
```

Then run the spike (engine data goes in gitignored `tmp/ocr-spike/`; see the script header
for every flag and environment variable):

```sh
python3 scripts/ocr_spike.py --tessdata-fast tmp/ocr-spike/tessdata_fast \
  --tessdata-best tmp/ocr-spike/tessdata_best --ocrs tmp/ocr-spike/cargo/bin/ocrs \
  --ocrs-detect-model tmp/ocr-spike/ocrs-models/text-detection.onnx \
  --ocrs-rec-model tmp/ocr-spike/ocrs-models/text-recognition.onnx --json tmp/ocr-spike/results.json
```
