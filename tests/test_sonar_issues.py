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
            for host in (
                "http://localhost:9000",
                "http://127.0.0.1",
                "http://[::1]:9000/",
                "https://localhost:9000",
            ):
                with self.subTest(host=host):
                    self.assertTrue(mod.host_allowed(host))

    def test_remote_and_userinfo_tricks_fail(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            for host in (
                "https://sonarcloud.io",
                "http://localhost@evil.test:9000",
                "http://localhost.evil.test",
                "ftp://localhost:9000",
                "http://[::1",
                "http://localhost:notaport",
                "http://localhost:9000?x=1",
                "http://localhost:9000/#frag",
                "http://localhost:9000?",
                "http://localhost:9000/#",
            ):
                with self.subTest(host=host):
                    self.assertFalse(mod.host_allowed(host))

    def test_exported_override_allows_https_remote_only(self) -> None:
        with mock.patch.dict(os.environ, {"SONAR_ALLOW_REMOTE": "1"}, clear=True):
            self.assertTrue(mod.host_allowed("https://sonar.example.com"))
            for host in ("http://sonar.example.com", "ftp://sonar.example.com", "http://[::1"):
                with self.subTest(host=host):
                    self.assertFalse(mod.host_allowed(host))

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


class FetchIssuesTests(unittest.TestCase):
    def test_two_pages_are_combined(self) -> None:
        pages = {
            "1": {"issues": [{"key": "a"}], "paging": {"total": 2}},
            "2": {"issues": [{"key": "b"}], "paging": {"total": 2}},
        }
        with mock.patch.object(mod, "api_get", side_effect=lambda h, t, p, q: pages[q["p"]]) as api:
            issues, total = mod.fetch_issues("http://localhost:9000", "tok", "proj")
        self.assertEqual([i["key"] for i in issues], ["a", "b"])
        self.assertEqual(total, 2)
        self.assertEqual([c.args[3]["p"] for c in api.call_args_list], ["1", "2"])

    def test_response_without_paging_makes_one_request(self) -> None:
        reply = {"issues": [{"key": "a"}, {"key": "b"}]}
        with mock.patch.object(mod, "api_get", return_value=reply) as api:
            issues, total = mod.fetch_issues("http://localhost:9000", "tok", "proj")
        self.assertEqual(len(issues), 2)
        self.assertEqual(total, 2)
        api.assert_called_once()

    def test_cap_bounds_the_request_loop(self) -> None:
        def page_of_three(_host, _token, _path, params):  # 3 does not divide the cap of 10
            first = (int(params["p"]) - 1) * 3
            return {
                "issues": [{"key": f"k{n}"} for n in range(first, first + 3)],
                "paging": {"total": 10_000},
            }

        with (
            mock.patch.object(mod, "ISSUE_CAP", 10),
            mock.patch.object(mod, "api_get", side_effect=page_of_three) as api,
        ):
            issues, total = mod.fetch_issues("http://localhost:9000", "tok", "proj")
        self.assertEqual(len(issues), 10)
        self.assertEqual([i["key"] for i in issues], [f"k{n}" for n in range(10)])
        self.assertEqual(total, 10_000)
        # 3, 6, 9, 12 fetched; the loop stops at the cap after pages 1 to 4, in order.
        self.assertEqual([c.args[3]["p"] for c in api.call_args_list], ["1", "2", "3", "4"])


class SummaryTests(unittest.TestCase):
    def test_truncation_is_flagged(self) -> None:
        text = mod.summarize([{"rule": "r", "component": "p:f.py", "line": 1}], 5)
        self.assertIn("Total: 5", text)
        self.assertIn("Truncated", text)
        self.assertNotIn("Truncated", mod.summarize([], 0))


class ReportNewTests(unittest.TestCase):
    def _run(self, previous, issues, total) -> str:
        import io
        from contextlib import redirect_stdout

        out = io.StringIO()
        with redirect_stdout(out):
            mod.report_new(previous, issues, total)
        return out.getvalue()

    def test_a_moved_issue_is_not_new(self) -> None:
        old = {"rule": "r", "component": "p:a.rs", "message": "m", "line": 3}
        moved = dict(old, line=9)
        self.assertIn("No new issues", self._run((1, [old]), [moved], 1))

    def test_a_new_issue_demands_a_fix(self) -> None:
        old = {"rule": "r", "component": "p:a.rs", "message": "m"}
        new = {"rule": "s", "component": "p:b.rs", "message": "n", "line": 4}
        text = self._run((1, [old]), [old, new], 2)
        self.assertIn("ACTION REQUIRED", text)
        self.assertIn("b.rs:4", text)
        self.assertIn("Fix these", text)

    def test_server_keys_win_over_messages(self) -> None:
        old = {"key": "k1", "rule": "S3776", "component": "p:a.rs", "message": "from 18"}
        fixed_some = dict(old, message="from 16")
        twin = dict(old, key="k2")
        self.assertIn("No new issues", self._run((1, [old]), [fixed_some], 1))
        self.assertIn("ACTION REQUIRED", self._run((1, [old]), [old, twin], 2))

    def test_load_previous_refuses_bad_records(self) -> None:
        import json

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            dump = root / "d.json"
            state = root / "s.json"
            with mock.patch.object(mod, "ROOT", root):
                for issues, truncated in (([{"key": "a"}], True), ({"a": 1}, False), ([1], False)):
                    dump.write_text(json.dumps(issues), encoding="utf-8")
                    state.write_text(
                        json.dumps(
                            {
                                "issues_path": "d.json",
                                "issues_count": 1,
                                "issues_truncated": truncated,
                            }
                        ),
                        encoding="utf-8",
                    )
                    self.assertIsNone(mod.load_previous(state), issues)
                dump.write_text(json.dumps([{"key": "a"}]), encoding="utf-8")
                state.write_text(
                    json.dumps({"issues_path": "d.json", "issues_count": 1}), encoding="utf-8"
                )
                self.assertEqual(mod.load_previous(state), (1, [{"key": "a"}]))

    def test_no_previous_scan(self) -> None:
        self.assertIn("No complete earlier scan", self._run(None, [], 0))


if __name__ == "__main__":
    unittest.main()
