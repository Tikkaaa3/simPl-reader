"""Regenerate the Material Symbols icon subset and its change notice.

Usage: python scripts/subset-icons.py [--check]

Downloads the source font pinned in the change notice once into
target/icon-source/, instances it, subsets it to CODEPOINTS, and writes the
font and the notice's codepoint list. --check builds into a temporary file and
compares codepoints and glyph outlines with the committed font, writing nothing.
Needs fontTools (`python -m pip install --user fonttools`; the committed subset
was checked with 4.68.0). Do not run it with `python -I`, which hides user installs.
"""
import argparse
import hashlib
import io
import re
import sys
import tempfile
import textwrap
import urllib.request
from pathlib import Path

from fontTools import subset
from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

CODEPOINTS = [
    0xE02F, 0xE145, 0xE15B, 0xE415, 0xE518, 0xE51C, 0xE5C4, 0xE5C8, 0xE5CB, 0xE5CC, 0xE5CD, 0xE5D4,
    0xE86F, 0xE880, 0xE8B6, 0xE8B8, 0xE8DE, 0xE98B, 0xEA19, 0xEAE7, 0xEF71, 0xEF7A, 0xF01F, 0xF6F1,
]
AXES = {"FILL": 0, "GRAD": 0, "opsz": 24, "wght": 400}

ROOT = Path(__file__).resolve().parents[1]
FONT = ROOT / "assets" / "fonts" / "MaterialSymbolsOutlined-Subset.ttf"
NOTICE = ROOT / "assets" / "licenses" / "Material-Symbols-CHANGES.txt"
CACHE = ROOT / "target" / "icon-source"
LISTED = re.compile(r"(?<=Unicode )[0-9A-F]{4,6}(?:,\s+[0-9A-F]{4,6})*")


def git_blob_sha1(data: bytes) -> str:
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def source_font(notice: str) -> bytes:
    pinned = re.search(r"Source Git blob SHA-1: ([0-9a-f]{40})", notice).group(1)
    cached = CACHE / f"{pinned}.ttf"
    if cached.exists():
        data = cached.read_bytes()
    else:
        request = urllib.request.Request(
            f"https://api.github.com/repos/google/material-design-icons/git/blobs/{pinned}",
            headers={"Accept": "application/vnd.github.raw", "User-Agent": "simPl-subset-icons"},
        )
        with urllib.request.urlopen(request) as response:
            data = response.read()
    actual = git_blob_sha1(data)
    if actual != pinned:
        sys.exit(f"source font blob is {actual}, the notice pins {pinned}; delete {cached} to download again")
    if not cached.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        cached.write_bytes(data)
    return data


def build(source: bytes, output: Path) -> None:
    font = TTFont(io.BytesIO(source))
    instancer.instantiateVariableFont(font, AXES, inplace=True)
    subsetter = subset.Subsetter(subset.Options())
    subsetter.populate(unicodes=CODEPOINTS)
    subsetter.subset(font)
    # Keep the source's timestamp so the same inputs give the same bytes.
    font.recalcTimestamp = False
    font.save(output)


def glyphs(path: Path) -> dict:
    font = TTFont(path)
    glyph_set = font.getGlyphSet()
    shapes = {}
    for codepoint, name in font.getBestCmap().items():
        pen = RecordingPen()
        glyph_set[name].draw(pen)
        shapes[codepoint] = (glyph_set[name].width, pen.value)
    return shapes


def with_codepoints(notice: str) -> str:
    match = LISTED.search(notice)
    if [int(code, 16) for code in re.split(r",\s+", match.group())] == CODEPOINTS:
        return notice
    notice = notice[: match.start()] + ", ".join(f"{code:04X}" for code in CODEPOINTS) + notice[match.end() :]
    start, end = notice.index("This font file was changed"), notice.index("Output:")
    return notice[:start] + textwrap.fill(" ".join(notice[start:end].split()), 76) + "\n" + notice[end:]


def check(source: bytes, notice: str) -> int:
    with tempfile.TemporaryDirectory() as scratch:
        built = Path(scratch) / "subset.ttf"
        build(source, built)
        expected, actual = glyphs(FONT), glyphs(built)
    details = []
    if missing := sorted(set(actual) - set(expected)):
        details.append("committed font lacks " + ", ".join(f"{c:04X}" for c in missing))
    if extra := sorted(set(expected) - set(actual)):
        details.append("committed font has extra " + ", ".join(f"{c:04X}" for c in extra))
    if changed := [c for c in sorted(set(expected) & set(actual)) if expected[c] != actual[c]]:
        details.append("outlines differ at " + ", ".join(f"{c:04X}" for c in changed))
    if with_codepoints(notice) != notice:
        details.append("the notice lists other codepoints")
    if details:
        print("icons differ: " + "; ".join(details))
        return 1
    print("icons match")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="compare with the committed font, write nothing")
    arguments = parser.parse_args()
    notice = NOTICE.read_text(encoding="utf-8", newline="")
    source = source_font(notice)
    if arguments.check:
        return check(source, notice)
    build(source, FONT)
    NOTICE.write_text(with_codepoints(notice), encoding="utf-8", newline="")
    print(f"wrote {FONT.relative_to(ROOT)} ({FONT.stat().st_size} bytes) with {len(CODEPOINTS)} codepoints")
    return 0


if __name__ == "__main__":
    sys.exit(main())
