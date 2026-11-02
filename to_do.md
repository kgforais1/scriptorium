# Scriptorium backlog

Last reviewed: 2026-11-02

Agents: start at **Next Up** (3–5 pointer lines). Detail lives in linked plans, not here.
Policy: [`policies/plans-and-todos.md`](policies/plans-and-todos.md).

## Next Up (keep 3–5; pointers only — not full checklists)

- [M1 exhaustive manual pass](docs/manual-test-m1.md) — 0.2.0 shipped after the main flows; the full list, A9 measurements and Windows/WebView2 run remain (see Unplanned)
- [M1 Phase 0 engine gate](plans/archive/completed/2026-08-23-m1-scan-viewer.md#phase-0--integration-gate-adr-already-done) — ADR-0001 open items: Windows/Linux webviews, real touch, WKWebView fps, RSS
- [M0.5 harness follow-ups](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri--remaining-quality) — Rust coverage, Python `fail_under`, Dependabot, S9–S11

## Active plans

| Plan | Status | Note |
|---|---|---|
| [Product roadmap](plans/2026-08-23-product-roadmap.md) | living | M0–M8 horizon; not a task queue |

## Unplanned / small

- **M1 exhaustive manual pass (non-blocking)** — go through [`docs/manual-test-m1.md`](docs/manual-test-m1.md)
  top to bottom and fill its Results table. The main flows were checked on 2026-11-02 and the
  app ran as expected (0.2.0 shipped on that basis). Still open: the full list, including the
  release build under the production CSP (A13), A4 with a 1-page book, A5–A7, A6 with a moved
  library, touch, reduced motion, the glass chrome, the A9 measurements on the Dulac corpus
  (open to first turn, RSS with texture height, fps), and a WebView2 run on Windows.
- **Trim jsonschema's network stack** — `jsonschema` 0.51 with default features pulls `reqwest`, `rustls` and AWS-LC (C) into the app only to resolve remote `$ref`s, which the embedded book schemas never use. Try `default-features = false` and drop AWS-LC from `THIRD_PARTY_NOTICES.md` if it goes.
- **Upgrade bundled OpenJPEG to 2.5.4+** — `openjpeg-sys` 1.0.12 bundles 2.5.3, which has CVE-2025-54874 (heap write on a truncated JP2 via `opj_read_header`). Watch for a newer `openjpeg-sys`, or use a `[patch.crates-io]` with a 2.5.4 vendor tree. Until then, `ci/bundled-c-exceptions.toml` excepts the CVE until 2026-11-29 so the weekly check fails again if nothing moves; extend it only with a re-check. Reading the C source shows the write is not reachable through `jpeg2k`: the write needs an uninitialised `*p_image` and `jpeg2k` passes a null-initialised one (every failure path in 2.5.3's `opj_j2k_read_header` leaves it null). The upstream fix is commit f809b80c (`if (ret && p_image && *p_image)`). Rejecting short JP2 input is then optional hardening. See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
- **Reader notes follow-up** — Reader notes: clear a page's failure note when it later loads (after a context restore), and the engine note after a successful resize or scene update; and cap concurrent texture rebuilds in refresh() — CodeRabbit #67.
- **Cap decoded scan size** — Cap decoded scan size (createImageBitmap resize) so huge scans don't spike memory — CodeRabbit #68.
- **Sonar backlog (22 local issues)** — all in template hook and CI scripts (complexity, regexes, duplicated literals). See
  `tmp/sonar-latest-issues.md` after a scan.

- **Grey+alpha JPEG 2000 fixture** — the transparency refusal test depends on macOS `sips`
  writing two components. Generate a two-component JP2 in-test (or check in a tiny fixture)
  so it runs on every OS. From CodeRabbit on PR #65.

## Icebox

Pointers only — promote via [`prompts/backlog-to-plans.md`](prompts/backlog-to-plans.md) or move to
[`plans/deferred/`](plans/deferred/) when a real plan exists.

- **M0.5 harness follow-ups** — Rust coverage, Dependabot;
  M0 senior-review docs/harness nits (**S9**–**S11**); raise pytest `fail_under` (**LC-L3**) →
  [roadmap M0.5](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri--remaining-quality)
- **M1 format/load hardening** — **A10–A13**, CI `cargo build` (**LC-M2**), CSP tracking (**LC-M3**),
  optional `pageLabel` maxLength (**LC-T11**) →
  [M1 plan follow-ups](plans/archive/completed/2026-08-23-m1-scan-viewer.md#follow-ups-from-m0-senior-reviews-2026-08-25)
- **Second skin (Gallery Daylight, mockup B) + skin picker** — M7 →
  [`design/DESIGN.md`](design/DESIGN.md#skins)
- **Later product (M1.5–M8)** — PDF import, OCR and curator-supplied text (M2), bookshelf
  inventory and `.scriptorium` export/import (M5), expandable
  annotations and curator detail views (M6), fullscreen page view and deep zoom (M7), text/hybrid
  modes, library, polish, web/IIIF →
  [roadmap](plans/2026-08-23-product-roadmap.md)
- **cargo-audit unmaintained warnings** — 6 warnings, `cargo audit` still exits 0 in
  `src-tauri`: `proc-macro-error` 1.0.4 via glib-macros → glib → gtk → tauri
  (`RUSTSEC-2024-0370`, Linux-only target tree, no patched release; advisory alternatives are
  `manyhow` and `proc-macro2-diagnostics`), and `unic-char-property`, `unic-char-range`,
  `unic-common`, `unic-ucd-ident`, `unic-ucd-version` 0.9.0 via urlpattern → tauri-utils.
  Do **not** add them to `audit.toml` — that file is for vulnerabilities with a dated excuse,
  and ignoring a warning changes nothing. Re-check when those parents drop them.
- **cargo Dependabot cooldown** — keep cargo `cooldown.default-days: 7` (version updates only;
  security updates ignore cooldown). Version updates moved to a monthly, grouped schedule
  for all ecosystems on 2026-10-27. Daily
  `schedule.interval` rejected 2026-10-24 as too noisy for version-update PRs. Revisit only if
  another RustSec advisory turns `Dependency audit` red before a Dependabot security PR exists.
  Manual path: [CI & hooks → when audit goes red](docs/ci-and-hooks.md#when-dependency-audit-goes-red)
