"""Build "Three Rooms", a three-chapter EPUB for checks that cross chapters,
and "Two Wings" beside it, for the contents drawer's current entry.

"Two Wings" has a chapter file with two sections, a second file that
continues the chapter with no contents entry of its own, and a third file
whose second entry points at an anchor that does not exist.

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


def paragraphs(name, count):
    return "".join(
        f"<p><b>{name}, paragraph {index}.</b> {sentence * 4}</p>" for index in range(1, count + 1)
    )


def xhtml(title, body):
    return (
        '<html xmlns="http://www.w3.org/1999/xhtml">'
        f"<head><title>{title}</title></head><body>{body}</body></html>"
    )


def write_book(path, identifier, title, files, entries):
    """`files` are (name, xhtml) pairs in reading order, `entries` (href, label) pairs."""
    manifest = "".join(
        f'<item id="chapter-{index}" href="{name}" media-type="application/xhtml+xml"/>'
        for index, (name, _) in enumerate(files, start=1)
    )
    spine = "".join(f'<itemref idref="chapter-{index}"/>' for index in range(1, len(files) + 1))
    contents = "".join(f'<li><a href="{href}">{label}</a></li>' for href, label in entries)
    package = (
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        f'<dc:identifier id="id">{identifier}</dc:identifier>'
        f"<dc:title>{title}</dc:title><dc:creator>simPl fixture</dc:creator>"
        "<dc:language>en</dc:language></metadata>"
        '<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>'
        f"{manifest}</manifest><spine>{spine}</spine></package>"
    )
    navigation = (
        '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>'
        f'<nav epub:type="toc"><ol>{contents}</ol></nav></body></html>'
    )
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        archive.writestr(
            "META-INF/container.xml",
            '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
            '<rootfiles><rootfile full-path="OPS/package.opf" media-type="application/oebps-package+xml"/>'
            "</rootfiles></container>",
        )
        archive.writestr("OPS/package.opf", package)
        archive.writestr("OPS/nav.xhtml", navigation)
        for name, document in files:
            archive.writestr(f"OPS/{name}", document)
    print(path)


titles = ["The first room", "The second room", "The third room"]
rooms = [
    (
        f"chapter-{number}.xhtml",
        xhtml(title, f"<h1>{number} · {title}</h1>{paragraphs(f'Room {number}', 14)}"),
    )
    for number, title in enumerate(titles, start=1)
]
write_book(
    output,
    "simpl-fixture-three-rooms",
    "Three Rooms",
    rooms,
    [(name, title) for (name, _), title in zip(rooms, titles)],
)

wings = [
    (
        "wing-1.xhtml",
        xhtml(
            "The east wing",
            "<h1>The east wing</h1>"
            f"{paragraphs('East wing', 14)}"
            '<h2 id="hall">The long hall</h2>'
            f"{paragraphs('Long hall', 14)}"
            '<h2 id="gallery">The gallery</h2>'
            f"{paragraphs('Gallery', 14)}",
        ),
    ),
    ("wing-2.xhtml", xhtml("The east wing, continued", paragraphs("East wing continued", 14))),
    ("wing-3.xhtml", xhtml("The west wing", f"<h1>The west wing</h1>{paragraphs('West wing', 14)}")),
]
write_book(
    output.parent / "Two Wings.epub",
    "simpl-fixture-two-wings",
    "Two Wings",
    wings,
    [
        ("wing-1.xhtml", "The east wing"),
        ("wing-1.xhtml#hall", "The long hall"),
        ("wing-1.xhtml#gallery", "The gallery"),
        ("wing-3.xhtml", "The west wing"),
        ("wing-3.xhtml#missing", "A room that is not there"),
    ],
)
