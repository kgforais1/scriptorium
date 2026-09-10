# Changelog

All notable **user-facing** changes are documented here.
Developer-only detail lives in [`CHANGELOG.dev.md`](CHANGELOG.dev.md).
See [`policies/changelog-conventions.md`](policies/changelog-conventions.md).

The format follows [Keep a Changelog](https://keepachangelog.com/), and this project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Public repository posture: [`SECURITY.md`](SECURITY.md) for vulnerability reporting; README reflects public/early-dev status.
- Tauri 2 desktop shell (Vite + React + TypeScript) at repo root; book package format v1 spec and JSON Schema ([`docs/book-format.md`](docs/book-format.md), [`docs/book-format.schema.json`](docs/book-format.schema.json)); Rust types + schema-bind test; deterministic fixture generator.

### Changed
- GitHub repository ownership transferred to [`kgforais1/scriptorium`](https://github.com/kgforais1/scriptorium); security reporting links updated in [`SECURITY.md`](SECURITY.md).
- CI dependency audit is a merge gate (`Dependency audit` job); see [`docs/ci-and-hooks.md`](docs/ci-and-hooks.md).
- Book package schema tightened: page images must be at least 1×1; page `file` paths reject traversal/absolutes (including `.` / `..` segments and trailing periods); book `id` and page path components reject Windows-reserved device names; page paths must be unique case-insensitively; `title`/`rights`/`attribution` length bounds and `pages` capped at 100000; numeric fields bounded to Rust integer ranges; book `id` constrained to a portable library directory slug; unknown JSON keys rejected at deserialize; `createdAt`/`updatedAt` must be RFC 3339; loaders must clamp or reject out-of-range `lastReadPage` and enforce `pages[i].index == i` / non-zero `byteSize` ([`docs/book-format.md`](docs/book-format.md)).
- Book format v1 evolution: because loaders reject unknown keys, adding any new `meta.json` field requires a `formatVersion` bump (not silent additive extension) ([`docs/book-format.md`](docs/book-format.md#evolution-rule)).

## [0.1.2] - 2026-08-23

### Added
- CI fast lane (`.github/workflows/ci.yml`): policy, lint/complexity, basedpyright, pytest coverage, gitleaks.
- Security slow lane (`.github/workflows/security.yml`): Semgrep on PRs, TruffleHog weekly.
- Pre-push **basedpyright** typecheck; install with `--hook-type pre-push`.
- Roadmap **M0.5** harness follow-ups (TS/Rust CI, coverage gates, CodeQL/Dependabot).
- `docs/ci-and-hooks.md`, `pyproject.toml`, `pyrightconfig.json`, gitignored `tmp/`.

### Changed
- Ruff enforces complexity (`C901`, branches, statements) per file-size policy.
- `VERSION` → 0.1.2.

## [0.1.1] - 2026-08-23

### Added
- Product roadmap (M0–M8): scan viewer, OCR-aware scans, text/hybrid modes, library,
  authoring/hotspots, polish, platform/interop — [`plans/2026-08-23-product-roadmap.md`](plans/2026-08-23-product-roadmap.md).
- Active MVP scaffold plan and root backlog [`to_do.md`](to_do.md).
- Domain inventory for virtual books / flipbooks / OCR / annotations
  (`inventory/virtual-books-flipbook.md`).

### Changed
- Harness trimmed to standard secret hygiene only; unused domain-specific data-gate
  prompts, policies, hooks, and CI examples removed. Project profile captured for
  Scriptorium (Tauri-first virtual book app).

## [0.1.0] - 2026-08-23

### Added
- Initialized **Scriptorium** from `kgrizz-git/project-seed-template` (harness baseline:
  prompts, policies, hooks, CI examples, inventories). Private GitHub remote created;
  full project bootstrap still pending.
