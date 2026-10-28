#!/usr/bin/env python3
"""Download every page of a IIIF book as full-size JPEGs (https only) into a local folder.

Used to build test corpora for the reader under the gitignored ``tmp/`` tree. Supports
IIIF Presentation v2 and v3 manifests. Nothing it writes is meant to be committed.

Example:
    python3 scripts/fetch-iiif-corpus.py \
        https://iiif.archive.org/iiif/3/edmunddulacspict00dulauoft/manifest.json \
        tmp/fixtures/corpus/dulac-picture-book
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

USER_AGENT = "scriptorium-corpus-fetch/0.1 (test fixtures)"
JPEG_MAGIC = b"\xff\xd8\xff"
JPEG_EOI = b"\xff\xd9"
JPEG_FORMATS = {"image/jpeg", "image/jpg"}
TAIL_WINDOW = 1024  # JPEGs may carry trailing padding after the end-of-image marker


def require_https(url: str) -> str:
    if urllib.parse.urlparse(url).scheme != "https":
        raise ValueError(f"refusing non-https URL: {url!r}")
    return url


class _HttpsOnlyRedirects(urllib.request.HTTPRedirectHandler):
    """Refuse a redirect to a non-https URL before following it."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: PLR0913 - stdlib signature
        require_https(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


_OPENER = urllib.request.build_opener(_HttpsOnlyRedirects)


def get(url: str, timeout: int = 120) -> bytes:
    req = urllib.request.Request(require_https(url), headers={"User-Agent": USER_AGENT})
    with _OPENER.open(req, timeout=timeout) as resp:
        return resp.read()


def _service_id(service: object) -> str | None:
    if isinstance(service, list):
        service = service[0] if service else None
    if isinstance(service, dict):
        return service.get("@id") or service.get("id")
    return None


def _canvas_url(canvas: dict, v3: bool) -> str | None:
    """Full-size JPEG URL for one canvas, or None when the canvas has no usable image."""
    try:
        body = canvas["items"][0]["items"][0]["body"] if v3 else canvas["images"][0]["resource"]
    except (KeyError, IndexError, TypeError):
        return None
    if not isinstance(body, dict):
        return None
    body_id = body.get("id" if v3 else "@id")
    fmt = body.get("format")
    looks_jpeg = (isinstance(fmt, str) and fmt.lower() in JPEG_FORMATS) or (
        fmt is None
        and isinstance(body_id, str)
        and urllib.parse.urlparse(body_id).path.lower().endswith((".jpg", ".jpeg"))
    )
    fallback = body_id if looks_jpeg else None
    base = _service_id(body.get("service"))
    if base:
        return f"{base}/full/{'max' if v3 else 'full'}/0/default.jpg"
    return fallback


def image_urls(manifest: dict) -> list[str | None]:
    """One full-size JPEG URL per canvas in reading order; None marks a canvas with no image."""
    if "sequences" in manifest:  # Presentation v2
        return [_canvas_url(c, v3=False) for c in manifest["sequences"][0]["canvases"]]
    return [_canvas_url(c, v3=True) for c in manifest["items"]]  # Presentation v3


def is_complete_jpeg(data: bytes) -> bool:
    """Start-of-image marker at the head and end-of-image marker near the tail."""
    return data[:3] == JPEG_MAGIC and JPEG_EOI in data[-TAIL_WINDOW:]


def _existing_is_complete(path: Path) -> bool:
    size = path.stat().st_size
    if size < 32:
        return False
    with path.open("rb") as f:
        head = f.read(3)
        f.seek(max(3, size - TAIL_WINDOW))
        tail = f.read(TAIL_WINDOW)
    return is_complete_jpeg(head + tail)


def fetch_page(job: tuple[int, str, Path]) -> str:
    index, url, out_dir = job
    dest = out_dir / f"{index:04d}.jpg"
    if dest.exists() and _existing_is_complete(dest):
        return f"skip {dest.name}"
    data = get(url)
    if not is_complete_jpeg(data):
        raise ValueError(f"response is not a complete JPEG ({len(data)} bytes)")
    tmp = dest.with_suffix(".part")
    tmp.write_bytes(data)
    tmp.rename(dest)
    return f"ok   {dest.name} {len(data) // 1024} KB"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("manifest", help="IIIF manifest URL (https)")
    parser.add_argument("out_dir", type=Path, help="Destination folder, under tmp/")
    parser.add_argument("--limit", type=int, default=0, help="Fetch only the first N pages")
    parser.epilog = "Exit codes: 0 all pages, 1 download failures, 2 canvases skipped."
    parser.add_argument("--workers", type=int, default=4, help="Parallel downloads (be polite)")
    args = parser.parse_args()

    if urllib.parse.urlparse(args.manifest).scheme != "https":
        parser.error("manifest must be an https URL")

    manifest = json.loads(get(args.manifest))
    urls = image_urls(manifest)
    if args.limit:
        urls = urls[: args.limit]
    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / "manifest.json").write_text(json.dumps(manifest, indent=1))

    # Index by canvas position (0-based) so file names stay aligned with the manifest.
    jobs = [(i, url, args.out_dir) for i, url in enumerate(urls) if url]
    skipped = [i for i, url in enumerate(urls) if not url]
    for i in skipped:
        print(f"none {i:04d}: canvas has no JPEG image or image service; skipped")
    failures = 0
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        for job, result in zip(jobs, pool.map(_safe_fetch, jobs)):
            print(result, flush=True)
            failures += result.startswith("FAIL")
    print(f"{len(jobs) - failures}/{len(urls)} pages in {args.out_dir}, {len(skipped)} skipped")
    if failures:
        return 1
    return 2 if skipped else 0


def _safe_fetch(job: tuple[int, str, Path], attempts: int = 4) -> str:
    for attempt in range(1, attempts + 1):
        try:
            return fetch_page(job)
        except Exception as exc:  # image servers time out on large transcodes; retry
            if attempt == attempts:
                return f"FAIL {job[0]:04d}: {exc}"
            time.sleep(5 * attempt)
    return f"FAIL {job[0]:04d}"


if __name__ == "__main__":
    sys.exit(main())
