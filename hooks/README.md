# Hooks

Last reviewed: 2026-08-23

Pre-commit hooks and policy-check scripts. The `.pre-commit-config.yaml` in this
directory is an **example** — copy it to your project root to activate it.

## Quick start

```bash
pip install pre-commit
cp hooks/.pre-commit-config.yaml .pre-commit-config.yaml  # if not already at root
pre-commit install --hook-type pre-commit --hook-type pre-push
pre-commit run --all-files
pre-commit run --all-files --hook-stage pre-push   # basedpyright
```

See [`docs/ci-and-hooks.md`](../docs/ci-and-hooks.md) for the full matrix (commit vs push vs CI)
and follow-up stages after the Tauri app lands (M0.5).

## Files

| File | Purpose |
|---|---|
| `.pre-commit-config.yaml` | Example config: policy checks + secrets + lint |
| `scripts/check_file_size.py` | Enforces [`policies/file-size-and-counts.md`](../policies/file-size-and-counts.md) (soft **600** / hard **1000** lines) |
| `scripts/check_doc_freshness.py` | Enforces [`policies/doc-freshness.md`](../policies/doc-freshness.md) |
| `scripts/check_todo_limits.py` | Enforces living backlog size ([`policies/plans-and-todos.md`](../policies/plans-and-todos.md); soft **150** / hard **300**) |
| `scripts/check_todo_plan_sync.py` | Advisory sync: Next Up count (3–5), active plan links, broken/archive refs, Icebox soft cap |
| `scripts/check_cleanup_hygiene.py` | Optional hygiene check: warns when completed items linger in `to_do.md`, plans aren't archived, `.context/` has old files, or changelog entries are missing |
| `scripts/prune_backups.sh` | Optional: delete `backups/` dirs older than last N commits |

## Built-in secret detection and linting

The example config already includes (leave them on unless the project cannot use them):

| Hook | Role |
|---|---|
| **gitleaks** | Secret scanning (hard gate, pre-commit) |
| **detect-private-key** | Private key detect (pre-commit-hooks) |
| **ruff** + **ruff-format** | Python lint/format + complexity |
| **basedpyright** | Python type check (**pre-push** stage) |
| **markdownlint** | Docs lint |
| **shellcheck** | Shell lint |

See [`policies/security-baseline.md`](../policies/security-baseline.md),
[`inventory/security-quality.md`](../inventory/security-quality.md), and
[`policies/github-repository-hygiene.md`](../policies/github-repository-hygiene.md) for
branch rules, required CI checks, and optional absolute-path gates.

## When to use pre-commit vs CI vs agent-side checks

| Check type | Best tier | Reasoning |
|---|---|---|
| Lint, format, file size, TODO size, secret scanning | **pre-commit** (local) | Fast, catches cheaply before push |
| SAST (Semgrep/CodeQL), dep audit, OWASP scan | **CI** | Slower; needs full context or network |
| Doc freshness, TODO comment audit, policy drift | **CI** or **agent** | Doesn't need to run on every commit |
| Open PRs after push / once a day | **agent** (+ optional advisory CI) | Informational; never a pre-push gate — see `ci/scripts/check_open_prs.py` |
| Security / architecture / refactor review | **agent prompt** / Codex Security plugin | Judgment-based; humans approve |
| Dependency updates | **Dependabot/Renovate** | Automated PR; not a hook |

Rule of thumb: anything that takes more than ~5 seconds or needs the internet belongs in CI,
not pre-commit.

## Tiers

- **Hard gate** — exits non-zero, blocks commit or CI.
- **Soft gate** — warns but does not block locally; may block in CI via `POLICY_WARN_AS_ERROR=1`.
- **Advisory** — output only, never blocks.

To promote a soft gate to a hard gate in CI, set the relevant env var
(e.g. `POLICY_WARN_AS_ERROR=1`) in the CI workflow.

## Configuring thresholds without editing scripts

The policy scripts read thresholds from environment variables:

```bash
# check_file_size.py
POLICY_SOFT_LINE_CAP=600        # warn above this (source files)
POLICY_HARD_LINE_CAP=1000       # block above this
POLICY_MAX_BYTES=512000         # non-source file size limit (500 KB)
POLICY_BINARY_HARD_BYTES=5242880 # binary hard limit (5 MB)
POLICY_DOC_SOFT_LINE_CAP=1000  # markdown soft cap
POLICY_WARN_AS_ERROR=0          # set to 1 to treat soft warnings as errors

# check_todo_limits.py
POLICY_TODO_SOFT_LINE_CAP=150
POLICY_TODO_HARD_LINE_CAP=300

# check_todo_plan_sync.py
POLICY_NEXT_UP_MIN=3
POLICY_NEXT_UP_MAX=5
POLICY_ICEBOX_SOFT_CAP=20

# check_doc_freshness.py
POLICY_FRESHNESS_WARN_DAYS=180  # warn after this many days
POLICY_FRESHNESS_HARD_DAYS=365  # block after this many days

# prune_backups.sh
POLICY_BACKUP_KEEP_COMMITS=5
```

Set these in CI environment config or a project-level `.env.ci` (not committed).

## Adding a new policy check

1. Write the script in `hooks/scripts/` using the same exit-code convention (0=pass, 1=error).
2. Write the policy it enforces in `policies/`.
3. Add an entry in `.pre-commit-config.yaml` under the `local` repo block.
4. Document the threshold env vars here.

## Optional: prune local `backups/`

If agents copy files into a gitignored `backups/` folder before edits, enable the
commented `prune-backups` hook in `.pre-commit-config.yaml`, or run:

```bash
bash hooks/scripts/prune_backups.sh
```

It removes `backups/*` directories whose mtime is older than the author date of
`HEAD~N` (default N=5 via `POLICY_BACKUP_KEEP_COMMITS`).

## Cleanup hygiene check

The optional `check-cleanup-hygiene` hook helps maintain project hygiene by ensuring completed work is properly logged and archived. It warns when:

- **Completed items linger in `to_do.md`** - Items marked `[x]` that should be removed or logged
- **Plans not archived** - Plans with status `complete` or `abandoned` still in `plans/`
- **Old `.context/` files** - Scratch files older than 14 days (configurable)
- **Missing changelog entries** - Recent significant commits without corresponding changelog updates

### When to enable

Enable this hook when:
- The project uses `to_do.md` for tracking work
- Plans are actively used and completed
- Agents or humans frequently complete work that needs logging
- You want to prevent accumulation of completed work artifacts

### Configuration

Environment variables (set in CI or `.env.ci`):

```bash
POLICY_CONTEXT_MAX_AGE_DAYS=14     # Max age for .context/ files before warning
POLICY_WARN_CHANGELOG_DAYS=7        # Days to check back for missing changelog entries
POLICY_WARN_AS_ERROR=0              # Set to 1 to treat warnings as errors
```

### Remediation

When the hook fires, run [`prompts/cleanup-completed-work.md`](../prompts/cleanup-completed-work.md) to systematically address the issues. This prompt guides agents through:

1. Removing completed items from `to_do.md`
2. Logging completions in appropriate changelogs
3. Archiving completed plans
4. Cleaning up `.context/` scratch files
5. Verifying cleanup with policy checks

This is a **soft gate** by default (warnings only). Set `POLICY_WARN_AS_ERROR=1` in CI to promote it to a hard gate for stricter enforcement.
