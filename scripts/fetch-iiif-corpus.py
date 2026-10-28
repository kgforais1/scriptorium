#!/usr/bin/env python3
"""Download every page of a IIIF book as full-size JPEGs into a local folder.

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
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

USER_AGENT = "scriptorium-corpus-fetch/0.1 (test fixtures)"


def get(url: str, timeout: int = 120) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310 - https manifests only
        return resp.read()


def image_urls(manifest: dict) -> list[str]:
    """Return one full-size JPEG URL per canvas, in reading order."""
    urls: list[str] = []
    if "sequences" in manifest:  # Presentation v2
        for canvas in manifest["sequences"][0]["canvases"]:
            res = canvas["images"][0]["resource"]
            service = res.get("service", {})
            base = service.get("@id") if isinstance(service, dict) else None
            urls.append(f"{base}/full/full/0/default.jpg" if base else res["@id"])
    else:  # Presentation v3
        for canvas in manifest["items"]:
            urls.append(canvas["items"][0]["items"][0]["body"]["id"])
    return urls


def fetch_page(job: tuple[int, str, Path]) -> str:
    index, url, out_dir = job
    dest = out_dir / f"{index:04d}.jpg"
    if dest.exists() and dest.stat().st_size > 0:
        return f"skip {dest.name}"
    data = get(url)
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

    if not args.manifest.startswith("https://"):
        parser.error("manifest must be an https URL")

    manifest = json.loads(get(args.manifest))
    urls = image_urls(manifest)
    if args.limit:
        urls = urls[: args.limit]
    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / "manifest.json").write_text(json.dumps(manifest, indent=1))

    jobs = [(i + 1, url, args.out_dir) for i, url in enumerate(urls)]
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
