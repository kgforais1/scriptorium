# Getting started with Scriptorium

Last reviewed: 2026-11-01

Scriptorium turns a folder of scanned pages into a virtual book. You open the book on a
wooden desk and turn its pages like a real one. This guide is for curators who make books and
for readers who open them.

## Install

There is no download yet. For now you build the app from source.

1. Install the tools listed under "Tauri prerequisites" in the
   [README](../../README.md#tauri-prerequisites): Rust, Node, and pnpm.
2. Run `pnpm install`.
3. Run `pnpm tauri build`.
4. The build prints the path of the finished app. Open that app.

To try the app without a full build, run `pnpm tauri dev` instead.

## Make a book from a folder of scans

1. Put your scans in one folder.
2. Start Scriptorium and click **Open…**.
3. Choose the folder.

Scriptorium reads these image types:

- JPEG, PNG, and WebP files.
- BMP and GIF files. A GIF uses its first frame.
- TIFF files. Each page of a multi-page TIFF becomes one page of the book.
- JPEG 2000 files (`.jp2`, `.j2k`, and `.jpf`).

Scriptorium puts the files in natural order. That means `page2` comes before `page10`, and
capital letters do not matter. If you want a page in a certain place, number the file names.
Two files may not share a name once the extension is removed. `a.png` and `a.jpg` clash.

The app then shows a dialog titled "Create a book from these scans?". It tells you how many
image files it found, how many pages they make, and about how much space the book will take.
You can fill in these fields:

- **Title** is the name shown above the book. It starts as the folder name.
- **Book id** is the name of the book's folder in your library. It starts as a short form of
  the title. It may use only lowercase letters, digits, dots, underscores, and dashes. The app tells you if the id
  is not allowed.
- **Rights** and **Attribution** are optional. Use them to record where the scans came from.

Press **Create**. A progress bar counts the pages as they are copied. Your original scans are
not changed. When the copy ends, the book opens.

You can also choose a folder that is already a Scriptorium book. It opens at once, with no
dialog and no copy.

## Read the book

The book shows two pages at a time. The first page lies alone on the right. The title and the
page numbers sit above the book. The controls sit in a bar below it.

With a mouse:

- Drag a page by its corner, then let go. Past the middle, the page finishes the turn. Before
  the middle, it falls back.
- Click a page to turn it.
- Move the pointer to a corner, and the corner lifts a little.

With the keyboard:

| Key | What it does |
|---|---|
| Right arrow, Page Down | Next page |
| Left arrow, Page Up | Previous page |
| Home | First page |
| End | Last page |

The keys do nothing while a dialog is open, while you are typing in a field, or while you hold
Shift, Ctrl, Alt, or Cmd.

With a touch screen:

- Drag a page with one finger.
- Tap a page to turn it.

In the bottom bar, **Previous page** and **Next page** turn the book. The **Go to page**
scrubber jumps to any page. The numbers beside it show the pages you will land on. **Notes**
opens a panel beside the book. Books have no notes yet.

If your computer is set to reduce motion, the book fades from one spread to the next. It does
not curl the page.

Press **Close book** to return to the home screen.

## Where your books live

Scriptorium copies each new book into its library folder. The folder name is the book id.

| System | Library folder |
|---|---|
| macOS | `~/Library/Application Support/dev.scriptorium.app/books/` |
| Windows | `%APPDATA%\dev.scriptorium.app\books\` |
| Linux | `~/.local/share/dev.scriptorium.app/books/` |

Each book folder holds a `meta.json` file and the page images. To move or back up a book, copy
its folder. To open a copy, click **Open…** and choose that folder.

## How your place is saved

Scriptorium saves the page you are on while you read. It writes at most once a second, and it
writes once more when you close the book or the window. The next time you start the app, it
opens the last book at that page. If you move the book's folder away, the app starts at the
home screen instead.

If the bar shows "Your place could not be saved.", Scriptorium could not write to the book's
folder. The folder may be read-only, or the disk may be full. The book keeps working. The app
tries again every few seconds, and the note goes away once a save works.

## When something goes wrong

A problem opens a dialog with a heading and a message. Press **OK** to close it.

| Heading | What it means and what to do |
|---|---|
| Folder is empty | The folder has nothing in it. Choose the folder that holds your scans. |
| No supported images | The folder has no image of a type that Scriptorium reads. Check the file types above. |
| Can't read a file | One file is damaged, or its type does not match its name. The message names the file. Replace it or take it out. |
| Book already exists | Your library already has a book with that id. Choose another id in the dialog. |
| Two files share a name | Two files differ only by their extension. Rename one of them. |
| Not enough disk space | The disk is too full to hold the copy. Free some space and try again. |
| Book is from a newer version | A newer Scriptorium made this book. Update the app to open it. |
| Not a valid book | The folder looks like a book but is missing parts or is malformed. Open the original scans instead. |
| Book is damaged | A page no longer matches what was saved. Restore the book from a backup, or make it again from the scans. |
| Access denied | Your system refused access to a folder. Check its permissions, or choose another folder. |
| Scriptorium hit a problem | Something unexpected failed. Try again. If it keeps happening, report it with the text in the message. |
| Something went wrong | Scriptorium could not tell what failed. Try again. |

If a single page fails to show, the bar below the book says which page could not be loaded.
The rest of the book still works.

## Next

Follow the [Dulac tutorial](tutorial-dulac.md) to build and read a real book.
