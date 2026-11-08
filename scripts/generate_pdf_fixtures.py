#!/usr/bin/env python3
"""Generate the tiny PDF fixtures used by the PDF import tests.

Stdlib-only and deterministic: re-running writes byte-identical files, so the outputs
under tests/fixtures/pdf/ can be committed (each is a few KB). The PDFs are hand-built, with
no timestamps and no random ids.

Fixtures written:
    text3.pdf        three pages with text ("Page one", "Page two", "Page three")
    rotate90.pdf     one 200x300 pt page with /Rotate 90, so it renders landscape
    cropbox.pdf      a 400x400 pt media box with a 200x100 pt crop box
    transparent.pdf  an image-only page whose left half is fully transparent
    a0.pdf           one A0 page (2384x3370 pt), over both render caps at 300 DPI
    encrypted.pdf    a PDF that needs a password (standard handler, R2) and matches none
    zero_pages.pdf   a valid PDF with an empty page tree
    junk_prefix.pdf  text3.pdf with 1000 bytes of junk before "%PDF-"
    truncated.pdf    text3.pdf cut off before its cross-reference table

Usage:
    python3 scripts/generate_pdf_fixtures.py
"""

from __future__ import annotations

import argparse
import sys
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "pdf"
JUNK_PREFIX_LEN = 1000
# The standard security handler's 32-byte /O and /U entries (PDF 32000-1, 7.6.3.4), as fixed
# patterns. They match no password, so the document cannot be opened.
ENCRYPT_O = bytes(range(32))
ENCRYPT_U = bytes(range(32, 64))
ENCRYPT_PERMISSIONS = -4
# Every fixture puts its page tree at object 2.
CATALOG = b"<< /Type /Catalog /Pages 2 0 R >>"
FILE_ID = b"scriptorium-fixture!"  # 20 bytes, fixed so the output is reproducible


def stream_obj(dictionary: str, data: bytes) -> bytes:
    """Return an indirect-object body for a stream with the given extra dictionary entries."""
    head = f"<< {dictionary} /Length {len(data)} >>\nstream\n".encode("latin-1")
    return head + data + b"\nendstream"


def build_pdf(objects: list[bytes], trailer_extra: str = "") -> bytes:
    """Serialise numbered object bodies (object 1 is the catalog) with a valid xref table."""
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets: list[int] = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n".encode("latin-1") + body + b"\nendobj\n"
    xref_at = len(out)
    out += f"xref\n0 {len(objects) + 1}\n".encode("latin-1")
    out += b"0000000000 65535 f \n"
    for offset in offsets:
        out += f"{offset:010d} 00000 n \n".encode("latin-1")
    out += (
        f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R {trailer_extra} >>\n"
        f"startxref\n{xref_at}\n%%EOF\n"
    ).encode("latin-1")
    return bytes(out)


def page_tree(page_bodies: list[str], font_obj: int, first_page_obj: int = 3) -> list[bytes]:
    """Return catalog, pages, and page objects. Content streams follow the page objects."""
    count = len(page_bodies)
    kids = " ".join(f"{first_page_obj + i} 0 R" for i in range(count))
    objects = [
        CATALOG,
        f"<< /Type /Pages /Kids [{kids}] /Count {count} >>".encode("latin-1"),
    ]
    for i, extra in enumerate(page_bodies):
        content = first_page_obj + count + i
        objects.append(
            (
                f"<< /Type /Page /Parent 2 0 R {extra} "
                f"/Resources << /Font << /F1 {font_obj} 0 R >> >> /Contents {content} 0 R >>"
            ).encode("latin-1")
        )
    return objects


def text_content(text: str, marks: str = "") -> bytes:
    """Return a content stream that draws `text` near the top-left of the page, then `marks`,
    extra drawing operators such as filled black squares.
    """
    return f"BT /F1 24 Tf 20 {200} Td ({text}) Tj ET {marks}".rstrip().encode("latin-1")


def simple_pdf(page_extras: list[str], texts: list[str], marks: list[str] | None = None) -> bytes:
    """Build a PDF with one text page per entry. `page_extras` holds per-page boxes, and
    `marks` optional per-page drawing.
    """
    font_obj = 3 + 2 * len(texts)
    objects = page_tree(page_extras, font_obj)
    page_marks = marks or [""] * len(texts)
    objects += [stream_obj("", text_content(t, m)) for t, m in zip(texts, page_marks, strict=True)]
    objects.append(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    return build_pdf(objects)


def text3() -> bytes:
    """Three portrait pages with text."""
    box = "/MediaBox [0 0 300 300]"
    return simple_pdf([box] * 3, ["Page one", "Page two", "Page three"])


def rotate90() -> bytes:
    """One portrait 200x300 pt page rotated by 90 degrees, with a black square in the unrotated
    page's bottom-left corner. Rotated clockwise for display, that corner is the top-left.
    """
    return simple_pdf(["/MediaBox [0 0 200 300] /Rotate 90"], ["Rotated"], ["0 g 0 0 50 50 re f"])


def cropbox() -> bytes:
    """A 400x400 pt media box with a 200x100 pt crop box. A black square fills the crop box's
    bottom-left 50x50 pt, and a second square sits outside the crop box, where it must not show.
    The text, at y=200, is outside the crop box too.
    """
    box = "/MediaBox [0 0 400 400] /CropBox [50 50 250 150]"
    return simple_pdf([box], ["Cropped"], ["0 g 50 50 50 50 re f 300 300 50 50 re f"])


def a0() -> bytes:
    """One A0 page, which at 300 DPI is about 9933x14043 px."""
    return simple_pdf(["/MediaBox [0 0 2384 3370]"], ["A0"])


def transparent() -> bytes:
    """An image-only page. A 4x4 image fills it, and its left two columns have zero alpha."""
    rgb = bytes([200, 30, 30] * 16)
    alpha = bytes([0, 0, 255, 255] * 4)
    image = stream_obj(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceRGB "
        "/BitsPerComponent 8 /SMask 6 0 R /Filter /FlateDecode",
        zlib.compress(rgb, 9),
    )
    mask = stream_obj(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray "
        "/BitsPerComponent 8 /Filter /FlateDecode",
        zlib.compress(alpha, 9),
    )
    objects = [
        CATALOG,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] "
        b"/Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>",
        stream_obj("", b"q 100 0 0 100 0 0 cm /Im1 Do Q"),
        image,
        mask,
    ]
    return build_pdf(objects)


def zero_pages() -> bytes:
    """A valid document whose page tree is empty."""
    return build_pdf(
        [
            CATALOG,
            b"<< /Type /Pages /Kids [] /Count 0 >>",
        ]
    )


def encrypted() -> bytes:
    """A document that needs a password. /O and /U are fixed bytes that match no password, the
    empty one included, so a reader can only refuse it. Nothing is hashed or encrypted, because
    the page has no content and only the refusal is under test.
    """
    owner, user = ENCRYPT_O, ENCRYPT_U
    encrypt = (
        f"<< /Filter /Standard /V 1 /R 2 /P {ENCRYPT_PERMISSIONS} "
        f"/O <{owner.hex()}> /U <{user.hex()}> >>"
    ).encode("latin-1")
    objects = [
        CATALOG,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
        encrypt,
    ]
    file_id = FILE_ID.hex()
    return build_pdf(objects, f"/Encrypt 4 0 R /ID [<{file_id}> <{file_id}>]")


def junk_prefix(document: bytes) -> bytes:
    """Put junk before the header, keeping `%PDF-` inside the first 1024 bytes."""
    junk = (b"junk before the pdf header\n" * 64)[:JUNK_PREFIX_LEN]
    return junk + document


def truncated(document: bytes) -> bytes:
    """Cut a document off before its cross-reference table."""
    return document[: document.index(b"xref") // 2]


def fixtures() -> dict[str, bytes]:
    """Return every fixture, keyed by file name."""
    base = text3()
    return {
        "text3.pdf": base,
        "rotate90.pdf": rotate90(),
        "cropbox.pdf": cropbox(),
        "transparent.pdf": transparent(),
        "a0.pdf": a0(),
        "encrypted.pdf": encrypted(),
        "zero_pages.pdf": zero_pages(),
        "junk_prefix.pdf": junk_prefix(base),
        "truncated.pdf": truncated(base),
    }


def main(argv: list[str]) -> int:
    """Write all fixtures into tests/fixtures/pdf/. The location is fixed, so there is nothing to
    pass on the command line.
    """
    argparse.ArgumentParser(description="Generate the PDF import test fixtures.").parse_args(argv)
    OUT.mkdir(parents=True, exist_ok=True)
    for name, data in fixtures().items():
        (OUT / name).write_bytes(data)
        print(f"{OUT / name}: {len(data)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
