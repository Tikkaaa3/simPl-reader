"""Build an authored EPUB for offline speech and empty-chapter regressions."""
from pathlib import Path
import zipfile

output = Path(__file__).resolve().parents[2] / "target" / "tts-qa.epub"
output.parent.mkdir(parents=True, exist_ok=True)
chapters = [
    '<img src="illustration.png" alt=""/>',
    "<h1>Bir</h1><p>Şimdi bu kısa bölümü Türkçe sesle okuyoruz.</p>",
    '<img src="illustration.png" alt=""/>',
    "<h1>Two</h1><p>We are reading this short chapter in English.</p>",
]
manifest = "".join(
    f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>'
    for i in range(len(chapters))
)
spine = "".join(f'<itemref idref="c{i}"/>' for i in range(len(chapters)))
with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    archive.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    archive.writestr("OPS/book.opf", f'<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Speech regression</dc:title></metadata><manifest>{manifest}<item id="image" href="illustration.png" media-type="image/png"/></manifest><spine>{spine}</spine></package>')
    archive.write(Path(__file__).resolve().parents[1] / "book-structure" / "illustration.png", "OPS/illustration.png")
    for index, content in enumerate(chapters):
        archive.writestr(f"OPS/c{index}.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter {index}</title></head><body>{content}</body></html>')
print(output)
