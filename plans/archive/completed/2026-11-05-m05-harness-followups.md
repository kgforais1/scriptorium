# Plan: M0.5 harness follow-ups and plan upkeep

Last reviewed: 2026-11-05
Date: 2026-11-05
Author: Claude (Opus 5.5)
Status: complete
Linked issue/PR: [#78](https://github.com/kgforais1/scriptorium/pull/78) (follows [#77](https://github.com/kgforais1/scriptorium/pull/77))

## Goal

Close the open [M0.5 roadmap items](../../2026-08-23-product-roadmap.md#m05--harness-follow-ups-after-tauri--remaining-quality)
and archive the finished M1 hardening plan, in one PR. Most M0.5 items are already done
and only need ticking with evidence. The real work is a Python coverage floor, a Rust
coverage report, and a few stale-wording fixes.

## Out of scope

- The manual M1 checks (macOS script, A9 on Dulac, Windows WebView2, release-build CSP
  console, ADR-0001 platform items, layout default). They need a person and real hardware,
  and stay open in [`2026-11-02-m1-followups.md`](../../2026-11-02-m1-followups.md).
- Changing the GitHub branch ruleset. That is a repo setting, not a file. Chunk 4 only
  proposes the change, and the user applies it or approves it after the PR.
- Prettier. See the alternatives table.

## Current state (checked 2026-11-05)

| Roadmap item | State | Evidence |
|---|---|---|
| TS lint in CI + hooks | CI and pre-commit done | `ci.yml` `Lint & test (TypeScript)`; `.pre-commit-config.yaml` `ts: eslint`, `types: tsc` |
| Rust coverage | Not started | `cargo llvm-cov` not installed; no CI step |
| Python `fail_under` (LC-L3) | `0` | `pyproject.toml:20`; total today is 48% |
| Dependabot | Done | `.github/dependabot.yml`: pip, npm, github-actions, cargo |
| Ruleset required checks | Mostly done | Ruleset 21519272 requires 9 checks including Semgrep. Missing: `Lint & test (TypeScript)`, `Third-party licences`, `Rust (windows-latest)`, `Rust (macos-latest)` |
| `cargo fmt/clippy/test` in M0 note | Done | `ci.yml` `rust` job |
| S9 | Open | Archived M0 plan has 24 unticked boxes; roadmap line 51 ticks "app-data library root" |
| S10 | Done | `pyproject.toml:4` description is current; the README `.nvmrc` hedge is gone (`docs/development.md:11`); `docs/book-format.md:22` says `{}` is tolerated on read. Slug lockstep tests exist |
| S11 | Done in substance | ESLint in CI and pre-commit. `pnpm typecheck` runs `tsc -p tsconfig.node.json --noEmit` as well, which checks `vite.config.ts` and `vitest.config.ts` |

## Approach

One PR with five small commits. Each is verified on its own, reviewed, then committed
after a clean local Sonar scan.

### Alternatives considered

| Option | Why not chosen |
|---|---|
| Add Prettier (S11) | It would reformat most of `src/` in one commit and bury the real changes. ESLint already enforces the rules that matter, and the code is consistent. Record as declined in the roadmap. |
| `tsc -b` (S11) | It emits `vite.config.js` and `.d.ts` files into the repo root, and Vite then loads the stale `.js` config. `tsc -b --noEmit` fails with TS6310. The two `--noEmit` runs already cover both projects. |
| Gate Rust coverage now | We have no baseline. Report first, and set a floor in a later PR once two or three runs agree. |
| Raise Python `fail_under` to 80 | Today's total is 48%, since many scripts are tested by subprocess. A floor just under today's number stops regressions without a test-writing project. |

## Phases & checklist

### Chunk 0: Archive the M1 hardening plan

- [x] Tick the hardening plan's four Verification boxes with #77 evidence.
- [x] Set `Status: complete` and move `plans/2026-11-05-m1-hardening.md` to
      `plans/archive/completed/`.
- [x] Rewrite both links (lines 35 and 46) in `plans/2026-11-02-m1-followups.md` to
      `archive/completed/2026-11-05-m1-hardening.md`. In the moved file, prepend `../../`
      to the links at lines 8, 11 and 474, and `../../../` to those at 25, 269 and 491. Line
      12 becomes the same-directory `2026-08-23-m1-scan-viewer.md`. Line 479 is a shell
      command; leave it.
- [x] Remove it from `to_do.md` (Next Up and Active plans) and the `plans/README.md`
      active table, and add it to the README's Completed list. Register this plan in both
      files, in this same commit.
- [x] Reference PR #77 as the completion. No changelog line; #77 already logged it.
- [x] Verify: `python ci/scripts/check_doc_links.py --internal-only --strict` on the
      touched files, and `python hooks/scripts/check_todo_limits.py to_do.md`.

### Chunk 1: Python coverage floor (LC-L3)

- [x] Measure where CI runs: take the total from the last green `main` run's
      `coverage-xml` artifact (Ubuntu, Python 3.12). Record it next to the local macOS
      number (48%).
- [x] Set `fail_under` in `pyproject.toml` to the CI total rounded down, minus 2
      (expected: 46). Record the slope in `docs/ci-and-hooks.md`: an untested
      100-statement script costs about 4.5 points.
- [x] Verify: `pytest --cov` passes; temporarily set the floor to 99 and see it fail.
- [x] `CHANGELOG.dev.md` Changed line. In `docs/ci-and-hooks.md`, update the coverage
      row and line 162, which omits `scripts` from the measured paths.

### Chunk 2: Rust coverage report

- [x] Add a `Rust coverage` job to `ci.yml` (Linux only, same apt deps as `rust`). Give
      it its own cache key over the registry and `src-tauri/target/llvm-cov-target`, so
      instrumented builds never land in the `rust` job's cache. (The target dir was dropped after the first run: ~800 MB.)
- [x] Install `cargo-llvm-cov` with `cargo install --locked --version X`, cached by a key
      that includes the version, as `cargo-audit` is. Add `llvm-tools-preview` with
      `rustup component add`.
- [x] Run `cargo llvm-cov --locked | tee -a "$GITHUB_STEP_SUMMARY"`, then
      `cargo llvm-cov report --lcov --output-path lcov.info`, and upload `lcov.info` as an
      artifact for 14 days. (`--summary-only` needs `--lcov`/`--json` and strips line data.)
- [x] Report only: no `--fail-under-lines`. Not a required check.
- [x] Pin new actions to the SHAs already in `ci.yml`.
- [x] First CI run (2026-11-06, run 42162918181): 82.25% lines, 72.13% functions, 82.35% regions; 3 min 54 s including a 55 s `cargo-llvm-cov` install. The ~1 GB instrumented target dir is not cached (repo caches were past 10 GB).
      A local macOS number runs
      the `sips` fixtures that CI skips, so it is not the baseline.
- [x] `CHANGELOG.dev.md` Added line; `docs/ci-and-hooks.md` row.

### Chunk 3: S9, S10, S11

- [x] S9: do not tick the archived M0 plan's checklist; `plans/README.md` forbids
      rewriting archives. Append a dated "Completion evidence (2026-11-05)" section that
      maps each of its 24 boxes to a file or PR, or says where it went. Rephrase roadmap line 51 so "app-data library root" reads as
      delivered in M1 Phase 2 (`app_data_dir()/books`), not M0.
- [x] S10: already done (see Current state). Record it in the roadmap with those lines.
- [x] S11: record as done. `tsc -b` and Prettier are declined, with the reasons from the
      alternatives table. No `package.json` change.
- [x] TS hooks: `eslint` and `tsc` run at pre-commit (`default_stages`), not pre-push.
      Rewrite roadmap line 66 to say pre-commit meets the intent, then tick it.
- [x] Verify: markdownlint, doc links.

### Chunk 4: Tick the roadmap and propose the ruleset change

- [x] Tick every M0.5 roadmap item that is now done, with a short evidence pointer.
- [x] In the PR description, propose adding `Lint & test (TypeScript)`,
      `Third-party licences`, `Rust (windows-latest)` and `Rust (macos-latest)` to the
      ruleset's required checks. The `rust-cross` item in the follow-ups plan closes when
      the user applies it.
- [x] Update `docs/ci-and-hooks.md`'s required-checks list (lines 40–58) to show those four
      as proposed, and fix line 29 ("Later (M0.5): Dependabot"), which is stale.
- [x] Remove the M0.5 Icebox pointer from `to_do.md`.
- [x] Mark this plan complete and archive it in the same commit if every box is ticked.

## Verification

- [x] `pytest --cov`, `cargo test --locked`, `pnpm typecheck`, `pnpm lint` pass.
- [x] The new `Rust coverage` job passes on the PR and posts a summary.
- [x] `check_doc_links.py --internal-only --strict` on every touched file reports 0 broken.
- [x] Local Sonar: no new issues before each commit.

## Risks

- A cold instrumented Tauri build may take well over 10 minutes. The job is not required,
  so it cannot block merges. Record the measured time.
- The macOS `sips` fixtures do not run on Linux, so Rust coverage will read low for ingest.
  The summary should say so.

## Completion steps (when status = complete)

1. Set status to `complete` and move this file to `plans/archive/completed/`.
2. Remove it from `to_do.md` and the `plans/README.md` active table, and add it to the
   README's Completed list.
3. Changelog lines are added per chunk. Do not duplicate them.
4. Reference the PR that landed it.
