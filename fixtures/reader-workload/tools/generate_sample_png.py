#!/usr/bin/env python3
"""Generate the reader-workload sample PNG.

Provenance: project-authored fixture asset for initiative I-001 / W3 (T-003).
This script is checked in so the image's content is reviewable and
regeneratable. The image is a *test asset*: it only needs to be
nonuniform (so a renderer that fails to draw or stretches it is
noticeable), have known intrinsic dimensions, and be tiny.

The script uses only the Python standard library (zlib + struct); no
imaging package, no network access. Run from the repository root:

    python fixtures/reader-workload/tools/generate_sample_png.py

It overwrites assets/images/reader-sample.png deterministically: the
same input always yields byte-identical output (zlib compresses the
fixed pixel rows deterministically).

Content: 480x320 RGB. Background is a horizontal gradient; overlaid are
two vertical color bars, a diagonal band, a filled disc, and a 16px
light frame, so every region of the image differs.
"""

import struct
import zlib

WIDTH = 480
HEIGHT = 320


def clamp(x: int) -> int:
    return 0 if x < 0 else 255 if x > 255 else x


def build_rows() -> list[bytes]:
    cx, cy, r = 340, 200, 70
    rows = []
    for y in range(HEIGHT):
        row = bytearray()
        for x in range(WIDTH):
            r_ = 40 + (190 * x) // (WIDTH - 1)
            g_ = 40 + (150 * y) // (HEIGHT - 1)
            b_ = 90
            if x < 40:
                r_, g_, b_ = 230, 40, 40  # left red bar
            elif 440 <= x < 480:
                r_, g_, b_ = 40, 40, 230  # right blue bar
            if 140 <= x + y < 200:
                r_, g_, b_ = 250, 200, 30  # diagonal yellow band
            if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                r_, g_, b_ = 20, 140, 60  # green disc
            if x < 16 or x >= WIDTH - 16 or y < 16 or y >= HEIGHT - 16:
                r_, g_, b_ = 240, 240, 240  # light frame
            row += bytes((clamp(r_), clamp(g_), clamp(b_)))
        rows.append(bytes(row))
    return rows


def png() -> bytes:
    def chunk(tag: bytes, payload: bytes) -> bytes:
        c = struct.pack(">I", len(payload)) + tag + payload
        return c + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 2, 0, 0, 0)  # 8-bit truecolor
    raw = b"".join(b"\x00" + row for row in build_rows())
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main() -> None:
    import pathlib

    out = (
        pathlib.Path(__file__).resolve().parent.parent
        / "assets"
        / "images"
        / "reader-sample.png"
    )
    out.write_bytes(png())
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
