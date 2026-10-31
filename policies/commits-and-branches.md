# Policy: Commits & Branches

Last reviewed: 2026-10-30
Enforced by: convention; optional CI check (commitlint / branch-name regex);
advisory open-PR check via [`ci/scripts/check_open_prs.py`](../ci/scripts/check_open_prs.py).

## Commit messages

Default to [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <summary>

<optional body — what & why, not how>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`, `build`, `perf`, `revert`.

- Imperative mood, ≤ 72-char summary.
- Body explains intent and tradeoffs; the diff already shows the mechanics.
- One logical change per commit where practical.

## Branch naming

```
<type>/<short-kebab-summary>      e.g. feat/search-api-inventory
```

- Keep work off the default branch; branch first.
- Long-lived feature branches should rebase on the default branch regularly.

## Pull requests

- Title follows the commit convention; body states purpose, scope, and verification done.
- Keep PRs small and reviewable; split unrelated changes.
- Keep each PR under **90 changed files**. CodeRabbit skips PRs over 100 files, so plan a phase
  as a stack of PRs before it gets there. Check with `git diff --name-only origin/main...HEAD | wc -l`.
  To split a branch that is already too big, push an earlier commit as its own branch, open a PR
  for it against `main`, and retarget the big PR's base to that branch. The repo merges with
  merge commits, so the stacked PR keeps its history when the lower PR merges.
- Link the plan/ADR/issue the PR implements.
- Do not merge red CI; do not bump version in feature PRs unless that is the PR's purpose.

### After a push: check for an existing open PR (advisory)

Agents and humans should **notice** open PRs so a push updates the right review
thread instead of spawning a duplicate. This is **not** a blocking git hook.

1. After pushing a feature branch, run:
   ```bash
   python3 ci/scripts/check_open_prs.py --branch
   ```
2. About once a day (session start is enough):
   - **First** inspect `.context/open-prs-check.stamp` (gitignored). If it exists
     and its mtime is within 24 hours, **do not run the script** — that avoids
     a Python/`gh` round-trip and saves tokens.
   - Only if the stamp is missing or older than 24 hours, run:
     ```bash
     python3 ci/scripts/check_open_prs.py --once-per-day
     ```
     The script refreshes the stamp (and also skips internally if the stamp is
     still fresh — defense in depth, not the preferred path).
3. If an open PR already covers the branch or task: update that PR (push more
   commits, edit description) — do not open a second PR for the same work.
4. Optional scheduled reminder in Actions logs:
   [`ci/examples/open-prs-advisory.yml`](../ci/examples/open-prs-advisory.yml)
   (`continue-on-error`, never a required check). Prefer the local script for
   most projects to avoid burning Actions minutes.

Do **not** add this as a `pre-push` or pre-commit hard gate.

## Versioning

The template uses [SemVer](https://semver.org/) in [`VERSION`](../VERSION); record notable
changes in `CHANGELOG.md` when releases matter.

## Enforcement (optional)

- `commitlint` + a CI job to validate commit messages.
- A branch-name regex check in CI.
- Advisory open-PR listing (script / optional daily workflow) — never blocking.
- Start advisory; promote to a gate only if the team finds it durable.
