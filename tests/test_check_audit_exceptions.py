#!/usr/bin/env python3
"""
Unit tests for ci/scripts/check_audit_exceptions.py.

Covers sync between audit.toml ignores and audit-exceptions.toml, expiry
inclusivity, parse errors, and duplicate ids — without network or cargo-audit.

Run:
  python3 -m unittest tests.test_check_audit_exceptions -v
  # or: pytest tests/test_check_audit_exceptions.py -q
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]
ADVISORY = "RUSTSEC-2024-0429"


def _load_module() -> ModuleType:
    """Load the hyphenated CI script as a module for direct calls."""
    path = ROOT / "ci" / "scripts" / "check_audit_exceptions.py"
    spec = importlib.util.spec_from_file_location("check_audit_exceptions", path)
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mod = _load_module()


def _write_pair(
    tmp: Path,
    *,
    ignore_ids: list[str],
    exceptions: list[tuple[str, str, str]],
    use_single_quotes: bool = False,
) -> tuple[Path, Path]:
    """Write audit.toml + audit-exceptions.toml under tmp; return their paths."""
    audit = tmp / "audit.toml"
    exc = tmp / "audit-exceptions.toml"
    if use_single_quotes:
        items = ", ".join(f"'{i}'" for i in ignore_ids)
    else:
        items = ", ".join(f'"{i}"' for i in ignore_ids)
    audit.write_text(f"[advisories]\nignore = [{items}]\n", encoding="utf-8")
    blocks = []
    for adv_id, expires, reason in exceptions:
        blocks.append(
            f'[[exceptions]]\nid = "{adv_id}"\nexpires = "{expires}"\nreason = "{reason}"\n'
        )
    exc.write_text("\n".join(blocks) or "exceptions = []\n", encoding="utf-8")
    return audit, exc


def _run(audit: Path, exceptions: Path, today: str, root: Path) -> int:
    """Invoke main() with argv for the given paths, --root, and --today."""
    argv = [
        "check_audit_exceptions.py",
        "--root",
        str(root),
        "--audit",
        str(audit),
        "--exceptions",
        str(exceptions),
        "--today",
        today,
    ]
    old = sys.argv
    try:
        sys.argv = argv
        return mod.main()
    finally:
        sys.argv = old


class CheckAuditExceptionsTests(unittest.TestCase):
    """Exercise the time-bound cargo-audit ignore sync checker."""

    def test_in_sync_pass(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-11-27", tmp), 0)

    def test_no_ignores_needs_no_exception_rows(self) -> None:
        """An empty ignore list matches `exceptions = []`, which is the committed pair."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(tmp, ignore_ids=[], exceptions=[])
            self.assertEqual(_run(audit, exc, "2026-11-27", tmp), 0)

    def test_single_quoted_ignore_list(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
                use_single_quotes=True,
            )
            self.assertEqual(mod.parse_ignore_ids(audit), [ADVISORY])
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 0)

    def test_exceptions_file_missing_table(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(tmp, ignore_ids=[ADVISORY], exceptions=[])
            exc.write_text("# no exceptions table\n", encoding="utf-8")
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 2)

    def test_missing_exception_row_for_ignore(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY, "RUSTSEC-2099-0001"],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_ignore_without_any_exception_rows(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(tmp, ignore_ids=[ADVISORY], exceptions=[])
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_orphan_exception(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_expired(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-11-28", tmp), 1)

    def test_today_rejects_compact_iso(self) -> None:
        """Python 3.11+ fromisoformat accepts YYYYMMDD; we must not."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "20261127", tmp), 2)

    def test_today_rejects_week_date(self) -> None:
        """Python 3.11+ fromisoformat accepts YYYY-Www-D; we must not."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-W48-5", tmp), 2)

    def test_bad_expires_date(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "not-a-date", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_expires_rejects_compact_iso(self) -> None:
        """expires must be YYYY-MM-DD, same contract as --today."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "20261127", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_expires_rejects_week_date(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-W48-5", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_duplicate_exception_id(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit, exc = _write_pair(
                tmp,
                ignore_ids=[ADVISORY],
                exceptions=[
                    (ADVISORY, "2026-11-27", "first"),
                    (ADVISORY, "2026-12-01", "second"),
                ],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 1)

    def test_malformed_ignore_not_a_list(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            audit = tmp / "audit.toml"
            exc = tmp / "exc.toml"
            audit.write_text('[advisories]\nignore = "RUSTSEC-2024-0429"\n', encoding="utf-8")
            exc.write_text(
                f'[[exceptions]]\nid = "{ADVISORY}"\nexpires = "2026-11-27"\nreason = "x"\n',
                encoding="utf-8",
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", tmp), 2)

    def test_path_escape_rejected(self) -> None:
        """CLI paths outside --root must fail closed (Sonar S8707)."""
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            outside = tmp / "outside"
            inside = tmp / "inside"
            outside.mkdir()
            inside.mkdir()
            audit, exc = _write_pair(
                outside,
                ignore_ids=[ADVISORY],
                exceptions=[(ADVISORY, "2026-11-27", "gtk glib")],
            )
            self.assertEqual(_run(audit, exc, "2026-08-27", inside), 2)

    def test_committed_repo_files_pass(self) -> None:
        """Guardrail: the real src-tauri/.cargo pair stays in sync for today."""
        audit = ROOT / "src-tauri" / ".cargo" / "audit.toml"
        exc = ROOT / "src-tauri" / ".cargo" / "audit-exceptions.toml"
        self.assertTrue(audit.is_file())
        self.assertTrue(exc.is_file())
        self.assertEqual(_run(audit, exc, "2026-08-27", ROOT), 0)


if __name__ == "__main__":
    unittest.main()
