# Scriptorium backlog

Last reviewed: 2026-08-26

Agents: start at **Next Up** (3–5 pointer lines). Detail lives in linked plans, not here.
Policy: [`policies/plans-and-todos.md`](policies/plans-and-todos.md).

## Next Up (keep 3–5; pointers only — not full checklists)

- [Pre-M1 flipbook spike](plans/2026-08-23-pre-m1-flipbook-spike.md) — ADR gates G1–G5 after M0 window exists
- [M1 scan viewer](plans/2026-08-23-m1-scan-viewer.md) — blocked on spike ADR; acceptance A1–A13
- [M0.5 harness follow-ups](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri-scaffold--when-app-code-exists) — TS lint, coverage gates, S9–S11

## Active plans

| Plan | Status | Note |
|---|---|---|
| [Product roadmap](plans/2026-08-23-product-roadmap.md) | living | M0–M8 horizon; not a task queue |
| [Pre-M1 flipbook spike](plans/2026-08-23-pre-m1-flipbook-spike.md) | draft | Blocks M1 |
| [M1 scan viewer](plans/2026-08-23-m1-scan-viewer.md) | draft | First product slice after M0 + spike |

## Unplanned / small

_(none — use this section only for work that fits in one session and does not warrant a plan.)_

## Icebox

Pointers only — promote via [`prompts/backlog-to-plans.md`](prompts/backlog-to-plans.md) or move to
[`plans/deferred/`](plans/deferred/) when a real plan exists.

- **M0.5 harness follow-ups** — TS lint pre-push, Vitest coverage gates, Dependabot;
  M0 senior-review docs/harness nits (**S9**–**S11**); raise pytest `fail_under` (**LC-L3**);
  harden audit job (**LC-L4**) →
  [roadmap M0.5](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri-scaffold--when-app-code-exists)
- **M1 format/load hardening** — **A10–A13**, CI `cargo build` (**LC-M2**), CSP tracking (**LC-M3**),
  optional `pageLabel` maxLength (**LC-T11**) →
  [M1 plan follow-ups](plans/2026-08-23-m1-scan-viewer.md#follow-ups-from-m0-senior-reviews-2026-08-25)
- **Later product (M2–M8)** — OCR, text/hybrid modes, library, hotspots, polish, web/IIIF →
  [roadmap](plans/2026-08-23-product-roadmap.md)
- **cargo-audit unmaintained warnings** — 6 warnings, `cargo audit` still exits 0 in
  `src-tauri`: `proc-macro-error` 1.0.4 via glib-macros → glib → gtk → tauri
  (`RUSTSEC-2024-0370`, Linux-only target tree, no patched release; advisory alternatives are
  `manyhow` and `proc-macro2-diagnostics`), and `unic-char-property`, `unic-char-range`,
  `unic-common`, `unic-ucd-ident`, `unic-ucd-version` 0.9.0 via urlpattern → tauri-utils.
  Do **not** add them to `audit.toml` — that file is for vulnerabilities with a dated excuse,
  and ignoring a warning changes nothing. Re-check when those parents drop them.
- **cargo Dependabot cooldown** — keep cargo `cooldown.default-days: 7` (version updates only;
  security updates ignore cooldown) and the weekly Thursday schedule. Daily
  `schedule.interval` rejected 2026-10-24 as too noisy for version-update PRs. Revisit only if
  another RustSec advisory turns `Dependency audit` red before a Dependabot security PR exists.
  Manual path: [CI & hooks → when audit goes red](docs/ci-and-hooks.md#when-dependency-audit-goes-red)
