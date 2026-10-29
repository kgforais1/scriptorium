# Developer Changelog

Internal / developer-facing changes that do not belong in the public
[`CHANGELOG.md`](CHANGELOG.md). See [`policies/changelog-conventions.md`](policies/changelog-conventions.md).

## [Unreleased]

### Added
- Input-format conversion for M1 ingest (`src-tauri/src/ingest/`): `inspect` classifies a JPEG, PNG, WebP, BMP, GIF, TIFF, or JPEG 2000 file and decides whether it needs a converted display copy; `display_frame` produces it (8-bit sRGB, upright, JPEG q92 or PNG). Covers CMYK with an ICC profile (rejected without one), 16-bit, non-sRGB ICC, EXIF and TIFF orientation, multi-page TIFF, and GIF first frame. New crates: `image`, `tiff`, `lcms2` (static), `jpeg2k` over bundled C OpenJPEG, `zune-jpeg`.
- Book format v2 ([`docs/book-format.md`](docs/book-format.md#originals-formatversion-2)): the schema splits into `docs/book-format.v1.schema.json` and `docs/book-format.v2.schema.json`. v2 adds optional originals (`sources[]` with `frames`, and a page `source` with `ref` and `frame`) and their validation rules. Rust reads v1 and v2 (`src-tauri/src/book_format/` is now a module with `sources.rs` and v2 tests); the fixture generator writes v2. The spec also reserves the provisional `ocr/` and `annotations.json` layout for M2 and M6.
- Pre-M1 flipbook spike complete: [ADR-0001](docs/adr/0001-flipbook-engine.md) adopts our own TypeScript port of mockup E's WebGL2 curl engine (no page-flip library) with a pad-to-uniform aspect policy. Gates G1–G6 pass on macOS. Throwaway prototype in `src/spike/flipbook/`, a `spike_pages` Tauri command compiled only when `tauri/custom-protocol` is off (`cfg(dev)`: `tauri dev` or a plain `cargo build`, never `tauri build`) with the `protocol-asset` feature, and `scripts/make-spike-fixtures.sh` for a mixed-aspect page set. `design/skins/lamplit/scene.json` gains `vignette`, `material.paper`, and `material.desk`.
- Opt-in local SonarQube: `sonar-project.properties` (local scope), `compose.sonarqube.yaml` (server on 127.0.0.1:9000), `scripts/sonar_scan.sh` and `scripts/sonar_issues.py` (manual scan and issue dump), and a freshness gate (`scripts/check_sonar_freshness.py`, pre-commit and pre-push hooks) that is a no-op unless `SONAR_FRESHNESS_GATE=1`. Never runs in CI and needs no token there. Docs: [`docs/sonarqube.md`](docs/sonarqube.md).
- Input formats planned for M1 (JPEG, PNG, WebP, BMP, GIF, TIFF, JPEG 2000; webview-safe display images, with originals kept under `originals/` only when converted; `formatVersion` 2 with book-level `sources[]`; macOS 11 floor) and a new M1.5 PDF import milestone. Public-domain test corpora and `scripts/fetch-iiif-corpus.py` documented in `tests/fixtures/README.md`. The fetch script allows only https URLs on allowlisted IIIF hosts (including redirects), writes only under `tmp/`, caps manifest, canvas, page size and download time, ties resumed pages to their source URL, and locks its output folder; offline tests in `tests/test_fetch_iiif_corpus.py`.
- Touch support planned as first-class input: `design/DESIGN.md` principle (44 px targets, no hover-only features), spike gate G1 covers one-finger drag, M1 adds touch checks, M7 deep-zoom adds pinch.
- Reader direction: mockup E (Lamplit gallery) chosen. [`design/DESIGN.md`](design/DESIGN.md) defines the skin model (one engine and layout; each skin is DTCG UI tokens plus a `scene.json` with a typed point or directional light). First skin: `design/skins/lamplit/`. Spike gains gate G6 (skin-ready, including a directional-light daylight scene); M1 gains E's layout, skin plumbing, and accessibility checks; Gallery Daylight (mockup B) is planned for M7.
- Remediation runbook for a red `Dependency audit` ([`docs/ci-and-hooks.md`](docs/ci-and-hooks.md#when-dependency-audit-goes-red)): which step failed, that a `cargo audit` vulnerability is distinct from a database-fetch error and from warning-only advisories (those exit 0), the lockfile-vs-manifest-vs-suppress decision, and the GitHub advisory-database gap that kept Dependabot from opening a security PR for RUSTSEC-2026-0382/0383 (`CVE-2026-111943`, absent as of 2026-10-24). Written after PR #49 cleared that 2-advisory red run by hand.
- Security workflow jobs: `dependency-audit` re-runs ci.yml's `pnpm audit --prod` + `cargo audit` (and the cargo-audit exception-expiry check) weekly, so a new advisory fails a scheduled run on a quiet repo; `vulnerable-actions` runs zizmor's `known-vulnerable-actions` audit on PRs and weekly, since Dependabot alerts never cover SHA-pinned actions. About 1 Linux minute per zizmor run and 1–4 per weekly audit (cargo-audit binary is cached).
- Dependabot `cargo` entry for `src-tauri/`, so Cargo.lock gets security-update PRs.
- Public-repo docs: `SECURITY.md`; README no longer marks the project private. GitHub description/topics updated via `gh` (not in-tree).
- Time-bound cargo-audit ignores: `src-tauri/.cargo/audit.toml` + `audit-exceptions.toml`, enforced by `ci/scripts/check_audit_exceptions.py` in the required **Policy** CI job (RUSTSEC-2024-0429 / glib via GTK expires 2026-11-27). Unit tests: `tests/test_check_audit_exceptions.py`, `tests/test_check_doc_links.py`. CLI `--audit`/`--exceptions` paths must resolve under `--root` (default cwd) to satisfy Sonar path-escape rule S8707. `--today` and exception `expires` accept only dashed `YYYY-MM-DD` (reject compact/week ISO forms accepted by Python 3.11+ `fromisoformat`).
- Fixture-generator unit tests (`tests/test_generate_fixture_book.py`); Rust round-trip test that deserializes generator `meta.json` into `BookMeta`.
- `BookMeta::validate` / `PageEntry::validate` for `lastReadPage` bounds, path safety, and non-zero dimensions (implementation detail for the public format rules in [`CHANGELOG.md`](CHANGELOG.md)).
- CI jobs: `Rust (src-tauri)` (cargo fmt/clippy `-D warnings`/test/check, with cargo cache), `Type check (TypeScript)` (pnpm + tsc), `Dependency audit` (`pnpm audit --prod` + `cargo audit`).
- pnpm v11 build-script approval via `pnpm-workspace.yaml`; Node 24 pinned (`engines` / `.nvmrc`); `tests/fixtures/README.md`.
- Schema-bind / lockstep tests for Rust `BookMeta` (jsonschema 0.51.0); `cargo fmt` clean.

### Changed
- CI guide: CodeQL is the existing branch-ruleset code-scanning requirement (GitHub default setup), not an M0.5 follow-up. A `Vulnerabilities found` pnpm result is a per-dependency lockfile or `package.json` update, not an automatic package bump.
- **Dependency audit** no longer dies on npm infrastructure: `pnpm audit --prod` (both `ci.yml` and the weekly `security.yml`) now runs through `ci/scripts/pnpm-audit.sh`. The pnpm version is the top-level `package.json` `packageManager` field, read as JSON so compact and multiline manifests both work. `corepack prepare` retries only registry-shaped output (default 3 attempts, capped at 5); a permanent prepare failure is `pnpm setup failed` and is not retried. `pnpm audit` output retries the same registry shapes (`error (503)`, `error (23)`, `TimeoutError`, `HTTP 503`, and pnpm 11's `ERR_PNPM_AUDIT_BAD_RESPONSE` text `responded with 5xx`). `corepack enable` is not retried. Exhausted registry retries fail closed with `Audit did not run`; a broken toolchain reports `Audit tooling unavailable`; a missing `pnpm` exits 127. A non-transient failure with no registry error reports `pnpm audit failed`. A report containing `vulnerabilities found` exits immediately with pnpm's status, even if it also mentions a 503. `PNPM_AUDIT_ATTEMPTS` and `PNPM_AUDIT_DELAY` accept integers of at most 3 digits. Contract tests: `tests/test_pnpm_audit.py`. Motivation: on 2026-10-22 four runs went red on 503s and client timeouts with no vulnerability present. CI shellcheck now uses one `scandir` step per directory (`find "$INPUT_SCANDIR"` is a single path; the old space-separated value matched nothing and still exited 0).
- Dependabot: replaced the semver-major `ignore` rules with `allow` (minor/patch) so version updates still skip majors but security updates that need a major are no longer suppressed; removed the `docker` entry (no Dockerfile in the repo).
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
