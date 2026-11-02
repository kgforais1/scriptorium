#!/usr/bin/env python3
"""
Unit tests for ci/scripts/check_bundled_c_cves.py.

Offline: the NVD call is replaced by an injected fake urlopen and sleep.

Run:
  python3 -m unittest tests.test_check_bundled_c_cves -v
  # or: pytest tests/test_check_bundled_c_cves.py -q
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from unittest import mock
import sys
import tempfile
import unittest
import unittest.mock
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
canary_version = "2.3.0"
canary_cve = "CVE-2018-5727"
"""
NO_CANARY_MANIFEST = MANIFEST.replace(
    'canary_version = "2.3.0"\ncanary_cve = "CVE-2018-5727"\n',
    'canary = "none"\ncanary_note = "no known CVE under this CPE"\n',
)
CANARY_BODY = {
    "totalResults": 1,
    "vulnerabilities": [{"cve": {"id": "CVE-2018-5727"}}],
}
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

    def read(self, size: int | None = -1) -> bytes:
        if size is None or size < 0:
            raise AssertionError("reads must be bounded")
        return super().read(size)

    def __enter__(self) -> FakeResponse:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


class FakeNvd:
    """Callable stand-in for urlopen that replays a script of bodies or HTTP codes."""

    def __init__(self, script: list[Any], canary: Any = CANARY_BODY) -> None:
        self.script = list(script)
        self.canary = canary
        self.calls: list[str] = []  # real-version queries only
        self.canary_calls = 0

    def __call__(self, req: Any, timeout: int = 0) -> FakeResponse:
        if ":2.3.0:" in req.full_url:  # the canary version
            self.canary_calls += 1
            return FakeResponse(json.dumps(self.canary).encode())
        self.calls.append(req.full_url)
        step = self.script.pop(0) if len(self.script) > 1 else self.script[0]
        if isinstance(step, int):
            raise urllib.error.HTTPError(req.full_url, step, "err", {}, None)  # type: ignore[arg-type]
        if isinstance(step, Exception):
            raise step
        return FakeResponse(json.dumps(step).encode())


def _make_tree(
    tmp: Path, *, with_source: bool = True, manifest_text: str = MANIFEST
) -> dict[str, Path]:
    """Write manifest, lockfile, exceptions dir and a fake cargo home under tmp."""
    manifest = tmp / "bundled-c.toml"
    manifest.write_text(manifest_text, encoding="utf-8")
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
        self,
        expires: str | None,
        nvd: Any,
        *,
        with_source: bool = True,
        manifest_text: str = MANIFEST,
    ) -> tuple[int, list[float]]:
        sleeps: list[float] = []
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            t = _make_tree(tmp, with_source=with_source, manifest_text=manifest_text)
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

    def test_canary_found_passes(self) -> None:
        nvd = FakeNvd([NVD_BODY])
        rc, _ = self._run("2026-11-29", nvd)
        self.assertEqual(rc, 0)
        self.assertEqual(nvd.canary_calls, 1)

    def test_canary_missing_exits_2(self) -> None:
        empty = {"totalResults": 0, "vulnerabilities": []}
        nvd = FakeNvd([NVD_BODY], canary=empty)
        buf = io.StringIO()
        with contextlib.redirect_stderr(buf):
            rc, _ = self._run("2026-11-29", nvd)
        self.assertEqual(rc, 2)
        self.assertIn("probably wrong", buf.getvalue())
        self.assertEqual(nvd.calls, [])  # the real query never ran

    def test_canary_none_warns(self) -> None:
        nvd = FakeNvd([NVD_BODY])
        buf = io.StringIO()
        with contextlib.redirect_stderr(buf):
            rc, _ = self._run("2026-11-29", nvd, manifest_text=NO_CANARY_MANIFEST)
        self.assertEqual(rc, 0)
        self.assertEqual(nvd.canary_calls, 0)
        self.assertIn("WARNING", buf.getvalue())

    def test_canary_none_requires_note(self) -> None:
        bad = NO_CANARY_MANIFEST.replace('canary_note = "no known CVE under this CPE"\n', "")
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "m.toml"
            path.write_text(bad, encoding="utf-8")
            with self.assertRaises(mod.CheckError):
                mod.load_manifest(path)

    def test_manifest_without_canary_rejected(self) -> None:
        bad = MANIFEST.replace('canary_cve = "CVE-2018-5727"\n', "")
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "m.toml"
            path.write_text(bad, encoding="utf-8")
            with self.assertRaises(mod.CheckError):
                mod.load_manifest(path)

    def test_fixed_version_found_in_nested_nodes(self) -> None:
        match = {
            "vulnerable": True,
            "criteria": "cpe:2.3:a:uclouvain:openjpeg:*:*:*:*:*:*:*:*",
            "versionEndExcluding": "2.5.4",
        }
        cve = {
            "configurations": [
                {"nodes": [{"operator": "AND", "nodes": [{"operator": "OR", "cpeMatch": [match]}]}]}
            ]
        }
        with tempfile.TemporaryDirectory() as d:
            lib = mod.load_manifest(_make_tree(Path(d))["manifest"])[0]
        self.assertEqual(mod._fixed_version(cve, lib), "fixed in 2.5.4")

    def _query(self, pages: list[Any]) -> list[Any]:
        with tempfile.TemporaryDirectory() as d:
            lib = mod.load_manifest(_make_tree(Path(d))["manifest"])[0]
        return mod.query_cves(lib, "2.5.3", None, FakeNvd(pages), lambda _s: None)

    def test_reply_missing_vulnerabilities_fails_closed(self) -> None:
        with self.assertRaises(mod.CheckError):
            self._query([{"totalResults": 0}])

    def test_reply_missing_total_results_fails_closed(self) -> None:
        with self.assertRaises(mod.CheckError):
            self._query([{"vulnerabilities": []}])

    def test_reply_wrong_types_fail_closed(self) -> None:
        for body in (
            {"totalResults": "1", "vulnerabilities": []},
            {"totalResults": 0, "vulnerabilities": {}},
            {"totalResults": True, "vulnerabilities": []},
        ):
            with self.subTest(body=body), self.assertRaises(mod.CheckError):
                self._query([body])

    def test_early_empty_page_fails_closed(self) -> None:
        first = {"totalResults": 3, "vulnerabilities": NVD_BODY["vulnerabilities"]}
        empty = {"totalResults": 3, "vulnerabilities": []}
        with self.assertRaises(mod.CheckError) as ctx:
            self._query([first, empty])
        self.assertIn("ended at 1 of 3", str(ctx.exception))

    def test_malformed_cve_entry_fails_closed(self) -> None:
        nested_bad = {"cve": {"id": CVE, "metrics": []}}
        score = {"cvssMetricV31": [{"cvssData": {"baseScore": "7"}}]}
        bad_score = {"cve": {"id": CVE, "metrics": score}}
        for item in (
            "x",
            {"cve": "x"},
            {"cve": {"id": 7}},
            {},
            {"cve": {"id": "nope"}},
            nested_bad,
            bad_score,
        ):
            body = {"totalResults": 1, "vulnerabilities": [item]}
            with self.subTest(item=item), self.assertRaises(mod.CheckError):
                self._query([body])

    def test_duplicate_cves_fail_closed(self) -> None:
        dup = NVD_BODY["vulnerabilities"] * 2
        with self.assertRaises(mod.CheckError) as ctx:
            self._query([{"totalResults": 2, "vulnerabilities": dup}])
        self.assertIn("duplicate", str(ctx.exception))

    def test_duplicates_across_pages_fail_closed(self) -> None:
        page = {"totalResults": 2, "vulnerabilities": NVD_BODY["vulnerabilities"]}
        with self.assertRaises(mod.CheckError) as ctx:
            self._query([page, page])
        self.assertIn("duplicate", str(ctx.exception))

    def test_undecodable_reply_fails_closed(self) -> None:
        for body in (b"\xff\xfe\xfa", b"[" * 100000):
            nvd = mock.Mock(side_effect=lambda req, timeout=0, b=body: FakeResponse(b))
            with self.subTest(body=body[:4]), self.assertRaises(mod.CheckError):
                mod.nvd_get("https://x", None, nvd, lambda _s: None)

    def test_duplicates_beyond_total_fail_closed(self) -> None:
        dup = NVD_BODY["vulnerabilities"] * 2
        with self.assertRaises(mod.CheckError) as ctx:
            self._query([{"totalResults": 1, "vulnerabilities": dup}])
        self.assertIn("duplicate", str(ctx.exception))

    def test_more_results_than_total_fail_closed(self) -> None:
        other = {"cve": {"id": "CVE-2025-00002"}}
        two = NVD_BODY["vulnerabilities"] + [other]
        with self.assertRaises(mod.CheckError) as ctx:
            self._query([{"totalResults": 1, "vulnerabilities": two}])
        self.assertIn("returned 2 results", str(ctx.exception))

    def test_oversized_reply_fails_closed(self) -> None:
        with mock.patch.object(mod, "MAX_BODY_BYTES", 10), self.assertRaises(mod.CheckError) as ctx:
            self._query([NVD_BODY])
        self.assertIn("larger than", str(ctx.exception))

    def test_total_that_shrinks_between_pages_fails_closed(self) -> None:
        first = {"totalResults": 3, "vulnerabilities": NVD_BODY["vulnerabilities"]}
        shrunk = {"totalResults": 1, "vulnerabilities": []}
        with self.assertRaises(mod.CheckError) as ctx:
            self._query([first, shrunk])
        self.assertIn("changed totalResults from 3 to 1", str(ctx.exception))

    def test_multi_page_reply_is_read_to_the_end(self) -> None:
        other = {"cve": {"id": "CVE-2025-00002"}}
        first = {"totalResults": 2, "vulnerabilities": NVD_BODY["vulnerabilities"]}
        second = {"totalResults": 2, "vulnerabilities": [other]}
        found = self._query([first, second])
        self.assertEqual([f.cve for f in found], ["CVE-2025-00002", CVE])

    def test_path_outside_root_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "root"
            root.mkdir()
            for bad in (Path("../outside.toml"), Path(d) / "outside.toml"):
                with self.subTest(path=str(bad)), self.assertRaises(ValueError):
                    mod.resolve_under_root(bad, root)
            inside = mod.resolve_under_root(Path("ci/x.toml"), root)
            self.assertEqual(inside, (root / "ci" / "x.toml").resolve())

    def test_symlink_escape_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "root"
            root.mkdir()
            (Path(d) / "secret.toml").write_text("x = 1\n", encoding="utf-8")
            (root / "link.toml").symlink_to(Path(d) / "secret.toml")
            with self.assertRaises(ValueError):
                mod.resolve_under_root(Path("link.toml"), root)

    def test_main_rejects_cli_path_outside_root(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            argv = ["prog", "--root", d, "--manifest", "/etc/passwd"]
            buf = io.StringIO()
            with (
                unittest.mock.patch.object(sys, "argv", argv),
                contextlib.redirect_stderr(buf),
            ):
                rc = mod.main()
        self.assertEqual(rc, 2)
        self.assertIn("must stay under", buf.getvalue())

    def test_version_file_escaping_crate_dir_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            t = _make_tree(Path(d))
            lib = mod.load_manifest(t["manifest"])[0]
            bad = mod.Library(**{**lib.__dict__, "version_file": "../../../../Cargo.lock"})
            with self.assertRaises(mod.CheckError):
                mod.extract_c_version(bad, t["src"])


if __name__ == "__main__":
    unittest.main()
