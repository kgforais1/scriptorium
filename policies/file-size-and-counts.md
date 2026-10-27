# Policy: File Size & Counts ("file life counts")

Last reviewed: 2026-07-09
Enforced by: [`hooks/scripts/check_file_size.py`](../hooks/scripts/check_file_size.py)

## Why

Large files and overstuffed directories are where complexity hides and where agents lose
context. Caps keep modules legible, reviewable, and easy for an agent to load whole.

## Rules & defaults

Defaults are deliberately generous; tighten per project. Configure via environment
variables (see [`hooks/README.md`](../hooks/README.md)) or the script defaults.

| Rule | Default | Tier |
|---|---|---|
| Max lines per source file | **600** (soft warn), **1000** (hard) | soft→hard gate |
| Max lines per function/method | 60 (soft), 100 (hard) | advisory → soft gate |
| Max cyclomatic complexity per function | 10 (soft), 15 (hard) | advisory |
| Max bytes per committed file (non-binary) | 500 KB | hard gate |
| Max files per directory (excl. generated) | 40 | advisory |
| Disallow committing large binaries | > 5 MB | hard gate (use Git LFS / release assets) |
| Doc (`.md`) max lines | 1000 | advisory (split into linked docs) |
| Living `to_do` / `TODO.md` backlog | 150 (soft), 300 (hard) | see [`plans-and-todos.md`](plans-and-todos.md) |

### Exemptions

- Generated code, lockfiles, vendored deps, migrations, and fixtures are exempt.
  Mark exempt paths in the checker's ignore list.
- A file may exceed a soft cap with a one-line justification comment:
  `# policy:file-size allow=600 reason=<why>`.

## Function size & complexity

Function length and cyclomatic complexity are advisory (no automated pre-commit block by
default) because they require language-level parsing.

**Python — check with radon or ruff:**

```bash
# Cyclomatic complexity (A=1-5, B=6-10, C=11-15, D=16-20, E=21-25, F=26+)
pip install radon
radon cc . --min C --show-complexity   # flag C-and-above

# Function length: ruff rule C901 (complexity) + PLR0912/PLR0915 (branches/statements)
ruff check --select C901,PLR0912,PLR0915 .
```

**Polyglot — check with lizard:**

```bash
pip install lizard
lizard . --CCN 10 --length 60
```

**JavaScript / TypeScript — ESLint:**

```json
"complexity": ["warn", 10],
"max-lines-per-function": ["warn", {"max": 60}]
```

**Rationale:** functions over 60 lines usually have more than one responsibility. High
cyclomatic complexity (>10) correlates with defect density and is hard to test. Treat
these as signals to extract helpers, not mandatory refactors on day one.

## Remediation when a check fails

1. Split by responsibility (one module → several focused modules).
2. Extract long functions; push helpers down.
3. Move large data/fixtures out of source (LFS, release assets, or `data/`).
4. For docs, split into topic files and link them from an index.

## Rationale notes

These are taste defaults, not science. The **600-line soft warn** is a practical
legibility threshold for agent-loaded modules; the point is a *consistent, visible*
limit with an easy override, not the exact number. Older projects may keep soft=400
via `POLICY_SOFT_LINE_CAP=400`.
