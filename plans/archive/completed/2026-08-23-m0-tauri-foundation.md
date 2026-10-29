# Plan: M0 — Tauri foundation

Last reviewed: 2026-08-23
Date: 2026-08-23
Author: bootstrap session (revised after Hy3 + ox-alpha plan reviews)
Status: complete
Linked issue/PR: n/a
Depends on: harness CI/hooks already on `main`
Blocks: [`../../2026-08-23-m1-scan-viewer.md`](../../2026-08-23-m1-scan-viewer.md)
Parallel OK: [`../../2026-08-23-pre-m1-flipbook-spike.md`](2026-08-23-pre-m1-flipbook-spike.md) after Phase 1 window exists

## Goal

Stand up a **runnable Tauri 2 desktop shell** (Vite + React + TypeScript), lock **data
architecture + book package schema v1**, and wire **Rust quality gates in CI** so M1 can implement
ingest/viewer without inventing layout, paths, or untested Rust mid-feature.

## Out of scope

- Page-turn viewer, ingest UX, last-read page (→ M1; flip library → pre-M1 spike)
- OCR, text layer, library shelf, hotspots, web deploy (→ M2+)
- Vitest / coverage gates / CodeQL / Dependabot as required checks (→ M0.5; advisory audits OK here)
- Visual polish beyond minimal chrome

## Decisions (locked in this plan — ADR-light in `docs/book-format.md` + short ADR if needed)

| Topic | Decision |
|---|---|
| App layout | Repo root: `src/` + `src-tauri/` (not `apps/desktop`) |
| UI | React + TypeScript + Vite + pnpm |
| Library root | OS **app-data** directory, subfolder `books/` (managed library) |
| Portability | Packages are **self-contained**; **relative paths only** in `meta.json` (no absolute home paths) |
| Ingest default | **Copy** pages into the package (not symlink). Future `pages[].storage: "copied" \| "referenced"` reserved; referenced not implemented in M1 |
| Source collisions | If two source files would map to the same destination stem (including case-insensitive match on APFS/NTFS, e.g. `1.jpg` + `1.JPG`), ingest **fails** with `DuplicatePageStem` — do not silently overwrite |
| Copy write | Atomic: write to temp dir under destination parent → validate → rename into place. If dest book id exists → **fail** (`DestinationExists`). On startup and before ingest, delete orphaned `*.scriptorium-tmp` / `.ingest-tmp-*` dirs under the library root older than 1 hour (or any left from a previous crashed run) |
| Checksums | Compute `sha256` at ingest and store in `meta.json`. `load_book` **re-verifies** sha256 for each page file (fail `ChecksumMismatch` if drift). Skip re-verify only behind an explicit future flag — not in M1 |
| Free space | Pre-flight check: refuse copy if free space is less than estimated bytes × 1.1 + 50 MB headroom |
| Schema version | `formatVersion: 1`; M1 may only make **additive** field changes; breaking change requires bump + migration note |
| Lockfiles | **Commit** `Cargo.lock` and `pnpm-lock.yaml`. Remove `Cargo.lock` from `.gitignore` |
| Large fixtures | Committed **generator script** writes synthetic corpora into gitignored `tmp/`; no large binaries in git |

### Alternatives considered

| Option | Why not chosen |
|---|---|
| User-picked library root as default | Forces persisted-scope + evaporating dialog grants; harder M1. Revisit later if needed |
| Symlink default | Breaks when source moves; doubles curator pain only if we force copy without a future referenced mode |
| Docs-after-I/O only | Schema mistakes expensive; decide architecture first, then document |
| Defer all Rust CI to M0.5 | M1 is where filesystem Rust lands; `fmt`/`clippy`/`test` are cheap on existing toolchain |

## Proposed file changes

```
package.json, pnpm-lock.yaml, .nvmrc
src/, index.html, vite.config.ts, tsconfig*.json
src-tauri/                       — Tauri 2; COMMIT Cargo.lock
docs/book-format.md              — prose + rules
docs/book-format.schema.json     — JSON Schema for meta.json
docs/adr/0001-…                  — only if flip spike needs it (spike-owned) or data-arch note
.envrc.example
AGENTS.md                        — agent-tooling contract block
README.md                        — prereqs, tauri dev, fixture generator
.github/workflows/ci.yml         — cargo fmt, clippy -D warnings, test, check; tsc; advisory audits
.gitignore                       — drop Cargo.lock ignore; keep tmp/*, target/, node_modules
scripts/generate-fixture-book.py — or .mjs — synthetic pages → tmp/
tests/fixtures/README.md
```

## Schema v1 fields (minimum in M0 docs + JSON Schema)

`meta.json` (illustrative):

- `formatVersion` (number, required, = 1)
- `id`, `title`, `createdAt`, `updatedAt`
- `renderMode` (`scan` default; reserve `text` / `hybrid`)
- `lastReadPage` (0-based index)
- `rights`, `attribution` (strings; may be empty)
- `pages[]`: `index`, `file` (relative path under package), `width`, `height`, `byteSize`,
  `sha256`, optional `pageLabel`, optional `storage` (default `"copied"`)
- Reserved dirs: `pages/`, `ocr/`, `annotations.json` (empty array/object OK)

**Provenance:** do not store absolute `sourcePath` in `meta.json`. Optional one-line note in
book-format on EXIF/XMP: M1 copies bytes as-is; stripping/policy deferred but documented.

## Phases & checklist

### Phase 0 — Harness closeout

- [ ] `.envrc.example` + README `direnv allow`
- [ ] Paste agent-tooling contract into `AGENTS.md`
- [ ] Confirm `.agent-state/` gitignored
- [ ] README / `validate-env.sh`: Tauri prereqs (Rust, Node LTS, pnpm, platform webview)

### Phase 1 — Scaffold

- [ ] Tauri 2 + Vite + React + TS at repo root
- [ ] `pnpm install` + `pnpm tauri dev` shows placeholder window
- [ ] Pin Node (`engines` / `.nvmrc`)
- [ ] Remove `Cargo.lock` from `.gitignore`; ensure lockfile committed after first `cargo build`

### Phase 2 — Data architecture + book format

- [ ] Write `docs/book-format.md` including locked decisions table above
- [ ] Write `docs/book-format.schema.json`
- [ ] Compile-only Rust types matching schema (full I/O in M1)
- [ ] **Schema bind test:** serialize a sample Rust `BookMeta` and validate it against `docs/book-format.schema.json` in `cargo test` (prevents silent drift)
- [ ] Fixture generator script + `tests/fixtures/README.md`

### Phase 3 — CI (Rust + TS smoke)

- [ ] CI jobs for `src-tauri/`: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo check` (job name e.g. `Rust (src-tauri)` — add to required-checks list in `docs/ci-and-hooks.md` in the same PR)
- [ ] CI: `pnpm exec tsc --noEmit`
- [ ] Advisory: `pnpm audit` / `cargo audit` (`continue-on-error: true` OK until public)
- [ ] Extend markdownlint path list / shellcheck scandir if new dirs need coverage
- [ ] Update `docs/ci-and-hooks.md` in the same change (doc-freshness + required-check names)

## Verification

- [ ] Fresh clone → install → `pnpm tauri dev` shows window
- [ ] `Cargo.lock` tracked; `git check-ignore -v Cargo.lock` does not ignore it under `src-tauri/`
- [ ] `cargo fmt --check`, `clippy -D warnings`, `test`, `check` pass locally and in CI
- [ ] JSON Schema validates a sample `meta.json` fixture
- [ ] `bash scripts/check-bootstrap.sh` findings explained or fixed

## Open questions

- [ ] Exact `create-tauri-app` template version at implementation time (record in README when chosen)

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Scaffold fights existing root files | med | med | Careful merge of package.json / gitignore |
| Over-building M0.5 | med | med | Vitest/coverage/CodeQL stay out; Rust gates stay in |
| Schema still wrong after M1 | low | med | `formatVersion` + additive-only rule |

## Completion steps

1. Status → `complete`; move to `plans/archive/completed/`
2. Changelog: user-facing scaffold note; harness/CI in `CHANGELOG.dev.md`
3. Unblock pre-M1 spike (if not done) and M1 after spike ADR
4. Update `to_do.md` / roadmap

## Related

- Roadmap: [`../../2026-08-23-product-roadmap.md`](../../2026-08-23-product-roadmap.md)
- Spike: [`../../2026-08-23-pre-m1-flipbook-spike.md`](2026-08-23-pre-m1-flipbook-spike.md)
- Next product: [`../../2026-08-23-m1-scan-viewer.md`](../../2026-08-23-m1-scan-viewer.md)
- Superseded draft: [`../superseded/2026-08-23-mvp-scaffold.md`](../superseded/2026-08-23-mvp-scaffold.md)
