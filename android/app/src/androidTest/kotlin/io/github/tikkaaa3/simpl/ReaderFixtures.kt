package io.github.tikkaaa3.simpl

import java.io.ByteArrayOutputStream
import java.util.zip.CRC32
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

internal fun readerHtml(marker: String) = """
    <!doctype html><html><head><meta charset="utf-8"><title>M4 Reading Journey</title></head><body>
    <h1 id="start">M4 Reading Journey</h1>
    <p id="origin"><a href="#far">a later passage</a>. Café 🛶 İstanbul. <strong>Keep these words.</strong> Follow <a href="#note" role="doc-noteref">the note</a>.</p>
    <p id="long">${"The little boats returned before the evening lights, and everyone had a story to tell. ".repeat(500)}</p>
    <h2 id="far">A later passage</h2>
    <blockquote><p>A quiet quotation.</p></blockquote><ol start="3"><li>A numbered item.<ul><li>A nested thought.</li></ul></li></ol>
    <pre>val reader = "quiet"
        return reader</pre><table><tr><th>Book</th><th>Document</th></tr><tr><td>Words</td><td>Source</td></tr></table>
    <figure><img src="m4-illustration.png" alt="An evening harbour"/><figcaption>A harbour illustration.</figcaption></figure>
    <p dir="rtl">نص عربي مع English (123) ورابط. שלום עולם.</p>
    <aside id="note" role="doc-footnote"><p>The note preserves its source. <a href="#origin" role="doc-backlink">Return to the passage</a>.</p></aside>
    <p>$marker</p></body></html>
""".trimIndent()

internal fun readerIllustration(folder: java.io.File) {
    val image = android.graphics.Bitmap.createBitmap(240, 80, android.graphics.Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(image)
    canvas.drawColor(android.graphics.Color.rgb(31, 66, 95))
    val paint = android.graphics.Paint().apply { color = android.graphics.Color.rgb(169, 202, 214) }
    canvas.drawRect(0f, 55f, 240f, 80f, paint)
    paint.color = android.graphics.Color.rgb(237, 211, 153)
    canvas.drawCircle(195f, 22f, 10f, paint)
    java.io.File(folder, "m4-illustration.png").outputStream().use { image.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
    image.recycle()
}

internal fun readerEpub(identity: String = ""): ByteArray {
    val files = linkedMapOf(
        "META-INF/container.xml" to """<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>""",
        "OPS/book.opf" to """<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>M4 Source Pages</dc:title></metadata><manifest><item id="main" href="main.xhtml" media-type="application/xhtml+xml"/><item id="next" href="next.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="main"/><itemref idref="next"/></spine></package>""",
        "OPS/nav.xhtml" to """<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="main.xhtml#origin">Opening</a></li><li><a href="next.xhtml#dawn">Dawn</a></li></ol></nav><nav epub:type="page-list"><ol><li><a href="main.xhtml#origin">iv</a></li><li><a href="next.xhtml#dawn">42</a></li></ol></nav></body></html>""",
        "OPS/main.xhtml" to """<h1 id="origin">Opening</h1><p>Café 🛶 İstanbul. Read <a role="doc-noteref" href="notes.xhtml#note">the harbour note</a>, then return. <a href="next.xhtml#dawn">Follow the dawn</a>.</p>""",
        "OPS/next.xhtml" to """<h1 id="dawn">Dawn</h1><p>The boats returned in the morning.</p>""",
        "OPS/notes.xhtml" to """<aside id="note" role="doc-footnote"><h2>Harbour note</h2><p>A supplementary note outside the reading order. <a role="doc-backlink" href="main.xhtml#origin">Return to the opening</a>.</p></aside><aside id="other" role="doc-footnote"><p>This second note is separate.</p></aside>""",
    )
    if (identity.isNotEmpty()) files["OPS/book.opf"] = files.getValue("OPS/book.opf").replace("M4 Source Pages", "M4 Source Pages $identity")
    val bytes = ByteArrayOutputStream()
    ZipOutputStream(bytes).use { zip ->
        val mime = "application/epub+zip".toByteArray()
        zip.putNextEntry(ZipEntry("mimetype").apply { method = ZipEntry.STORED; size = mime.size.toLong(); crc = CRC32().apply { update(mime) }.value })
        zip.write(mime); zip.closeEntry()
        files.forEach { (name, text) -> zip.putNextEntry(ZipEntry(name)); zip.write(text.toByteArray()); zip.closeEntry() }
    }
    return bytes.toByteArray()
}
