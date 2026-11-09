"""Build "Three Rooms", a three-chapter EPUB for checks that cross chapters.

No third-party packages needed. Every paragraph is numbered, so each page
looks different and a page turn is visible at a glance.
"""
from pathlib import Path
import sys
import zipfile

root = Path(__file__).resolve().parents[2]
output = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "target" / "lane-fixtures" / "Three Rooms.epub"
output.parent.mkdir(parents=True, exist_ok=True)

sentence = (
    "The lamp burned low while the house settled into its evening sounds, and the "
    "reader turned another page without noticing how late it had become. "
)
titles = ["The first room", "The second room", "The third room"]
chapters = []
for number, title in enumerate(titles, start=1):
    paragraphs = "".join(
        f"<p><b>Room {number}, paragraph {index}.</b> {sentence * 4}</p>" for index in range(1, 15)
    )
    chapters.append((
        f"chapter-{number}.xhtml",
        title,
        '<html xmlns="http://www.w3.org/1999/xhtml">'
        f"<head><title>{title}</title></head>"
        f"<body><h1>{number} · {title}</h1>{paragraphs}</body></html>",
    ))

manifest = "".join(
    f'<item id="chapter-{index}" href="{name}" media-type="application/xhtml+xml"/>'
    for index, (name, _, _) in enumerate(chapters, start=1)
)
spine = "".join(f'<itemref idref="chapter-{index}"/>' for index in range(1, len(chapters) + 1))
contents = "".join(f'<li><a href="{name}">{title}</a></li>' for name, title, _ in chapters)
package = (
    '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">'
    '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
    '<dc:identifier id="id">simpl-fixture-three-rooms</dc:identifier>'
    "<dc:title>Three Rooms</dc:title><dc:creator>simPl fixture</dc:creator>"
    "<dc:language>en</dc:language></metadata>"
    '<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
    f"{manifest}</manifest><spine>{spine}</spine></package>"
)
navigation = (
    '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>'
    f'<nav epub:type="toc"><ol>{contents}</ol></nav></body></html>'
)

with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    archive.writestr(
        "META-INF/container.xml",
        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
        '<rootfiles><rootfile full-path="OPS/package.opf" media-type="application/oebps-package+xml"/>'
        "</rootfiles></container>",
    )
    archive.writestr("OPS/package.opf", package)
    archive.writestr("OPS/nav.xhtml", navigation)
    for name, _, xhtml in chapters:
        archive.writestr(f"OPS/{name}", xhtml)
print(output)
