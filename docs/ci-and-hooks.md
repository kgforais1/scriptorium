# Scriptorium — CI & hooks

Last reviewed: 2026-10-24

## Recommendation (what runs where)

| Check | Pre-commit | Pre-push | CI fast (`ci.yml`) | CI slow (`security.yml`) |
|---|---|---|---|---|
| **File line count** | ✅ `check-file-size` | — | ✅ all tracked files | — |
| **Living backlog size** | ✅ `check-todo-limits` | — | ✅ `to_do.md` | — |
| **Backlog ↔ plans sync** | ✅ `check-todo-plan-sync` (advisory) | — | ✅ advisory (allowed to fail) | — |
| **Complexity** | ✅ ruff `C901` and `PLR0912` | — | ✅ same | — |
| **Secrets** | ✅ gitleaks + private-key | — | ✅ gitleaks full history | TruffleHog weekly (advisory) |
| **Lint** | ✅ ruff, markdownlint, shellcheck | — | ✅ same + actionlint | — |
| **Type check (Python)** | — | ✅ basedpyright | ✅ basedpyright | — |
| **Rust fmt / clippy / test** | — | — | ✅ required from **M0** once `src-tauri/` exists | — |
| **Type check (TypeScript)** | — | — | ✅ `pnpm exec tsc --noEmit` | — |
| **Audit (pnpm + cargo)** | — | — | ✅ required (`Dependency audit`; vulns fail, informational warnings OK) | ✅ weekly re-run (`Dependency audit (weekly)`) |
| **Tests + coverage** | — | — | ✅ pytest-cov (report; gate later) | — |
| **SAST** | — | — | — | ✅ Semgrep on PR (whole repo; templates/scratch in `.semgrepignore`) |
| **CodeQL** | — | — | — | later — [M0.5](../plans/2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri-scaffold--when-app-code-exists) |

**Later (M0.5):** Vitest coverage gates, TS eslint pre-push, CodeQL/Dependabot. **Rust fmt/clippy/test belong in M0**, not M0.5 — see foundation plan.

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
- `Type check (TypeScript)` — `pnpm install` + `tsc --noEmit`
- `Dependency audit` — `pnpm audit --prod` + `cargo audit` (uses `src-tauri/.cargo/audit.toml`; fails on vulnerabilities only)
- `CodeQL` — GitHub **default setup** (`dynamic/github-code-scanning/codeql`), not a workflow file in this repo; already a ruleset-required check

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
`ci/scripts/pnpm-audit.sh` retries registry-shaped `pnpm audit` output (default 3 attempts,
`PNPM_AUDIT_ATTEMPTS` 1–5). Exhausted retries fail with `Audit did not run`. A corepack or
missing-pnpm failure reports `Audit tooling unavailable` (missing `pnpm` exits 127).
`corepack prepare` is retried as a registry download, not classified by output. Those
annotations are infrastructure: re-run the job, do not bump a package, and do not read the
run as a clean audit. A non-transient failure with no registry error reports
`pnpm audit failed`; read the log above that annotation before changing a dependency. If the
annotation is `Vulnerabilities found`, the cargo steps below do not apply: bump the production
package and re-run `pnpm audit --prod`. This repo has no pnpm ignore file.

Dependabot security updates are not a backstop until GitHub has imported the advisory. A
compatible bump can still arrive as a version update after `cooldown.default-days`, on the
cargo schedule (weekly Thursday), with no advisory import. As of 2026-10-24,
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
