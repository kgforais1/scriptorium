#!/usr/bin/env python3
"""Static lock on Tauri production CSP, asset protocol, and default capabilities.

Reads committed JSON only (stdlib). Does not prove the webview enforces CSP at runtime;
see docs/manual-test-m1.md section 8 for the release-build check.

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


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _assert_exact(field: str, actual: Any, expected: Any) -> None:
    assert actual == expected, field


def _assert_contains(field: str, values: list[Any], item: Any) -> None:
    assert item in values, field


def test_tauri_production_csp_and_capabilities() -> None:
    """Lock app.security and default capability permissions against drift."""
    conf = _load_json(TAURI_CONF)
    caps = _load_json(DEFAULT_CAPABILITY)

    security = conf["app"]["security"]
    csp = security["csp"]
    # A dict is neither null nor a CSP string. Tauri accepts both of those, and
    # neither is the production policy this test locks.
    assert isinstance(csp, dict), "app.security.csp"

    _assert_exact("csp.default-src", csp["default-src"], ["'self'"])
    _assert_exact("csp.script-src", csp["script-src"], ["'self'"])
    assert "'unsafe-inline'" not in csp["script-src"], "csp.script-src"
    assert "'unsafe-eval'" not in csp["script-src"], "csp.script-src"

    img_src = csp["img-src"]
    assert isinstance(img_src, list), "csp.img-src"
    for token in (
        "'self'",
        "asset:",
        "http://asset.localhost",
        "https://asset.localhost",
        "data:",
        "blob:",
    ):
        _assert_contains("csp.img-src", img_src, token)

    connect_src = csp["connect-src"]
    assert isinstance(connect_src, list), "csp.connect-src"
    for token in (
        "'self'",
        "asset:",
        "http://asset.localhost",
        "https://asset.localhost",
        "ipc:",
        "http://ipc.localhost",
    ):
        _assert_contains("csp.connect-src", connect_src, token)

    _assert_exact("csp.object-src", csp["object-src"], ["'none'"])
    _assert_exact("csp.base-uri", csp["base-uri"], ["'none'"])
    _assert_exact(
        "csp.style-src",
        csp["style-src"],
        ["'self'", "'unsafe-inline'"],
    )

    assert "devCsp" in security, "app.security.devCsp"
    dev_csp = security["devCsp"]
    assert isinstance(dev_csp, dict), "app.security.devCsp"
    # Production `csp` stays the locked object above. `devCsp` is a second key.
    assert "csp" in security, "app.security.csp"
    dev_script_src = dev_csp["script-src"]
    assert isinstance(dev_script_src, list), "devCsp.script-src"
    _assert_contains("devCsp.script-src", dev_script_src, "'unsafe-inline'")
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
