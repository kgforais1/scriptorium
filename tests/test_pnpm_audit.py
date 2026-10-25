#!/usr/bin/env python3
"""Contract tests for ci/scripts/pnpm-audit.sh.

The script classifies `pnpm audit --prod` output so a registry outage retries
and a vulnerability report never does. These tests stub `corepack` and `pnpm`
on PATH and never contact the npm registry.

Inputs: PNPM_AUDIT_ATTEMPTS, PNPM_AUDIT_DELAY, and the stub scripts' stdout,
stderr, and exit codes.
Outputs: the wrapper's exit status, GitHub annotations on stderr, and how many
times each stub was invoked.

Run:
  python3 -m unittest tests.test_pnpm_audit -v
  # or: pytest tests/test_pnpm_audit.py -q
"""

from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "ci" / "scripts" / "pnpm-audit.sh"

# Local shim only. A failing enable must not be retried.
COREPACK_ENABLE_FAILS = """#!/usr/bin/env bash
printf 'enable\\n' >> "$CALLS"
exit 1
"""

# prepare fails the first N times with the corepack 503 text, then succeeds.
COREPACK_PREPARE_FLAKY = """#!/usr/bin/env bash
printf '%s\\n' "$*" >> "$CALLS"
if [[ "$1" == "enable" ]]; then
  exit 0
fi
n=0
if [[ -f "$PREPARE_COUNT" ]]; then
  n=$(cat "$PREPARE_COUNT")
fi
printf '%s\\n' "$((n + 1))" > "$PREPARE_COUNT"
if ((n < PREPARE_FAILS)); then
  printf 'Server answered with HTTP 503\\n' >&2
  exit 1
fi
exit 0
"""


def _write_executable(path: Path, body: str) -> None:
    """Install a stub the audit script can exec from PATH."""
    path.write_text(body, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def _pnpm_script(body_after_log: str) -> str:
    """Wrap a pnpm stub that records every invocation before its behavior."""
    return f'#!/usr/bin/env bash\nprintf \'pnpm %s\\n\' "$*" >> "$CALLS"\n{body_after_log}\n'


class PnpmAuditContractTests(unittest.TestCase):
    """Retry and classification contract, driven by PATH stubs."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.bindir = self.tmp / "bin"
        self.bindir.mkdir()
        self.calls = self.tmp / "calls"
        self.prepare_count = self.tmp / "prepare-count"
        _write_executable(
            self.bindir / "corepack",
            '#!/usr/bin/env bash\nprintf \'%s\\n\' "$*" >> "$CALLS"\nexit 0\n',
        )

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        """Run the real wrapper with stubs ahead of the inherited PATH."""
        env = os.environ.copy()
        # Stub directory only, plus the system bins the wrapper and stubs call.
        # A developer PATH that already contains pnpm must not satisfy the
        # "pnpm missing" case or turn a stub failure into a live audit.
        env["PATH"] = os.pathsep.join((str(self.bindir), "/usr/bin", "/bin"))
        env["CALLS"] = str(self.calls)
        env["PREPARE_COUNT"] = str(self.prepare_count)
        env["PNPM_AUDIT_ATTEMPTS"] = "3"
        env["PNPM_AUDIT_DELAY"] = "0"
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            ["bash", str(SCRIPT)],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        )

    def _call_lines(self) -> list[str]:
        if not self.calls.exists():
            return []
        return [line for line in self.calls.read_text(encoding="utf-8").splitlines() if line]

    def test_clean_audit_exits_zero_after_one_pnpm_call(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'No known vulnerabilities found\\n'\nexit 0\n"),
        )
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("No known vulnerabilities found", result.stdout)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 1)
        self.assertIn("enable", self._call_lines())
        self.assertTrue(any(line.startswith("prepare ") for line in self._call_lines()))

    def test_findings_are_not_retried(self) -> None:
        # A later attempt would be clean. The wrapper must not take it.
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script(
                "count=$(grep -c '^pnpm ' \"$CALLS\" || true)\n"
                'if [[ "$count" -gt 1 ]]; then\n'
                "  printf 'No known vulnerabilities found\\n'\n"
                "  exit 0\n"
                "fi\n"
                "printf '1 vulnerabilities found\\n' >&2\n"
                "exit 1\n"
            ),
        )
        result = self._run()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Vulnerabilities found", result.stderr)
        self.assertNotIn("Audit did not run", result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 1)

    def test_findings_win_when_the_report_also_mentions_503(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'error (503)\\n1 vulnerabilities found\\n' >&2\nexit 1\n"),
        )
        result = self._run()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Vulnerabilities found", result.stderr)
        self.assertNotIn("retrying", result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 1)

    def test_pnpm11_audit_bad_response_503_is_retried(self) -> None:
        # pnpm 11.17.0: PnpmError AUDIT_BAD_RESPONSE for a non-200 audit HTTP
        # status. The code alone is not transient; "responded with 503" is.
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script(
                "count=$(grep -c '^pnpm ' \"$CALLS\" || true)\n"
                'if [[ "$count" -lt 2 ]]; then\n'
                "  printf '%s\\n' 'ERR_PNPM_AUDIT_BAD_RESPONSE The audit endpoint"
                " (at https://registry.npmjs.org/-/npm/v1/security/advisories/bulk)"
                " responded with 503: unavailable' >&2\n"
                "  exit 1\n"
                "fi\n"
                "printf 'No known vulnerabilities found\\n'\n"
                "exit 0\n"
            ),
        )
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("retrying", result.stderr)
        self.assertNotIn("Vulnerabilities found", result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 2)

    def test_pnpm11_audit_bad_response_invalid_json_is_not_retried(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script(
                "printf '%s\\n' 'ERR_PNPM_AUDIT_BAD_RESPONSE The audit endpoint"
                " returned invalid JSON' >&2\n"
                "exit 1\n"
            ),
        )
        result = self._run()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("pnpm audit failed", result.stderr)
        self.assertNotIn("retrying", result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 1)

    def test_transient_503_recovers_on_a_later_attempt(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script(
                "count=$(grep -c '^pnpm ' \"$CALLS\" || true)\n"
                'if [[ "$count" -lt 2 ]]; then\n'
                "  printf 'error (503)\\n' >&2\n"
                "  exit 1\n"
                "fi\n"
                "printf 'No known vulnerabilities found\\n'\n"
                "exit 0\n"
            ),
        )
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 2)

    def test_exhausted_registry_outage_fails_closed(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'error (503)\\n' >&2\nexit 1\n"),
        )
        result = self._run()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Audit did not run", result.stderr)
        self.assertNotIn("Vulnerabilities found", result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 3)

    def test_err_pnpm_prefix_alone_is_not_retried(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'ERR_PNPM_AUDIT_FAIL\\n' >&2\nexit 1\n"),
        )
        result = self._run()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("pnpm audit failed", result.stderr)
        self.assertNotIn("Audit did not run", result.stderr)
        self.assertNotIn("retrying", result.stderr)
        self.assertEqual(self._call_lines().count("pnpm audit --prod"), 1)

    def test_missing_pnpm_exits_127(self) -> None:
        result = self._run()
        self.assertEqual(result.returncode, 127, result.stderr)
        self.assertIn("pnpm is not on PATH", result.stderr)
        self.assertNotIn("Vulnerabilities found", result.stderr)

    def test_corepack_enable_is_not_retried(self) -> None:
        _write_executable(self.bindir / "corepack", COREPACK_ENABLE_FAILS)
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'should not run\\n'\nexit 0\n"),
        )
        result = self._run()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("corepack enable failed", result.stderr)
        self.assertNotIn("retrying", result.stderr)
        self.assertEqual(self._call_lines(), ["enable"])

    def test_corepack_prepare_503_is_retried(self) -> None:
        _write_executable(self.bindir / "corepack", COREPACK_PREPARE_FLAKY)
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'No known vulnerabilities found\\n'\nexit 0\n"),
        )
        result = self._run({"PREPARE_FAILS": "2"})
        self.assertEqual(result.returncode, 0, result.stderr)
        prepare_calls = [line for line in self._call_lines() if line.startswith("prepare ")]
        self.assertEqual(len(prepare_calls), 3)
        self.assertEqual(self._call_lines().count("enable"), 1)

    def test_non_numeric_attempts_do_not_evaluate(self) -> None:
        sentinel = self.tmp / "pwned"
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'No known vulnerabilities found\\n'\nexit 0\n"),
        )
        result = self._run(
            {"PNPM_AUDIT_ATTEMPTS": f"1+$(touch {sentinel})", "PNPM_AUDIT_DELAY": "0"}
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("non-negative integers", result.stderr)
        self.assertFalse(sentinel.exists())
        self.assertEqual(self._call_lines(), [])

    def test_attempts_above_cap_are_rejected(self) -> None:
        result = self._run({"PNPM_AUDIT_ATTEMPTS": "6"})
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("must be 1-5", result.stderr)
        self.assertEqual(self._call_lines(), [])

    def test_leading_zero_attempts_are_decimal(self) -> None:
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'No known vulnerabilities found\\n'\nexit 0\n"),
        )
        result = self._run({"PNPM_AUDIT_ATTEMPTS": "03", "PNPM_AUDIT_DELAY": "00"})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_non_numeric_delay_does_not_evaluate(self) -> None:
        sentinel = self.tmp / "pwned-delay"
        _write_executable(
            self.bindir / "pnpm",
            _pnpm_script("printf 'No known vulnerabilities found\\n'\nexit 0\n"),
        )
        result = self._run({"PNPM_AUDIT_DELAY": f"0+$(touch {sentinel})"})
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("non-negative integers", result.stderr)
        self.assertFalse(sentinel.exists())
        self.assertEqual(self._call_lines(), [])

    def test_delay_above_cap_is_rejected(self) -> None:
        result = self._run({"PNPM_AUDIT_DELAY": "121"})
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("0-120", result.stderr)
        self.assertEqual(self._call_lines(), [])

    def test_digit_string_that_would_wrap_is_rejected(self) -> None:
        # 2**64 + 3 wraps to 3 in bash arithmetic and would pass a range check.
        wrapped = str((1 << 64) + 3)
        result = self._run({"PNPM_AUDIT_ATTEMPTS": wrapped})
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("at most 3 digits", result.stderr)
        self.assertEqual(self._call_lines(), [])


if __name__ == "__main__":
    unittest.main()
