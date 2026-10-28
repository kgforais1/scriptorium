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


def fetch_issues(host: str, token: str, project_key: str) -> list[dict]:
    issues: list[dict] = []
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
        if not batch or len(issues) >= int(data.get("paging", {}).get("total", 0)):
            break
        page += 1
    return issues


def summarize(issues: list[dict]) -> str:
    lines = ["# SonarQube unresolved issues", "", f"Total: {len(issues)}", ""]
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


def host_allowed(host: str) -> bool:
    """Loopback only, like sonar_scan.sh, unless SONAR_ALLOW_REMOTE=1."""
    if os.environ.get("SONAR_ALLOW_REMOTE", "0") == "1":
        return True
    parts = urllib.parse.urlparse(host)
    return parts.scheme == "http" and (parts.hostname or "") in LOOPBACK_HOSTS


def main() -> int:
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
        issues = fetch_issues(host, token, project_key)
    except (OSError, ValueError, RuntimeError) as exc:
        # HTTPError/URLError are OSError subclasses; the message carries no token.
        print(f"sonar_issues: {exc}; not writing sonar-state.json.", file=sys.stderr)
        return 1

    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H%M%SZ")
    dump_dir = TMP / "sonar-issues"
    dump_dir.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(issues, indent=1)
    summary = summarize(issues)
    (dump_dir / f"{stamp}.json").write_text(payload, encoding="utf-8")
    (dump_dir / f"{stamp}.md").write_text(summary, encoding="utf-8")
    (TMP / "sonar-latest-issues.json").write_text(payload, encoding="utf-8")
    (TMP / "sonar-latest-issues.md").write_text(summary, encoding="utf-8")

    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, env=env
    ).strip()
    state = {
        "last_scan_commit": head,
        "last_scan_time": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "issues_count": len(issues),
        "issues_path": f"tmp/sonar-issues/{stamp}.json",
        "issues_summary_path": "tmp/sonar-latest-issues.md",
        "ce_task_id": task_id,
    }
    (TMP / "sonar-state.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    try:
        prune(dump_dir, KEEP_DUMPS)
    except OSError as exc:  # housekeeping must not fail a good scan
        print(f"sonar_issues: warning: pruning failed ({exc}).", file=sys.stderr)

    print(f"Issues on record: {len(issues)} (commit {head[:12]})")
    print("Summary: tmp/sonar-latest-issues.md  State: tmp/sonar-state.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
