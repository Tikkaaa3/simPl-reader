"""Create original demonstration books/covers; never reads a user's library.

Requires Pillow (also used for the PNG/WebP website exports).
"""
import html
import json
from pathlib import Path
import sys
import textwrap
import zipfile

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(sys.argv[1]).resolve()
OUT.mkdir(parents=True, exist_ok=True)

BOOKS = [
    ("The Quiet Hours", "Mira Vale", "epub", "#223c36", "#e4d7ad"),
    ("Letters from the Coast", "Leon Reed", "epub", "#c7dce0", "#274c60"),
    ("An Atlas of Small Things", "Ada Finch", "html", "#ecd7bc", "#8d4939"),
    ("Field Notes", "Mira Vale", "md", "#b8c4a6", "#324336"),
    ("Pocket Poems", "Leon Reed", "txt", "#8a4147", "#f6e3cd"),
    ("The Art of Paying Attention", "Ada Finch", "pdf", "#ded7e9", "#524869"),
    ("A Garden in Winter", "Mira Vale", "epub", "#e1dfd3", "#52654e"),
    ("Learning to Wander", "Ada Finch", "md", "#dfaa75", "#493e3a"),
]

PARAGRAPHS = [
    "The book lay open on the kitchen table, exactly where she had left it the evening before. Beyond the window, the garden was slowly becoming visible: first the stone path, then the pear tree, then the small blue gate at the end of the wall.",
    "Mara liked this hour best. Nothing had quite begun, and nothing was asking to be finished. She filled the kettle, opened the window a little, and listened to the ordinary sounds of a street waking up.",
    "There was a letter between the pages. It had arrived three days earlier, folded twice, with her name written in the careful handwriting she remembered from childhood. She had read it once and put it away. Today, she thought, she would read it again.",
    "Outside, a bicycle passed through the pale morning light. Somewhere a door closed; somewhere else, someone laughed. The world was already making its small arrangements, carrying on with the patient business of being alive.",
    "She turned a page. A sentence she had underlined years ago seemed to belong to a different book now, or perhaps to a different version of herself. Some words waited quietly until you were ready to hear them.",
    "By the time the kettle began to sing, the sun had reached the edge of the table. Mara moved her chair into the light and began again, with no particular hurry and nowhere else she needed to be.",
    "In the garden, the pear tree held the last of yesterday's rain. A breeze passed through it, and the leaves answered softly. She thought of all the things that could be noticed in a day if you left a little room for them.",
    "For a while, she simply read. The street grew brighter, the tea grew cooler, and the morning gathered itself around the open window. When she looked up again, she felt that she had been somewhere, although she had never left her chair.",
]


def cover(index, title, author, background, foreground):
    im = Image.new("RGB", (600, 900), background)
    draw = ImageDraw.Draw(im)
    draw.rectangle((32, 32, 568, 868), outline=foreground, width=2)
    regular = lambda size: ImageFont.truetype(str(ROOT / "assets/fonts/Literata-Regular.ttf"), size)
    sans = lambda size: ImageFont.truetype(str(ROOT / "assets/fonts/Inter-Regular.ttf"), size)
    draw.text((65, 68), "A SIMPL DEMO BOOK", fill=foreground, font=sans(18))
    y = 136
    for line in textwrap.wrap(title, 15):
        draw.text((62, y), line, fill=foreground, font=regular(49))
        y += 72
    # Original geometric cover art: a sun, horizon and offset arch.
    x = 300 + (index % 3 - 1) * 40
    draw.ellipse((x - 95, 450, x + 95, 640), outline=foreground, width=3)
    draw.arc((100, 510, 500, 910), 180, 360, fill=foreground, width=3)
    for step in range(5):
        draw.line((80, 665 + step * 22, 520, 665 + step * 22), fill=foreground, width=2)
    draw.text((65, 805), author.upper(), fill=foreground, font=sans(23))
    path = OUT / f"cover-{index}.png"
    im.save(path, optimize=True)
    return path


def body(title, rounds=10):
    return f"<h1>{html.escape(title)}</h1><h2>1 · A little room for the morning</h2>" + "".join(
        f'<p id="p-{r}-{i}">{html.escape(p)}</p>'
        for r in range(rounds) for i, p in enumerate(PARAGRAPHS)
    )


def epub(path, title, author, cover_path):
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        z.writestr("OPS/book.opf", f'''<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">simpl-demo-{path.stem}</dc:identifier><dc:title>{title}</dc:title><dc:creator>{author}</dc:creator><dc:language>en</dc:language><meta property="dcterms:modified">2026-09-30T00:00:00Z</meta></metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/></manifest><spine><itemref idref="chapter"/></spine></package>''')
        z.writestr("OPS/chapter.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head><body>{body(title)}</body></html>')
        z.writestr("OPS/nav.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="chapter.xhtml">A little room for the morning</a></li></ol></nav></body></html>')
        z.write(cover_path, "OPS/cover.png")


def pdf(path, title, author):
    """An authored, selectable-text PDF with a real text layer and vector art."""
    escape = lambda s: s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
    stream = ["0.98 0.97 0.94 rg 0 0 612 792 re f", "0.20 0.25 0.22 rg"]
    def text(x, y, size, value, font="F1"):
        stream.append(f"BT /{font} {size} Tf {x} {y} Td ({escape(value)}) Tj ET")
    text(62, 725, 10, "ESSAYS ON THE EVERYDAY")
    text(62, 677, 29, "The Art of")
    text(62, 641, 29, "Paying Attention")
    text(62, 603, 12, f"{author}  /  A simPl demonstration document")
    stream += ["0.56 0.64 0.54 rg 62 501 488 66 re f", "0.90 0.87 0.72 rg 460 501 45 66 re f", "0.20 0.25 0.22 rg"]
    text(62, 466, 18, "Make space for the ordinary", "F2")
    y = 438
    for paragraph in PARAGRAPHS[1:4]:
        for line in textwrap.wrap(paragraph, 79):
            text(62, y, 11, line, "F2")
            y -= 18
        y -= 13
    text(62, 65, 9, "SIMPL DEMO COLLECTION                                             01")
    data = "\n".join(stream).encode("ascii")
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>", b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>", b"<< /Length " + str(len(data)).encode() + b" >>\nstream\n" + data + b"\nendstream", f"<< /Title ({escape(title)}) /Author ({escape(author)}) >>".encode()]
    result = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for n, obj in enumerate(objects, 1):
        offsets.append(len(result))
        result += f"{n} 0 obj\n".encode() + obj + b"\nendobj\n"
    start = len(result)
    result += f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode()
    result += b"".join(f"{offset:010} 00000 n \n".encode() for offset in offsets[1:])
    result += f"trailer\n<< /Size {len(offsets)} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{start}\n%%EOF\n".encode()
    path.write_bytes(result)


manifest = []
for index, (title, author, ext, bg, fg) in enumerate(BOOKS):
    cover_path = cover(index, title, author, bg, fg)
    path = OUT / f"{title}.{ext}"
    if ext == "epub":
        epub(path, title, author, cover_path)
    elif ext == "pdf":
        pdf(path, title, author)
    elif ext == "html":
        path.write_text(f'<html><head><title>{title}</title><meta name="author" content="{author}"></head><body>{body(title)}</body></html>', encoding="utf-8")
    elif ext == "md":
        path.write_text(f"# {title}\n\n## A little room for the morning\n\n" + "\n\n".join(PARAGRAPHS * 8), encoding="utf-8")
    else:
        path.write_text(title + "\n\n" + "\n\n".join(PARAGRAPHS * 8), encoding="utf-8")
    manifest.append({"source": path.name, "cover": cover_path.name, "title": title, "author": author})
(OUT / "books.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
print(f"Created {len(manifest)} original demonstration books in {OUT}")
