# Developer Changelog

Internal / developer-facing changes that do not belong in the public
[`CHANGELOG.md`](CHANGELOG.md). See [`policies/changelog-conventions.md`](policies/changelog-conventions.md).

## [Unreleased]

### Added
- Public-repo docs: `SECURITY.md`; README no longer marks the project private. GitHub description/topics updated via `gh` (not in-tree).
- Time-bound cargo-audit ignores: `src-tauri/.cargo/audit.toml` + `audit-exceptions.toml`, enforced by `ci/scripts/check_audit_exceptions.py` in the required **Policy** CI job (RUSTSEC-2024-0429 / glib via GTK expires 2026-11-27). Unit tests: `tests/test_check_audit_exceptions.py`, `tests/test_check_doc_links.py`. CLI `--audit`/`--exceptions` paths must resolve under `--root` (default cwd) to satisfy Sonar path-escape rule S8707. `--today` and exception `expires` accept only dashed `YYYY-MM-DD` (reject compact/week ISO forms accepted by Python 3.11+ `fromisoformat`).
- Fixture-generator unit tests (`tests/test_generate_fixture_book.py`); Rust round-trip test that deserializes generator `meta.json` into `BookMeta`.
- `BookMeta::validate` / `PageEntry::validate` for `lastReadPage` bounds, path safety, and non-zero dimensions (implementation detail for the public format rules in [`CHANGELOG.md`](CHANGELOG.md)).
- CI jobs: `Rust (src-tauri)` (cargo fmt/clippy `-D warnings`/test/check, with cargo cache), `Type check (TypeScript)` (pnpm + tsc), `Dependency audit` (`pnpm audit --prod` + `cargo audit`).
- pnpm v11 build-script approval via `pnpm-workspace.yaml`; Node 24 pinned (`engines` / `.nvmrc`); `tests/fixtures/README.md`.
- Schema-bind / lockstep tests for Rust `BookMeta` (jsonschema 0.51.0); `cargo fmt` clean.

### Changed
- GitHub owner is now `kgforais1/scriptorium`: local `origin` URL, `AGENTS.md` protect-remotes note, `SECURITY.md` advisory/profile links, and `src-tauri/Cargo.toml` `authors`.
- CI **Dependency audit** job: run `cargo audit` directly (respects `src-tauri/.cargo/audit.toml`), drop `rustsec/audit-check` (Check API permission failures on `main`), remove `continue-on-error` / `pnpm audit || true` so the job is a real gate. Cache pinned `cargo-audit` 0.22.2 binary.
- Removed unused seed-template `scaffolds/` trees (Next.js, Workers, Go, Rust CLI, Python). Scriptorium ships Tauri + Vite/React; example lockfiles only fed Dependabot/Trivy noise.
- Link checker: normalize trailing-dot FQDN hostnames before `SKIP_HOSTS` / GitHub redirect checks (`ci/scripts/check_doc_links.py`).
- CI: install Tauri Linux system deps in `Rust (src-tauri)`; point `cargo audit` at `src-tauri/` (config under `src-tauri/.cargo/`); fix plan archive relative links broken by M0 move; pin Rust via `rust-toolchain.toml` (`1.96.0`); install `jsonschema` in the pytest job so schema conformance cannot silently skip (**S1**).
- Book schema / Rust validate (dev): table-driven negative path/index tests; slug lockstep tests (Rust + Python vs schema); structural validate documents M1 checksum gap (wrong-but-valid-shape `sha256` accepted); review IDs **S2–S6**/SEC5. User-visible format/schema rules are listed once in [`CHANGELOG.md`](CHANGELOG.md) and [`docs/book-format.md`](docs/book-format.md).
- Fixture generator: wipe and recreate `pages/` on regenerate (clears non-PNG orphans too); write `meta.json`/`annotations.json` as UTF-8 LF bytes for cross-platform determinism; CLI validates book-id dirname before mkdir; reject `--title` / `--pages` / `rights` / `attribution` outside schema `maxLength` / `maxItems` before creating the output dir.
- Documented page `file` schema-pattern invariants in `docs/book-format.md`.
- LongCat review follow-ups tracked: **LC-M2** (`cargo build` in CI), **LC-M3**/A13 CSP, **LC-L3**/L4 coverage+audit → M1 plan / M0.5 icebox.
- `annotations.json` empty form canonicalized to `[]` (`{}` back-compat only).
- Removed unused Tauri `greet` command; Cargo.toml `authors` set to `kgrizz-git`.
- Doc freshness: bumped Last reviewed on AGENTS/README/`to_do`/NAVIGATION; README covers fixture generator + book format.
- `.gitignore`: dropped `Cargo.lock` ignore (lockfile tracked); anchored `Icon?` to repo root (`/Icon?`) to stop swallowing `src-tauri/icons/` on case-insensitive macOS; added `src-tauri/gen/schemas/` and `.cargo-target/`.
- `docs/ci-and-hooks.md`: added Rust, TS typecheck, and audit to the recommendation table and required-checks list.
- M0 plan archived to [`plans/archive/completed/2026-08-23-m0-tauri-foundation.md`](plans/archive/completed/2026-08-23-m0-tauri-foundation.md); `to_do.md` updated to remove M0 from Next Up and Active.
- `to_do.md` is now strictly actionable: completed work is removed rather than retained in a `Recently done` section, and every meaningful completion is logged once.
- Semgrep (`security.yml`) scans the whole repo. Path skips are in `.semgrepignore` (`ci/examples/`, scratch dirs). Live `.github/` workflows are included and SHA-pinned; Dependabot has an explicit 7-day `cooldown` so Semgrep's Dependabot rule passes.
- Changelog routing: user-facing security → `CHANGELOG.md`; CI/SAST harness → `CHANGELOG.dev.md`; operational maintenance only → `MAINTENANCE.md` (no double-logging). Aligned `templates/maintenance-log.md` with the same tree.

## [0.1.2] - 2026-08-23

### Added
- `.github/workflows/ci.yml` (policy, ruff+complexity, basedpyright, pytest+coverage report, gitleaks).
- `.github/workflows/security.yml` (Semgrep on PR; TruffleHog weekly advisory).
- Pre-push hook: `basedpyright` (`stages: [pre-push]`); `default_stages: [pre-commit]`.
- Roadmap M0.5 + `to_do.md` sections for post-Tauri CI/hooks follow-ups.
- `docs/ci-and-hooks.md`, `pyproject.toml`, `pyrightconfig.json`, `tmp/README.md`.

### Changed
- Ruff complexity rules aligned with `policies/file-size-and-counts.md`.
- Removed `template-checks.yml` (superseded by `ci.yml`).
- Formatted four legacy Python scripts for `ruff format --check`.
- M0 / pre-M1 spike / M1 plans tightened after ox-alpha review: copy collisions + orphan temps +
  checksum re-verify; natural-sort tie-break; named perf machine/corpus/fps; `lastReadPage` by
  book id; spike candidate shortlist; roadmap schema synced to M0; Rust required-check name in
  `docs/ci-and-hooks.md`.
- Backlog system: `to_do.md` **Next Up** / **Active** / **Icebox** sections; policy + agent
  guidance; advisory `check_todo_plan_sync.py` hook (CI + optional pre-commit).

## [0.1.1] - 2026-08-23

### Added
- `inventory/virtual-books-flipbook.md` and source-repo links for flipbook/IIIF/OCR candidates.

### Changed
- Removed unused domain-specific data-gate documentation, prompts, policies, hooks, and CI
  examples from the harness. Data classification is public/internal/confidential with standard
  secret hygiene (gitleaks).
- Project profile written (Tauri-first, hub-and-spoke, cultural-heritage page imagery).

## [0.1.0] - 2026-08-23

### Added
- New project cut from `project-seed-template` at template version **0.4.4**. Fresh
  git history; private `origin` → `kgrizz-git/scriptorium`. Template changelog history
  remains in the seed repo, not carried forward here.
