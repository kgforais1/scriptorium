# Manual test: M1 (create, open, and read a book in the flipbook reader)

Last reviewed: 2026-10-30

This script checks what the automated tests cannot. Run it once on a reference machine
before M1 is called done. Tick each box as you go. Record the run in the Results table at
the end.

The acceptance IDs (A4, A5, A6, A7, A9, A13) come from
[`plans/2026-08-23-m1-scan-viewer.md`](../plans/2026-08-23-m1-scan-viewer.md). The book view
is the flipbook reader. It draws the open book on a WebGL2 canvas, with a header and a bottom
bar over it. Sections 1 to 8 are the Phase 2 checks, updated for the reader. Section 9 holds the
Phase 3 checks.

`pnpm shots` gives a quick automated look at the reader in headless Chrome, with a stubbed
Tauri bridge. It does not replace this script. It does not cover the Tauri runtime, the CSP, or
the asset scope.

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
- [ ] When the import finishes, the reader opens. Expected: the title is in the header, the
  book is on the canvas, the first page lies alone on the right without stretching, and the
  indicator reads "page 1 of 12" (or the page count of your folder).
- [ ] Look at where the focus is. Expected: the title has the focus, with no ring around it.
- [ ] Check the library folder. Expected: one new folder named after the book id, holding
  `meta.json` and `pages/`.

## 3. Page through the book (A5)

The reader shows two pages at a time. The header indicator reads "page 1 of 12" for a lone
page and "pages 2–3 of 12" for a pair. The bottom bar holds **Previous page**, the **Go to
page** scrubber, **Next page**, and **Notes**.

- [ ] Click **Next page**. Expected: the page turns and the indicator reads "pages 2–3 of 12".
- [ ] Press the right arrow key. Expected: the next pair shows.
- [ ] Press the left arrow key. Expected: the pair before it shows.
- [ ] Press Page Down, then Page Up. Expected: they turn forward and back like the arrows.
- [ ] Press End. Expected: the last spread shows and **Next page** is disabled.
- [ ] Press Home. Expected: page 1 shows alone and **Previous page** is disabled.
- [ ] Press the left arrow key on page 1. Expected: nothing happens and no error appears.
- [ ] Press End, then the right arrow key. Expected: nothing happens and no error appears.
- [ ] Drag the **Go to page** scrubber. Expected: the label beside it follows, and the book turns
  to the spread under the thumb.
- [ ] Hold Shift, Ctrl, Alt, or Cmd and press an arrow key. Expected: the page does not turn.
- [ ] Press the right arrow key five times quickly. Expected: the turns queue and settle on the
  right spread, with no stuck page.

## 4. Close and reopen without a copy (A7)

- [ ] Go to pages 4–5, then click **Close book**. Expected: the home screen returns.
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
per book, with no further turn needed. If a write fails, the bottom bar shows a note, the
worker retries about every ten seconds, and the note clears by itself once a retry succeeds.

The reader saves the first scan of each spread, as a zero-based index in `meta.json`. Pages
4–5 save as `lastReadPage` 3. The book reopens on the spread that holds that scan.

- [ ] Open a book, go to pages 6–7, wait a few seconds, and quit the app (Cmd+Q, or close the
  window). Relaunch with `pnpm tauri dev`. Expected: the book opens by itself on pages 6–7.
- [ ] Open the book, press the right arrow key once, and quit within a second. Use Cmd+Q on
  macOS or the window close button elsewhere. Relaunch. Expected: the book reopens at the
  spread you turned to. This checks the close flush in Rust.
- [ ] Open a book at page 1, turn one spread, and wait 3 seconds. Look at `meta.json` in the
  book's library folder. Expected: `lastReadPage` is 1, the first scan of pages 2–3, without
  quitting.
- [ ] Block writes to the book, then turn a page and wait 2 seconds. The app saves by writing
  a temp file and renaming it over `meta.json`, so a read-only `meta.json` does not stop it on
  macOS or Linux. Make the book's folder read-only instead, for example `chmod a-w <book>`
  (not `-R`). On Windows, set the read-only attribute on `meta.json`. Expected: the bottom bar
  shows "Your place could not be saved." Restore write access (`chmod u+w <book>`, or clear the
  attribute) and wait up to 10 seconds, without turning a page. Expected: the note disappears
  by itself, and `meta.json` holds the page.
- [ ] Block writes again as in the previous step (`chmod a-w <book>` on macOS or Linux, the
  read-only attribute on `meta.json` on Windows), turn a page, and click **Close book**.
  Expected: the book stays open, the button is enabled again, and the note shows. Restore
  write access, then click **Close book** again. Expected: the home screen returns.
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
  and turn pages. Expected: the bottom bar shows "Your place could not be saved.", and no
  dialog appears on each turn. The note stays until a save succeeds. The worker retries about
  every ten seconds, and once more on close.
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
- [ ] On the home screen, when it first appears, **Open…** has the focus.
- [ ] In the reader, press Tab from the title. Expected: focus visits **Close book**,
  **Previous page**, the **Go to page** scrubber, **Next page**, and **Notes**, in that order.
  Tab on past **Notes** and focus wraps round to the canvas. Each stop has a visible ring. A
  disabled button is skipped.
- [ ] Focus the canvas. Expected: a visible focus ring appears, and a screen reader reads
  "Open book, pages 2–3 of 12" (or the spread you are on).
- [ ] Click **Notes**. Expected: the notes panel opens and focus moves to its close button. Press
  Escape. Expected: the panel closes and focus returns to **Notes**.
- [ ] With the scrubber focused, press the arrow keys. Expected: the scrubber moves. The
  reader's own page keys do not fire on a field.
- [ ] Check that every bottom-bar and header control has an accessible name: **Close book**,
  **Previous page**, **Go to page**, **Next page**, and **Notes**.
- [ ] With the confirm dialog open, press the arrow keys inside the Title field. Expected: the
  cursor moves in the text. The page does not turn.
- [ ] Check that a screen reader, or the browser accessibility tree, names the dialog by its
  heading and the progress bar "Import progress".
- [ ] With a dialog open, press an arrow key outside the fields. Expected: the page does not
  turn.

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
  render on the reader canvas. A blocked page shows a note in the reader instead.
- [ ] Repeat this section once on Windows. Expected: pages show there too.
- [ ] Open the devtools console. In a `--debug` build, right-click the window and choose
  Inspect Element. Expected: no message that mentions "Content Security Policy" or "Refused
  to".
- [ ] Turn pages with the arrow keys. Expected: each spread renders and the console stays free
  of CSP violations.

If a page does not render, or the console reports a violation, record the exact message in
the Results table. Compare it with the `csp` block in `tauri.conf.json`.

## 9. Phase 3 reader checks

### Page turns (A4)

- [ ] Make a 1-page book: a scans folder with one image. Open it. Expected: the page lies alone
  on the right, the back pastedown shows, and the indicator reads "page 1 of 1".
- [ ] Press the right arrow key, then click **Next page**. Expected: nothing can turn, and
  **Previous page** and **Next page** are both disabled.
- [ ] Open the 12-page fixture. Turn from the front to the back with **Next page**. Expected:
  every spread renders. The last spread shows page 12 beside the back pastedown, and the
  indicator reads "page 12 of 12".
- [ ] Turn back to the front with **Previous page**. Expected: each turn mirrors the one forward.

### Pointer and touch

- [ ] Drag a page by its corner with the mouse. Expected: the page follows the pointer. Let go
  past the middle and it completes the turn. Let go before the middle and it falls back.
- [ ] Hover a corner without pressing. Expected: the corner lifts a little (the peek). Move
  away and it settles.
- [ ] Click inside the right tap zone, then the left. Expected: the book turns forward, then
  back.
- [ ] If you have a touch device, drag a page with one finger. Expected: the page follows the
  finger. Tap near each edge. Expected: the book turns. Pinch the canvas. Expected: the page does not zoom.
- [ ] Flick quickly with the mouse or a finger. Expected: a short fast stroke completes the
  turn, even if it ends before the middle.

### Reduced motion

- [ ] Open a book. In System Settings, turn on Reduce motion (macOS: Accessibility, Display).
  Do not restart the app. Expected: turns are instant cross-fades, and the corner peek no longer
  appears on hover.
- [ ] Turn Reduce motion off again. Expected: curls and the peek return.

### Chrome

- [ ] Start a turn. Expected: the header and bottom bar fade out, and fade back when it ends.
- [ ] Look at the book. Expected: the header and the bar never cover the gutter, the fold in the
  middle of the book, at any window size.
- [ ] Hover the bottom bar, or Tab onto a control in it, then turn a page. Expected: that bar
  stays visible during the turn.
- [ ] Look at the header and the bar. Expected: they are translucent glass, with the book
  blurred faintly behind them.

### Moved library (A6)

- [ ] Open a book, go to pages 6–7, and wait 3 seconds. Quit the app.
- [ ] Move the whole library folder (the one that holds `books/` and `state.json`) to another
  place, and note where the app expects it. If the app cannot be pointed at the new place, copy
  one book package to another folder instead.
- [ ] Open the moved package with **Open…**. Expected: it opens on pages 6–7. Progress is a
  scan index inside `meta.json`, so it follows the folder.

### Measurements (A9)

Use the Dulac corpus (202 pages): build a book from `tmp/fixtures/corpus/dulac-picture-book/jpg/`.
Run a release build (`pnpm tauri build`), because dev builds are slower. Record each number in
the Results table.

- [ ] **Open to first turn.** Start a stopwatch when you confirm the folder (or choose a
  finished package with **Open…**). Stop it when the first turn finishes. Run it three times
  and record the median. The time includes `load_book`'s checksum pass.
- [ ] **RSS 30 s after the first turn.** Turn one page, wait 30 seconds, and read the memory
  of the app's process and its webview process together. On macOS, use Activity Monitor and
  read "Real Memory" for Scriptorium and for "Scriptorium Web Content". On Windows, use Task
  Manager and sum the app and its WebView2 processes. The target is 2 GB or less.
- [ ] **Texture height.** Record it in the same row as the RSS. No screen shows it. The engine
  has a `stats()` method that reports `texHeight` and the cached texture count, but nothing
  in the app calls it. Work it out
  instead. The height is the smallest of the canvas height in device pixels, 2048, the memory
  budget limit, and the GPU limits (see ADR-0001). On a window shorter than 2048 device pixels,
  the canvas height is the answer. Say in the Notes column that you derived it.
- [ ] **Mean fps over ten turns.** Open the webview devtools (a `--debug` build, then right-click
  and Inspect Element) and use the Performance panel. Record, turn ten pages with the
  right arrow key, stop, and read the mean frame rate across the ten turns. On a 120 Hz display
  the engine reaches about 120 fps in Chrome. Record the display refresh rate with the result.

### Failure states

- [ ] No WebGL2. A built app cannot easily be made to lose WebGL2, so this check is optional.
  If a webview without WebGL2 is at hand (an old WebKitGTK, or a
  VM with no GPU), open a book in it. Expected: the reader shows "WebGL2 is not available in
  this webview. The reader needs it and has no fallback." and no canvas. Otherwise, write
  "not checked" in the Results notes.
- [ ] A page that fails to load shows the paper colour with a note, "Page N could not be loaded."
  Rename one page file in a copy of a package and open it. Expected: the note shows and the other
  pages still turn. The checksum may refuse the book first. In that case "Book is damaged"
  is the expected result.

### Windows (WebView2)

- [ ] Run `pnpm tauri build` on Windows 10 or 11 and start the built app.
- [ ] Repeat sections 2, 3, 5, and 9 on it. Expected: pages render, turns are smooth, keys work,
  and the CSP console stays clean.
- [ ] Record the WebView2 version and the GPU. Record the same four measurements as above.

## Results

Leave this table empty in the repository. The tester adds one row per run.

| Date | Machine (OS, CPU, RAM) | Build (dev, debug, or release) | Open to first turn | RSS at 30 s, texture height | Mean fps over ten turns | Pass or fail | Notes |
|---|---|---|---|---|---|---|---|
|  |  |  |  |  |  |  |  |
