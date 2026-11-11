"""Validate full frames, make lossless website assets and package the media kit.

The kit (every PNG and WebP, a gallery and the notices) is written outside the
repository. Only the WebP images that README.md shows are copied into
docs/screenshots.
"""
from datetime import date
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
import zipfile

from PIL import Image, ImageChops, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
SOURCE, OUTPUT, ARCHIVE = map(lambda p: Path(p).resolve(), sys.argv[1:4])
CAPTIONS = {
    "library-light": "Local library · light appearance",
    "library-dark": "Local library · dark appearance",
    **{f"reading-{theme}-{tone}": f"{theme.title()} reading theme · {tone} appearance"
       for theme in ["default", "soft", "clear", "compact"] for tone in ["light", "dark"]},
    "reading-tools": "The title bar and the floating reading tools",
    "aa-panel": "Aa · theme, text and zoom for the open book",
    "contents-drawer": "Contents in a drawer beside the page",
    "progress-line": "Progress line with chapters and a preview of the jump",
    "notes-highlights": "Bookmark, saved highlight and note sidebar",
    "word-translation": "Offline English → Turkish lookup for book",
    "listen-word-highlight": "Listen paused with the current spoken word retained",
    "reading-settings": "Settings · reading defaults for every book",
    "theme-settings": "Settings · themes, light or dark and the progress line",
    "listen-settings": "Settings · read-aloud voice and speed",
    "dictionary-downloads": "Optional dictionaries · installed pack and European languages",
    "dictionary-downloads-asian": "Optional dictionaries · Japanese, Chinese and Korean",
    "backup-export": "Export bookmarks, highlighted quotes and notes",
    "shortcuts": "Settings · the shortcut list that F1 opens",
    "reading-fullscreen": "Fullscreen reading with everything hidden",
    "library-data": "Create or restore a library backup",
    "pdf-document": "Original PDF page, typography and vector artwork",
    "pdf-book": "The same PDF reconstructed in Book view",
}
OUTPUT.mkdir(parents=True, exist_ok=True)
(OUTPUT / "web").mkdir(exist_ok=True)
items = []
for name, caption in CAPTIONS.items():
    source = SOURCE / f"{name}.png"
    with Image.open(source) as im:
        assert im.size == (2560, 1600), (name, im.size)
        assert im.convert("RGBA").getchannel("A").getextrema() == (255, 255), name
        rgb = im.convert("RGB")
        web = OUTPUT / "web" / f"{name}.webp"
        rgb.save(web, "WEBP", lossless=True, method=6)
        with Image.open(web) as decoded:
            assert ImageChops.difference(rgb, decoded.convert("RGB")).getbbox() is None, name
    png = OUTPUT / source.name
    shutil.copy2(source, png)
    items.append({"name": name, "caption": caption, "width": 2560, "height": 1600,
                  "png": png.name, "webp": f"web/{name}.webp",
                  "png_bytes": png.stat().st_size, "webp_bytes": web.stat().st_size,
                  "png_sha256": hashlib.sha256(png.read_bytes()).hexdigest(),
                  "webp_sha256": hashlib.sha256(web.read_bytes()).hexdigest()})
# Retain the original README hero URL as an exact alias of Default dark.
for suffix, folder in [("png", OUTPUT), ("webp", OUTPUT / "web")]:
    shutil.copy2(folder / f"reading-default-dark.{suffix}", folder / f"reading-dark.{suffix}")
metadata = {"application": "simPl Reader", "date": str(date.today()),
            "capture": "Full production widget tree rendered by iced/tiny-skia at 2x; no OS desktop capture",
            "profile": "Isolated original demonstration library; no personal documents",
            "aliases": {"reading-dark": "reading-default-dark"}, "images": items,
            "dictionary_content": {"images": ["word-translation.png", "web/word-translation.webp", "overview.webp"],
                "license": "CC-BY-SA-4.0", "license_url": "https://creativecommons.org/licenses/by-sa/4.0/",
                "attribution": "ATTRIBUTION.md", "website_caption": "attribution.html"}}
(OUTPUT / "manifest.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
shutil.copy2(ROOT / "assets/dictionaries/README.md", OUTPUT / "dictionary-credits.md")
shutil.copy2(ROOT / "assets/dictionaries/CC-BY-SA-4.0.txt", OUTPUT / "CC-BY-SA-4.0.txt")
shutil.copy2(ROOT / "LICENSE.md", OUTPUT / "project-license.md")
for notice in ["ATTRIBUTION.md", "attribution.html"]:
    original = ROOT / "docs/screenshots" / notice
    if original.resolve() != (OUTPUT / notice).resolve():
        shutil.copy2(original, OUTPUT / notice)
rows = ["| Preview | Description | Full PNG | WebP |", "| --- | --- | --- | --- |"]
for item in items:
    name = item["name"]
    rows.append(f'| <img src="web/{name}.webp" width="240" alt="{item["caption"]}"> | {item["caption"]} | [PNG]({name}.png) | [WebP](web/{name}.webp) |')
readme = f'''# simPl promotional media

{len(items)} full-window images, each **2560 × 1600** (16:10). PNG files preserve
the complete captured frame; `web/` contains pixel-identical, lossless WebP copies
at the same resolution. `reading-dark` is a compatibility alias for Default dark.

Use WebP on the website and PNG when editing or sharing originals. Suggested hero:
[Soft light](reading-soft-light.png) or [Default dark](reading-default-dark.png).
`manifest.json` provides captions, dimensions, byte sizes and SHA-256 hashes.

These images use simPl's production widgets, layout, fonts and tiny-skia renderer
at a 2× scale. The Windows native capture connection was unavailable in this
session, so these are **full-window application renders, not OS desktop captures**.
They contain no added controls, fake feature overlays or retouched interface.
The fullscreen image shows the application's fullscreen state; it does not
verify an OS fullscreen transition. Demo reading progress is saved through
actual page navigation. Listen's word marker comes from the installed Windows
SAPI engine (muted, then paused); the translation comes from the verified local
English → Turkish dictionary package.

The books, author names, prose, geometric covers and PDF artwork were created
for these demonstrations. They are not bundled with the reader and do not come
from a user's profile. Prepared for simPl's website, documentation and promotion.
The word-translation image and its preview in `overview.webp` include dictionary
data from **WikDict** (Karl Bartel), **Wiktionary contributors** via **DBnary**,
under [**CC BY-SA 4.0**](https://creativecommons.org/licenses/by-sa/4.0/).
simPl normalizes and excerpts this data. Keep the visible provider credit and
publish a linked attribution with these images; use [the ready-to-use HTML](attribution.html)
and [full image attribution](ATTRIBUTION.md). [Dictionary credits](dictionary-credits.md)
cover all downloadable directions. The existing [project license](project-license.md)
and third-party terms continue to apply to their respective material and do not
restrict CC-licensed dictionary content. This kit does not grant additional
rights to the simPl name or logo.

Required Notice: Copyright 2026 Tikkaaa3 (https://github.com/Tikkaaa3/simPl-reader)

## Gallery

{chr(10).join(rows)}

## Regenerate

On Windows with the repo's Rust/MSVC tools and Python with Pillow installed:

```powershell
.\\scripts\\capture-promotion.ps1
```

The script creates fresh fixtures under `target/` and an isolated demo profile in
`C:\\simPl-demo`, which it removes afterwards. It runs
the opt-in production render test, writes this kit to `target/promotion/kit`
with a ZIP beside it, verifies the lossless exports, and copies the images the
project README shows into `docs/screenshots`.
It never changes the normal `%LOCALAPPDATA%\\simPl` profile. Test helpers are
excluded from the shipped executable.
'''
(OUTPUT / "README.md").write_text(readme, encoding="utf-8")
# All gallery cards are contact-sheet previews only; originals remain untouched.
sheet = Image.new("RGB", (1280, ((len(items) + 3) // 4) * 222), "#e8e7e3")
draw = ImageDraw.Draw(sheet)
font = ImageFont.truetype(str(ROOT / "assets/fonts/Inter-Regular.ttf"), 13)
for n, item in enumerate(items):
    x, y = (n % 4) * 320, (n // 4) * 222
    with Image.open(OUTPUT / item["png"]) as im:
        im.thumbnail((304, 190))
        sheet.paste(im.convert("RGB"), (x + 8, y + 8))
    draw.text((x + 8, y + 201), item["name"], font=font, fill="#252525")
sheet.save(OUTPUT / "overview.webp", "WEBP", lossless=True, method=6)
# The repository keeps only what its README shows.
DOCS = ROOT / "docs/screenshots"
shown = sorted(set(re.findall(r"!\[[^\]]*\]\(docs/screenshots/([a-z-]+\.webp)\)", (ROOT / "README.md").read_text(encoding="utf-8"))))
assert shown, "README.md shows no screenshots"
for old in DOCS.glob("*.webp"):
    old.unlink()
for name in shown:
    shutil.copy2(OUTPUT / "web" / name, DOCS / name)
print(f"README images: {len(shown)} in {DOCS}")
ARCHIVE.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(ARCHIVE, "w", compression=zipfile.ZIP_DEFLATED) as z:
    for path in sorted(OUTPUT.rglob("*")):
        if path.is_file():
            z.write(path, path.relative_to(OUTPUT))
with zipfile.ZipFile(ARCHIVE) as z:
    assert z.testzip() is None
png_bytes = sum(item["png_bytes"] for item in items)
web_bytes = sum(item["webp_bytes"] for item in items)
print(f"Verified {len(items)} full frames + hero alias; PNG {png_bytes / 1e6:.2f} MB, lossless WebP {web_bytes / 1e6:.2f} MB")
print(f"ZIP: {ARCHIVE} ({ARCHIVE.stat().st_size / 1e6:.2f} MB)")
