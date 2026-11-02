<!-- markdownlint-disable MD033 MD041 -->
<p align="center">
  <img src="design/icons/app-icon.svg" alt="The Scriptorium icon: an open book with a gold quill." width="128" height="128">
</p>

<h1 align="center">Scriptorium</h1>

<p align="center">Turn folders of scanned pages into books you can page through.</p>

<p align="center">

[![CI](https://github.com/kgforais1/scriptorium/actions/workflows/ci.yml/badge.svg)](https://github.com/kgforais1/scriptorium/actions/workflows/ci.yml)
[![License: PolyForm Shield 1.0.0](https://img.shields.io/badge/license-PolyForm%20Shield%201.0.0-b8873f)](LICENSE)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=kgforais1_scriptorium&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=kgforais1_scriptorium)

</p>
<!-- markdownlint-enable MD033 MD041 -->

Last reviewed: 2026-11-02

Tauri-first app for assembling scanned page folders into textured virtual books
(page-turn reading, faithful page images, curator hotspots with lore popups) and
browsing a multi-book library. Built for librarians and gallery directors (authors)
and visitors/students (readers).

**Status:** public, early development (pre-1.0). Bootstrapped from
[`kgrizz-git/project-seed-template`](https://github.com/kgrizz-git/project-seed-template).
Local agent profile (gitignored): `.context/project-profile.md`.

**Security:** [`SECURITY.md`](SECURITY.md) · **Third-party notices:** [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) · **Agent sessions:** [`prompts/new-agent-session.md`](prompts/new-agent-session.md).
**Roadmap:** [`plans/2026-08-23-product-roadmap.md`](plans/2026-08-23-product-roadmap.md) · **Backlog:** [`to_do.md`](to_do.md).
**CI & hooks:** [`docs/ci-and-hooks.md`](docs/ci-and-hooks.md).
**Domain menu:** [`inventory/virtual-books-flipbook.md`](inventory/virtual-books-flipbook.md).
**User guides:** [`docs/user/getting-started.md`](docs/user/getting-started.md) · [`docs/user/tutorial-dulac.md`](docs/user/tutorial-dulac.md).
**Book format:** [`docs/book-format.md`](docs/book-format.md) · schema [`docs/book-format.v2.schema.json`](docs/book-format.v2.schema.json).

---

## Tauri prerequisites

- **Rust** — install via [rustup](https://rustup.rs); `cargo` and `rustc` must be on `PATH`.
- **Node LTS + pnpm** — Node LTS required (see `engines` / `.nvmrc` once scaffolded);
  enable pnpm with `corepack enable` (preferred) or `npm i -g pnpm`.
- **Webview** — the OS-native webview Tauri renders into:
  - macOS: built-in WKWebView (no extra install)
  - Linux: `webkit2gtk` (distro package, e.g. `libwebkit2gtk-4.1-dev`)
  - Windows: WebView2 (ships with modern Windows; Evergreen bootstrapper otherwise)
- **cargo-about** — `pnpm tauri build` regenerates `THIRD_PARTY_LICENSES.generated.md` and fails with
  a clear message without it. Install: `cargo install cargo-about --locked --features cli`.
  `pnpm tauri dev` does not need it.
- **direnv (optional)** — `cp .envrc.example .envrc && direnv allow` to auto-load
  project env vars. `.envrc.example` documents the available variables.

## Development

```sh
pnpm install
pnpm tauri dev
```

Scaffold generated with `create-tauri-app@4.6.2` (template `react-ts`, pnpm).
Tauri 2 + Vite 7 + React 19 + TypeScript 5.8. Node pinned to v24 (`engines` / `.nvmrc`).

### Book fixtures

Regenerate the deterministic schema-valid fixture book (stdlib Python, no pip deps):

```sh
python3 scripts/generate-fixture-book.py
python3 scripts/generate-fixture-book.py --pages 8 --seed 42
```

Output lands under gitignored `tmp/fixtures/` (see [`tests/fixtures/README.md`](tests/fixtures/README.md)).
Spec: [`docs/book-format.md`](docs/book-format.md).

### Reader screenshots

`pnpm shots` runs the reader in headless Chrome and writes screenshots to gitignored
`tmp/shots/`. It uses the Dulac corpus in `tmp/fixtures/corpus/dulac-picture-book/jpg/` when present.
Otherwise it uses the fixture book, so run `python3 scripts/generate-fixture-book.py` first. `pnpm shots --save` also refreshes [`docs/screenshots/`](docs/screenshots/).
The first run needs `pnpm exec playwright install chromium`.

## Try it

Run `pnpm tauri dev` to start the app. The home screen has one button, **Open…**.

| | |
|---|---|
| ![A two-page spread of an open book on a wooden desk.](docs/screenshots/reader-spread.jpg) | ![A page curling over mid-turn, with the next page showing beneath it.](docs/screenshots/reader-turn.jpg) |

### Open a book

Click **Open…** and choose a folder of scanned pages. For a quick sample, generate the fixture
book (see [Book fixtures](#book-fixtures)) and choose `tmp/fixtures/fixture-book/pages`. The app counts the images and shows a
dialog titled "Create a book from these scans?". Check the title and press **Create**. The
app copies the pages into your library and opens the book. Pages sort in natural order, so
`page2` comes before `page10`.

You can also choose a folder that is already a book. It opens at once, with no dialog.

### Turn the pages

- Drag a page by its corner. Let go past the middle to finish the turn.
- Click or tap a page to turn it.
- Press **Previous page** or **Next page** in the bottom bar.
- Press the left and right arrow keys, or Page Up and Page Down.
- Press Home for the first page and End for the last.
- Drag the **Go to page** scrubber to jump anywhere in the book.

If your system asks for reduced motion, a turn is a quick fade instead of a curl.

### Notes and your place

**Notes** opens a panel beside the book. Books have no notes yet. Press **Close book** to
return to the home screen. The app remembers your page. It reopens the last book at that page
the next time you start it.

| | |
|---|---|
| ![The reader with the Notes panel open beside the book.](docs/screenshots/reader-notes.jpg) | ![The Go to page scrubber jumping to another spread mid-turn.](docs/screenshots/reader-scrub.jpg) |

For a longer walk through, read [`docs/user/getting-started.md`](docs/user/getting-started.md)
or the [Dulac tutorial](docs/user/tutorial-dulac.md). To check a build by hand, follow
[`docs/manual-test-m1.md`](docs/manual-test-m1.md).

## What's here and why

| Directory | What it is |
|---|---|
| [`src-tauri/`](src-tauri/) | Tauri 2 Rust host + `book_format` types |
| [`src/`](src/) | Vite + React + TypeScript app: the home screen and the book reader |
| [`docs/book-format.md`](docs/book-format.md) | Book package format (formatVersion 2; reads 1) |
| [`docs/user/`](docs/user/) | User guides: getting started and a Dulac tutorial |
| [`scripts/generate-fixture-book.py`](scripts/generate-fixture-book.py) | Deterministic fixture generator |
| [`prompts/`](prompts/) | Reusable agent prompts: bootstrap, session-start, maintenance, reviews, audits |
| [`templates/`](templates/) | Fill-in artifacts: briefs, plans, designs, ADRs, runbooks, release checklists, reviews, assessments |
| [`policies/`](policies/) | Durable repo rules: file size, plans/todos, changelogs, doc freshness, commits, security, GC |
| [`hooks/`](hooks/) | Pre-commit config + policy scripts (file size, TODO limits, secrets, lint) |
| [`ci/`](ci/) | CI selection guidance and example GitHub Actions workflows |
| [`inventory/`](inventory/) | Curated menus of tools, skills, platforms, libraries, and references — load what you need |
| [`plans/`](plans/) | Product roadmap, active MVP plan, archived completed plans |
| [`tests/`](tests/) | Python tests; fixture docs under `tests/fixtures/` |

Full contents: see [`inventory/README.md`](inventory/README.md) and [`AGENTS.md`](AGENTS.md).

Changelogs: [`CHANGELOG.md`](CHANGELOG.md) / [`CHANGELOG.dev.md`](CHANGELOG.dev.md).
GitHub hygiene: [`policies/github-repository-hygiene.md`](policies/github-repository-hygiene.md).

## Licence

Scriptorium is source-available under the [PolyForm Shield License 1.0.0](LICENSE). Anyone,
companies included, may use, change, and share it for free. Nobody may use it to offer a
product that competes with it, or with any product the licensor or its affiliates provide
using it. The bundled C libraries keep their own licences; see
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). The licences of the Rust and npm
dependencies are listed in the generated [`THIRD_PARTY_LICENSES.generated.md`](THIRD_PARTY_LICENSES.generated.md).
