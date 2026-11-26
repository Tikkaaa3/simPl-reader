"""Generate MSIX visual assets and copyright-safe Microsoft Store listing assets.

Usage:
  python scripts/msix-assets.py package <assets_dir>
  python scripts/msix-assets.py store <store_dir>
"""
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
SVG_PATH = ROOT / "installer" / "simPl.svg"
SCREENSHOTS_DIR = ROOT / "docs" / "screenshots"

# Curated desktop screenshots from docs/screenshots (all 100% original demo books;
# word-translation.webp is intentionally excluded to avoid CC BY-SA attribution
# requirements inside the Store screenshot gallery).
STORE_SCREENSHOTS = [
    ("01-reading-dark.png", "reading-dark.webp", "Default dark reading view with calm paper layout and chapter progress line"),
    ("02-reading-soft-light.png", "reading-soft-light.webp", "Warm Soft light reading theme with bundled serif typography"),
    ("03-library-light.png", "library-light.webp", "Local library with Continue Reading, favourites, shelves and search (light)"),
    ("04-library-dark.png", "library-dark.webp", "Local library in dark mode"),
    ("05-pdf-document.png", "pdf-document.webp", "PDF Document view preserving original page layout and illustrations"),
    ("06-pdf-book.png", "pdf-book.webp", "PDF Book view reconstructing prose and artwork in your chosen theme"),
    ("07-aa-panel.png", "aa-panel.webp", "Per-book Aa panel for theme, font, text size, spacing, margins and zoom"),
    ("08-notes-highlights.png", "notes-highlights.webp", "Bookmarks, multi-color highlights and notes sidebar"),
    ("09-listen-word-highlight.png", "listen-word-highlight.webp", "Offline read aloud using Windows voices with spoken-word tracking"),
    ("10-contents-drawer.png", "contents-drawer.webp", "Table of contents drawer beside the reading page"),
]


def _render_book_glyph(canvas_size: int, glyph_scale: float = 1.0, bg_color: str | None = "#0c0c0c", rounded: bool = False) -> Image.Image:
    """Render the simPl open-book SVG vector at >=8x supersampling with clean alpha."""
    hi = max(canvas_size * 8, 2048)
    # Render the glyph onto a solid dark surface first so downsampling never bleeds
    # transparent black into anti-aliased stroke edges.
    surface_bg = "#0c0c0c" if (rounded or bg_color is None) else bg_color
    rgb = Image.new("RGB", (hi, hi), surface_bg)
    draw = ImageDraw.Draw(rgb)

    # SVG viewBox is 0..256.
    scale = (hi / 256.0) * glyph_scale
    offset = (hi - 256.0 * scale) / 2.0

    stroke_w = max(2, round(10.0 * scale))
    cap_r = stroke_w / 2.0

    def draw_strokes(target_draw: ImageDraw.ImageDraw, fill_color) -> None:
        for path in ET.parse(SVG_PATH).iter("{http://www.w3.org/2000/svg}path"):
            tokens = iter(re.findall(r"[MQV]|-?\d+(?:\.\d+)?", path.attrib["d"]))
            points: list[tuple[float, float]] = []
            current = (0.0, 0.0)

            def next_point() -> tuple[float, float]:
                return float(next(tokens)), float(next(tokens))

            def flush() -> None:
                if len(points) > 1:
                    scaled = [(offset + x * scale, offset + y * scale) for x, y in points]
                    for a, b in zip(scaled, scaled[1:]):
                        target_draw.line((a, b), fill=fill_color, width=stroke_w)
                    for x, y in scaled:
                        target_draw.ellipse((x - cap_r, y - cap_r, x + cap_r, y + cap_r), fill=fill_color)

            for command in tokens:
                if command == "M":
                    flush()
                    current = next_point()
                    points = [current]
                elif command == "V":
                    current = (current[0], float(next(tokens)))
                    points.append(current)
                elif command == "Q":
                    control, end = next_point(), next_point()
                    for step in range(1, 65):
                        t = step / 64.0
                        points.append((
                            (1 - t) ** 2 * current[0] + 2 * (1 - t) * t * control[0] + t ** 2 * end[0],
                            (1 - t) ** 2 * current[1] + 2 * (1 - t) * t * control[1] + t ** 2 * end[1],
                        ))
                    current = end
                else:
                    raise ValueError(f"Unsupported SVG path command: {command}")
            flush()

    if bg_color is None and not rounded:
        # Pure white-stroke glyph on transparent background (used for wide/hero overlays).
        alpha = Image.new("L", (hi, hi), 0)
        draw_strokes(ImageDraw.Draw(alpha), 255)
        alpha_sm = alpha.resize((canvas_size, canvas_size), Image.Resampling.LANCZOS)
        out = Image.new("RGBA", (canvas_size, canvas_size), (237, 237, 237, 255))
        out.putalpha(alpha_sm)
        return out

    draw_strokes(draw, "#ededed")
    rgb_sm = rgb.resize((canvas_size, canvas_size), Image.Resampling.LANCZOS)
    if not rounded:
        return rgb_sm.convert("RGBA")

    # Separate smooth alpha mask for the rounded squircle avoids LANCZOS dark-edge halos.
    mask = Image.new("L", (hi, hi), 0)
    pad = 8.0 * scale
    radius = 52.0 * scale
    ImageDraw.Draw(mask).rounded_rectangle(
        (offset + pad, offset + pad, offset + 256.0 * scale - pad, offset + 256.0 * scale - pad),
        radius=radius,
        fill=255,
    )
    mask_sm = mask.resize((canvas_size, canvas_size), Image.Resampling.LANCZOS)
    out = rgb_sm.convert("RGBA")
    out.putalpha(mask_sm)
    return out


def _render_rect_tile(width: int, height: int, bg_color: str = "#0c0c0c", glyph_ratio: float = 0.72) -> Image.Image:
    """Render a rectangular tile with the book glyph centered cleanly."""
    canvas = Image.new("RGBA", (width, height), bg_color)
    icon_dim = max(16, round(min(width, height) * glyph_ratio))
    glyph = _render_book_glyph(icon_dim, glyph_scale=1.0, bg_color=None, rounded=False)
    pos = ((width - icon_dim) // 2, (height - icon_dim) // 2)
    canvas.alpha_composite(glyph, dest=pos)
    return canvas


def generate_package_assets(assets_dir: Path) -> None:
    assets_dir.mkdir(parents=True, exist_ok=True)

    # Scale-qualified assets indexed by makepri.exe
    square_scales = {
        "StoreLogo": [(100, 50), (125, 63), (150, 75), (200, 100), (400, 200)],
        "Square44x44Logo": [(100, 44), (125, 55), (150, 66), (200, 88), (400, 176)],
        "SmallTile": [(100, 71), (125, 89), (150, 107), (200, 142), (400, 284)],
        "Square150x150Logo": [(100, 150), (125, 188), (150, 225), (200, 300), (400, 600)],
        "LargeTile": [(100, 310), (125, 388), (150, 465), (200, 620), (400, 1240)],
    }

    for base_name, entries in square_scales.items():
        for scale_pct, px in entries:
            glyph_scale = 0.84 if base_name in ("Square150x150Logo", "LargeTile") else 0.92
            img = _render_book_glyph(px, glyph_scale=glyph_scale, bg_color="#0c0c0c", rounded=False)
            img.save(assets_dir / f"{base_name}.scale-{scale_pct}.png", "PNG")

    wide_entries = [
        (100, 310, 150),
        (125, 388, 188),
        (150, 465, 225),
        (200, 620, 300),
        (400, 1240, 600),
    ]
    for scale_pct, w, h in wide_entries:
        img = _render_rect_tile(w, h, bg_color="#0c0c0c", glyph_ratio=0.76)
        img.save(assets_dir / f"Wide310x150Logo.scale-{scale_pct}.png", "PNG")

    # Full Windows 10/11 taskbar/Start/Explorer targetsize set so icons stay crisp at every DPI.
    for px in (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256):
        plated = _render_book_glyph(px, glyph_scale=0.92, bg_color="#0c0c0c", rounded=False)
        plated.save(assets_dir / f"Square44x44Logo.targetsize-{px}.png", "PNG")

        unplated = _render_book_glyph(px, glyph_scale=0.96, bg_color=None, rounded=True)
        unplated.save(assets_dir / f"Square44x44Logo.altform-unplated_targetsize-{px}.png", "PNG")
        unplated.save(assets_dir / f"Square44x44Logo.altform-lightunplated_targetsize-{px}.png", "PNG")

    print(f"Generated MSIX package assets in {assets_dir}")


def generate_store_assets(store_dir: Path) -> None:
    store_dir.mkdir(parents=True, exist_ok=True)
    shots_out = store_dir / "screenshots"
    shots_out.mkdir(parents=True, exist_ok=True)

    # 1. Primary 1:1 Store App Tile Icons (1080x1080 HiDPI master + 300x300 standard)
    icon_1080 = _render_book_glyph(1080, glyph_scale=0.88, bg_color="#0c0c0c", rounded=False).convert("RGB")
    icon_1080.save(store_dir / "app-icon-1080x1080.png", "PNG")
    icon_300 = _render_book_glyph(300, glyph_scale=0.88, bg_color="#0c0c0c", rounded=False).convert("RGB")
    icon_300.save(store_dir / "app-icon-300x300.png", "PNG")

    # 2. Optional Promotional Hero Image (1920x1080 PNG, no text per Store guidelines)
    hero = _render_rect_tile(1920, 1080, bg_color="#121417", glyph_ratio=0.46).convert("RGB")
    hero.save(store_dir / "promotional-hero-1920x1080.png", "PNG")

    # 3. Optional 2:3 Poster Art (720x1080 PNG, clean dark icon card)
    poster = _render_rect_tile(720, 1080, bg_color="#121417", glyph_ratio=0.58).convert("RGB")
    poster.save(store_dir / "poster-art-720x1080.png", "PNG")

    # 4. Copyright-safe Desktop Screenshots (opaque RGB PNGs, 2560x1600)
    exported = []
    for out_name, src_name, caption in STORE_SCREENSHOTS:
        src = SCREENSHOTS_DIR / src_name
        if not src.is_file():
            raise FileNotFoundError(f"Missing screenshot source: {src}")
        with Image.open(src) as im:
            rgb = im.convert("RGB")
            dest = shots_out / out_name
            rgb.save(dest, "PNG", optimize=True)
            exported.append((out_name, rgb.size[0], rgb.size[1], caption))

    # 5. Partner Center ready-to-paste copy
    listing_md = f"""# Microsoft Store Listing Copy — simPl Reader

All text and screenshots in this folder use original project-authored demonstration
material only (no copyrighted books and no CC BY-SA dictionary screenshots).

## Basic Details

- **App display name (reserved):** `simPl Reader`
- **Category:** Books + reference *(Subcategory: E-reader)* — or Productivity
- **Privacy policy URL:** `https://github.com/Tikkaaa3/simPl-reader/blob/main/docs/privacy.md` *(or `https://simplreader.app/privacy` once published on your website)*
- **Website:** `https://simplreader.app`
- **Support contact info (URL):** `https://github.com/Tikkaaa3/simPl-reader/issues`

## Short Description (up to 200 characters)

A quiet, native offline reader for your PDF, EPUB, HTML, TXT and Markdown books with themes, notes, read aloud and portable backups.

## Description (Partner Center Store Listing)

A quiet place for your books.

simPl Reader is a fast, native, offline desktop reader for PDF, EPUB, HTML, plain text and Markdown. Keep a local library on your PC, pick up right where you stopped, and read in a calm interface that stays out of your way.

Made for focused reading:
• Read your local files: Open a document where it is, or copy it into your local library. Organize books with favourites, shelves and instant title/author search.
• Calm, predictable layout: One page at a time with visible paper boundaries, a chapter-segmented progress line, and reading tools that hide automatically as soon as you scroll.
• Stable page numbers: Zoom, themes and typography change how a page looks without changing its page number or total.
• Two ways to read PDFs: Keep the original page layout in Document view, or switch to Book view for reconstructed prose and extracted illustrations in your chosen reading theme.
• Four reading themes in light and dark: Choose Default, Soft, Clear or Compact, plus bundled reading fonts, text size, line spacing and margins — either globally or per book.
• Listen offline: Read continuously across pages and chapters using your Windows installed voices, with live spoken-word highlighting and adjustable speed.
• Offline word lookup: Optionally download verified offline dictionary packages across 13 language directions (English, Turkish, Spanish, German, French, Japanese, Korean and Chinese). Downloads are explicit and send no document text.
• Bookmarks, highlights and notes: Mark pages, highlight passages in four colors, attach notes, and export them to Markdown, plain text or JSON.
• Portable backups & full keyboard navigation: Create and restore verified library backups, and navigate every control from the keyboard (press F1 for shortcuts).

No accounts, no ads, no analytics, and no telemetry. Bring your own DRM-free books and documents.

## Product Features (Partner Center bullet fields — up to 20)

1. Native offline reader for PDF, EPUB, HTML, TXT and Markdown
2. Two PDF modes: original Document view and reflowable Book view
3. Four reading themes (Default, Soft, Clear, Compact) in light and dark modes
4. Stable page numbers independent of zoom, font and theme changes
5. Local library with favourites, shelves, Continue Reading and instant search
6. Bookmarks, four-color highlights and notes with Markdown, TXT and JSON export
7. Offline read aloud using Windows installed voices with word highlighting
8. Optional offline word lookup across 13 downloadable language pairs
9. Verified portable ZIP library backup and restore
10. No accounts, advertisements, analytics or telemetry

## Search Terms (up to 7, max 30 chars each)

1. epub reader
2. pdf reader
3. ebook reader
4. offline book reader
5. markdown reader
6. read aloud reader
7. document reader

## Notes for Certification (paste into Submission Options → Notes for certification)

simPl Reader is a native Win32/Rust desktop application (Iced + tiny-skia CPU renderer) for reading user-selected local PDF, EPUB, HTML, TXT and Markdown documents offline.

- No login, account, or internet connection is required to test the application.
- The `runFullTrust` capability is declared because this is a packaged Win32 desktop app that opens user-chosen files (including via Explorer "Open with"), renders PDFs via bundled PDFium (`pdfium.dll`), and uses locally installed Windows SAPI speech voices for offline read-aloud.
- Network access is only used if the user explicitly downloads an optional offline dictionary pack in Settings → Translation.
- All user data stays local on the device.

## Exported Images

| File | Resolution | Notes |
| --- | --- | --- |
| `app-icon-1080x1080.png` | 1080 × 1080 | Upload to **Store logos** (1:1 HiDPI app tile / box art; use `app-icon-300x300.png` if a slot specifically requires 300 × 300) |
| `app-icon-300x300.png` | 300 × 300 | Standard 300 × 300 1:1 app tile icon |
| `promotional-hero-1920x1080.png` | 1920 × 1080 | Optional **Promotional artwork** (16:9 hero, text-free) |
| `poster-art-720x1080.png` | 720 × 1080 | Optional **2:3 Poster art** |
"""
    for fname, w, h, cap in exported:
        listing_md += f"| `screenshots/{fname}` | {w} × {h} | {cap} |\n"

    (store_dir / "store-listing.md").write_text(listing_md, encoding="utf-8")
    print(f"Generated Microsoft Store listing assets in {store_dir} ({len(exported)} screenshots)")


def main() -> None:
    if len(sys.argv) != 3 or sys.argv[1] not in ("package", "store"):
        raise SystemExit("Usage: python scripts/msix-assets.py <package|store> <output_dir>")
    mode = sys.argv[1]
    out_dir = Path(sys.argv[2]).resolve()
    if mode == "package":
        generate_package_assets(out_dir)
    else:
        generate_store_assets(out_dir)


if __name__ == "__main__":
    main()
