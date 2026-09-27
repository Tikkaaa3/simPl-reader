"""Build the project-authored EPUB smoke fixture; no third-party packages needed."""
from pathlib import Path
import zipfile

source = Path(__file__).resolve().parent
output = source.parents[1] / "target" / "book-milestone2" / "structured.epub"
output.parent.mkdir(parents=True, exist_ok=True)
html = (source / "structured.html").read_text(encoding="utf-8")
package = """<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>A quieter page</dc:title><dc:creator>simPl fixture</dc:creator></metadata><manifest><item id="main" href="main.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/><item id="image" href="illustration.png" media-type="image/png"/></manifest><spine><itemref idref="main"/><itemref idref="notes" linear="no"/></spine></package>"""
with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    archive.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    archive.writestr("OPS/book.opf", package)
    archive.writestr("OPS/main.xhtml", html.replace('href="#note"', 'href="notes.xhtml#note"'))
    archive.writestr("OPS/notes.xhtml", '<aside id="note" role="doc-footnote"><h2>A note</h2><p>A supplementary note outside the linear spine. <a href="main.xhtml#origin" role="doc-backlink">Return to the book</a>.</p></aside>')
    archive.write(source / "illustration.png", "OPS/illustration.png")
print(output)
