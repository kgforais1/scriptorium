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


def require_https(url: str) -> str:
    if urllib.parse.urlparse(url).scheme != "https":
        raise ValueError(f"refusing non-https URL: {url!r}")
    return url


def get(url: str, timeout: int = 120) -> bytes:
    req = urllib.request.Request(require_https(url), headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310 - https enforced above
        require_https(resp.geturl())  # a redirect must not leave https
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
        if v3:
            body = canvas["items"][0]["items"][0]["body"]
            fallback = body["id"] if body.get("format") == "image/jpeg" else None
        else:
            body = canvas["images"][0]["resource"]
            fallback = body["@id"] if body.get("format") == "image/jpeg" else None
    except (KeyError, IndexError, TypeError):
        return None
    base = _service_id(body.get("service"))
    if base:
        return f"{base}/full/{'max' if v3 else 'full'}/0/default.jpg"
    return fallback


def image_urls(manifest: dict) -> list[str | None]:
    """One full-size JPEG URL per canvas in reading order; None marks a canvas with no image."""
    if "sequences" in manifest:  # Presentation v2
        return [_canvas_url(c, v3=False) for c in manifest["sequences"][0]["canvases"]]
    return [_canvas_url(c, v3=True) for c in manifest["items"]]  # Presentation v3


def is_jpeg(data: bytes) -> bool:
    return data[:3] == JPEG_MAGIC


def fetch_page(job: tuple[int, str, Path]) -> str:
    index, url, out_dir = job
    dest = out_dir / f"{index:04d}.jpg"
    if dest.exists() and is_jpeg(dest.read_bytes()[:3]):
        return f"skip {dest.name}"
    data = get(url)
    if not is_jpeg(data):
        raise ValueError(f"response is not a JPEG ({len(data)} bytes)")
    tmp = dest.with_suffix(".part")
    tmp.write_bytes(data)
    tmp.rename(dest)
    return f"ok   {dest.name} {len(data) // 1024} KB"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("manifest", help="IIIF manifest URL (https)")
    parser.add_argument("out_dir", type=Path, help="Destination folder, under tmp/")
    parser.add_argument("--limit", type=int, default=0, help="Fetch only the first N pages")
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
    for i, url in enumerate(urls):
        if not url:
            print(f"none {i:04d}: canvas has no JPEG image or image service; skipped")
    failures = 0
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        for job, result in zip(jobs, pool.map(_safe_fetch, jobs)):
            print(result, flush=True)
            failures += result.startswith("FAIL")
    print(f"{len(jobs) - failures}/{len(jobs)} pages in {args.out_dir}")
    return 1 if failures else 0


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
