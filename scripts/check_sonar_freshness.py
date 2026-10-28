#!/usr/bin/env python3
"""Opt-in local SonarQube freshness gate (stdlib only).

Off by default. Unless ``SONAR_FRESHNESS_GATE`` is ``1``/``true``/``yes`` in
the environment or in the repo-root ``.env``, this exits 0 immediately: no
git calls, no network, no state file read. CI and cloud agents therefore pass
without a Sonar server or token.

When on:

* ``--stage commit`` blocks once commits since the last successful local scan,
  plus the in-flight commit, reach ``SONAR_GATE_MAX_COMMITS`` (default 10).
* ``--stage push`` blocks if any commit has landed since the last scan.

The gate reads ``tmp/sonar-state.json``, which ``scripts/sonar_issues.py``
writes after each successful scan (``scripts/sonar_scan.sh``). It never
contacts the server. An exported variable always wins over ``.env``.

Exit codes: 0 gate off or within budget; 1 blocked or state missing/invalid.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

DEFAULT_STATE_PATH = "tmp/sonar-state.json"
DEFAULT_MAX_COMMITS = 10
WARN_FRACTION = 0.8
# Full SHA-1 or SHA-256; rejects option-like values before they reach git argv.
COMMIT_SHA_RE = re.compile(r"[0-9a-f]{40}|[0-9a-f]{64}")
REFRESH_COMMAND = "bash scripts/sonar_scan.sh  (with the local SonarQube server running)"
# Hooks export these; drop them so child git uses cwd=repo_root().
GIT_DIR_OVERRIDE_VARS = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
)


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def clean_env_value(raw: str) -> str:
    """Tolerate quoted values and trailing `` #`` comments in .env files."""
    value = raw.strip().split(" #", 1)[0].strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    return value


def load_env_file(path: Path) -> None:
    """Read KEY=VALUE lines without overriding exported values.

    An unreadable or non-UTF-8 .env is ignored: this runs before the opt-in
    check, so a crash here would block every commit.
    """
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if line.startswith("export "):
            line = line[len("export ") :].strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), clean_env_value(value))


def env_is_on(name: str) -> bool:
    return os.environ.get(name, "").strip().lower() in {"1", "true", "yes"}


def resolve_max_commits() -> int:
    raw = os.environ.get("SONAR_GATE_MAX_COMMITS", "")
    if not raw:
        return DEFAULT_MAX_COMMITS
    try:
        value = int(raw)
    except ValueError:
        print(
            f"SONAR_GATE_MAX_COMMITS is not an integer: {raw!r}; using {DEFAULT_MAX_COMMITS}.",
            file=sys.stderr,
        )
        return DEFAULT_MAX_COMMITS
    return value if value >= 1 else DEFAULT_MAX_COMMITS


def _git_env() -> dict[str, str]:
    env = os.environ.copy()
    for key in GIT_DIR_OVERRIDE_VARS:
        env.pop(key, None)
    return env


def _git(*args: str) -> str:
    return subprocess.check_output(
        ["git", *args], cwd=repo_root(), text=True, env=_git_env()
    ).strip()


def is_ancestor(ancestor: str, descendant: str) -> bool:
    try:
        _git("merge-base", "--is-ancestor", ancestor, descendant)
    except subprocess.CalledProcessError:
        return False
    return True


def checked_sha(stage: str) -> str | None:
    """Commit to judge: the pushed commit on push (PRE_COMMIT_TO_REF), else HEAD."""
    if stage == "push":
        pushed = os.environ.get("PRE_COMMIT_TO_REF", "").strip().lower()
        if COMMIT_SHA_RE.fullmatch(pushed):
            return pushed
    try:
        return _git("rev-parse", "HEAD")
    except (OSError, subprocess.CalledProcessError) as exc:
        print(f"Sonar freshness gate: failed to resolve HEAD ({exc}).", file=sys.stderr)
        return None


def load_state(state_path: Path) -> dict | None:
    """Return the state dict, or None after printing why it is unusable."""
    state: object = None
    if not state_path.exists():
        reason = f"no scan on record ({DEFAULT_STATE_PATH} missing)"
    else:
        try:
            state = json.loads(state_path.read_text(encoding="utf-8"))
            reason = ""
        except (OSError, ValueError) as exc:
            reason = f"state file is unreadable ({type(exc).__name__})"
    if isinstance(state, dict) and COMMIT_SHA_RE.fullmatch(str(state.get("last_scan_commit"))):
        return state
    if not reason:
        reason = "state file has no valid last_scan_commit"
    print(
        f"Sonar freshness gate: {reason}.\nRun a scan first:\n    {REFRESH_COMMAND}",
        file=sys.stderr,
    )
    return None


def _hint(state: dict) -> str:
    issues = state.get("issues_count")
    return (
        f"Last scan: {state['last_scan_commit'][:12]} at {state.get('last_scan_time', 'unknown')}\n"
        f"Issues on record: {'unknown' if issues is None else issues}\n"
        f"Dumped issues: {state.get('issues_summary_path', 'tmp/sonar-latest-issues.md')}\n"
        f"Re-run the scan to refresh:\n    {REFRESH_COMMAND}"
    )


def run_gate(state_path: Path, max_commits: int, stage: str) -> int:
    state = load_state(state_path)
    if state is None:
        return 1
    last = state["last_scan_commit"]
    head = checked_sha(stage)
    if head is None:
        return 1
    if not is_ancestor(last, head):
        print(
            f"Sonar freshness gate: last scan {last[:12]} is not an ancestor of "
            f"{head[:12]} (history rewritten or another branch).\n{_hint(state)}",
            file=sys.stderr,
        )
        return 1
    try:
        count = int(_git("rev-list", "--count", f"{last}..{head}"))
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"Sonar freshness gate: failed to count commits ({exc}).", file=sys.stderr)
        return 1

    if stage == "push":
        if count > 0:
            print(
                f"Sonar freshness gate BLOCKED push: {count} commit(s) since the last scan.\n"
                f"{_hint(state)}",
                file=sys.stderr,
            )
            return 1
        return 0

    pending = count + 1  # the in-flight commit
    if pending >= max_commits:
        print(
            f"Sonar freshness gate BLOCKED: {pending} commits pending "
            f"(budget {max_commits}, counting this one).\n{_hint(state)}",
            file=sys.stderr,
        )
        return 1
    if pending >= max(1, int(max_commits * WARN_FRACTION)):
        print(
            f"Sonar freshness gate WARNING: {pending} commits pending (budget {max_commits}).",
            file=sys.stderr,
        )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--stage", choices=("commit", "push"), default="commit")
    parser.add_argument(
        "--state",
        default=DEFAULT_STATE_PATH,
        help=f"State file, relative to the repo root (default: {DEFAULT_STATE_PATH}).",
    )
    args = parser.parse_args(argv)

    root = repo_root()
    load_env_file(root / ".env")
    if not env_is_on("SONAR_FRESHNESS_GATE"):
        return 0

    state_path = (root / args.state).resolve()
    if not state_path.is_relative_to(root.resolve()):
        print("Sonar freshness gate: --state must stay inside the repo.", file=sys.stderr)
        return 1
    return run_gate(state_path, resolve_max_commits(), args.stage)


if __name__ == "__main__":
    raise SystemExit(main())
