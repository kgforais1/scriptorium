# Tutorial: read Edmund Dulac's Picture-Book

Last reviewed: 2026-11-01

This tutorial builds a book from a public-domain scan, reads it, and reopens it at the same
page. The scan is *Edmund Dulac's Picture-Book* (1916), from the Internet Archive. It has 202
pages of colour plates and text. The steps that fetch it need a Mac, because they use `sips`
to convert the images.

Before you start, finish the install steps in the [getting-started guide](getting-started.md).

## 1. Get the scans

Run these commands from the root of the Scriptorium folder. They download the page images and
convert them to JPEG.

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

The download is large, so it can take a few minutes. You can run the commands again safely.
When they finish, the folder `tmp/fixtures/corpus/dulac-picture-book/jpg/` holds the page
images, one JPEG per page.

## 2. Make the book

1. Start the app with `pnpm tauri dev`, or open your built app.
2. Click **Open…**.
3. Choose `tmp/fixtures/corpus/dulac-picture-book/jpg`.
4. The dialog says "Create a book from these scans?". It lists the image files and an
   estimated size.
5. Change the title to "Edmund Dulac's Picture-Book".
6. In **Rights**, type "Public domain". In **Attribution**, type "Internet Archive,
   edmunddulacspict00dulauoft".
7. Press **Create**.

The progress bar fills as the pages are copied. This book is large, so it takes a little
while. Then the book opens.

## 3. Read it

The first page lies alone on the right. Try each way to turn the page:

- Drag the lower corner of the right-hand page to the left, and let go.
- Click the left-hand page to turn back.
- Press the right arrow key a few times.
- Press End to jump to the last page, then Home to return to the first.
- Drag the **Go to page** scrubber to the middle of the book.

Dulac's plates are full-colour paintings. Turn a few pages to find one.

## 4. Close the book and open it again

1. Turn to any page you like, and note its number.
2. Wait a few seconds, then press **Close book**.
3. Quit the app and start it again.

The app opens Dulac's book on the page you left. Your place was saved in the book's folder in
your library. The [getting-started guide](getting-started.md#where-your-books-live) says where
that folder is.

## Clean up

To remove the book, close it and delete its folder from your library. To remove the scans,
delete `tmp/fixtures/corpus/dulac-picture-book`.
