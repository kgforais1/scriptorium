#!/usr/bin/env python3
"""Static lock on Tauri production CSP, asset protocol, and default capabilities.

Reads committed JSON only (stdlib). Each locked production directive must equal
its approved source list (compared sorted, so order does not matter). A source
that ``devCsp`` adds on top of that list must not appear on the production
directive. This does not prove the webview enforces CSP at runtime; see
docs/manual-test-m1.md section 8 for the release-build check.

Run:
  python -m pytest tests/test_tauri_csp.py -q
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
TAURI_CONF = ROOT / "src-tauri" / "tauri.conf.json"
DEFAULT_CAPABILITY = ROOT / "src-tauri" / "capabilities" / "default.json"

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


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _assert_exact(field: str, actual: Any, expected: Any) -> None:
    assert actual == expected, field


def _source_list(field: str, value: Any) -> list[str]:
    """Return the directive's sources sorted, or fail with the field name."""
    assert isinstance(value, list), field
    assert all(isinstance(item, str) for item in value), field
    return sorted(value)


def _assert_dev_does_not_override(
    directive: str,
    production: Any,
    dev: Any,
    approved: list[str],
) -> None:
    """Fail if production sources are not the approved set, or include a dev-only source."""
    field = f"csp.{directive}"
    prod = _source_list(field, production)
    dev_sources = set(_source_list(f"devCsp.{directive}", dev))
    approved_set = set(approved)
    assert prod == sorted(approved), field
    assert set(prod).isdisjoint(dev_sources - approved_set), field


def test_tauri_production_csp_and_capabilities() -> None:
    """Lock app.security and default capability permissions against drift."""
    conf = _load_json(TAURI_CONF)
    caps = _load_json(DEFAULT_CAPABILITY)

    security = conf["app"]["security"]
    csp = security["csp"]
    # A dict is neither null nor a CSP string. Tauri accepts both of those, and
    # neither is the production policy this test locks.
    assert isinstance(csp, dict), "app.security.csp"

    assert "devCsp" in security, "app.security.devCsp"
    dev_csp = security["devCsp"]
    assert isinstance(dev_csp, dict), "app.security.devCsp"
    assert "csp" in security, "app.security.csp"

    for directive, approved in _PRODUCTION_SOURCES.items():
        assert directive in dev_csp, f"devCsp.{directive}"
        _assert_dev_does_not_override(directive, csp[directive], dev_csp[directive], approved)

    dev_script_src = _source_list("devCsp.script-src", dev_csp["script-src"])
    assert "'unsafe-inline'" in dev_script_src, "devCsp.script-src"
    assert "'unsafe-eval'" not in dev_script_src, "devCsp.script-src"

    asset_protocol = security["assetProtocol"]
    assert isinstance(asset_protocol, dict), "app.security.assetProtocol"
    _assert_exact("app.security.assetProtocol.enable", asset_protocol["enable"], True)
    _assert_exact("app.security.assetProtocol.scope", asset_protocol["scope"], [])

    _assert_exact(
        "capabilities/default.json permissions",
        caps["permissions"],
        ["core:default", "dialog:allow-open"],
    )
