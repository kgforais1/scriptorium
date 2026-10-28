#!/usr/bin/env python3
"""Tests for scripts/fetch-iiif-corpus.py. No network: only pure helpers and local state.

Run:
  python3 -m unittest tests.test_fetch_iiif_corpus -v
"""

from __future__ import annotations

import importlib.util

import tempfile
import unittest
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[1]


def _load() -> ModuleType:
    path = ROOT / "scripts" / "fetch-iiif-corpus.py"
    spec = importlib.util.spec_from_file_location("fetch_iiif_corpus", path)
    assert spec is not None
    assert spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


fetch = _load()
JPEG = b"\xff\xd8\xff" + b"0" * 64 + b"\xff\xd9"


def v3_canvas(body: object) -> dict:
    return {"items": [{"items": [{"body": body}]}]}


class CheckUrlTests(unittest.TestCase):
    def test_allows_allowlisted_https(self) -> None:
        url = "https://iiif.archive.org/iiif/3/x/manifest.json?a=1"
        self.assertEqual(fetch.check_url(url), url)

    def test_rebuilt_url_drops_credentials_and_fragment(self) -> None:
        url = "https://user:pw@IIIF.archive.org:443/p#frag"
        self.assertEqual(fetch.check_url(url), "https://iiif.archive.org/p")

    def test_rejects_other_schemes_and_hosts(self) -> None:
        for url in (
            "http://iiif.archive.org/x",
            "file:///etc/passwd",
            "https://127.0.0.1/x",
            "https://169.254.169.254/latest",
            "https://example.com/x",
            "https://iiif.archive.org.evil.test/x",
            "https://iiif.archive.org:8443/x",
            "https://iiif.archive.org:abc/x",
        ):
            with self.subTest(url=url), self.assertRaises(fetch.FetchError):
                fetch.check_url(url)


class OutputDirTests(unittest.TestCase):
    def test_accepts_folder_under_tmp(self) -> None:
        resolved = fetch.resolve_output_dir(ROOT / "tmp" / "fixtures" / "corpus" / "x")
        self.assertTrue(resolved.is_relative_to(ROOT / "tmp"))

    def test_rejects_escape_and_tmp_itself(self) -> None:
        for path in (ROOT / "tmp", ROOT / "tmp" / ".." / "src", Path("/etc")):
            with self.subTest(path=path), self.assertRaises(ValueError):
                fetch.resolve_output_dir(path)


class ManifestParsingTests(unittest.TestCase):
    def test_v3_service_uses_max(self) -> None:
        body = {
            "id": "https://h/a.jp2",
            "format": "image/jp2",
            "service": [{"id": "https://h/s", "type": "ImageService3"}],
        }
        self.assertEqual(fetch.canvas_url(v3_canvas(body)), "https://h/s/full/max/0/default.jpg")

    def test_service_version_wins_over_presentation_version(self) -> None:
        v2_canvas = {
            "images": [
                {
                    "resource": {
                        "@id": "https://h/a",
                        "service": {
                            "@id": "https://h/s",
                            "@context": "http://iiif.io/api/image/3/context.json",
                        },
                    }
                }
            ]
        }
        self.assertEqual(fetch.canvas_url(v2_canvas), "https://h/s/full/max/0/default.jpg")
        v2_service = {
            "images": [
                {
                    "resource": {
                        "@id": "https://h/a",
                        "service": {
                            "@id": "https://h/s/",
                            "profile": "http://iiif.io/api/image/2/level1.json",
                        },
                    }
                }
            ]
        }
        self.assertEqual(fetch.canvas_url(v2_service), "https://h/s/full/full/0/default.jpg")

    def test_non_image_services_are_skipped(self) -> None:
        auth = {"id": "https://h/auth", "type": "AuthCookieService1"}
        image = {"id": "https://h/img", "type": "ImageService2"}
        body = {"id": "https://h/p.jpg", "service": [auth, image]}
        self.assertEqual(fetch.canvas_url(v3_canvas(body)), "https://h/img/full/full/0/default.jpg")
        only_auth = {"id": "https://h/p.jpg", "service": [auth]}
        self.assertEqual(fetch.canvas_url(v3_canvas(only_auth)), "https://h/p.jpg")
        by_profile = {"@id": "https://h/s", "profile": "http://iiif.io/api/image/2/level2.json"}
        self.assertEqual(
            fetch.canvas_url(
                {"images": [{"resource": {"@id": "https://h/a", "service": by_profile}}]}
            ),
            "https://h/s/full/full/0/default.jpg",
        )

    def test_jpeg_body_without_format_or_with_query(self) -> None:
        self.assertEqual(
            fetch.canvas_url(v3_canvas({"id": "https://h/p.JPG?t=1"})), "https://h/p.JPG?t=1"
        )
        self.assertEqual(
            fetch.canvas_url(v3_canvas({"id": "https://h/p", "format": "image/JPEG"})),
            "https://h/p",
        )

    def test_choice_specificresource_and_later_annotation(self) -> None:
        choice = {
            "type": "Choice",
            "items": [{"id": "https://h/a.tif", "format": "image/tiff"}, {"id": "https://h/b.jpg"}],
        }
        self.assertEqual(fetch.canvas_url(v3_canvas(choice)), "https://h/b.jpg")
        specific = {"type": "SpecificResource", "source": {"id": "https://h/c.jpg"}}
        self.assertEqual(fetch.canvas_url(v3_canvas(specific)), "https://h/c.jpg")
        two = {
            "items": [{"items": [{"body": "not-an-object"}, {"body": {"id": "https://h/d.jpg"}}]}]
        }
        self.assertEqual(fetch.canvas_url(two), "https://h/d.jpg")

    def test_unusable_canvases_are_none(self) -> None:
        manifest = {
            "items": [
                {},
                v3_canvas("https://h/str"),
                v3_canvas({"id": "https://h/x.jp2", "format": "image/jp2"}),
            ]
        }
        self.assertEqual(fetch.image_urls(manifest), [None, None, None])

    def test_canvas_cap_applies_after_limit(self) -> None:
        huge = {"items": [{}] * (fetch.MAX_CANVASES + 1)}
        with self.assertRaises(fetch.FetchError):
            fetch.image_urls(huge)
        self.assertEqual(fetch.image_urls(huge, limit=2), [None, None])

    def test_malformed_canvas_lists_are_refused(self) -> None:
        for manifest in ({"items": None}, {"sequences": [{"canvases": None}]}, {"sequences": "x"}):
            with self.subTest(manifest=manifest), self.assertRaises(fetch.FetchError):
                fetch.image_urls(manifest)


class StateTests(unittest.TestCase):
    def test_complete_jpeg(self) -> None:
        self.assertTrue(fetch.is_complete_jpeg(JPEG + b"\0" * 100))
        self.assertFalse(fetch.is_complete_jpeg(JPEG[:-2]))
        self.assertFalse(fetch.is_complete_jpeg(b"<html>" + JPEG))

    def test_folder_belongs_to_one_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            self.assertEqual(fetch.load_state(out, "https://h/m1"), {})
            fetch.save_state(out, "https://h/m1", {"0000.jpg": "https://h/p0"})
            self.assertEqual(fetch.load_state(out, "https://h/m1"), {"0000.jpg": "https://h/p0"})
            with self.assertRaises(fetch.FetchError):
                fetch.load_state(out, "https://h/m2")

    def test_malformed_state_is_refused(self) -> None:
        for content in ("[]", '{"manifest": "https://h/m1", "pages": []}', "not json"):
            with self.subTest(content=content), tempfile.TemporaryDirectory() as tmp:
                (Path(tmp) / fetch.STATE_FILE).write_text(content)
                with self.assertRaises(fetch.FetchError):
                    fetch.load_state(Path(tmp), "https://h/m1")

    def test_atomic_write_cleans_up_on_failed_check(self) -> None:
        def reject(_path: Path) -> None:
            raise fetch.FetchError("bad")

        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "0000.jpg"
            with self.assertRaises(fetch.FetchError):
                fetch.atomic_write(dest, iter([b"x"]), reject)
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_pages_without_state_are_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "0000.jpg").write_bytes(JPEG)
            with self.assertRaises(fetch.FetchError):
                fetch.load_state(Path(tmp), "https://h/m1")

    def test_lock_is_exclusive(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            lock = fetch.acquire_lock(Path(tmp))
            with self.assertRaises(fetch.FetchError):
                fetch.acquire_lock(Path(tmp))
            lock.unlink()


if __name__ == "__main__":
    unittest.main()
