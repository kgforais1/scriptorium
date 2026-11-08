#!/usr/bin/env python3
"""
Unit tests for hostname / SKIP_HOSTS helpers in ci/scripts/check_doc_links.py.

These cover the hardening that replaced substring `host in url` matching:
trailing-dot FQDNs, case folding, and subdomain matches — without network I/O.

Run:
  python3 -m unittest tests.test_check_doc_links -v
  # or: pytest tests/test_check_doc_links.py -q
"""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path
from types import ModuleType
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def _load_module() -> ModuleType:
    """Load the hyphenated CI script as a module for direct calls."""
    path = ROOT / "ci" / "scripts" / "check_doc_links.py"
    spec = importlib.util.spec_from_file_location("check_doc_links", path)
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mod = _load_module()


class HostnameHelperTests(unittest.TestCase):
    """Pure hostname extraction and normalization."""

    def test_hostname_lowercases(self) -> None:
        self.assertEqual(mod._hostname("https://GitHub.COM/org/repo"), "github.com")

    def test_hostname_missing_returns_empty(self) -> None:
        self.assertEqual(mod._hostname("not-a-url"), "")

    def test_normalized_strips_trailing_dot(self) -> None:
        self.assertEqual(
            mod._normalized_hostname("https://twitter.com./page"),
            "twitter.com",
        )

    def test_normalized_preserves_plain_host(self) -> None:
        self.assertEqual(
            mod._normalized_hostname("https://example.com/path"),
            "example.com",
        )


class StripCodeTests(unittest.TestCase):
    """Fenced blocks are blanked so their links are not checked."""

    def test_backtick_fence_is_blanked(self) -> None:
        text = "a\n```\n[x](missing.md)\n```\nb\n"
        self.assertEqual(mod.strip_code(text), "a\n\n\n\nb\n")

    def test_tilde_fence_is_blanked(self) -> None:
        text = "a\n~~~~text\n[x](missing.md)\n~~~~\nb\n"
        self.assertNotIn("missing.md", mod.strip_code(text))
        self.assertEqual(mod.strip_code(text).count("\n"), text.count("\n"))

    def test_closing_fence_may_have_trailing_whitespace(self) -> None:
        for fence in ("```", "~~~"):
            text = f"{fence}\n[x](inside.md)\n{fence} \t\n[y](after.md)\n"
            stripped = mod.strip_code(text)
            self.assertNotIn("inside.md", stripped, msg=fence)
            self.assertIn("after.md", stripped, msg=fence)

    def test_fences_may_be_indented_three_spaces(self) -> None:
        text = "   ~~~\n[x](inside.md)\n   ~~~\n[y](after.md)\n"
        stripped = mod.strip_code(text)
        self.assertNotIn("inside.md", stripped)
        self.assertIn("after.md", stripped)

    def test_a_closer_with_an_info_string_does_not_close(self) -> None:
        text = "```\n```python\n[x](inside.md)\n```\n[y](after.md)\n"
        stripped = mod.strip_code(text)
        self.assertNotIn("inside.md", stripped)
        self.assertIn("after.md", stripped)

    def test_link_outside_fences_survives(self) -> None:
        text = "~~~\ncode\n~~~\n[x](kept.md)\n"
        self.assertIn("kept.md", mod.strip_code(text))


class SkipHostTests(unittest.TestCase):
    """SKIP_HOSTS matching used before any HEAD request."""

    def test_exact_skip_hosts(self) -> None:
        for host in mod.SKIP_HOSTS:
            self.assertTrue(mod._is_skipped_host(host), msg=host)

    def test_subdomain_of_skip_host(self) -> None:
        self.assertTrue(mod._is_skipped_host("mobile.twitter.com"))
        self.assertTrue(mod._is_skipped_host("www.linkedin.com"))

    def test_non_skip_host(self) -> None:
        self.assertFalse(mod._is_skipped_host("github.com"))
        self.assertFalse(mod._is_skipped_host("example.com"))

    def test_check_link_skips_trailing_dot_without_network(self) -> None:
        """Regression: twitter.com. must short-circuit before urlopen."""
        with mock.patch.object(mod.urllib.request, "urlopen") as urlopen:
            result = mod.check_link("https://twitter.com./intent/tweet")
            self.assertIsNone(result)
            urlopen.assert_not_called()

    def test_check_link_skips_x_com_without_network(self) -> None:
        with mock.patch.object(mod.urllib.request, "urlopen") as urlopen:
            result = mod.check_link("https://x.com/someone")
            self.assertIsNone(result)
            urlopen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
