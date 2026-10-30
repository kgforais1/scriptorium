# Manual test: M1 Phase 2 (create, open, and page through a book)

Last reviewed: 2026-10-30

This script checks what the automated tests cannot. Run it once on a reference machine
before M1 is called done. Tick each box as you go. Record the run in the Results table at
the end.

The acceptance IDs (A5, A6, A7, A13) come from
[`plans/2026-08-23-m1-scan-viewer.md`](../plans/2026-08-23-m1-scan-viewer.md). The page-turn
flipbook is not part of Phase 2. The book view shows one plain image per page. Phase 3 adds
the flipbook.

## Where books live

The app copies each new book into its library folder. The folder name is the book id.

| OS | Library folder |
|---|---|
| macOS | `~/Library/Application Support/dev.scriptorium.app/books/` |
| Windows | `%APPDATA%\dev.scriptorium.app\books\` |
| Linux | `~/.local/share/dev.scriptorium.app/books/` |

The last opened book is remembered in `state.json`, one level above `books/`.

## 1. Setup

- [ ] Run `pnpm install`. Expected: it finishes with no errors.
- [ ] Generate the small fixture book with `python3 scripts/generate-fixture-book.py`.
  Expected: `tmp/fixtures/fixture-book/` holds `meta.json` and a `pages/` folder of 12 PNG
  files.
- [ ] Optional, for a realistic run: build the Dulac corpus with the recipe in
  [`tests/fixtures/README.md`](../tests/fixtures/README.md#fastest-download-the-page-image-zip).
  Expected: 202 JPEG files in `tmp/fixtures/corpus/dulac-picture-book/jpg/`.
- [ ] Note which folder you will use as the scans folder. The fixture's own `pages/` folder
  works: `tmp/fixtures/fixture-book/pages`. The `fixture-book` folder itself is a finished
  package, so Open… loads it without a confirm dialog.
- [ ] Start with an empty library. Move any existing folders out of the `books/` folder shown
  above, and delete `state.json` next to it. Expected: the first launch shows the home screen.

## 2. Create a book from scans

Run `pnpm tauri dev`. The first build takes a few minutes.

In `pnpm tauri dev` the app asks for the last book twice at startup, because React
StrictMode runs effects twice in development. This is expected. It does not happen in a
built app.

- [ ] The window shows the title "Scriptorium", the text "Open a book, or a folder of scanned
  pages.", and an **Open…** button.
- [ ] Click **Open…** and choose the scans folder. Expected: a dialog titled "Create a book
  from these scans?" appears.
- [ ] The dialog shows the file count, the page count, and an estimated size. For the
  fixture it reads "12 image files, 12 pages, about" a small size.
- [ ] The **Title** field holds the folder name. The **Book id** field holds a slug of it.
  Edit the title to something else. Expected: the field accepts the text.
- [ ] Type an invalid id, such as `Bad Id!`. Expected: a hint appears under the field and
  **Create** is disabled.
- [ ] Clear the title. Expected: **Create** is disabled until you type a title again.
- [ ] Restore a valid id. Expected: the hint disappears and **Create** is enabled.
- [ ] Press **Cancel**. Expected: the dialog closes and nothing is added to the library.
- [ ] Click **Open…**, choose the same folder again, and click **Create**. Expected: the form
  locks, a progress bar fills, and the text beside it shows the page count going up.
- [ ] While the progress bar runs, press Escape. Expected: the dialog stays open. **Cancel**
  is disabled.
- [ ] When the import finishes, the book view opens. Expected: the title is in the header, the
  first page is shown without stretching, and the indicator reads "1 / 12" (or the page count
  of your folder).
- [ ] Check the library folder. Expected: one new folder named after the book id, holding
  `meta.json` and `pages/`.

## 3. Page through the book (A5)

- [ ] Click **Next page**. Expected: page 2 shows and the indicator reads "2 / 12".
- [ ] Press the right arrow key. Expected: page 3.
- [ ] Press the left arrow key. Expected: page 2.
- [ ] Click **Previous page** until page 1. Expected: **Previous page** is disabled at page 1.
- [ ] Press the left arrow key on page 1. Expected: nothing happens and no error appears.
- [ ] Go to the last page. Expected: **Next page** is disabled.
- [ ] Press the right arrow key on the last page. Expected: nothing happens and no error
  appears.
- [ ] Hold Shift, Ctrl, Alt, or Cmd and press an arrow key. Expected: the page does not turn.

## 4. Close and reopen without a copy (A7)

- [ ] Go to page 4, then click **Close book**. Expected: the home screen returns.
- [ ] Note the folder names in the library folder.
- [ ] Click **Open…** and choose the book's own folder in the library (the folder named
  after the book id). Expected: the book opens with no confirm dialog and no progress bar.
- [ ] Check the library folder again. Expected: no new folder. The list matches the one you
  noted.
- [ ] Close the book. Click **Open…** and choose `tmp/fixtures/fixture-book` (the package, not
  its `pages/` folder). Expected: it opens with no confirm dialog. The library folder still
  has no new folder.

## 5. Progress survives a restart (A6)

Progress is saved in the book's `meta.json`, at most once a second while you page. A turn
made within a second of the last write is held, then written about a second later. Closing
the book or the window writes it at once. A held page is written by one background worker
per book, with no further turn needed. If a write fails, the page bar shows a note, the
worker retries about every ten seconds, and the note clears by itself once a retry succeeds.

- [ ] Open a book, go to page 6, wait a few seconds, and quit the app (Cmd+Q, or close the
  window). Relaunch with `pnpm tauri dev`. Expected: the book opens by itself at page 6.
- [ ] Open the book, press the right arrow key once, and quit within a second. Use Cmd+Q on
  macOS or the window close button elsewhere. Relaunch. Expected: the book reopens at the
  page you turned to. This checks the close flush in Rust.
- [ ] Open a book at page 1, turn one page, and wait 3 seconds. Look at `meta.json` in the
  book's library folder. Expected: `lastReadPage` holds the new page (zero-based), without
  quitting.
- [ ] Make the book's `meta.json` read-only (or the book's folder, on Windows), then turn a
  page and wait 2 seconds. Expected: the page bar shows "Your place could not be saved."
  Make it writable again and wait up to 10 seconds, without turning a page. Expected: the
  note disappears by itself, and `meta.json` holds the page.
- [ ] Make `meta.json` read-only again, turn a page, and click **Close book**. Expected: the
  book stays open, the button is enabled again, and the note shows. Make it writable and
  click **Close book** again. Expected: the home screen returns.
- [ ] Click **Close book**, then quit. Relaunch. Expected: the app reopens the last book
  that was opened, at its saved page.
- [ ] Move the book's library folder away and relaunch. Expected: the home screen shows and no
  error dialog appears. Move the folder back afterwards.

## 6. Error cases

Each error opens a dialog with a heading, a message, and an **OK** button. Press **OK** or
Escape to dismiss it.

- [ ] Open… an empty folder. Expected: heading "Folder is empty".
- [ ] Open… a folder that holds only `.txt` files. Expected: heading "No supported images".
- [ ] Make a folder with `a.png` and `a.jpg`, using copies of any two images. Open… it.
  Expected: heading "Two files share a name".
- [ ] Open… a folder with an unreadable file, such as a text file renamed to `page.png`.
  Expected: heading "Can't read a file". It may appear only after you press **Create**.
- [ ] Create a book, then create another from the same scans with the same id. Expected:
  heading "Book already exists", and the confirm dialog stays usable after you press **OK**.
- [ ] Close the app. In a book's library folder, change one byte of `pages/000.png` (or
  `000.jpg`). On macOS or Linux:
  `printf '\x00' | dd of="<library>/<id>/pages/000.png" bs=1 seek=100 conv=notrunc`.
  Relaunch, click **Open…**, and choose that library folder. Expected: heading "Book is
  damaged".
- [ ] Open a book from a read-only folder (for example, `chmod -R a-w` a copy of a package)
  and turn pages. Expected: the page bar shows "Your place could not be saved", and no
  dialog appears on each turn. The note stays until a save succeeds. Saving retries about
  every ten seconds of page turning, and once more on close.
- [ ] With the damaged book as the last opened one, relaunch. Expected: the home screen shows
  with no error dialog. Delete the damaged folder afterwards.

## 7. Keyboard and focus

- [ ] On the home screen, press Tab. Expected: focus lands on **Open…** with a visible focus
  ring.
- [ ] In the confirm dialog, press Tab repeatedly. Expected: focus moves through Title, Book
  id, Rights, Attribution, **Cancel**, and **Create**, and stays inside the dialog. Every
  stop has a visible focus ring.
- [ ] Press Escape in the confirm dialog. Expected: it closes, and focus returns to **Open…**.
- [ ] Trigger an error dialog (section 6) and press Escape. Expected: it closes.
- [ ] Start an import and press Escape during it. Expected: the dialog stays open (see
  section 2).
- [ ] In the book view, press Tab. Expected: focus visits **Close book**, **Previous page**,
  and **Next page**, in that order, each with a visible ring. A disabled button is skipped.
- [ ] With the confirm dialog open, press the arrow keys inside the Title field. Expected: the
  cursor moves in the text. The page does not turn.
- [ ] Check that a screen reader, or the browser accessibility tree, names the dialog by its
  heading and the progress bar "Import progress".

## 8. Production CSP gate (A13)

Tauri applies the content security policy only to bundled assets. `pnpm tauri dev` never
enforces it. No automated test covers the asset scope either, because it needs a Tauri
runtime: each page file is admitted when its book opens. This section is the only check that
the policy in
[`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) lets a book render.

- [ ] Run `pnpm tauri build`. Use `pnpm tauri build --debug` if you want devtools in the built
  app. Expected: the build finishes and prints the path of the app bundle.
- [ ] Run the built app. Do not use `pnpm tauri dev`.
- [ ] Open a book (create one from scans, or open the fixture package). Expected: the pages
  show as images. A blocked page shows "This page could not be loaded." with a **Try again**
  button instead.
- [ ] Repeat this section once on Windows. Expected: pages show there too.
- [ ] Open the devtools console. In a `--debug` build, right-click the window and choose
  Inspect Element. Expected: no message that mentions "Content Security Policy" or "Refused
  to".
- [ ] Turn pages with the keys and the buttons. Expected: each page loads and the console
  stays free of CSP violations.
- [ ] In the built app, set `location.hash = "spike"` in the console. Expected: nothing
  changes, because the release bundle has no spike.

If a page does not render, or the console reports a violation, record the exact message in
the Results table. Compare it with the `csp` block in `tauri.conf.json`.

## 9. The dev-only flipbook spike

The flipbook spike is throwaway code that proves the page-turn engine. It stays in dev
builds until Phase 3 replaces it.

- [ ] Run `pnpm tauri dev`.
- [ ] Open the devtools console and run `location.hash = "spike"`. Expected: the engine
  harness replaces the reader. Since Phase 3 chunk 1 it shows the last opened book on the new
  engine (`src/features/reader/engine/`), with Prev, Next, Scene, Remount, and Reduced motion
  buttons; the old spike files stay on disk until chunk 5. Run `location.hash = ""` to return.
- [ ] For a realistic page set, build the spike pages with `scripts/make-spike-fixtures.sh`.
  It needs macOS and the Dulac and Princess corpora from
  [`tests/fixtures/README.md`](../tests/fixtures/README.md).

## Results

Leave this table empty in the repository. The tester adds one row per run.

| Date | Machine (OS, CPU, RAM) | Build (dev, debug, or release) | Pass or fail | Notes |
|---|---|---|---|---|
|  |  |  |  |  |
