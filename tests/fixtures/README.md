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

## Public-domain test corpora

Real scans for load, aspect, and format testing. Download them into gitignored
`tmp/fixtures/corpus/`. Never commit them. Record the source and rights in the book's
`rights` and `attribution` when you ingest one.

| Book | Archive.org id | Pages | Why |
|---|---|---|---|
| *Edmund Dulac's Picture-Book* (1916) | `edmunddulacspict00dulauoft` | 204 | Colour plates; the M1 gate corpus (204 pages) |
| *The Princess and the Goblin* (1920) | `princessgoblin00macd` | 240 | Text with plates; second long book |
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

Verify the gate corpus before an A9 run. Expect 204 pages at 2808 × 3906:

```sh
ls tmp/fixtures/corpus/dulac-picture-book/jpg/*.jpg | wc -l
sips -g pixelWidth -g pixelHeight tmp/fixtures/corpus/dulac-picture-book/jpg/0010.jpg
```

### Any IIIF manifest

`scripts/fetch-iiif-corpus.py` downloads every page of a IIIF v2 or v3 manifest as
full-size JPEGs. It accepts only https page URLs, retries, and resumes. Pages are numbered
from `0000`, like the zip recipe. It is slow against archive.org, and its JPEG quality is
whatever the server chooses, so do not use it for the gate corpus.

```sh
python3 scripts/fetch-iiif-corpus.py <manifest-url> tmp/fixtures/corpus/<name> [--limit N]
```
