# Scriptorium — CI & hooks

Last reviewed: 2026-10-30

## Recommendation (what runs where)

| Check | Pre-commit | Pre-push | CI fast (`ci.yml`) | CI slow (`security.yml`) |
|---|---|---|---|---|
| **File line count** | ✅ `check-file-size` | — | ✅ all tracked files | — |
| **Living backlog size** | ✅ `check-todo-limits` | — | ✅ `to_do.md` | — |
| **Backlog ↔ plans sync** | ✅ `check-todo-plan-sync` (advisory) | — | ✅ advisory (allowed to fail) | — |
| **Complexity** | ✅ ruff `C901` and `PLR0912`; ESLint `complexity` (`ts: eslint`) | — | ✅ same, plus clippy `cognitive_complexity` in `Rust (src-tauri)` | — |
| **Secrets** | ✅ gitleaks + private-key | — | ✅ gitleaks full history | TruffleHog weekly (advisory) |
| **Lint** | ✅ ruff, markdownlint, shellcheck | — | ✅ same + actionlint | — |
| **Lint (TypeScript)** | ✅ `ts: eslint` on staged `.ts`/`.tsx` | — | ✅ `pnpm lint` (`Lint & test (TypeScript)`) | — |
| **Doc comments** | ✅ `ts: eslint` (`jsdoc/require-jsdoc`, exports only) | — | ✅ same, plus `missing_docs` in `Rust (src-tauri)` | — |
| **Type check (Python)** | — | ✅ basedpyright | ✅ basedpyright | — |
| **Rust fmt / clippy / test** | — | — | ✅ required from **M0** once `src-tauri/` exists | — |
| **Type check (TypeScript)** | ✅ `types: tsc` on `.ts`/`.tsx` changes | — | ✅ `pnpm typecheck` (app and config files) | — |
| **Audit (pnpm + cargo)** | — | — | ✅ required (`Dependency audit`; informational findings pass, audit and tooling failures fail closed) | ✅ weekly re-run (`Dependency audit (weekly)`) |
| **Tests + coverage** | — | — | ✅ pytest-cov (report; gate later) | — |
| **Tests + coverage (TypeScript)** | — | — | ✅ `pnpm test:coverage` (Vitest; lines ≥ 80 on `src/lib/**`) | — |
| **SAST** | — | — | — | ✅ Semgrep on PR (whole repo; templates/scratch in `.semgrepignore`) |
| **SonarQube (local, opt-in)** | `sonar-freshness` (no-op unless `SONAR_FRESHNESS_GATE=1`) | `sonar-freshness-push` (same opt-in) | — never | — never |
| **CodeQL** | — | — | — | ✅ already required: GitHub default setup, ruleset code scanning (not a job in `security.yml`) |

SonarQube is local and manual: see [`sonarqube.md`](sonarqube.md). CI never needs `SONAR_TOKEN`.

**Later (M0.5):** Dependabot. CodeQL is already the ruleset code-scanning requirement above, not an M0.5 follow-up. **Rust fmt/clippy/test belong in M0**, not M0.5 — see foundation plan.

## Quick start

```bash
pip install pre-commit
pre-commit install --hook-type pre-commit --hook-type pre-push
pre-commit run --all-files
pre-commit run --all-files --hook-stage pre-push   # basedpyright
```

## Required GitHub checks (suggested)

After the first green `CI` run, set default-branch ruleset required checks to:

- `Policy (file size, docs, backlog)`
- `Lint & format`
- `Type check (Python)`
- `Tests & coverage`
- `Secret scan (gitleaks)`
- `SAST — Semgrep` (from Security workflow)
- `Rust (src-tauri)` — **required from M0** once `src-tauri/` exists (`cargo fmt` / `clippy` / `test` / `check`; exact job name must match `ci.yml`)
- `Rust (windows-latest)` and `Rust (macos-latest)` — the `rust-cross` matrix (`clippy --all-targets`, `test`). Not required yet; make them required once they have been green on a few PRs
- `Type check (TypeScript)` — `pnpm install` + `pnpm typecheck` (app and config files)
- `Lint & test (TypeScript)` — `pnpm install` + `pnpm lint` (ESLint) + `pnpm test:coverage` (Vitest, lines ≥ 80 on `src/lib/**`)
- `Dependency audit` — `pnpm audit --prod` + `cargo audit` (uses `src-tauri/.cargo/audit.toml`). Informational findings pass. The job fails closed on a vulnerability and when the audit did not complete: an advisory-database fetch failure, a command error, or exhausted registry retries.
- `CodeQL` — GitHub **default setup** (`dynamic/github-code-scanning/codeql`). Already required by the branch ruleset as code scanning (alert threshold errors; security alerts high or higher), not a status-check context and not a workflow file

`Dependency audit` replaced the old `Audit (advisory)` job (`rustsec/audit-check` + `continue-on-error`), which failed spuriously on `main` for missing `checks: write` while still going green on PRs.

### When `Dependency audit` goes red

The job in `.github/workflows/ci.yml` fails if either step fails: `pnpm audit --prod`, then
`cargo audit` in `src-tauri`. Read the failing step name in the log before doing anything else.

`cargo audit` (CI installs `cargo-audit` 0.22.2) fails this job for a vulnerability in
`src-tauri/Cargo.lock`. It also exits non-zero when the advisory database cannot be fetched
or the audit command itself errors — read that error before changing a dependency.
Unmaintained, unsound, notice, and yanked findings are warnings that exit 0 — this job
passes no `--deny`. Do not suppress a warning to clear this check.

If the failing step is `pnpm audit --prod`, read the annotation before bumping anything.
`ci/scripts/pnpm-audit.sh` installs the pnpm version from `package.json` `packageManager`
and retries only registry-shaped output (default 3 attempts, `PNPM_AUDIT_ATTEMPTS` 1–5).
Exhausted `pnpm audit` retries fail with `Audit did not run`. A failed `corepack enable`
or a missing `pnpm` reports `Audit tooling unavailable` (missing `pnpm` exits 127).
`corepack prepare` retries only when its output is a registry error; after those retries
it reports `Audit tooling unavailable`. A permanent prepare or `packageManager` failure
reports `pnpm setup failed` — read that log and fix the toolchain, and do not rerun the
job as if it were a registry flake. `Audit did not run` and an exhausted registry
`Audit tooling unavailable` are infrastructure: re-run the job, do not bump a package,
and do not read the run as a clean audit. A non-transient `pnpm audit` failure reports
`pnpm audit failed`; read the log above that annotation before changing a dependency. If the
annotation is `Vulnerabilities found`, the cargo steps below do not apply. Inspect each
affected production dependency in the log and choose the update that contains the fix:
a lockfile update when a patched version is already inside the declared range, or a
`package.json` range change plus a lockfile update when the range blocks it. Re-run
`pnpm audit --prod`. This repo has no pnpm ignore file.

Dependabot security updates are not a backstop until GitHub has imported the advisory. A
compatible bump can still arrive as a version update after `cooldown.default-days`, on the
cargo schedule (monthly, grouped into one minor/patch PR), with no advisory import, so
that path can take up to about five weeks. Do not wait for it; apply the fix by hand. As of 2026-10-24,
RUSTSEC-2026-0382 (published 2026-10-20) and RUSTSEC-2026-0383 (published 2026-10-19), both
aliased to `CVE-2026-111943`, were absent from GitHub's advisory database, and this repo had
neither a Dependabot alert nor a Dependabot security-update PR for `rustls-webpki` or
`webpki-root-certs` (PR #49 is the hand-written fix). Re-check with
`gh api "/advisories?cve_id=CVE-2026-111943"` before assuming that gap is still open.
`cooldown` does not delay security updates; it only delays version updates.

The weekly `Dependency audit (weekly)` job (`security.yml`, Mondays 04:00 UTC, skipped on pull
requests) re-runs both audits plus the expiry script, so a quiet repo turns red within a week.
It is a separate Security workflow run, not the ruleset-required `Dependency audit` check.

For a `cargo audit` vulnerability, work in `src-tauri` and decide from the patched version in
the log:

1. **Patched version is inside the current requirement.** `cargo update -p <crate> --dry-run`
   and apply only if the dry-run moves that crate to the patched version — `Locking 0 packages`
   means the compatible set did not move. Drop `--dry-run` to write `src-tauri/Cargo.lock`.
2. **Patched version is outside the requirement, and the bound is a direct dependency this
   repo controls.** Bump it in `src-tauri/Cargo.toml`, then update the lockfile. That is a
   manifest change, not a suppression.
3. **No version you can select contains the fix, because a third-party requirement blocks it.**
   Existing case: `RUSTSEC-2024-0429` (glib 0.20.0 contains the fix; gtk 0.18, pulled in by
   tauri 2.11.5, does not allow it; `expires = "2026-11-27"`). Add the ID
   under `advisories.ignore` in `src-tauri/.cargo/audit.toml` and a matching `id` / `expires` /
   `reason` row in `src-tauri/.cargo/audit-exceptions.toml`. `cargo audit` stays green while
   the ignore is present; required `Policy (file size, docs, backlog)` fails once `today` is
   after `expires`.

Then `cargo audit` (expect exit 0) and `cargo test`, both from `src-tauri`. PR the lockfile —
and `Cargo.toml` when step 2 applied.

Worked example: RUSTSEC-2026-0382/0383, lockfile-only, PR #49.

## Thresholds

- Source lines: soft **600** / hard **1000** — [`policies/file-size-and-counts.md`](../policies/file-size-and-counts.md)
- Complexity: cyclomatic **15**, branches **15**, statements **100** — `ruff.toml`
- Coverage: **reported** on `hooks/scripts` + `ci/scripts` today; raise `fail_under` after app tests (M0.5)

## Temp files

Use gitignored [`tmp/`](../tmp/) for scratch runs; never commit large scan corpora.
