#!/usr/bin/env python3
"""Compare OCR engines on the hand-checked ground-truth pages (M2a Phase 1 spike).

Stdlib-only. Runs each configured engine on the 12 pages whose transcriptions live in
tests/fixtures/ocr/<book>-<NNNN>.txt, and prints one row per engine:

    CER / WER over all pages     total edit distance / total ground-truth chars or words
    plain CER                    CER over the plain-prose pages only
    invented                     non-space characters the engine wrote on textless pages
    s/page                       median wall-clock seconds per page, one thread

Whitespace runs are collapsed to single spaces in both texts before comparing.

Engines:
    tesseract-fast   Tesseract with the tessdata_fast eng.traineddata (--tessdata-dir)
    tesseract-best   Tesseract with the tessdata_best eng.traineddata (--tessdata-dir)
    ocrs             the ocrs CLI with its detection and recognition models

Binaries and data come from flags or environment variables (flag wins):
    --tesseract / TESSERACT_BIN              default: ``tesseract`` on PATH
    --tessdata-fast / TESSDATA_FAST_DIR      directory holding eng.traineddata
    --tessdata-best / TESSDATA_BEST_DIR      directory holding eng.traineddata
    --ocrs / OCRS_BIN                        default: ``ocrs`` on PATH
    --ocrs-detect-model / OCRS_DETECT_MODEL  text-detection model file
    --ocrs-rec-model / OCRS_REC_MODEL        text-recognition model file

An engine whose binary or data is missing is reported as skipped. If the page images are
missing the script says how to rebuild them (tests/fixtures/README.md) and exits 0. If the
default ground-truth set is incomplete or empty, it names the missing files and exits 2.

Usage:
    python3 scripts/ocr_spike.py [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple

REPO = Path(__file__).resolve().parent.parent
DEFAULT_CORPUS = REPO / "tmp" / "fixtures" / "corpus"
DEFAULT_TRUTH = REPO / "tests" / "fixtures" / "ocr"
# Plain prose pages: no display type, drop cap, or plate. The 3% gate uses these.
DEFAULT_PLAIN = ("dulac-picture-book-0109", "princess-and-goblin-0036")
# The committed ground truth. A run on the default directory scores only this complete set.
EXPECTED_PAGES = frozenset(
    {
        "dulac-picture-book-0013",
        "dulac-picture-book-0025",
        "dulac-picture-book-0031",
        "dulac-picture-book-0049",
        "dulac-picture-book-0109",
        "dulac-picture-book-0154",
        "princess-and-goblin-0003",
        "princess-and-goblin-0007",
        "princess-and-goblin-0015",
        "princess-and-goblin-0019",
        "princess-and-goblin-0021",
        "princess-and-goblin-0036",
    }
)
ENGINES = ("tesseract-fast", "tesseract-best", "ocrs")
PAGE_TIMEOUT_S = 300
BATCH_SUFFIXES = (".bat", ".cmd")


@dataclass
class Page:
    """One ground-truth page and the image it transcribes."""

    key: str
    image: Path
    truth: str


@dataclass
class EngineResult:
    """Scores for one engine over all pages."""

    engine: str
    cer: float = 0.0
    wer: float = 0.0
    plain_cer: Optional[float] = None
    invented: int = 0
    median_s: float = 0.0
    failures: int = 0
    pages: List[Dict[str, Any]] = field(default_factory=list)


Recognizer = Callable[[Path], str]


def normalize(text: str) -> str:
    """Collapse whitespace runs to single spaces and trim the ends."""
    return " ".join(text.split())


def edit_distance(a: Sequence[str], b: Sequence[str]) -> int:
    """Levenshtein distance between two sequences (insert, delete, substitute = 1)."""
    if not a:
        return len(b)
    prev = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        cur = [i]
        for j, y in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x != y)))
        prev = cur
    return prev[-1]


def load_pages(truth_dir: Path, corpus: Path) -> Tuple[List[Page], List[Path]]:
    """Read every truth file and pair it with its image; also return missing images."""
    pages: List[Page] = []
    missing: List[Path] = []
    for txt in sorted(truth_dir.glob("*.txt")):
        book, _, num = txt.stem.rpartition("-")
        image = corpus / book / "jpg" / f"{num}.jpg"
        if not image.is_file():
            missing.append(image)
        pages.append(Page(txt.stem, image, txt.read_text(encoding="utf-8")))
    return pages, missing


def run_engine(cmd: List[str], env: Dict[str, str]) -> str:
    """Run one OCR command and return its stdout; raise on a non-zero exit or timeout."""
    # Engine setup already skips batch files; this guards any other caller.
    if is_batch_file(cmd[0]):
        raise RuntimeError(f"refusing to run a batch file as an engine: {cmd[0]}")
    # The argv is a list built from resolved binary paths, and no shell runs it.
    proc = subprocess.run(  # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-audit
        cmd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        timeout=PAGE_TIMEOUT_S,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"exit {proc.returncode}: {proc.stderr.strip()[:200]}")
    return proc.stdout


def single_thread_env() -> Dict[str, str]:
    """Environment that pins Tesseract (OpenMP) and ocrs (rten) to one thread."""
    env = dict(os.environ)
    env["OMP_THREAD_LIMIT"] = "1"
    env["RTEN_NUM_THREADS"] = "1"
    return env


def which_or_none(name: str) -> Optional[str]:
    """Resolve a binary name or path, or return None when it cannot be found."""
    return shutil.which(name)


def is_batch_file(exe: str) -> bool:
    """True for a Windows .bat or .cmd file, which cmd.exe runs and re-parses."""
    return Path(exe).suffix.lower() in BATCH_SUFFIXES


def make_tesseract(
    binary: Optional[str], tessdata: Optional[str]
) -> Tuple[Optional[Recognizer], str]:
    """Build a Tesseract recognizer, or (None, reason) when it cannot run."""
    exe = which_or_none(binary or "tesseract")
    if exe is None:
        return None, "tesseract binary not found (set --tesseract or TESSERACT_BIN)"
    if is_batch_file(exe):
        return None, f"tesseract is a batch file, which is not run: {exe}"
    if not tessdata or not (Path(tessdata) / "eng.traineddata").is_file():
        return None, "eng.traineddata not found in the tessdata directory"
    env = single_thread_env()

    def recognize(image: Path) -> str:
        """Return Tesseract's plain text for one page image."""
        cmd = [exe, str(image), "stdout", "--tessdata-dir", tessdata, "-l", "eng"]
        return run_engine(cmd, env)

    return recognize, ""


def make_ocrs(
    binary: Optional[str], detect: Optional[str], rec: Optional[str]
) -> Tuple[Optional[Recognizer], str]:
    """Build an ocrs recognizer, or (None, reason) when it cannot run."""
    exe = which_or_none(binary or "ocrs")
    if exe is None:
        return None, "ocrs binary not found (set --ocrs or OCRS_BIN)"
    if is_batch_file(exe):
        return None, f"ocrs is a batch file, which is not run: {exe}"
    if not detect or not rec or not Path(detect).is_file() or not Path(rec).is_file():
        return None, "ocrs models not found (set --ocrs-detect-model and --ocrs-rec-model)"
    env = single_thread_env()

    def recognize(image: Path) -> str:
        """Return ocrs's plain text for one page image."""
        return run_engine([exe, "--detect-model", detect, "--rec-model", rec, str(image)], env)

    return recognize, ""


def score_engine(
    name: str, recognize: Recognizer, pages: List[Page], plain: Sequence[str]
) -> EngineResult:
    """Run one engine on every page and compute the summary metrics."""
    res = EngineResult(name)
    totals = {"c_err": 0, "c_len": 0, "w_err": 0, "w_len": 0, "p_err": 0, "p_len": 0}
    times: List[float] = []
    for page in pages:
        start = time.perf_counter()
        error = ""
        try:
            out = recognize(page.image)
        except (RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
            out, error = "", str(exc)
            res.failures += 1
        times.append(time.perf_counter() - start)
        record = score_page(page, out, error, times[-1], page.key in plain, totals)
        res.invented += int(record["invented"])
        res.pages.append(record)
    res.cer = ratio(totals["c_err"], totals["c_len"])
    res.wer = ratio(totals["w_err"], totals["w_len"])
    # No plain-prose characters means there is nothing to score, not a perfect score.
    res.plain_cer = ratio(totals["p_err"], totals["p_len"]) if totals["p_len"] else None
    res.median_s = statistics.median(times) if times else 0.0
    return res


def score_page(
    page: Page,
    output: str,
    error: str,
    seconds: float,
    is_plain: bool,
    totals: Dict[str, int],
) -> Dict[str, Any]:
    """Score one page, add its edits to ``totals``, and return its per-page record."""
    truth, got = normalize(page.truth), normalize(output)
    c_err = edit_distance(truth, got)
    w_err = edit_distance(truth.split(), got.split())
    totals["c_err"] += c_err
    totals["c_len"] += len(truth)
    totals["w_err"] += w_err
    totals["w_len"] += len(truth.split())
    if is_plain:
        totals["p_err"] += c_err
        totals["p_len"] += len(truth)
    invented = len(got.replace(" ", "")) if not truth else 0
    return {
        "page": page.key,
        "truth_chars": len(truth),
        "char_errors": c_err,
        "word_errors": w_err,
        "invented": invented,
        "seconds": round(seconds, 3),
        "error": error,
    }


def ratio(num: int, den: int) -> float:
    """num/den, or 0.0 when the denominator is zero."""
    return num / den if den else 0.0


def format_table(results: List[EngineResult]) -> str:
    """Render the results as a fixed-width text table."""
    head = f"{'engine':<16}{'CER':>8}{'WER':>8}{'plain CER':>11}{'invented':>10}{'s/page':>9}"
    rows = [head, "-" * len(head)]
    for r in results:
        plain = "n/a" if r.plain_cer is None else f"{r.plain_cer:.2%}"
        rows.append(
            f"{r.engine:<16}{r.cer:>8.2%}{r.wer:>8.2%}{plain:>11}"
            f"{r.invented:>10d}{r.median_s:>9.2f}"
        )
        if r.failures:
            rows.append(f"  ({r.failures} page(s) failed; counted as empty output)")
    return "\n".join(rows)


def inventory_problems(pages: List[Page], truth_dir: Path, plain: Sequence[str]) -> List[str]:
    """Describe what is wrong with the loaded pages, or return an empty list.

    The committed set is checked only for the default directory, so a custom --truth still works.
    Plain-page keys must name loaded pages in every case.
    """
    keys = {p.key for p in pages}
    problems: List[str] = []
    if truth_dir.resolve() == DEFAULT_TRUTH.resolve():
        missing = sorted(EXPECTED_PAGES - keys)
        extra = sorted(keys - EXPECTED_PAGES)
        if missing:
            problems.append(f"missing ground-truth files: {', '.join(missing)}")
        if extra:
            problems.append(f"unexpected ground-truth files: {', '.join(extra)}")
    unknown = sorted(set(plain) - keys)
    if unknown:
        problems.append(f"--plain names pages with no ground truth: {', '.join(unknown)}")
    return problems


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    """Parse command-line flags; environment variables supply the defaults."""
    env = os.environ.get
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS, help="corpus root (tmp/...)")
    p.add_argument("--truth", type=Path, default=DEFAULT_TRUTH, help="ground-truth directory")
    p.add_argument("--engines", nargs="+", choices=ENGINES, default=list(ENGINES))
    p.add_argument("--plain", nargs="+", default=list(DEFAULT_PLAIN), help="plain-prose page keys")
    p.add_argument("--tesseract", default=env("TESSERACT_BIN"))
    p.add_argument("--tessdata-fast", default=env("TESSDATA_FAST_DIR"))
    p.add_argument("--tessdata-best", default=env("TESSDATA_BEST_DIR"))
    p.add_argument("--ocrs", default=env("OCRS_BIN"))
    p.add_argument("--ocrs-detect-model", default=env("OCRS_DETECT_MODEL"))
    p.add_argument("--ocrs-rec-model", default=env("OCRS_REC_MODEL"))
    p.add_argument("--json", type=Path, help="also write the full results to this file")
    return p.parse_args(argv)


def build_recognizers(args: argparse.Namespace) -> Dict[str, Tuple[Optional[Recognizer], str]]:
    """Construct each requested engine, keeping the reason when one cannot run."""
    made: Dict[str, Tuple[Optional[Recognizer], str]] = {}
    for name in args.engines:
        if name == "tesseract-fast":
            made[name] = make_tesseract(args.tesseract, args.tessdata_fast)
        elif name == "tesseract-best":
            made[name] = make_tesseract(args.tesseract, args.tessdata_best)
        else:
            made[name] = make_ocrs(args.ocrs, args.ocrs_detect_model, args.ocrs_rec_model)
    return made


def main(argv: Optional[Sequence[str]] = None) -> None:
    """Entry point: score each available engine and print (and optionally save) the table."""
    args = parse_args(argv)
    pages, missing = load_pages(args.truth, args.corpus)
    # An empty custom directory is a notice; the default set must be complete, so it falls
    # through to the inventory check and fails there.
    if not pages and args.truth.resolve() != DEFAULT_TRUTH.resolve():
        print(f"No ground-truth files in {args.truth}.")
        return
    problems = inventory_problems(pages, args.truth, args.plain)
    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        sys.exit(2)
    if missing:
        print(
            f"{len(missing)} page image(s) missing under {args.corpus} "
            "(they are gitignored and never committed).\n"
            "Rebuild them with the recipe in tests/fixtures/README.md "
            '("OCR ground-truth pages"), then rerun this script.'
        )
        return
    results: List[EngineResult] = []
    for name, (recognize, reason) in build_recognizers(args).items():
        if recognize is None:
            print(f"skipped {name}: {reason}", file=sys.stderr)
            continue
        print(f"running {name} on {len(pages)} pages ...", file=sys.stderr)
        results.append(score_engine(name, recognize, pages, args.plain))
    print(format_table(results))
    if args.json:
        args.json.write_text(
            json.dumps([r.__dict__ for r in results], indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    main()
