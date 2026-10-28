#!/usr/bin/env python3
"""Download every page of a IIIF book as full-size JPEGs into a folder under tmp/.

Used to build test corpora for the reader. Supports IIIF Presentation v2 and v3 manifests
and Image API v2 and v3 services. Nothing it writes is meant to be committed.

Safety limits, because the manifest is untrusted input:
- Only https URLs on the public IIIF hosts in ALLOWED_HOSTS are fetched, including
  redirects. Add a host here, in code review, to use another library.
- Output must stay under the repository's tmp/ folder.
- Manifest size, canvas count, page size, and per-download time are capped.

Resume is safe: .fetch-state.json records the manifest URL and each page's source URL,
so a page is reused only when it came from the same URL. A folder belongs to one
manifest; a second manifest into the same folder is refused.

Example:
    python3 scripts/fetch-iiif-corpus.py \
        https://iiif.archive.org/iiif/3/edmunddulacspict00dulauoft/manifest.json \
        tmp/fixtures/corpus/dulac-picture-book
"""

from __future__ import annotations

import argparse
import http.client
import json
import os
import socket
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request
from collections.abc import Iterator
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

USER_AGENT = "scriptorium-corpus-fetch/0.2 (test fixtures)"
REPO_ROOT = Path(__file__).resolve().parent.parent
OUTPUT_ROOT = REPO_ROOT / "tmp"
ALLOWED_HOSTS = frozenset(
    {
        "archive.org",
        "iiif.archive.org",
        "www.e-codices.unifr.ch",
        "tile.loc.gov",
        "www.loc.gov",
        "gallica.bnf.fr",
        "www.biodiversitylibrary.org",
        "iiif.biodiversitylibrary.org",
        "iiif.bodleian.ox.ac.uk",
    }
)
JPEG_MAGIC = b"\xff\xd8\xff"
JPEG_EOI = b"\xff\xd9"
JPEG_FORMATS = {"image/jpeg", "image/jpg"}
TAIL_WINDOW = 1024  # JPEGs may carry trailing padding after the end-of-image marker
MAX_MANIFEST_BYTES = 50 * 2**20
MAX_PAGE_BYTES = 200 * 2**20
MAX_CANVASES = 10_000
DOWNLOAD_DEADLINE_S = 600
SOCKET_TIMEOUT_S = 120
CHUNK = 2**16
STATE_FILE = ".fetch-state.json"
_STATE_LOCK = threading.Lock()  # guards the shared page record across worker threads
LOCK_FILE = ".fetch.lock"


class FetchError(Exception):
    """A download that should be reported, and retried where that makes sense."""


class RefusedURL(FetchError):  # noqa: N818 - reads better at the raise sites
    """A URL the allowlist rejects. Permanent, so it is never retried."""


def check_url(url: str) -> str:
    """Allow only https URLs on an allowlisted public IIIF host.

    The returned URL is rebuilt with the host taken from ALLOWED_HOSTS itself, so no
    caller-supplied host or credentials reach the request.
    """
    parts = urllib.parse.urlparse(url)
    if parts.scheme != "https":
        raise RefusedURL(f"refusing non-https URL: {url!r}")
    requested = (parts.hostname or "").lower()
    host = next((h for h in ALLOWED_HOSTS if h == requested), None)
    try:
        port = parts.port
    except ValueError as exc:
        raise RefusedURL(f"invalid port in URL: {url!r}") from exc
    if host is None:
        raise RefusedURL(f"refusing host not in ALLOWED_HOSTS: {parts.hostname!r}")
    if port not in (None, 443):
        raise RefusedURL(f"refusing port {port} for {host!r}")
    return urllib.parse.urlunparse(("https", host, parts.path, parts.params, parts.query, ""))


def resolve_output_dir(path: Path) -> Path:
    """Resolve the output folder and require it to stay under the repository tmp/ folder."""
    root = OUTPUT_ROOT.resolve()
    candidate = path if path.is_absolute() else Path.cwd() / path
    resolved = candidate.resolve()
    if resolved == root or not resolved.is_relative_to(root):
        raise ValueError(f"{path}: output must be a folder under {root}")
    return resolved


class _AllowlistedRedirects(urllib.request.HTTPRedirectHandler):
    """Check a redirect target before following it."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: PLR0913 - stdlib signature
        return super().redirect_request(req, fp, code, msg, headers, check_url(newurl))


_TLS = threading.local()  # connections opened by the current stream() call, per thread


class _TrackedHTTPSHandler(urllib.request.HTTPSHandler):
    """Record each HTTPS connection so a deadline watchdog can close it."""

    def do_open(self, http_class, req, **http_conn_args):
        def tracked(host, **kwargs):
            conn = http_class(host, **kwargs)
            conns = getattr(_TLS, "conns", None)
            if conns is not None:
                conns.append(conn)
            return conn

        return super().do_open(tracked, req, **http_conn_args)


_OPENER = urllib.request.build_opener(_AllowlistedRedirects, _TrackedHTTPSHandler)


def _expire(conns: list, expired: threading.Event) -> None:
    """Watchdog: unblock any read, including the wait for response headers."""
    expired.set()
    for conn in conns:
        sock = getattr(conn, "sock", None)
        if sock is not None:
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass


def _read_body(
    resp, max_bytes: int, expired: threading.Event, deadline_s: float
) -> Iterator[bytes]:
    """Yield the body, enforcing the byte cap, the deadline, and the declared length."""
    # resp.length is http.client's parsed Content-Length; it is None for chunked
    # responses, which ignore any stale Content-Length header.
    expected = getattr(resp, "length", None)
    if not isinstance(expected, int):
        expected = None
    if expected is not None and expected > max_bytes:
        raise FetchError(f"response too large: {expected} bytes")
    received = 0
    # read1 returns after at most one socket read, so the byte cap and the deadline are
    # checked between reads.
    while chunk := resp.read1(CHUNK):
        received += len(chunk)
        if received > max_bytes:
            raise FetchError(f"response exceeded {max_bytes} bytes")
        if expired.is_set():
            raise FetchError(f"download exceeded {deadline_s} s")
        yield chunk
    if expired.is_set():
        raise FetchError(f"download exceeded {deadline_s} s")
    if expected is not None and received < expected:
        raise FetchError(f"response ended early: {received} of {expected} bytes")


def stream(url: str, max_bytes: int, deadline_s: float = DOWNLOAD_DEADLINE_S) -> Iterator[bytes]:
    """Yield response chunks, enforcing a byte cap and a total deadline.

    The deadline covers waiting for headers and the body: a watchdog shuts the socket
    down when it expires. Connecting, and each single read, are bounded by SOCKET_TIMEOUT_S.
    """
    req = urllib.request.Request(check_url(url), headers={"User-Agent": USER_AGENT})
    conns: list = []
    expired = threading.Event()
    watchdog = threading.Timer(deadline_s, _expire, (conns, expired))
    watchdog.daemon = True
    _TLS.conns = conns
    watchdog.start()
    try:
        with _OPENER.open(req, timeout=SOCKET_TIMEOUT_S) as resp:
            _TLS.conns = None
            yield from _read_body(resp, max_bytes, expired, deadline_s)
    except http.client.HTTPException as exc:  # e.g. IncompleteRead; not an OSError
        if expired.is_set():  # the watchdog's shutdown surfaces as RemoteDisconnected
            raise FetchError(f"download exceeded {deadline_s} s") from exc
        raise FetchError(f"broken response: {exc!r}") from exc
    except OSError as exc:
        if expired.is_set():
            raise FetchError(f"download exceeded {deadline_s} s") from exc
        raise
    finally:
        _TLS.conns = None
        watchdog.cancel()


def fetch_manifest(url: str, attempts: int = 4) -> dict:
    manifest: object = None
    for attempt in range(1, attempts + 1):
        try:
            # Invalid JSON is retried: a body cut short by a clean EOF looks exactly like it.
            manifest = json.loads(b"".join(stream(url, MAX_MANIFEST_BYTES)))
            break
        except RefusedURL:
            raise  # an allowlist refusal (even via redirect) is permanent
        except RecursionError as exc:  # absurdly deep nesting is permanent
            raise FetchError("manifest is nested too deeply to parse") from exc
        except (OSError, FetchError, ValueError) as exc:
            if attempt == attempts:
                raise FetchError(f"manifest failed after {attempts} attempts: {exc}") from exc
            time.sleep(5 * attempt)
    if not isinstance(manifest, dict):  # a shape error is permanent; it is not retried
        raise FetchError("manifest is not a JSON object")
    return manifest


# --- manifest parsing -------------------------------------------------------------------


def _first(value: object) -> object:
    return value[0] if isinstance(value, list) and value else value


IMAGE_SERVICE_TYPES = {"ImageService1", "ImageService2", "ImageService3"}


def _image_service_version(service: dict) -> int | None:
    """1, 2 or 3 for an IIIF Image API service, None for any other kind of service."""
    kind = str(service.get("type") or service.get("@type") or "")
    marks = json.dumps([service.get("@context"), service.get("profile")])
    if kind == "ImageService3" or "iiif.io/api/image/3" in marks:
        return 3
    if kind == "ImageService1" or "iiif.io/api/image/1" in marks or "/iiif/image-api/1." in marks:
        return 1
    if kind in IMAGE_SERVICE_TYPES or "iiif.io/api/image/" in marks:
        return 2
    return None


def _service_url(services: object) -> str | None:
    """Full-size JPEG request for the first Image API service, in that service's version."""
    for service in services if isinstance(services, list) else [services]:
        if not isinstance(service, dict):
            continue
        base = service.get("@id") or service.get("id")
        version = _image_service_version(service)
        if isinstance(base, str) and version:
            size = "max" if version == 3 else "full"
            quality = "native" if version == 1 else "default"  # Image API 1.x quality name
            return f"{base.rstrip('/')}/full/{size}/0/{quality}.jpg"
    return None


def _jpeg_id(body: dict) -> str | None:
    body_id = body.get("id") or body.get("@id")
    if not isinstance(body_id, str):
        return None
    fmt = body.get("format")
    if isinstance(fmt, str):
        return body_id if fmt.lower() in JPEG_FORMATS else None
    path = urllib.parse.urlparse(body_id).path.lower()
    return body_id if path.endswith((".jpg", ".jpeg")) else None


def _image_bodies(body: object) -> Iterator[dict]:
    """Flatten body arrays, Choice items, and SpecificResource sources into plain bodies."""
    if isinstance(body, list):
        for item in body:
            yield from _image_bodies(item)
    elif isinstance(body, dict):
        kind = body.get("type") or body.get("@type")
        if kind in ("Choice", "oa:Choice"):
            yield from _image_bodies(body.get("items") or [body.get("default"), body.get("item")])
        elif kind in ("SpecificResource", "oa:SpecificResource"):
            yield from _image_bodies(body.get("source") or body.get("full"))
        else:
            yield body


def _canvas_bodies(canvas: dict) -> Iterator[object]:
    for image in canvas.get("images") or []:  # Presentation v2
        if isinstance(image, dict):
            yield image.get("resource")
    for page in canvas.get("items") or []:  # Presentation v3
        for anno in (page.get("items") or []) if isinstance(page, dict) else []:
            if isinstance(anno, dict):
                yield anno.get("body")


def _allowed(url: str | None) -> str | None:
    """The allowlisted form of url, or None when it is missing or refused."""
    if not url:
        return None
    try:
        return check_url(url)
    except RefusedURL:
        return None


def _body_url(body: dict) -> str | None:
    """Prefer an allowlisted Image API service, then an allowlisted JPEG body."""
    return _allowed(_service_url(body.get("service"))) or _allowed(_jpeg_id(body))


def canvas_url(canvas: object) -> str | None:
    """First usable full-size JPEG URL for a canvas, or None when it has no image.

    Off-allowlist URLs make a canvas unusable (skipped) rather than a failed page.
    """
    if not isinstance(canvas, dict):
        return None
    for raw in _canvas_bodies(canvas):
        for body in _image_bodies(raw):
            url = _body_url(body)
            if url:
                return url
    return None


def image_urls(manifest: dict, limit: int = 0) -> list[str | None]:
    """One URL per canvas in reading order; None marks a canvas with no usable image.

    `limit` keeps only the first N canvases and applies before the canvas cap, so a huge
    manifest can still be sampled.
    """
    if "sequences" in manifest:
        sequence = _first(manifest["sequences"])
        canvases = sequence.get("canvases") if isinstance(sequence, dict) else None
    else:
        canvases = manifest.get("items")
    if not isinstance(canvases, list):
        raise FetchError("manifest has no canvas list")
    if limit:
        canvases = canvases[:limit]
    if len(canvases) > MAX_CANVASES:
        raise FetchError(f"manifest has {len(canvases)} canvases; limit is {MAX_CANVASES}")
    return [canvas_url(c) for c in canvases]


# --- downloading -------------------------------------------------------------------------


def is_complete_jpeg(data: bytes) -> bool:
    """Start-of-image marker at the head and end-of-image marker near the tail."""
    return data[:3] == JPEG_MAGIC and JPEG_EOI in data[-TAIL_WINDOW:]


def file_is_complete_jpeg(path: Path) -> bool:
    size = path.stat().st_size
    if size < 32:
        return False
    with path.open("rb") as f:
        head = f.read(3)
        f.seek(max(3, size - TAIL_WINDOW))
        tail = f.read(TAIL_WINDOW)
    return is_complete_jpeg(head + tail)


def atomic_write(dest: Path, chunks: Iterator[bytes], check=None) -> None:
    """Write chunks to an exclusively created temp file, check it, then replace dest."""
    fd, name = tempfile.mkstemp(prefix=f".{dest.name}.", suffix=".part", dir=dest.parent)
    tmp = Path(name)
    try:
        with os.fdopen(fd, "wb") as f:
            for chunk in chunks:
                f.write(chunk)
        if check is not None:
            check(tmp)
        os.replace(tmp, dest)  # overwrites on every platform
    finally:
        tmp.unlink(missing_ok=True)


def _require_complete_jpeg(path: Path) -> None:
    if not file_is_complete_jpeg(path):
        raise FetchError(f"response is not a complete JPEG ({path.stat().st_size} bytes)")


def fetch_page(index: int, url: str, out_dir: Path, done: dict[str, str]) -> str:
    dest = out_dir / f"{index:04d}.jpg"
    if done.get(dest.name) == url and dest.exists() and file_is_complete_jpeg(dest):
        return f"skip {dest.name}"
    atomic_write(dest, stream(url, MAX_PAGE_BYTES), _require_complete_jpeg)
    with _STATE_LOCK:
        done[dest.name] = url
    return f"ok   {dest.name} {dest.stat().st_size // 1024} KB"


def safe_fetch(index: int, url: str, out_dir: Path, done: dict[str, str], attempts: int = 4) -> str:
    for attempt in range(1, attempts + 1):
        try:
            return fetch_page(index, url, out_dir, done)
        except RefusedURL as exc:
            return f"FAIL {index:04d}: {exc}"
        except (OSError, FetchError) as exc:  # servers time out on large transcodes; retry
            if attempt == attempts:
                return f"FAIL {index:04d}: {exc}"
            time.sleep(5 * attempt)
    return f"FAIL {index:04d}"


# --- state and locking --------------------------------------------------------------------


def load_state(out_dir: Path, manifest_url: str) -> dict[str, str]:
    path = out_dir / STATE_FILE
    if not path.exists():
        if any(out_dir.glob("*.jpg")):
            raise FetchError(f"{out_dir} has pages but no {STATE_FILE}; use an empty folder")
        return {}
    try:
        state = json.loads(path.read_text())
    except (OSError, ValueError) as exc:
        raise FetchError(f"{path} is unreadable: {exc}") from exc
    if not isinstance(state, dict):
        raise FetchError(f"{path} is not a JSON object")
    if state.get("manifest") != manifest_url:
        raise FetchError(f"{out_dir} belongs to another manifest: {state.get('manifest')}")
    pages = state.get("pages", {})
    if not isinstance(pages, dict) or not all(
        isinstance(k, str) and isinstance(v, str) for k, v in pages.items()
    ):
        raise FetchError(f"{path} has an invalid pages record")
    return dict(pages)


def save_state(out_dir: Path, manifest_url: str, done: dict[str, str]) -> None:
    with _STATE_LOCK:
        payload = json.dumps({"manifest": manifest_url, "pages": done}, indent=1, sort_keys=True)
    atomic_write(out_dir / STATE_FILE, iter([payload.encode()]))


def acquire_lock(out_dir: Path) -> Path:
    lock = out_dir / LOCK_FILE
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        raise FetchError(
            f"{lock} exists: another run is using this folder (delete it if stale)"
        ) from None
    os.write(fd, str(os.getpid()).encode())
    os.close(fd)
    return lock


def non_negative(text: str) -> int:
    value = int(text)
    if value < 0:
        raise argparse.ArgumentTypeError("must be 0 or more")
    return value


def positive(text: str) -> int:
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError("must be 1 or more")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(__doc__ or "").splitlines()[0],
        epilog="Exit codes: 0 all pages, 1 failures or refused input, 2 canvases skipped.",
    )
    parser.add_argument("manifest", help="IIIF manifest URL (https, allowlisted host)")
    parser.add_argument("out_dir", type=Path, help="Destination folder under tmp/")
    parser.add_argument(
        "--limit", type=non_negative, default=0, help="Fetch only the first N canvases"
    )
    parser.add_argument(
        "--workers", type=positive, default=4, help="Parallel downloads (be polite)"
    )
    args = parser.parse_args()

    try:
        manifest_url = check_url(args.manifest)
        out_dir = resolve_output_dir(args.out_dir)
        out_dir.mkdir(parents=True, exist_ok=True)
        lock = acquire_lock(out_dir)
    except (FetchError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    try:
        done = load_state(out_dir, manifest_url)
        manifest = fetch_manifest(manifest_url)
        urls = image_urls(manifest, args.limit)
        save_state(out_dir, manifest_url, done)  # claim the folder before the first page
        atomic_write(out_dir / "manifest.json", iter([json.dumps(manifest, indent=1).encode()]))

        skipped = [i for i, url in enumerate(urls) if not url]
        for i in skipped:
            print(f"none {i:04d}: canvas has no JPEG image or image service; skipped")
        jobs = [(i, url) for i, url in enumerate(urls) if url]
        failures = 0
        try:
            with ThreadPoolExecutor(max_workers=args.workers) as pool:
                results = pool.map(lambda job: safe_fetch(job[0], job[1], out_dir, done), jobs)
                for result in results:
                    print(result, flush=True)
                    failures += result.startswith("FAIL")
                    save_state(out_dir, manifest_url, done)  # an interrupted run can resume
        finally:
            save_state(out_dir, manifest_url, done)  # keep pages finished before an error
    except (FetchError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    finally:
        lock.unlink(missing_ok=True)

    print(f"{len(jobs) - failures}/{len(urls)} pages in {out_dir}, {len(skipped)} skipped")
    if failures:
        return 1
    return 2 if skipped else 0


if __name__ == "__main__":
    sys.exit(main())
