#!/usr/bin/env python3
"""
check_bundled_c_cves.py — flag known CVEs in C libraries bundled by Rust -sys crates.

cargo-audit, Dependabot and OSV match advisories to crate versions. They cannot
see C sources vendored inside a -sys crate (for example OpenJPEG inside
openjpeg-sys). This script reads each bundled C library's version from the
crate source, asks the NVD for CVEs that affect that exact version, and fails
on any CVE that is not listed in a time-bound exceptions file.

Data source (decision): the NVD CVE API 2.0, queried with
`cpeName=cpe:2.3:a:<vendor>:<product>:<version>:*:...`. NVD expands cpeName
over version ranges (versionStart/EndIncluding/Excluding), checked live:
openjpeg 2.5.3 returns CVE-2025-54874, whose NVD config is "<= 2.5.3". OSV
was tried and rejected: its GIT ecosystem needs a commit hash, not a release
version, so it cannot answer "which CVEs affect OpenJPEG 2.5.3". Limits: NVD
analysis can lag a new CVE by days, and a library with no NVD CPE (see
bundled-c.toml) is only as well covered as its CPE.

Inputs:
  --manifest PATH    bundled library list (default: ci/bundled-c.toml)
  --exceptions PATH  exceptions (default: ci/bundled-c-exceptions.toml)
  --lockfile PATH    Cargo.lock (default: src-tauri/Cargo.lock)
  --root PATH        directory that --manifest/--exceptions/--lockfile must stay
                     under (default: current working directory)
  --today YYYY-MM-DD override "today" for tests
Environment:
  CARGO_HOME         cargo home (default: ~/.cargo); crates are read from
                     registry/src/*/<crate>-<version> after `cargo fetch`
  NVD_API_KEY        optional; raises the NVD rate limit (sleep drops to ~1s)

Outputs:
  One status line per library and per CVE. Exit 0 when every CVE is excepted
  and no exception has expired; exit 1 on an unexcepted CVE or expired
  exception; exit 2 on usage, parse, missing-source or feed errors (the check
  fails closed when the NVD is unreachable or keeps rate limiting).

Requirements:
  Python 3.11+ (stdlib only).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from typing import Any

NVD_URL = "https://services.nvd.nist.gov/rest/json/cves/2.0"
RETRY_STATUS = frozenset({403, 429, 503})
MAX_ATTEMPTS = 5
TIMEOUT_S = 60
SLEEP_NO_KEY_S = 7.0  # NVD allows 5 requests per 30 s without a key
SLEEP_KEY_S = 1.0  # and 50 per 30 s with one
CANONICAL_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
CVE_ID_RE = re.compile(r"^CVE-\d{4}-\d{4,}$")

UrlOpen = Callable[..., Any]
Sleep = Callable[[float], None]


class CheckError(Exception):
    """A setup or feed problem that stops the check (exit 2)."""


@dataclass(frozen=True)
class Library:
    """One bundled C library from the manifest."""

    name: str
    crate: str
    version_file: str
    version_regex: str
    cpe_vendor: str
    cpe_product: str
    canary_version: str | None = None
    canary_cve: str | None = None
    canary_note: str | None = None  # set only when the entry declares canary = "none"


@dataclass
class Finding:
    """A CVE that NVD says affects a bundled library version."""

    cve: str
    library: str
    version: str
    severity: str = "unknown"
    fixed: str = "not stated"


@dataclass
class ExceptionRow:
    """One time-bound exception row."""

    id: str
    library: str
    reason: str
    expires: date


@dataclass
class Report:
    """Outcome of a run: problems fail the check, notes are informational."""

    problems: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)


def parse_canonical_date(value: str, *, label: str) -> date:
    """Parse YYYY-MM-DD only; reject compact and week-date ISO forms."""
    if not CANONICAL_DATE_RE.fullmatch(value):
        raise ValueError(f"{label} must be YYYY-MM-DD, got {value!r}")
    return date.fromisoformat(value)


def resolve_under_root(path: Path, root: Path) -> Path:
    """Resolve path and require it stays under root (blocks CLI path escape)."""
    root_resolved = root.resolve()
    candidate = path if path.is_absolute() else root_resolved / path
    resolved = candidate.resolve()
    if not resolved.is_relative_to(root_resolved):
        raise ValueError(f"{path}: must stay under {root_resolved}")
    return resolved


def load_toml(path: Path) -> dict[str, Any]:
    """Parse a TOML file; raise CheckError with path context on failure."""
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as err:
        raise CheckError(f"{path}: {err}") from err


def _require_str(row: Any, key: str, where: str) -> str:
    """Return row[key] as a non-empty string or raise CheckError."""
    value = row.get(key) if isinstance(row, dict) else None
    if not isinstance(value, str) or not value.strip():
        raise CheckError(f"{where}: missing string field '{key}'")
    return value.strip()


def load_manifest(path: Path) -> list[Library]:
    """Load [[library]] rows from the manifest."""
    rows = load_toml(path).get("library")
    if not isinstance(rows, list) or not rows:
        raise CheckError(f"{path}: missing [[library]] table array")
    libs: list[Library] = []
    for i, row in enumerate(rows):
        where = f"{path}: library[{i}]"
        libs.append(
            Library(
                name=_require_str(row, "name", where),
                crate=_require_str(row, "crate", where),
                version_file=_require_str(row, "version_file", where),
                version_regex=_require_str(row, "version_regex", where),
                cpe_vendor=_require_str(row, "cpe_vendor", where),
                cpe_product=_require_str(row, "cpe_product", where),
                **_load_canary(row, where),
            )
        )
    return libs


def _load_canary(row: dict[str, Any], where: str) -> dict[str, str | None]:
    """Read the canary fields: canary_version + canary_cve, or canary = "none" + canary_note."""
    if row.get("canary") is not None:
        if row["canary"] != "none":
            raise CheckError(f'{where}: canary must be "none" or absent, got {row["canary"]!r}')
        return {"canary_note": _require_str(row, "canary_note", where)}
    cve = _require_str(row, "canary_cve", where)
    if not CVE_ID_RE.fullmatch(cve):
        raise CheckError(f"{where}: canary_cve {cve!r} is not a CVE id")
    return {"canary_version": _require_str(row, "canary_version", where), "canary_cve": cve}


def load_exceptions(path: Path, library_names: set[str]) -> list[ExceptionRow]:
    """Load [[exceptions]] rows; an empty or absent table is allowed."""
    rows = load_toml(path).get("exceptions", [])
    if not isinstance(rows, list):
        raise CheckError(f"{path}: 'exceptions' must be a [[exceptions]] table array")
    out: list[ExceptionRow] = []
    for i, row in enumerate(rows):
        where = f"{path}: exceptions[{i}]"
        cve = _require_str(row, "id", where)
        library = _require_str(row, "library", where)
        if not CVE_ID_RE.fullmatch(cve):
            raise CheckError(f"{where}: id {cve!r} is not a CVE id")
        if library not in library_names:
            raise CheckError(f"{where}: library {library!r} is not in the manifest")
        try:
            expires = parse_canonical_date(
                _require_str(row, "expires", where), label=f"{cve} expires"
            )
        except ValueError as err:
            raise CheckError(f"{where}: {err}") from err
        out.append(ExceptionRow(cve, library, _require_str(row, "reason", where), expires))
    keys = [(e.id, e.library) for e in out]
    if len(set(keys)) != len(keys):
        raise CheckError(f"{path}: duplicate exception (id, library) pairs")
    return out


def locked_version(lockfile: Path, crate: str) -> str:
    """Return the version of `crate` in Cargo.lock; fail if absent or ambiguous."""
    packages = load_toml(lockfile).get("package", [])
    versions = sorted({p["version"] for p in packages if p.get("name") == crate})
    if not versions:
        raise CheckError(f"{lockfile}: crate {crate} is not locked (remove it from the manifest?)")
    if len(versions) > 1:
        raise CheckError(
            f"{lockfile}: {crate} is locked at several versions {versions}; the check needs one"
        )
    return versions[0]


def find_crate_source(cargo_home: Path, crate: str, version: str) -> Path:
    """Find registry/src/*/<crate>-<version> or tell the user to run cargo fetch."""
    matches = sorted((cargo_home / "registry" / "src").glob(f"*/{crate}-{version}"))
    if not matches:
        raise CheckError(
            f"source for {crate} {version} not found under {cargo_home}/registry/src; "
            "run `cargo fetch --manifest-path src-tauri/Cargo.toml` first"
        )
    return matches[0]


def extract_c_version(lib: Library, crate_dir: Path) -> str:
    """Read the C library version from the crate source with the manifest regex."""
    try:
        target = resolve_under_root(Path(lib.version_file), crate_dir)
        text = target.read_text(encoding="utf-8", errors="replace")
        pattern = re.compile(lib.version_regex)
    except (OSError, ValueError, re.error) as err:
        raise CheckError(f"{lib.name}: cannot read version from {lib.version_file}: {err}") from err
    match = pattern.search(text)
    if not match or not match.groups():
        raise CheckError(f"{lib.name}: version_regex found no version in {target}")
    version = ".".join(g for g in match.groups() if g is not None)
    if not re.fullmatch(r"\d+(\.\d+)+", version):
        raise CheckError(f"{lib.name}: extracted version {version!r} is not dotted numeric")
    return version


def _retry_delay(err: urllib.error.HTTPError, attempt: int, base: float) -> float:
    """Seconds to wait before a retry: Retry-After if sane, else exponential."""
    retry_after = err.headers.get("Retry-After") if err.headers else None
    if retry_after and retry_after.isdigit():
        return min(float(retry_after), 120.0)
    return max(base, 5.0) * (2**attempt)


def nvd_get(
    url: str,
    api_key: str | None,
    urlopen: UrlOpen,
    sleep: Sleep,
    attempts: int = MAX_ATTEMPTS,
) -> dict[str, Any]:
    """GET one NVD page as JSON; retry 403/429/503 a bounded number of times."""
    headers = {"User-Agent": "scriptorium-ci-bundled-c-cves"}
    if api_key:
        headers["apiKey"] = api_key
    base = SLEEP_KEY_S if api_key else SLEEP_NO_KEY_S
    last = "no attempt made"
    for attempt in range(attempts):
        try:
            req = urllib.request.Request(url, headers=headers)  # noqa: S310 - fixed https host
            with urlopen(req, timeout=TIMEOUT_S) as resp:
                data = json.load(resp)
            if not isinstance(data, dict):
                raise CheckError("NVD returned a non-object JSON body")
            return data
        except urllib.error.HTTPError as err:
            if err.code not in RETRY_STATUS:
                raise CheckError(f"NVD returned HTTP {err.code} for {url}") from err
            last = f"HTTP {err.code}"
            if attempt + 1 < attempts:
                sleep(_retry_delay(err, attempt, base))
        except (OSError, json.JSONDecodeError) as err:
            raise CheckError(f"NVD unreachable or unreadable ({err}); failing closed") from err
    raise CheckError(f"NVD still rate limiting after {attempts} attempts ({last}); failing closed")


def _best_metric(cve: dict[str, Any]) -> str:
    """Return 'SEVERITY score' from the newest CVSS metric present, else 'unknown'."""
    metrics = cve.get("metrics", {})
    for key in ("cvssMetricV40", "cvssMetricV31", "cvssMetricV30", "cvssMetricV2"):
        for entry in metrics.get(key, []):
            data = entry.get("cvssData", {})
            score = data.get("baseScore")
            severity = data.get("baseSeverity") or entry.get("baseSeverity")
            if score is not None:
                return f"{severity or 'UNRATED'} {score} ({key.removeprefix('cvssMetric')})"
    return "unknown"


def _walk_matches(nodes: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Collect cpeMatch entries from nodes and any nested nodes (AND/OR trees)."""
    out: list[dict[str, Any]] = []
    for node in nodes:
        out.extend(node.get("cpeMatch", []))
        out.extend(_walk_matches(node.get("nodes", [])))
    return out


def _matches_for(cve: dict[str, Any], lib: Library) -> list[dict[str, Any]]:
    """Return vulnerable cpeMatch entries for this library's CPE product."""
    needle = f":{lib.cpe_vendor}:{lib.cpe_product}:"
    nodes = [n for cfg in cve.get("configurations", []) for n in cfg.get("nodes", [])]
    return [
        m for m in _walk_matches(nodes) if m.get("vulnerable") and needle in m.get("criteria", "")
    ]


def _fixed_version(cve: dict[str, Any], lib: Library) -> str:
    """Describe the fixed version from NVD configs for this CPE product, if given."""
    notes: list[str] = []
    for match in _matches_for(cve, lib):
        if "versionEndExcluding" in match:
            notes.append(f"fixed in {match['versionEndExcluding']}")
        elif "versionEndIncluding" in match:
            notes.append(f"affects up to {match['versionEndIncluding']}; no fixed version stated")
    return "; ".join(dict.fromkeys(notes)) or "not stated"


def query_cves(
    lib: Library,
    version: str,
    api_key: str | None,
    urlopen: UrlOpen,
    sleep: Sleep,
) -> list[Finding]:
    """Ask NVD for CVEs affecting exactly `version` of the library (all pages)."""
    cpe = f"cpe:2.3:a:{lib.cpe_vendor}:{lib.cpe_product}:{version}:*:*:*:*:*:*:*"
    found: list[Finding] = []
    start = 0
    expected: int | None = None
    while True:
        query = urllib.parse.urlencode(
            {"cpeName": cpe, "isVulnerable": "", "startIndex": start}, safe=":*"
        )
        page = nvd_get(f"{NVD_URL}?{query}", api_key, urlopen, sleep)
        sleep(SLEEP_KEY_S if api_key else SLEEP_NO_KEY_S)
        items = page.get("vulnerabilities")
        total = page.get("totalResults")
        if not isinstance(items, list) or not isinstance(total, int) or isinstance(total, bool):
            raise CheckError(
                f"NVD reply for {lib.name} {version} lacks a list 'vulnerabilities' "
                "or an integer 'totalResults'; failing closed"
            )
        # A total that shrinks between pages could end the loop early, so pin the first one.
        if expected is None:
            expected = total
        elif total != expected:
            raise CheckError(
                f"NVD reply for {lib.name} {version} changed totalResults from {expected} "
                f"to {total} mid-query; failing closed"
            )
        for item in items:
            cve = item.get("cve", {})
            found.append(
                Finding(cve["id"], lib.name, version, _best_metric(cve), _fixed_version(cve, lib))
            )
        start += len(items)
        if start >= total:
            return sorted(found, key=lambda f: f.cve)
        if not items:
            raise CheckError(
                f"NVD pages for {lib.name} {version} ended at {start} of {total} results; "
                "failing closed"
            )


def check_canary(
    lib: Library,
    api_key: str | None,
    urlopen: UrlOpen,
    sleep: Sleep,
) -> None:
    """Prove the CPE works: NVD must list the canary CVE for the canary version.

    A wrong CPE vendor or product silently returns zero CVEs, which would pass
    the check forever. Raises CheckError when the canary CVE is not returned.
    """
    if lib.canary_version is None or lib.canary_cve is None:
        print(
            f"[bundled-c] WARNING: {lib.name} has no canary ({lib.canary_note}); "
            "a clean result is weak evidence",
            file=sys.stderr,
        )
        return
    ids = {f.cve for f in query_cves(lib, lib.canary_version, api_key, urlopen, sleep)}
    if lib.canary_cve not in ids:
        raise CheckError(
            f"{lib.name}: canary failed: NVD did not return {lib.canary_cve} for "
            f"{lib.cpe_vendor}:{lib.cpe_product}:{lib.canary_version}; the CPE vendor or "
            "product in the manifest is probably wrong (or NVD changed it)"
        )
    print(f"[bundled-c] {lib.name} canary ok ({lib.canary_cve} at {lib.canary_version})")


def evaluate(
    findings: list[Finding],
    exceptions: list[ExceptionRow],
    today: date,
) -> Report:
    """Compare findings with exceptions; expired exceptions always fail."""
    report = Report()
    by_key = {(e.id, e.library): e for e in exceptions}
    seen: set[tuple[str, str]] = set()
    for f in findings:
        key = (f.cve, f.library)
        seen.add(key)
        line = f"{f.cve} {f.library} {f.version}: {f.severity}; {f.fixed}"
        exc = by_key.get(key)
        if exc is None:
            report.problems.append(f"{line} - not excepted")
        elif today > exc.expires:
            report.problems.append(
                f"{line} - exception expired {exc.expires.isoformat()} ({exc.reason})"
            )
        else:
            report.notes.append(f"excepted until {exc.expires.isoformat()}: {line} ({exc.reason})")
    for key, exc in by_key.items():
        if key in seen:
            continue
        if today > exc.expires:
            report.problems.append(
                f"{exc.id} {exc.library}: exception expired {exc.expires.isoformat()}; remove it"
            )
        else:
            report.notes.append(
                f"{exc.id} {exc.library}: exception no longer matches a CVE; remove it"
            )
    return report


def run(
    manifest: Path,
    exceptions_path: Path,
    lockfile: Path,
    cargo_home: Path,
    today: date,
    api_key: str | None,
    urlopen: UrlOpen = urllib.request.urlopen,
    sleep: Sleep = time.sleep,
) -> int:
    """Run the whole check and return the process exit code."""
    try:
        libs = load_manifest(manifest)
        exceptions = load_exceptions(exceptions_path, {lib.name for lib in libs})
        findings: list[Finding] = []
        for lib in libs:
            crate_version = locked_version(lockfile, lib.crate)
            crate_dir = find_crate_source(cargo_home, lib.crate, crate_version)
            version = extract_c_version(lib, crate_dir)
            print(f"[bundled-c] {lib.name} {version} (via {lib.crate} {crate_version})")
            check_canary(lib, api_key, urlopen, sleep)
            findings.extend(query_cves(lib, version, api_key, urlopen, sleep))
    except CheckError as err:
        print(f"[bundled-c] ERROR: {err}", file=sys.stderr)
        return 2
    report = evaluate(findings, exceptions, today)
    for note in report.notes:
        print(f"[bundled-c] {note}")
    for problem in report.problems:
        print(f"[bundled-c] ERROR: {problem}", file=sys.stderr)
    if not report.problems:
        print(f"[bundled-c] ok: {len(findings)} CVE(s), all excepted")
    return 1 if report.problems else 0


def main() -> int:
    """CLI entry point."""
    parser = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n", 1)[0])
    parser.add_argument("--manifest", type=Path, default=Path("ci/bundled-c.toml"))
    parser.add_argument("--exceptions", type=Path, default=Path("ci/bundled-c-exceptions.toml"))
    parser.add_argument("--lockfile", type=Path, default=Path("src-tauri/Cargo.lock"))
    parser.add_argument(
        "--root",
        type=Path,
        default=Path.cwd(),
        help="directory that --manifest/--exceptions/--lockfile must stay under",
    )
    parser.add_argument("--today", type=str, default=None, help="YYYY-MM-DD override")
    args = parser.parse_args()
    try:
        today = parse_canonical_date(args.today, label="--today") if args.today else date.today()
        root = args.root.resolve()
        manifest = resolve_under_root(args.manifest, root)
        exceptions = resolve_under_root(args.exceptions, root)
        lockfile = resolve_under_root(args.lockfile, root)
    except ValueError as err:
        print(f"[bundled-c] ERROR: {err}", file=sys.stderr)
        return 2
    cargo_home = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo")
    api_key = os.environ.get("NVD_API_KEY") or None
    return run(manifest, exceptions, lockfile, cargo_home, today, api_key)


if __name__ == "__main__":
    sys.exit(main())
