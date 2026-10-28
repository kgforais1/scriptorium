#!/usr/bin/env python3
"""Post-scan step for scripts/sonar_scan.sh (stdlib only, manual use).

Waits for the server's Compute Engine task named in
``.scannerwork/report-task.txt``, then dumps unresolved issues to
``tmp/sonar-issues/<UTC>.{json,md}`` with stable ``tmp/sonar-latest-issues.*``
copies, and records ``tmp/sonar-state.json`` for the freshness gate.
On any API failure it exits non-zero and leaves the previous state alone,
so a failed scan never resets the gate.

Reads SONAR_TOKEN and SONAR_HOST_URL from the environment; the token is never
printed and is only sent to the configured host (redirects are refused).
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TMP = ROOT / "tmp"
PAGE_SIZE = 500
ISSUE_CAP = 2000
CE_TIMEOUT_S = 180
KEEP_DUMPS = 20


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(newurl, code, "redirect refused", headers, fp)


_OPENER = urllib.request.build_opener(_NoRedirect)


def api_get(host: str, token: str, path: str, params: dict[str, str]) -> dict:
    url = f"{host.rstrip('/')}{path}?{urllib.parse.urlencode(params)}"
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
    with _OPENER.open(req, timeout=30) as resp:
        return json.load(resp)


def read_properties(path: Path) -> dict[str, str]:
    props: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            props[key.strip()] = value.strip()
    return props


def wait_for_ce(host: str, token: str, task_id: str) -> None:
    deadline = time.monotonic() + CE_TIMEOUT_S
    while time.monotonic() < deadline:
        status = api_get(host, token, "/api/ce/task", {"id": task_id}).get("task", {}).get("status")
        if status == "SUCCESS":
            return
        if status in {"FAILED", "CANCELED"}:
            raise RuntimeError(f"Compute Engine task ended with status {status}")
        time.sleep(5)
    raise RuntimeError(f"timed out after {CE_TIMEOUT_S}s waiting for the Compute Engine task")


def fetch_issues(host: str, token: str, project_key: str) -> tuple[list[dict], int]:
    """Return (issues, server total). The list stops at ISSUE_CAP; the total does not."""
    issues: list[dict] = []
    total = 0
    page = 1
    while len(issues) < ISSUE_CAP:
        params = {
            "componentKeys": project_key,
            "resolved": "false",
            "ps": str(PAGE_SIZE),
            "p": str(page),
        }
        data = api_get(host, token, "/api/issues/search", params)
        batch = data.get("issues")
        if not isinstance(batch, list):
            raise RuntimeError("api/issues/search response has no issues array")
        issues.extend(batch)
        total = int(data.get("paging", {}).get("total", 0))
        if not batch or len(issues) >= total:
            break
        page += 1
    return issues[:ISSUE_CAP], max(total, len(issues))


def summarize(issues: list[dict], total: int) -> str:
    lines = ["# SonarQube unresolved issues", "", f"Total: {total}", ""]
    if total > len(issues):
        lines += [f"**Truncated:** only the first {len(issues)} of {total} issues are listed.", ""]
    for issue in issues:
        component = (issue.get("component") or "?").rsplit(":", 1)[-1]
        loc = f"{component}:{issue['line']}" if issue.get("line") else component
        message = (issue.get("message") or "").replace("\n", " ")
        lines.append(
            f"- [{issue.get('severity', '?')}] {issue.get('rule', '?')} — {loc} — {message}"
        )
    return "\n".join(lines) + "\n"


def prune(dump_dir: Path, keep: int) -> None:
    stamps = sorted({p.stem for p in dump_dir.glob("*.json")}, reverse=True)
    for stamp in stamps[keep:]:
        for suffix in (".json", ".md"):
            (dump_dir / f"{stamp}{suffix}").unlink(missing_ok=True)


LOOPBACK_HOSTS = frozenset({"localhost", "127.0.0.1", "::1"})


def _git_out(*args: str) -> bytes:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    return subprocess.check_output(["git", *args], cwd=ROOT, env=env)


def tree_fingerprint() -> str:
    """Hash of HEAD plus every uncommitted change, so a scan can prove nothing moved."""
    digest = hashlib.sha256()
    for args in (("rev-parse", "HEAD"), ("diff", "HEAD", "--binary")):
        digest.update(_git_out(*args))
        digest.update(b"\0")
    untracked = _git_out("ls-files", "--others", "--exclude-standard", "-z")
    for name in sorted(filter(None, untracked.split(b"\0"))):
        digest.update(name + b"\0")
        path = ROOT / os.fsdecode(name)
        if path.is_symlink() or not path.is_file():  # never follow links or read devices
            digest.update(b"<not-a-file>")
            continue
        try:
            file_digest = hashlib.sha256()
            with path.open("rb") as f:
                while block := f.read(2**20):
                    file_digest.update(block)
            digest.update(file_digest.digest())
        except OSError:  # vanished or unreadable: still part of the state
            digest.update(b"<unreadable>")
    return digest.hexdigest()


def host_allowed(host: str) -> bool:
    """Loopback over http or https. A remote host only over https, and only when
    SONAR_ALLOW_REMOTE=1 is exported, so the token never crosses the network in clear."""
    try:
        parts = urllib.parse.urlparse(host)
        _ = parts.port  # raises ValueError for a non-numeric or out-of-range port
    except ValueError:
        return False
    if parts.scheme not in {"http", "https"} or not parts.hostname:
        return False
    if parts.hostname in LOOPBACK_HOSTS:
        return True
    return parts.scheme == "https" and os.environ.get("SONAR_ALLOW_REMOTE", "0") == "1"


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if args == ["--fingerprint"]:
        print(tree_fingerprint())
        return 0
    if args == ["--check-host"]:
        return 0 if host_allowed(os.environ.get("SONAR_HOST_URL", "http://localhost:9000")) else 1
    token = os.environ.get("SONAR_TOKEN", "")
    host = os.environ.get("SONAR_HOST_URL", "http://localhost:9000")
    if not host_allowed(host):
        print(
            "sonar_issues: SONAR_HOST_URL is not loopback; set SONAR_ALLOW_REMOTE=1 to override.",
            file=sys.stderr,
        )
        return 1
    if not token:
        print("sonar_issues: SONAR_TOKEN is not set.", file=sys.stderr)
        return 1
    report = ROOT / ".scannerwork" / "report-task.txt"
    try:
        task_id = read_properties(report)["ceTaskId"]
        project_key = read_properties(ROOT / "sonar-project.properties")["sonar.projectKey"]
    except (OSError, KeyError) as exc:
        print(f"sonar_issues: missing scanner report or project key ({exc}).", file=sys.stderr)
        return 1

    try:
        wait_for_ce(host, token, task_id)
        issues, total = fetch_issues(host, token, project_key)
    except (OSError, ValueError, RuntimeError) as exc:
        # HTTPError/URLError are OSError subclasses; the message carries no token.
        print(f"sonar_issues: {exc}; not writing sonar-state.json.", file=sys.stderr)
        return 1

    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H%M%SZ")
    dump_dir = TMP / "sonar-issues"
    dump_dir.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(issues, indent=1)
    summary = summarize(issues, total)
    (dump_dir / f"{stamp}.json").write_text(payload, encoding="utf-8")
    (dump_dir / f"{stamp}.md").write_text(summary, encoding="utf-8")
    (TMP / "sonar-latest-issues.json").write_text(payload, encoding="utf-8")
    (TMP / "sonar-latest-issues.md").write_text(summary, encoding="utf-8")

    before = os.environ.get("SONAR_SCAN_FINGERPRINT", "")
    if not before or before != tree_fingerprint():
        print(
            "sonar_issues: the tree changed during the scan (or no pre-scan fingerprint);"
            " issues dumped, but not writing sonar-state.json. Re-run the scan.",
            file=sys.stderr,
        )
        return 1
    head = _git_out("rev-parse", "HEAD").decode().strip()
    state = {
        "last_scan_commit": head,
        "last_scan_time": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "issues_count": total,
        "issues_truncated": total > len(issues),
        "issues_path": f"tmp/sonar-issues/{stamp}.json",
        "issues_summary_path": "tmp/sonar-latest-issues.md",
        "ce_task_id": task_id,
    }
    (TMP / "sonar-state.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    try:
        prune(dump_dir, KEEP_DUMPS)
    except OSError as exc:  # housekeeping must not fail a good scan
        print(f"sonar_issues: warning: pruning failed ({exc}).", file=sys.stderr)

    print(f"Issues on record: {total} (commit {head[:12]})")
    print("Summary: tmp/sonar-latest-issues.md  State: tmp/sonar-state.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
