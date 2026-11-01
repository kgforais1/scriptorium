#!/usr/bin/env python3
"""
Unit tests for ci/scripts/check_bundled_c_cves.py.

Offline: the NVD call is replaced by an injected fake urlopen and sleep.

Run:
  python3 -m unittest tests.test_check_bundled_c_cves -v
  # or: pytest tests/test_check_bundled_c_cves.py -q
"""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import tempfile
import unittest
import urllib.error
from datetime import date
from pathlib import Path
from types import ModuleType
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CVE = "CVE-2025-54874"


def _load_module() -> ModuleType:
    """Load the CI script as a module for direct calls."""
    path = ROOT / "ci" / "scripts" / "check_bundled_c_cves.py"
    spec = importlib.util.spec_from_file_location("check_bundled_c_cves", path)
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod  # dataclasses look the module up by name
    spec.loader.exec_module(mod)
    return mod


mod = _load_module()

MANIFEST = """
[[library]]
name = "OpenJPEG"
crate = "openjpeg-sys"
version_file = "config/opj_config.h"
version_regex = '(?s)OPJ_VERSION_MAJOR\\s+(\\d+).*?OPJ_VERSION_MINOR\\s+(\\d+).*?OPJ_VERSION_BUILD\\s+(\\d+)'
cpe_vendor = "uclouvain"
cpe_product = "openjpeg"
"""
OPJ_CONFIG = (
    "#define OPJ_VERSION_MAJOR 2\n#define OPJ_VERSION_MINOR 5\n#define OPJ_VERSION_BUILD 3\n"
)
LOCK = '[[package]]\nname = "openjpeg-sys"\nversion = "1.0.12"\n'
NVD_BODY = {
    "totalResults": 1,
    "vulnerabilities": [
        {
            "cve": {
                "id": CVE,
                "metrics": {
                    "cvssMetricV31": [{"cvssData": {"baseScore": 7.8, "baseSeverity": "HIGH"}}]
                },
                "configurations": [
                    {
                        "nodes": [
                            {
                                "cpeMatch": [
                                    {
                                        "vulnerable": True,
                                        "criteria": "cpe:2.3:a:uclouvain:openjpeg:*:*:*:*:*:*:*:*",
                                        "versionEndExcluding": "2.5.4",
                                    }
                                ]
                            }
                        ]
                    }
                ],
            }
        }
    ],
}


class FakeResponse(io.BytesIO):
    """A urlopen result usable as a context manager."""

    def __enter__(self) -> FakeResponse:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


class FakeNvd:
    """Callable stand-in for urlopen that replays a script of bodies or HTTP codes."""

    def __init__(self, script: list[Any]) -> None:
        self.script = list(script)
        self.calls: list[str] = []

    def __call__(self, req: Any, timeout: int = 0) -> FakeResponse:
        self.calls.append(req.full_url)
        step = self.script.pop(0) if len(self.script) > 1 else self.script[0]
        if isinstance(step, int):
            raise urllib.error.HTTPError(req.full_url, step, "err", {}, None)  # type: ignore[arg-type]
        if isinstance(step, Exception):
            raise step
        return FakeResponse(json.dumps(step).encode())


def _make_tree(tmp: Path, *, with_source: bool = True) -> dict[str, Path]:
    """Write manifest, lockfile, exceptions dir and a fake cargo home under tmp."""
    manifest = tmp / "bundled-c.toml"
    manifest.write_text(MANIFEST, encoding="utf-8")
    lock = tmp / "Cargo.lock"
    lock.write_text(LOCK, encoding="utf-8")
    home = tmp / "cargo"
    src = home / "registry" / "src" / "index.crates.io-x" / "openjpeg-sys-1.0.12"
    if with_source:
        (src / "config").mkdir(parents=True)
        (src / "config" / "opj_config.h").write_text(OPJ_CONFIG, encoding="utf-8")
    else:
        (home / "registry" / "src").mkdir(parents=True)
    return {"manifest": manifest, "lock": lock, "home": home, "src": src}


def _write_exceptions(tmp: Path, expires: str | None) -> Path:
    """Write an exceptions file with one OpenJPEG row, or none when expires is None."""
    path = tmp / "exceptions.toml"
    if expires is None:
        path.write_text("exceptions = []\n", encoding="utf-8")
    else:
        path.write_text(
            f'[[exceptions]]\nid = "{CVE}"\nlibrary = "OpenJPEG"\nreason = "r"\nexpires = "{expires}"\n',
            encoding="utf-8",
        )
    return path


class CheckBundledCvesTests(unittest.TestCase):
    """Behaviour of the bundled-C CVE check."""

    def _run(
        self, expires: str | None, nvd: Any, *, with_source: bool = True
    ) -> tuple[int, list[float]]:
        sleeps: list[float] = []
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            t = _make_tree(tmp, with_source=with_source)
            rc = mod.run(
                t["manifest"],
                _write_exceptions(tmp, expires),
                t["lock"],
                t["home"],
                date(2026, 11, 1),
                None,
                urlopen=nvd,
                sleep=sleeps.append,
            )
        return rc, sleeps

    def test_extracts_version_from_source(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            t = _make_tree(Path(d))
            lib = mod.load_manifest(t["manifest"])[0]
            self.assertEqual(mod.extract_c_version(lib, t["src"]), "2.5.3")

    def test_extract_fails_when_regex_misses(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            t = _make_tree(Path(d))
            (t["src"] / "config" / "opj_config.h").write_text("nothing\n", encoding="utf-8")
            lib = mod.load_manifest(t["manifest"])[0]
            with self.assertRaises(mod.CheckError):
                mod.extract_c_version(lib, t["src"])

    def test_parses_locked_version(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            t = _make_tree(Path(d))
            self.assertEqual(mod.locked_version(t["lock"], "openjpeg-sys"), "1.0.12")
            with self.assertRaises(mod.CheckError):
                mod.locked_version(t["lock"], "nope-sys")

    def test_cve_reported_fails(self) -> None:
        nvd = FakeNvd([NVD_BODY])
        rc, _ = self._run(None, nvd)
        self.assertEqual(rc, 1)
        self.assertIn("cpeName=cpe:2.3:a:uclouvain:openjpeg:2.5.3:", nvd.calls[0])

    def test_finding_has_severity_and_fixed_version(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            t = _make_tree(Path(d))
            lib = mod.load_manifest(t["manifest"])[0]
            found = mod.query_cves(lib, "2.5.3", None, FakeNvd([NVD_BODY]), lambda _s: None)
        self.assertEqual(found[0].cve, CVE)
        self.assertEqual(found[0].severity, "HIGH 7.8 (V31)")
        self.assertEqual(found[0].fixed, "fixed in 2.5.4")

    def test_exception_suppresses(self) -> None:
        rc, _ = self._run("2026-11-29", FakeNvd([NVD_BODY]))
        self.assertEqual(rc, 0)

    def test_expired_exception_fails(self) -> None:
        rc, _ = self._run("2026-10-31", FakeNvd([NVD_BODY]))
        self.assertEqual(rc, 1)

    def test_bad_date_format_rejected(self) -> None:
        rc, _ = self._run("20261129", FakeNvd([NVD_BODY]))
        self.assertEqual(rc, 2)

    def test_missing_crate_source_fails_with_cargo_fetch_hint(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            t = _make_tree(Path(d), with_source=False)
            with self.assertRaises(mod.CheckError) as ctx:
                mod.find_crate_source(t["home"], "openjpeg-sys", "1.0.12")
            self.assertIn("cargo fetch", str(ctx.exception))
        rc, _ = self._run("2026-11-29", FakeNvd([NVD_BODY]), with_source=False)
        self.assertEqual(rc, 2)

    def test_feed_unreachable_fails_closed(self) -> None:
        rc, _ = self._run("2026-11-29", FakeNvd([urllib.error.URLError("down")]))
        self.assertEqual(rc, 2)

    def test_rate_limit_is_retried_then_succeeds(self) -> None:
        nvd = FakeNvd([429, 503, NVD_BODY])
        rc, sleeps = self._run("2026-11-29", nvd)
        self.assertEqual(rc, 0)
        self.assertEqual(len(nvd.calls), 3)
        self.assertGreaterEqual(len(sleeps), 2)

    def test_rate_limit_gives_up_and_fails_closed(self) -> None:
        nvd = FakeNvd([403])
        rc, _ = self._run("2026-11-29", nvd)
        self.assertEqual(rc, 2)
        self.assertEqual(len(nvd.calls), mod.MAX_ATTEMPTS)


if __name__ == "__main__":
    unittest.main()
