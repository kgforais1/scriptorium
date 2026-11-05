# Scriptorium backlog

Last reviewed: 2026-11-05

Agents: start at **Next Up** (3–5 pointer lines). Detail lives in linked plans, not here.
Policy: [`policies/plans-and-todos.md`](policies/plans-and-todos.md).

## Next Up (keep 3–5; pointers only — not full checklists)

- [M1 hardening](plans/2026-11-05-m1-hardening.md) — CI embed build (LC-M2), A10–A12 confirmation and static CSP test, asset-protocol verdict, `pageLabel` cap, trim `jsonschema` HTTP stack
- [M1 follow-ups](plans/2026-11-02-m1-followups.md) — manual script (macOS exhaustive, A9 Dulac), Windows WebView2 release build, ADR-0001 open platform items, layout default, `rust-cross` ruleset
- [Drop the openjpeg-sys patch](#unplanned--small) — when an `openjpeg-sys` release bundles OpenJPEG 2.5.4
- [M0.5 harness follow-ups](plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri--remaining-quality) — Rust coverage, Python `fail_under`, Dependabot, S9–S11

## Active plans

| Plan | Status | Note |
|---|---|---|
| [Product roadmap](plans/2026-08-23-product-roadmap.md) | living | M0–M8 horizon; not a task queue |
| [M1 follow-ups](plans/2026-11-02-m1-followups.md) | in-progress | Manual and platform remainder after 0.2.0 |
| [M1 hardening](plans/2026-11-05-m1-hardening.md) | in-progress | Agent-doable hardening (CI, CSP pytest, label cap, jsonschema) |

## Unplanned / small

- **Trim jsonschema's network stack** — `jsonschema` 0.51 with default features pulls `reqwest`, `rustls` and AWS-LC (C) into the app only to resolve remote `$ref`s, which the embedded book schemas never use. Try `default-features = false` and drop AWS-LC from `THIRD_PARTY_NOTICES.md` if it goes.
- **Drop the openjpeg-sys patch** — `src-tauri/Cargo.toml` patches `openjpeg-sys` 1.0.12 to a fork commit that bundles OpenJPEG 2.5.4 (fixes CVE-2025-54874). Upstream PR [Enet4/openjpeg-sys#5](https://github.com/Enet4/openjpeg-sys/pull/5) asks for a release with 2.5.4. When one ships, bump the crate, delete the `[patch.crates-io]` block, and update `THIRD_PARTY_NOTICES.md`.
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
