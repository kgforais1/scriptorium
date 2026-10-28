#!/usr/bin/env python3
"""Opt-in contract tests for scripts/check_sonar_freshness.py.

The gate must be a no-op (exit 0, no git, no network, no state read) unless
SONAR_FRESHNESS_GATE is on in the environment or the repo-root .env. These
tests point the script at a temp repo root and fail if git or a socket is
touched while the gate is off.

Run:
  python3 -m unittest tests.test_check_sonar_freshness -v
  # or: pytest tests/test_check_sonar_freshness.py -q
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import os
import socket
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import ModuleType
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check_sonar_freshness.py"
GATE_VARS = ("SONAR_FRESHNESS_GATE", "SONAR_GATE_MAX_COMMITS", "PRE_COMMIT_TO_REF")


def _load_module() -> ModuleType:
    spec = importlib.util.spec_from_file_location("check_sonar_freshness", SCRIPT)
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mod = _load_module()


def _forbidden(*_args: object, **_kwargs: object) -> None:
    raise AssertionError("gate touched git or the network while off")


class FreshnessGateOptInTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)
        env = {k: v for k, v in os.environ.items() if k not in GATE_VARS}
        patches = [
            mock.patch.dict(os.environ, env, clear=True),
            mock.patch.object(mod, "repo_root", return_value=self.root),
        ]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)

    def _run_offline(self, *argv: str) -> int:
        with contextlib.ExitStack() as stack:
            stack.enter_context(mock.patch.object(subprocess, "check_output", _forbidden))
            stack.enter_context(mock.patch.object(subprocess, "run", _forbidden))
            stack.enter_context(mock.patch.object(socket, "create_connection", _forbidden))
            stack.enter_context(mock.patch.object(socket.socket, "connect", _forbidden))
            return mod.main(list(argv))

    def test_env_unset_is_noop_on_commit_and_push(self) -> None:
        self.assertEqual(self._run_offline(), 0)
        self.assertEqual(self._run_offline("--stage", "push"), 0)

    def test_gate_off_values_are_noop(self) -> None:
        for value in ("0", "false", "no", "", "off"):
            with self.subTest(value=value):
                os.environ["SONAR_FRESHNESS_GATE"] = value
                self.assertEqual(self._run_offline("--stage", "push"), 0)

    def test_dotenv_off_is_noop(self) -> None:
        (self.root / ".env").write_text("SONAR_FRESHNESS_GATE=0\n", encoding="utf-8")
        self.assertEqual(self._run_offline("--stage", "push"), 0)

    def test_exported_off_beats_dotenv_on(self) -> None:
        (self.root / ".env").write_text("SONAR_FRESHNESS_GATE=1\n", encoding="utf-8")
        os.environ["SONAR_FRESHNESS_GATE"] = "0"
        self.assertEqual(self._run_offline(), 0)

    def test_unreadable_dotenv_is_noop(self) -> None:
        (self.root / ".env").write_bytes(b"\xff\xfeSONAR_FRESHNESS_GATE=1\n")
        self.assertEqual(self._run_offline(), 0)

    def test_dotenv_on_without_state_blocks(self) -> None:
        (self.root / ".env").write_text(
            'export SONAR_FRESHNESS_GATE="1"  # opt in\n', encoding="utf-8"
        )
        with mock.patch("sys.stderr"):
            self.assertEqual(self._run_offline(), 1)

    def test_env_on_with_invalid_state_blocks(self) -> None:
        os.environ["SONAR_FRESHNESS_GATE"] = "1"
        state = self.root / "tmp" / "sonar-state.json"
        state.parent.mkdir()
        state.write_text('{"last_scan_commit": "--help"}', encoding="utf-8")
        with mock.patch("sys.stderr"):
            self.assertEqual(self._run_offline("--stage", "push"), 1)

    def test_state_outside_repo_is_refused(self) -> None:
        os.environ["SONAR_FRESHNESS_GATE"] = "1"
        with mock.patch("sys.stderr"):
            self.assertEqual(self._run_offline("--state", "../outside.json"), 1)


class SubprocessOptInTests(unittest.TestCase):
    """End to end through the interpreter, as the pre-commit hook runs it."""

    def test_script_exits_zero_with_gate_unset(self) -> None:
        env = {k: v for k, v in os.environ.items() if k not in GATE_VARS}
        env["SONAR_FRESHNESS_GATE"] = "0"  # overrides any developer .env
        for stage in ("commit", "push"):
            with self.subTest(stage=stage):
                result = subprocess.run(
                    ["python3", str(SCRIPT), "--stage", stage],
                    env=env,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout + result.stderr, "")


class MaxCommitsTests(unittest.TestCase):
    def test_bad_values_fall_back_to_default(self) -> None:
        for raw in ("", "abc", "0", "-3"):
            with self.subTest(raw=raw):
                env = {"SONAR_GATE_MAX_COMMITS": raw}
                with mock.patch.dict(os.environ, env), mock.patch("sys.stderr"):
                    self.assertEqual(mod.resolve_max_commits(), mod.DEFAULT_MAX_COMMITS)

    def test_positive_value_is_used(self) -> None:
        with mock.patch.dict(os.environ, {"SONAR_GATE_MAX_COMMITS": "4"}):
            self.assertEqual(mod.resolve_max_commits(), 4)


if __name__ == "__main__":
    unittest.main()


class FreshnessGateOnTests(unittest.TestCase):
    """Gate enabled against a real temporary git repository."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)
        env = {k: v for k, v in os.environ.items() if k not in GATE_VARS}
        env.update(
            {
                "GIT_AUTHOR_NAME": "t",
                "GIT_AUTHOR_EMAIL": "t@example.com",
                "GIT_COMMITTER_NAME": "t",
                "GIT_COMMITTER_EMAIL": "t@example.com",
            }
        )
        for patcher in (
            mock.patch.dict(os.environ, env, clear=True),
            mock.patch.object(mod, "repo_root", return_value=self.root),
        ):
            patcher.start()
            self.addCleanup(patcher.stop)
        self._git("init", "-q")
        self.scanned = self._commit()
        self.state = self.root / "state.json"
        self.state.write_text(f'{{"last_scan_commit": "{self.scanned}"}}', encoding="utf-8")

    def _git(self, *args: str) -> str:
        return subprocess.check_output(
            ["git", *args], cwd=self.root, text=True, env=mod._git_env()
        ).strip()

    def _commit(self) -> str:
        self._git("commit", "-q", "--allow-empty", "--no-verify", "-m", "c")
        return self._git("rev-parse", "HEAD")

    def _run(self, stage: str, budget: int = 3) -> int:
        with contextlib.redirect_stderr(io.StringIO()):
            return mod.run_gate(self.state, budget, stage)

    def test_fresh_scan_passes_both_stages(self) -> None:
        self.assertEqual(self._run("push"), 0)
        self.assertEqual(self._run("commit"), 0)

    def test_push_blocks_on_any_new_commit(self) -> None:
        self._commit()
        self.assertEqual(self._run("push"), 1)

    def test_commit_blocks_at_budget(self) -> None:
        self._commit()  # 1 since scan + the in-flight commit = 2 pending
        self.assertEqual(self._run("commit", budget=3), 0)
        self._commit()  # 3 pending
        self.assertEqual(self._run("commit", budget=3), 1)

    def test_rewritten_history_blocks(self) -> None:
        self.state.write_text('{"last_scan_commit": "' + "0" * 40 + '"}', encoding="utf-8")
        self.assertEqual(self._run("commit"), 1)


class CleanShaTests(unittest.TestCase):
    def test_accepts_and_normalises_full_shas(self) -> None:
        self.assertEqual(mod.clean_sha("AB" * 20), "ab" * 20)
        self.assertEqual(mod.clean_sha(" " + "c" * 64 + "\n"), "c" * 64)

    def test_rejects_everything_else(self) -> None:
        for value in ("--help", "-n", "ab" * 19, "g" * 40, "a" * 41, None, 123):
            with self.subTest(value=value):
                self.assertIsNone(mod.clean_sha(value))

    def test_hostile_pushed_ref_falls_back_to_head(self) -> None:
        with (
            mock.patch.dict(os.environ, {"PRE_COMMIT_TO_REF": "--output=/tmp/x"}),
            mock.patch.object(mod, "_git", return_value="d" * 40),
        ):
            self.assertEqual(mod.checked_sha("push"), "d" * 40)
