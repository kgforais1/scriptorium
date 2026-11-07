#!/usr/bin/env python3
"""
check_doc_links.py — keep documentation links and tool catalogs from rotting.

Three checks, two of which run offline and are cheap enough for CI on every push:

1. Internal links  — every relative markdown link resolves to a file that exists.
                     Catches renamed/moved docs and wrong `../` depth, the most common
                     rot in a repo whose entire value is cross-linked guidance.
2. Link liveness   — external links in tool catalogs still resolve (network).
                     404/410 = gone; a GitHub redirect = renamed or transferred.
3. Catalog review  — files carrying `Catalog reviewed through: YYYY-MM-DD` must refresh it
                     more often than the prose window, because the *menu* goes stale faster
                     than the *description*. An entry can read perfectly while the project
                     behind it has been archived.

Checks 2 and 3 are advisory: they report, a human decides whether a tool was replaced, is
merely quiet, or should be dropped. Automating that judgement is how inventories fill with
noise. Check 1 is mechanical and safe to gate on.

Usage:
  python3 ci/scripts/check_doc_links.py                    # everything, incl. network
  python3 ci/scripts/check_doc_links.py --offline          # internal links + dates only
  python3 ci/scripts/check_doc_links.py --internal-only    # just check 1
  python3 ci/scripts/check_doc_links.py --strict           # exit 1 on findings
  python3 ci/scripts/check_doc_links.py docs/NAVIGATION.md

Exit codes: 0 = ok (or advisory), 1 = findings under --strict, 2 = usage error.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import date
from pathlib import Path

CATALOG_WINDOW_DAYS = 120  # tighter than the 180-day prose window on purpose
CATALOG_DIRS = ("inventory",)

LINK_RE = re.compile(r"\[[^\]]*\]\(\s*([^)\s]+?)\s*\)")
CATALOG_RE = re.compile(r"^Catalog reviewed through:\s*(\d{4}-\d{2}-\d{2})\s*$", re.MULTILINE)
# Opens a fenced block: three or more backticks or tildes at the start of a line.
FENCE_OPEN_RE = re.compile(r"^(`{3,}|~{3,})")

TIMEOUT = 10
USER_AGENT = "template-repo-doc-link-check/1.0"

SKIP_DIRS = {".git", "node_modules", "__pycache__", ".pytest_cache", ".ruff_cache", "worktrees"}

# Hosts that reject automated HEAD requests; a failure there is not evidence of rot.
SKIP_HOSTS = ("twitter.com", "x.com", "linkedin.com", "reddit.com")

# Relative targets that are generated at runtime and correctly absent from the template.
EXPECTED_ABSENT = (".context/",)


def iter_markdown(paths: list[Path]) -> list[Path]:
    if paths:
        return [p for p in paths if p.suffix == ".md" and p.exists()]
    found = []
    for path in Path(".").rglob("*.md"):
        if SKIP_DIRS & set(path.parts):
            continue
        found.append(path)
    return sorted(found)


def strip_code(text: str) -> str:
    """Blank out fenced blocks so template placeholders are not read as real links."""
    out: list[str] = []
    fence = ""  # the open fence's marker, or "" outside a block
    for line in text.splitlines(keepends=True):
        if fence:
            # A block closes on a run of the same character at least as long.
            if line.startswith(fence) and not line.rstrip("\r\n").lstrip(fence[0]):
                fence = ""
            out.append("\n" if line.endswith("\n") else "")
            continue
        opened = FENCE_OPEN_RE.match(line)
        if opened:
            fence = opened.group(1)
            out.append("\n" if line.endswith("\n") else "")
            continue
        out.append(line)
    return "".join(out)


def check_internal(path: Path, text: str) -> list[str]:
    problems = []
    for match in LINK_RE.finditer(text):
        target = match.group(1)
        if target.startswith(("http://", "https://", "mailto:", "#", "<")):
            continue
        clean = target.split("#", 1)[0].split("?", 1)[0]
        if not clean:
            continue
        if any(frag in clean for frag in EXPECTED_ABSENT):
            continue
        if not (path.parent / clean).resolve().exists():
            line = text[: match.start()].count("\n") + 1
            problems.append(f"{path}:{line}: broken relative link → {target}")
    return problems


def find_external(text: str) -> list[str]:
    seen: dict[str, None] = {}
    for match in LINK_RE.finditer(text):
        url = match.group(1)
        if url.startswith(("http://", "https://")):
            seen.setdefault(url.rstrip("."), None)
    return list(seen)


def _hostname(url: str) -> str:
    """Extract lowercased hostname safely; returns '' on parse error or missing host."""
    try:
        return (urllib.parse.urlparse(url).hostname or "").lower()
    except ValueError:
        return ""


def _normalized_hostname(url: str) -> str:
    """Hostname for skip/GitHub checks; strips trailing-dot FQDN form (e.g. twitter.com.)."""
    return _hostname(url).rstrip(".")


def _is_skipped_host(hostname: str) -> bool:
    """True when hostname is a SKIP_HOSTS entry or a subdomain of one."""
    return any(hostname == host or hostname.endswith("." + host) for host in SKIP_HOSTS)


def check_link(url: str) -> tuple[str, str] | None:
    """Return (url, problem) when the link looks dead or moved, else None."""
    # Trailing-dot FQDNs (e.g. twitter.com.) must still match SKIP_HOSTS.
    h = _normalized_hostname(url)
    if _is_skipped_host(h):
        return None
    request = urllib.request.Request(url, method="HEAD", headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
            final = response.geturl()
            # A GitHub repo redirect means the project was renamed or transferred.
            if (h == "github.com" or h.endswith(".github.com")) and final.rstrip("/") != url.rstrip(
                "/"
            ):
                return (url, f"redirects to {final} — renamed or transferred?")
        return None
    except urllib.error.HTTPError as error:
        if error.code in (404, 410):
            return (url, f"HTTP {error.code} — gone")
        if error.code in (401, 403, 405, 429):
            return None  # auth walls, method rejection, and rate limits are not rot
        return (url, f"HTTP {error.code}")
    except (urllib.error.URLError, TimeoutError, ValueError) as error:
        return (url, f"unreachable: {error}")


def check_catalog_date(text: str) -> str | None:
    match = CATALOG_RE.search(text)
    if not match:
        return None  # marker is opt-in; only files claiming a catalog review are checked
    try:
        reviewed = date.fromisoformat(match.group(1))
    except ValueError:
        return f"unparseable 'Catalog reviewed through' date: {match.group(1)}"
    age = (date.today() - reviewed).days
    if age > CATALOG_WINDOW_DAYS:
        return (
            f"catalog last reviewed {reviewed} ({age} days ago, window {CATALOG_WINDOW_DAYS}). "
            "Re-ask whether this is still the right menu, then refresh the date."
        )
    return None


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("files", nargs="*", type=Path, help="markdown files (default: all)")
    parser.add_argument("--offline", action="store_true", help="skip network checks")
    parser.add_argument("--internal-only", action="store_true", help="only check relative links")
    parser.add_argument("--strict", action="store_true", help="exit 1 when findings exist")
    args = parser.parse_args()

    paths = iter_markdown(args.files)
    if not paths:
        print("No markdown files found.", file=sys.stderr)
        return 2

    broken = 0
    advisory = 0
    network = not (args.offline or args.internal_only)

    for path in paths:
        raw = path.read_text(encoding="utf-8", errors="ignore")
        text = strip_code(raw)

        for problem in check_internal(path, text):
            print(f"[links] BROKEN {problem}")
            broken += 1

        if args.internal_only:
            continue

        is_catalog = bool(set(path.parts) & set(CATALOG_DIRS))
        if is_catalog:
            stale = check_catalog_date(raw)
            if stale:
                print(f"[links] STALE  {path}: {stale}")
                advisory += 1

        if not (network and is_catalog):
            continue
        urls = find_external(text)
        if not urls:
            continue
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            for result in pool.map(check_link, urls):
                if result:
                    url, problem = result
                    print(f"[links] DEAD   {path}: {url} — {problem}")
                    advisory += 1

    mode = " (internal only)" if args.internal_only else " (offline)" if args.offline else ""
    print(f"\nChecked {len(paths)} file(s){mode}: {broken} broken, {advisory} advisory.")
    if broken:
        print("Broken relative links are mechanical — fix the path or remove the link.")
    if advisory:
        print(
            "A dead link, rename, or stale catalog date is a prompt to re-evaluate the entry, "
            "not to delete it automatically. Record the outcome and refresh the review date."
        )

    if args.strict and broken:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
