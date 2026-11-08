# Developing Scriptorium

Last reviewed: 2026-11-02

How to build Scriptorium from source, run its tools, and find your way around the repository.
Agents start at [`AGENTS.md`](../AGENTS.md).

## Prerequisites

- **Rust** — install via [rustup](https://rustup.rs); `cargo` and `rustc` must be on `PATH`.
- **Node + pnpm** — Node 24.15.0 or later (`engines` in `package.json`; `.nvmrc` selects Node 24);
  enable pnpm with `corepack enable` (preferred) or `npm i -g pnpm`.
- **Webview** — the OS-native webview Tauri renders into:
  - macOS: built-in WKWebView (no extra install)
  - Linux: `webkit2gtk` (distro package, e.g. `libwebkit2gtk-4.1-dev`)
  - Windows: WebView2 (ships with modern Windows; Evergreen bootstrapper otherwise)
- **cargo-about** — `pnpm tauri build` first checks that `THIRD_PARTY_LICENSES.generated.md` is
  current, and fails with a clear message without cargo-about. After a dependency change, run
  `pnpm licenses:gen` and commit the file. Install the version pinned in `package.json` (`config.cargoAboutVersion`); the build prints the exact
  `cargo install` command if yours is missing or different.
  `pnpm tauri dev` does not need it.
- **pdfium** — PDF import loads a pinned pdfium library. `pnpm tauri dev` and `pnpm tauri build`
  fetch it first with `pnpm pdfium:fetch` (`scripts/fetch_pdfium.sh`, which needs `bash` and
  `curl`; on Windows use Git Bash). Run that once yourself before a bare `cargo build`, `cargo
  test`, or `cargo clippy`: the platform Tauri config lists the library as a resource, so a Rust
  build fails without it. The script returns at once when the library is already there.
- **direnv (optional)** — `cp .envrc.example .envrc && direnv allow` to auto-load
  project env vars. `.envrc.example` documents the available variables.

## Run from source

```sh
pnpm install
pnpm tauri dev
```

Scaffold generated with `create-tauri-app@4.6.2` (template `react-ts`, pnpm).
Tauri 2 + Vite 7 + React 19 + TypeScript 5.8.

## Book fixtures

Regenerate the deterministic schema-valid fixture book (stdlib Python, no pip deps):

```sh
python3 scripts/generate-fixture-book.py
python3 scripts/generate-fixture-book.py --pages 8 --seed 42
```

Output lands under gitignored `tmp/fixtures/` (see [`tests/fixtures/README.md`](../tests/fixtures/README.md)).
Spec: [`docs/book-format.md`](book-format.md).

## Reader screenshots

`pnpm shots` runs the reader in headless Chrome and writes screenshots to gitignored
`tmp/shots/`. It uses the Dulac corpus in `tmp/fixtures/corpus/dulac-picture-book/jpg/` when present.
Otherwise it uses the fixture book, so run `python3 scripts/generate-fixture-book.py` first. `pnpm shots --save` also refreshes [`docs/screenshots/`](screenshots/).
The first run needs `pnpm exec playwright install chromium`.

## Checks and CI

Pre-commit hooks and the CI jobs are described in [`ci-and-hooks.md`](ci-and-hooks.md). To check a
build by hand, follow [`manual-test-m1.md`](manual-test-m1.md).

## Repository layout

| Directory | What it is |
|---|---|
| [`src-tauri/`](../src-tauri/) | Tauri 2 Rust host + `book_format` types |
| [`src/`](../src/) | Vite + React + TypeScript app: the home screen and the book reader |
| [`docs/book-format.md`](book-format.md) | Book package format (formatVersion 2; reads 1) |
| [`docs/user/`](user/) | User guides: getting started and a Dulac tutorial |
| [`scripts/generate-fixture-book.py`](../scripts/generate-fixture-book.py) | Deterministic fixture generator |
| [`prompts/`](../prompts/) | Reusable agent prompts: bootstrap, session-start, maintenance, reviews, audits |
| [`templates/`](../templates/) | Fill-in artifacts: briefs, plans, designs, ADRs, runbooks, release checklists, reviews, assessments |
| [`policies/`](../policies/) | Durable repo rules: file size, plans/todos, changelogs, doc freshness, commits, security, GC |
| [`hooks/`](../hooks/) | Pre-commit config + policy scripts (file size, TODO limits, secrets, lint) |
| [`ci/`](../ci/) | CI selection guidance and example GitHub Actions workflows |
| [`inventory/`](../inventory/) | Curated menus of tools, skills, platforms, libraries, and references — load what you need |
| [`plans/`](../plans/) | Product roadmap, active MVP plan, archived completed plans |
| [`tests/`](../tests/) | Python tests; fixture docs under `tests/fixtures/` |

Full contents: see [`inventory/README.md`](../inventory/README.md) and [`AGENTS.md`](../AGENTS.md).

Changelogs: [`CHANGELOG.md`](../CHANGELOG.md) / [`CHANGELOG.dev.md`](../CHANGELOG.dev.md).
GitHub hygiene: [`policies/github-repository-hygiene.md`](../policies/github-repository-hygiene.md).
