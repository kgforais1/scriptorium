# Scriptorium backlog

Last reviewed: 2026-10-28

Agents: start at **Next Up** (3–5 pointer lines). Detail lives in linked plans, not here.
Policy: [`policies/plans-and-todos.md`](policies/plans-and-todos.md).

## Next Up (keep 3–5; pointers only — not full checklists)

- [M1 scan viewer](plans/2026-08-23-m1-scan-viewer.md) — engine per [ADR-0001](docs/adr/0001-flipbook-engine.md); acceptance A1–A13
- [M1 Phase 0 engine gate](plans/2026-08-23-m1-scan-viewer.md#phase-0--integration-gate-adr-already-done) — ADR-0001 open items: Windows/Linux webviews, real touch, WKWebView fps, RSS
- [M0.5 harness follow-ups](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri-scaffold--when-app-code-exists) — Rust coverage, Python `fail_under`, Dependabot, S9–S11

## Active plans

| Plan | Status | Note |
|---|---|---|
| [Product roadmap](plans/2026-08-23-product-roadmap.md) | living | M0–M8 horizon; not a task queue |
| [M1 scan viewer](plans/2026-08-23-m1-scan-viewer.md) | in-progress | Phase 0 gate open; Phases 1–2 done (manual checks pending); Phase 3 reader in progress on `m1/phase3-reader` |

## Unplanned / small

- **M1 manual smoke (non-blocking)** — run [`docs/manual-test-m1.md`](docs/manual-test-m1.md):
  release build under the production CSP (A13), A5–A7, and one Windows release run. Does not
  block merging Phase 2 PRs or starting Phase 3.
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
  [roadmap M0.5](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri-scaffold--when-app-code-exists)
- **M1 format/load hardening** — **A10–A13**, CI `cargo build` (**LC-M2**), CSP tracking (**LC-M3**),
  optional `pageLabel` maxLength (**LC-T11**) →
  [M1 plan follow-ups](plans/2026-08-23-m1-scan-viewer.md#follow-ups-from-m0-senior-reviews-2026-08-25)
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
