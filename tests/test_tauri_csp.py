#!/usr/bin/env python3
"""Static lock on Tauri production CSP, asset protocol, and default capabilities.

Reads committed JSON only (stdlib). The production CSP object must have exactly
the approved directive names, and each source list must equal the approved set
(compared sorted, so order does not matter). Permissions are the union of every
capability file Tauri 2.12 would apply to the ``main`` window: a ``windows`` or
``webviews`` glob that matches ``main``. An omitted ``windows`` key deserializes
to an empty list and does not grant access by itself (tauri 2.12
``ipc::authority::RuntimeAuthority::resolve_access``). Capability files other
than ``*.json``, and ``app.security.capabilities`` entries that are inline
objects or that name a file this test does not read, fail until the test is
extended. Tauri 2.12 otherwise loads ``json`` and ``toml`` (and ``json5`` only
with the ``config-json5`` feature, which this lockfile does not enable) and
treats a string in that list as a capability identifier. This does not prove
the webview enforces CSP at runtime; see docs/manual-test-m1.md section 8.

Run:
  python -m pytest tests/test_tauri_csp.py -q
"""

from __future__ import annotations

import fnmatch
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
TAURI_CONF = ROOT / "src-tauri" / "tauri.conf.json"
CAPABILITIES_DIR = ROOT / "src-tauri" / "capabilities"

# Approved production sources, in the sets committed in tauri.conf.json.
_PRODUCTION_SOURCES: dict[str, list[str]] = {
    "default-src": ["'self'"],
    "img-src": [
        "'self'",
        "asset:",
        "http://asset.localhost",
        "https://asset.localhost",
        "data:",
        "blob:",
    ],
    "style-src": ["'self'", "'unsafe-inline'"],
    "script-src": ["'self'"],
    "connect-src": [
        "'self'",
        "asset:",
        "http://asset.localhost",
        "https://asset.localhost",
        "ipc:",
        "http://ipc.localhost",
    ],
    "object-src": ["'none'"],
    "base-uri": ["'none'"],
}

_MAIN_WINDOW_PERMISSIONS = {"core:default", "dialog:allow-open"}
_EXTEND_MSG = "extend tests/test_tauri_csp.py before adding this capability source"


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _assert_exact(field: str, actual: Any, expected: Any) -> None:
    assert actual == expected, field


def _source_list(field: str, value: Any) -> list[str]:
    """Return the directive's sources sorted, or fail with the field name."""
    assert isinstance(value, list), field
    assert all(isinstance(item, str) for item in value), field
    return sorted(value)


def _assert_sources(field: str, value: Any, expected: list[str]) -> None:
    """Fail with the field name unless the sources are exactly the approved set."""
    assert _source_list(field, value) == sorted(expected), field


def _patterns_match_main(patterns: Any, field: str) -> bool:
    """Return whether any Tauri glob matches the ``main`` window label."""
    assert isinstance(patterns, list), field
    for pattern in patterns:
        assert isinstance(pattern, str), field
        if fnmatch.fnmatchcase("main", pattern):
            return True
    return False


def _applies_to_main_window(capability: dict[str, Any], field: str) -> bool:
    """Return whether this capability grants the local ``main`` window.

    Tauri matches a window when a ``windows`` glob matches its label, or a
    ``webviews`` glob matches its webview label (the default webview label is
    ``main``). Both keys default to an empty list, so a missing ``windows`` key
    does not apply the file. ``local: false`` is remote-only and is skipped.
    """
    if capability.get("local", True) is False:
        return False
    return _patterns_match_main(
        capability.get("windows", []), f"{field}.windows"
    ) or _patterns_match_main(capability.get("webviews", []), f"{field}.webviews")


def _permission_id(entry: Any, field: str) -> str:
    """Return a permission identifier from a string or an ``{identifier}`` object."""
    if isinstance(entry, str):
        return entry
    if isinstance(entry, dict) and isinstance(entry.get("identifier"), str):
        return str(entry["identifier"])
    raise AssertionError(field)


def _json_capability_files() -> list[Path]:
    """Return every ``*.json`` file under capabilities, or fail on any other file."""
    files = [path for path in CAPABILITIES_DIR.rglob("*") if path.is_file()]
    other = sorted(path.relative_to(ROOT).as_posix() for path in files if path.suffix != ".json")
    assert not other, f"{other}: {_EXTEND_MSG}"
    paths = sorted(path for path in files if path.suffix == ".json")
    assert paths, "src-tauri/capabilities"
    return paths


def _capability_identifiers(paths: list[Path]) -> set[str]:
    """Return the identifier of each JSON capability object this test reads."""
    identifiers: set[str] = set()
    for path in paths:
        field = path.relative_to(ROOT).as_posix()
        capability = _load_json(path)
        assert isinstance(capability, dict), f"{field}: {_EXTEND_MSG}"
        identifier = capability.get("identifier")
        assert isinstance(identifier, str), f"{field}: {_EXTEND_MSG}"
        assert identifier, f"{field}: {_EXTEND_MSG}"
        identifiers.add(identifier)
    return identifiers


def _assert_config_capabilities(security: Any, identifiers: set[str]) -> None:
    """Fail unless ``app.security.capabilities`` is absent or names JSON files already read.

    A string is a capability identifier. An object is an inline capability.
    """
    if "capabilities" not in security:
        return
    entries = security["capabilities"]
    assert isinstance(entries, list), f"app.security.capabilities: {_EXTEND_MSG}"
    for entry in entries:
        assert isinstance(entry, str), f"app.security.capabilities: {_EXTEND_MSG}"
        assert entry in identifiers, f"app.security.capabilities: {_EXTEND_MSG}"


def _main_window_permissions(paths: list[Path]) -> set[str]:
    """Union of permissions from every capability file that applies to ``main``."""
    granted: set[str] = set()
    for path in paths:
        field = f"capabilities/{path.name}"
        capability = _load_json(path)
        assert isinstance(capability, dict), field
        if not _applies_to_main_window(capability, field):
            continue
        entries = capability.get("permissions")
        assert isinstance(entries, list), f"{field}.permissions"
        for entry in entries:
            granted.add(_permission_id(entry, f"{field}.permissions"))
    return granted


def test_tauri_production_csp_and_capabilities() -> None:
    """Lock app.security and the main window's capability permissions against drift."""
    conf = _load_json(TAURI_CONF)

    security = conf["app"]["security"]
    csp = security["csp"]
    # A dict is neither null nor a CSP string. Tauri accepts both of those, and
    # neither is the production policy this test locks.
    assert isinstance(csp, dict), "app.security.csp"
    assert set(csp) == set(_PRODUCTION_SOURCES), "app.security.csp"

    assert "devCsp" in security, "app.security.devCsp"
    dev_csp = security["devCsp"]
    assert isinstance(dev_csp, dict), "app.security.devCsp"
    assert "csp" in security, "app.security.csp"

    for directive, approved in _PRODUCTION_SOURCES.items():
        _assert_sources(f"csp.{directive}", csp[directive], approved)

    dev_script_src = _source_list("devCsp.script-src", dev_csp["script-src"])
    assert "'unsafe-inline'" in dev_script_src, "devCsp.script-src"
    assert "'unsafe-eval'" not in dev_script_src, "devCsp.script-src"

    asset_protocol = security["assetProtocol"]
    assert isinstance(asset_protocol, dict), "app.security.assetProtocol"
    _assert_exact("app.security.assetProtocol.enable", asset_protocol["enable"], True)
    _assert_exact("app.security.assetProtocol.scope", asset_protocol["scope"], [])

    capability_files = _json_capability_files()
    _assert_config_capabilities(security, _capability_identifiers(capability_files))
    assert _main_window_permissions(capability_files) == _MAIN_WINDOW_PERMISSIONS, "capabilities"
