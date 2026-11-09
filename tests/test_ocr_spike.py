#!/usr/bin/env python3
"""
Unit tests for scripts/ocr_spike.py, the M2a Phase 1 engine comparison.

Real engines are never run. Recognizers are plain functions, so scoring, page loading,
the table, and main() are tested with fakes and temporary directories.

Run:
  python3 -m unittest tests.test_ocr_spike -v
  # or: pytest tests/test_ocr_spike.py -q
"""

from __future__ import annotations

import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import ModuleType
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def _load_module() -> ModuleType:
    """Load the script as a module for direct calls."""
    path = ROOT / "scripts" / "ocr_spike.py"
    spec = importlib.util.spec_from_file_location("ocr_spike", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    # dataclasses look the module up in sys.modules while the class is built.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


spike = _load_module()


def _corpus(root: Path, pages: dict[str, str], with_images: bool = True) -> tuple[Path, Path]:
    """Write truth files (and, optionally, empty page images) under ``root``."""
    truth = root / "truth"
    corpus = root / "corpus"
    truth.mkdir()
    for key, text in pages.items():
        (truth / f"{key}.txt").write_text(text, encoding="utf-8")
        if with_images:
            book, _, num = key.rpartition("-")
            image = corpus / book / "jpg" / f"{num}.jpg"
            image.parent.mkdir(parents=True, exist_ok=True)
            image.write_bytes(b"")
    return truth, corpus


class TextMetrics(unittest.TestCase):
    """Normalisation, edit distance, and error ratios."""

    def test_normalize_collapses_whitespace(self) -> None:
        self.assertEqual(spike.normalize("  a\n\nb \t c  "), "a b c")

    def test_edit_distance(self) -> None:
        self.assertEqual(spike.edit_distance("kitten", "sitting"), 3)
        self.assertEqual(spike.edit_distance("", "abc"), 3)
        self.assertEqual(spike.edit_distance("abc", ""), 3)
        self.assertEqual(spike.edit_distance(["a", "b"], ["a", "b"]), 0)

    def test_ratio_handles_zero(self) -> None:
        self.assertEqual(spike.ratio(1, 4), 0.25)
        self.assertEqual(spike.ratio(3, 0), 0.0)


class Scoring(unittest.TestCase):
    """Scoring an engine over a set of pages."""

    def _pages(self) -> list:
        return [
            spike.Page("book-0001", Path("a.jpg"), "the cat sat"),
            spike.Page("book-0002", Path("b.jpg"), ""),
        ]

    def test_perfect_engine(self) -> None:
        def recognize(image: Path) -> str:
            return "the  cat\nsat" if image.name == "a.jpg" else ""

        res = spike.score_engine("fake", recognize, self._pages(), ["book-0001"])
        self.assertEqual(res.cer, 0.0)
        self.assertEqual(res.wer, 0.0)
        self.assertEqual(res.plain_cer, 0.0)
        self.assertEqual(res.invented, 0)
        self.assertEqual(res.failures, 0)

    def test_errors_and_invented_text(self) -> None:
        def recognize(image: Path) -> str:
            return "the bat sat" if image.name == "a.jpg" else "x y"

        res = spike.score_engine("fake", recognize, self._pages(), ["book-0001"])
        # The blank page's invented "x y" counts against the whole-set totals too.
        self.assertAlmostEqual(res.cer, (1 + 3) / 11)
        self.assertAlmostEqual(res.wer, (1 + 2) / 3)
        self.assertAlmostEqual(res.plain_cer, 1 / 11)
        self.assertEqual(res.invented, 2)

    def test_failure_counts_as_empty(self) -> None:
        def recognize(image: Path) -> str:
            raise RuntimeError("exit 1: boom")

        res = spike.score_engine("fake", recognize, self._pages(), [])
        self.assertEqual(res.failures, 2)
        self.assertEqual(res.cer, 1.0)
        self.assertIn("boom", res.pages[0]["error"])
        self.assertIn("2 page(s) failed", spike.format_table([res]))

    def test_no_plain_pages_is_unavailable(self) -> None:
        res = spike.score_engine("fake", lambda image: "", self._pages(), [])
        self.assertIsNone(res.plain_cer)
        self.assertIn("n/a", spike.format_table([res]))

    def test_table_has_one_row_per_engine(self) -> None:
        rows = spike.format_table([spike.EngineResult("one"), spike.EngineResult("two")])
        self.assertEqual(len(rows.splitlines()), 4)


class Loading(unittest.TestCase):
    """Pairing ground-truth files with page images."""

    def test_pairs_truth_with_images(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            truth, corpus = _corpus(Path(tmp), {"my-book-0007": "hi"})
            pages, missing = spike.load_pages(truth, corpus)
        self.assertEqual(missing, [])
        self.assertEqual(pages[0].key, "my-book-0007")
        self.assertEqual(pages[0].image.parts[-3:], ("my-book", "jpg", "0007.jpg"))

    def test_reports_missing_images(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            truth, corpus = _corpus(Path(tmp), {"book-0001": "hi"}, with_images=False)
            _, missing = spike.load_pages(truth, corpus)
        self.assertEqual(len(missing), 1)


class Engines(unittest.TestCase):
    """Building the Tesseract and ocrs recognizers."""

    def test_tesseract_needs_binary_and_data(self) -> None:
        with mock.patch.object(spike, "which_or_none", return_value=None):
            rec, reason = spike.make_tesseract(None, "/nowhere")
        self.assertIsNone(rec)
        self.assertIn("binary", reason)
        with mock.patch.object(spike, "which_or_none", return_value="/bin/tess"):
            rec, reason = spike.make_tesseract(None, "/nowhere")
        self.assertIsNone(rec)
        self.assertIn("traineddata", reason)

    def test_tesseract_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "eng.traineddata").write_bytes(b"")
            with mock.patch.object(spike, "which_or_none", return_value="/bin/tess"):
                rec, _ = spike.make_tesseract(None, tmp)
            assert rec is not None
            with mock.patch.object(spike, "run_engine", return_value="ok") as run:
                self.assertEqual(rec(Path("p.jpg")), "ok")
        cmd, env = run.call_args.args
        self.assertEqual(cmd[:3], ["/bin/tess", "p.jpg", "stdout"])
        self.assertEqual(cmd[cmd.index("--tessdata-dir") + 1], tmp)
        self.assertEqual(env["OMP_THREAD_LIMIT"], "1")

    def test_ocrs_needs_models(self) -> None:
        with mock.patch.object(spike, "which_or_none", return_value="/bin/ocrs"):
            rec, reason = spike.make_ocrs(None, None, None)
        self.assertIsNone(rec)
        self.assertIn("models", reason)

    def test_ocrs_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            det, rec_model = Path(tmp) / "d.onnx", Path(tmp) / "r.onnx"
            det.write_bytes(b"")
            rec_model.write_bytes(b"")
            with mock.patch.object(spike, "which_or_none", return_value="/bin/ocrs"):
                rec, _ = spike.make_ocrs(None, str(det), str(rec_model))
            assert rec is not None
            with mock.patch.object(spike, "run_engine", return_value="ok") as run:
                rec(Path("p.jpg"))
        self.assertEqual(run.call_args.args[0][-1], "p.jpg")
        self.assertEqual(run.call_args.args[1]["RTEN_NUM_THREADS"], "1")

    def test_run_engine_raises_on_failure(self) -> None:
        with self.assertRaises(RuntimeError):
            spike.run_engine([sys.executable, "-c", "import sys; sys.exit(3)"], dict(os.environ))
        out = spike.run_engine([sys.executable, "-c", "print('hi')"], dict(os.environ))
        self.assertEqual(out.strip(), "hi")


class Inventory(unittest.TestCase):
    """Checking the committed ground-truth set is complete."""

    def _pages(self, keys: set[str]) -> list:
        return [spike.Page(k, Path(f"{k}.jpg"), "x") for k in sorted(keys)]

    def test_committed_set_is_complete(self) -> None:
        pages = self._pages(set(spike.EXPECTED_PAGES))
        self.assertEqual(spike.inventory_problems(pages, spike.DEFAULT_TRUTH, []), [])

    def test_default_dir_reports_missing_and_extra(self) -> None:
        keys = set(spike.EXPECTED_PAGES) - {"dulac-picture-book-0154"} | {"stray-0001"}
        problems = spike.inventory_problems(self._pages(keys), spike.DEFAULT_TRUTH, [])
        self.assertTrue(any("dulac-picture-book-0154" in p for p in problems))
        self.assertTrue(any("stray-0001" in p for p in problems))

    def test_custom_dir_skips_the_set_check(self) -> None:
        pages = self._pages({"book-0001"})
        self.assertEqual(spike.inventory_problems(pages, Path("/elsewhere"), []), [])

    def test_unknown_plain_key(self) -> None:
        pages = self._pages({"book-0001"})
        problems = spike.inventory_problems(pages, Path("/elsewhere"), ["book-0002"])
        self.assertIn("book-0002", problems[0])

    def test_the_committed_files_match(self) -> None:
        pages, _ = spike.load_pages(spike.DEFAULT_TRUTH, Path("/no-corpus"))
        self.assertEqual({p.key for p in pages}, set(spike.EXPECTED_PAGES))


class Main(unittest.TestCase):
    """The command-line entry point."""

    def _run(self, argv: list[str]) -> tuple[str, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            spike.main(argv)
        return out.getvalue(), err.getvalue()

    def test_no_truth_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out, _ = self._run(["--truth", tmp, "--corpus", tmp])
        self.assertIn("No ground-truth files", out)

    def test_missing_images_prints_notice(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            truth, corpus = _corpus(Path(tmp), {"book-0001": "hi"}, with_images=False)
            out, _ = self._run(
                ["--truth", str(truth), "--corpus", str(corpus), "--plain", "book-0001"]
            )
        self.assertIn("Rebuild them", out)

    def test_bad_plain_key_exits(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            truth, corpus = _corpus(Path(tmp), {"book-0001": "hi"})
            with self.assertRaises(SystemExit) as caught:
                self._run(["--truth", str(truth), "--corpus", str(corpus), "--plain", "nope-0001"])
        self.assertEqual(caught.exception.code, 2)

    def test_scores_and_writes_json(self) -> None:
        def fake(_: object) -> dict[str, tuple[object, str]]:
            return {"fake": (lambda image: "hi", ""), "gone": (None, "not installed")}

        with tempfile.TemporaryDirectory() as tmp:
            truth, corpus = _corpus(Path(tmp), {"book-0001": "hi"})
            report = Path(tmp) / "out.json"
            with mock.patch.object(spike, "build_recognizers", side_effect=fake):
                out, err = self._run(
                    [
                        *("--truth", str(truth), "--corpus", str(corpus)),
                        *("--plain", "book-0001", "--json", str(report)),
                    ]
                )
            data = json.loads(report.read_text(encoding="utf-8"))
        self.assertIn("fake", out)
        self.assertIn("skipped gone", err)
        self.assertEqual(data[0]["cer"], 0.0)

    def test_build_recognizers_maps_names(self) -> None:
        args = spike.parse_args(["--engines", "tesseract-fast", "tesseract-best", "ocrs"])
        with mock.patch.object(spike, "which_or_none", return_value=None):
            made = spike.build_recognizers(args)
        self.assertEqual(set(made), {"tesseract-fast", "tesseract-best", "ocrs"})


if __name__ == "__main__":
    unittest.main()
