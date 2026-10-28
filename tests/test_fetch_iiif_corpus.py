#!/usr/bin/env python3
"""Tests for scripts/fetch-iiif-corpus.py. No network: only pure helpers and local state.

Run:
  python3 -m unittest tests.test_fetch_iiif_corpus -v
"""

from __future__ import annotations

import http.client
import importlib.util

import socket
import tempfile
import unittest
from pathlib import Path
from types import ModuleType
from unittest import mock

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
            "id": "https://iiif.archive.org/a.jp2",
            "format": "image/jp2",
            "service": [{"id": "https://iiif.archive.org/s", "type": "ImageService3"}],
        }
        self.assertEqual(
            fetch.canvas_url(v3_canvas(body)), "https://iiif.archive.org/s/full/max/0/default.jpg"
        )

    def test_service_version_wins_over_presentation_version(self) -> None:
        v2_canvas = {
            "images": [
                {
                    "resource": {
                        "@id": "https://iiif.archive.org/a",
                        "service": {
                            "@id": "https://iiif.archive.org/s",
                            "@context": "http://iiif.io/api/image/3/context.json",
                        },
                    }
                }
            ]
        }
        self.assertEqual(
            fetch.canvas_url(v2_canvas), "https://iiif.archive.org/s/full/max/0/default.jpg"
        )
        v2_service = {
            "images": [
                {
                    "resource": {
                        "@id": "https://iiif.archive.org/a",
                        "service": {
                            "@id": "https://iiif.archive.org/s/",
                            "profile": "http://iiif.io/api/image/2/level1.json",
                        },
                    }
                }
            ]
        }
        self.assertEqual(
            fetch.canvas_url(v2_service), "https://iiif.archive.org/s/full/full/0/default.jpg"
        )

    def test_non_image_services_are_skipped(self) -> None:
        auth = {"id": "https://iiif.archive.org/auth", "type": "AuthCookieService1"}
        image = {"id": "https://iiif.archive.org/img", "type": "ImageService2"}
        body = {"id": "https://iiif.archive.org/p.jpg", "service": [auth, image]}
        self.assertEqual(
            fetch.canvas_url(v3_canvas(body)),
            "https://iiif.archive.org/img/full/full/0/default.jpg",
        )
        only_auth = {"id": "https://iiif.archive.org/p.jpg", "service": [auth]}
        self.assertEqual(fetch.canvas_url(v3_canvas(only_auth)), "https://iiif.archive.org/p.jpg")
        by_profile = {
            "@id": "https://iiif.archive.org/s",
            "profile": "http://iiif.io/api/image/2/level2.json",
        }
        self.assertEqual(
            fetch.canvas_url(
                {
                    "images": [
                        {"resource": {"@id": "https://iiif.archive.org/a", "service": by_profile}}
                    ]
                }
            ),
            "https://iiif.archive.org/s/full/full/0/default.jpg",
        )

    def test_jpeg_body_without_format_or_with_query(self) -> None:
        self.assertEqual(
            fetch.canvas_url(v3_canvas({"id": "https://iiif.archive.org/p.JPG?t=1"})),
            "https://iiif.archive.org/p.JPG?t=1",
        )
        self.assertEqual(
            fetch.canvas_url(
                v3_canvas({"id": "https://iiif.archive.org/p", "format": "image/JPEG"})
            ),
            "https://iiif.archive.org/p",
        )

    def test_choice_specificresource_and_later_annotation(self) -> None:
        choice = {
            "type": "Choice",
            "items": [
                {"id": "https://iiif.archive.org/a.tif", "format": "image/tiff"},
                {"id": "https://iiif.archive.org/b.jpg"},
            ],
        }
        self.assertEqual(fetch.canvas_url(v3_canvas(choice)), "https://iiif.archive.org/b.jpg")
        specific = {"type": "SpecificResource", "source": {"id": "https://iiif.archive.org/c.jpg"}}
        self.assertEqual(fetch.canvas_url(v3_canvas(specific)), "https://iiif.archive.org/c.jpg")
        two = {
            "items": [
                {
                    "items": [
                        {"body": "not-an-object"},
                        {"body": {"id": "https://iiif.archive.org/d.jpg"}},
                    ]
                }
            ]
        }
        self.assertEqual(fetch.canvas_url(two), "https://iiif.archive.org/d.jpg")

    def test_unusable_canvases_are_none(self) -> None:
        manifest = {
            "items": [
                {},
                v3_canvas("https://iiif.archive.org/str"),
                v3_canvas({"id": "https://iiif.archive.org/x.jp2", "format": "image/jp2"}),
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


class ServiceSelectionTests(unittest.TestCase):
    def test_image_api_1_uses_native_quality(self) -> None:
        body = {
            "id": "https://iiif.archive.org/a",
            "service": {
                "@id": "https://iiif.archive.org/s",
                "profile": "http://library.stanford.edu/iiif/image-api/1.1/compliance.html#level1",
                "@context": "http://library.stanford.edu/iiif/image-api/1.1/context.json",
            },
        }
        self.assertEqual(
            fetch.canvas_url(v3_canvas(body)), "https://iiif.archive.org/s/full/full/0/native.jpg"
        )

    def test_off_allowlist_jpeg_body_is_unusable(self) -> None:
        self.assertIsNone(fetch.canvas_url(v3_canvas({"id": "https://evil.test/p.jpg"})))

    def test_refused_service_falls_back_to_jpeg_body(self) -> None:
        body = {
            "id": "https://iiif.archive.org/p.jpg",
            "service": {"id": "https://evil.test/s", "type": "ImageService2"},
        }
        self.assertEqual(fetch.canvas_url(v3_canvas(body)), "https://iiif.archive.org/p.jpg")


def fake_response(chunks: list, length: int | None = None) -> mock.MagicMock:
    resp = mock.MagicMock()
    resp.__enter__.return_value = resp
    resp.length = length
    resp.read1.side_effect = chunks
    return resp


class BrokenResponseTests(unittest.TestCase):
    def test_short_body_against_content_length_fails(self) -> None:
        resp = fake_response([b"ab", b""], length=10)
        with mock.patch.object(fetch._OPENER, "open", return_value=resp):
            chunks = fetch.stream("https://iiif.archive.org/p.jpg", 100)
            with self.assertRaises(fetch.FetchError):
                list(chunks)

    def test_full_body_against_content_length_passes(self) -> None:
        resp = fake_response([b"ab", b""], length=2)
        with mock.patch.object(fetch._OPENER, "open", return_value=resp):
            self.assertEqual(list(fetch.stream("https://iiif.archive.org/p.jpg", 100)), [b"ab"])

    def test_deadline_covers_waiting_for_headers(self) -> None:
        server = socket.socket()
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        self.addCleanup(server.close)
        port = server.getsockname()[1]

        def connect_silently(req, timeout):
            conn = http.client.HTTPConnection("127.0.0.1", port, timeout=min(timeout, 2))
            fetch._TLS.conns.append(conn)
            conn.request("GET", "/")
            return conn.getresponse()  # the server never answers

        with mock.patch.object(fetch._OPENER, "open", side_effect=connect_silently):
            chunks = fetch.stream("https://iiif.archive.org/p.jpg", 100, deadline_s=0.3)
            with self.assertRaisesRegex(fetch.FetchError, "exceeded"):
                list(chunks)

    def test_deeply_nested_manifest_is_a_permanent_error(self) -> None:
        deep = b"[" * 100_000 + b"]" * 100_000
        with mock.patch.object(fetch, "stream", return_value=iter([deep])) as stream:
            with self.assertRaises(fetch.FetchError):
                fetch.fetch_manifest("https://iiif.archive.org/m.json")
        stream.assert_called_once()

    def test_truncated_manifest_is_retried(self) -> None:
        replies = [iter([b'{"items": [']), iter([b'{"items": []}'])]
        with (
            mock.patch.object(fetch, "stream", side_effect=replies) as stream,
            mock.patch.object(fetch.time, "sleep"),
        ):
            self.assertEqual(fetch.fetch_manifest("https://iiif.archive.org/m.json"), {"items": []})
        self.assertEqual(stream.call_count, 2)

    def test_incomplete_read_becomes_fetch_error(self) -> None:
        resp = mock.MagicMock()
        resp.__enter__.return_value = resp
        resp.length = None
        resp.read1.side_effect = [b"ab", http.client.IncompleteRead(b"ab", 10)]
        with mock.patch.object(fetch._OPENER, "open", return_value=resp):
            with self.assertRaises(fetch.FetchError):
                list(fetch.stream("https://iiif.archive.org/p.jpg", 100))


class StateTests(unittest.TestCase):
    def test_complete_jpeg(self) -> None:
        self.assertTrue(fetch.is_complete_jpeg(JPEG + b"\0" * 100))
        self.assertFalse(fetch.is_complete_jpeg(JPEG[:-2]))
        self.assertFalse(fetch.is_complete_jpeg(b"<html>" + JPEG))

    def test_folder_belongs_to_one_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            self.assertEqual(fetch.load_state(out, "https://iiif.archive.org/m1"), {})
            fetch.save_state(
                out, "https://iiif.archive.org/m1", {"0000.jpg": "https://iiif.archive.org/p0"}
            )
            self.assertEqual(
                fetch.load_state(out, "https://iiif.archive.org/m1"),
                {"0000.jpg": "https://iiif.archive.org/p0"},
            )
            with self.assertRaises(fetch.FetchError):
                fetch.load_state(out, "https://iiif.archive.org/m2")

    def test_malformed_state_is_refused(self) -> None:
        for content in (
            "[]",
            '{"manifest": "https://iiif.archive.org/m1", "pages": []}',
            "not json",
        ):
            with self.subTest(content=content), tempfile.TemporaryDirectory() as tmp:
                (Path(tmp) / fetch.STATE_FILE).write_text(content)
                with self.assertRaises(fetch.FetchError):
                    fetch.load_state(Path(tmp), "https://iiif.archive.org/m1")

    def test_atomic_write_cleans_up_on_failed_check(self) -> None:
        def reject(_path: Path) -> None:
            raise fetch.FetchError("bad")

        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "0000.jpg"
            chunks = iter([b"x"])
            with self.assertRaises(fetch.FetchError):
                fetch.atomic_write(dest, chunks, reject)
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_pages_without_state_are_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "0000.jpg").write_bytes(JPEG)
            with self.assertRaises(fetch.FetchError):
                fetch.load_state(Path(tmp), "https://iiif.archive.org/m1")

    def test_lock_is_exclusive(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            lock = fetch.acquire_lock(Path(tmp))
            with self.assertRaises(fetch.FetchError):
                fetch.acquire_lock(Path(tmp))
            lock.unlink()


if __name__ == "__main__":
    unittest.main()
