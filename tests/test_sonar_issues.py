#!/usr/bin/env python3
"""Tests for scripts/sonar_issues.py helpers. No server and no network.

Run:
  python3 -m unittest tests.test_sonar_issues -v
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import ModuleType
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def _load() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "sonar_issues", ROOT / "scripts" / "sonar_issues.py"
    )
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mod = _load()


class HostAllowedTests(unittest.TestCase):
    def test_loopback_hosts_pass(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            for host in ("http://localhost:9000", "http://127.0.0.1", "http://[::1]:9000/"):
                with self.subTest(host=host):
                    self.assertTrue(mod.host_allowed(host))

    def test_remote_and_userinfo_tricks_fail(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            for host in (
                "https://sonarcloud.io",
                "http://localhost@evil.test:9000",
                "http://localhost.evil.test",
                "https://localhost:9000",
            ):
                with self.subTest(host=host):
                    self.assertFalse(mod.host_allowed(host))

    def test_exported_override(self) -> None:
        with mock.patch.dict(os.environ, {"SONAR_ALLOW_REMOTE": "1"}, clear=True):
            self.assertTrue(mod.host_allowed("https://sonarcloud.io"))

    def test_check_host_cli(self) -> None:
        with mock.patch.dict(os.environ, {"SONAR_HOST_URL": "http://localhost:9000"}, clear=True):
            self.assertEqual(mod.main(["--check-host"]), 0)
        with mock.patch.dict(os.environ, {"SONAR_HOST_URL": "https://sonarcloud.io"}, clear=True):
            self.assertEqual(mod.main(["--check-host"]), 1)


class FingerprintTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.root = Path(self._tmp.name)
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        env.update(
            {
                "GIT_AUTHOR_NAME": "t",
                "GIT_AUTHOR_EMAIL": "t@example.com",
                "GIT_COMMITTER_NAME": "t",
                "GIT_COMMITTER_EMAIL": "t@example.com",
            }
        )
        self.env = env
        self._git("init", "-q")
        (self.root / "a.txt").write_text("one\n")
        self._git("add", "a.txt")
        self._git("commit", "-q", "--no-verify", "-m", "c")
        patcher = mock.patch.object(mod, "ROOT", self.root)
        patcher.start()
        self.addCleanup(patcher.stop)

    def _git(self, *args: str) -> None:
        subprocess.check_call(["git", *args], cwd=self.root, env=self.env)

    def test_stable_when_nothing_changes(self) -> None:
        first = mod.tree_fingerprint()
        second = mod.tree_fingerprint()
        self.assertEqual(first, second)

    def test_tracked_edit_changes_it(self) -> None:
        before = mod.tree_fingerprint()
        (self.root / "a.txt").write_text("two\n")
        self.assertNotEqual(before, mod.tree_fingerprint())

    def test_untracked_content_edit_changes_it(self) -> None:
        (self.root / "new.txt").write_text("x\n")
        before = mod.tree_fingerprint()
        (self.root / "new.txt").write_text("y\n")
        self.assertNotEqual(before, mod.tree_fingerprint())

    def test_new_commit_changes_it(self) -> None:
        before = mod.tree_fingerprint()
        self._git("commit", "-q", "--allow-empty", "--no-verify", "-m", "d")
        self.assertNotEqual(before, mod.tree_fingerprint())


class SummaryTests(unittest.TestCase):
    def test_truncation_is_flagged(self) -> None:
        text = mod.summarize([{"rule": "r", "component": "p:f.py", "line": 1}], 5)
        self.assertIn("Total: 5", text)
        self.assertIn("Truncated", text)
        self.assertNotIn("Truncated", mod.summarize([], 0))


if __name__ == "__main__":
    unittest.main()
