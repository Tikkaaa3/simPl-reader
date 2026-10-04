package io.github.tikkaaa3.simpl

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.CoreException
import io.github.tikkaaa3.simpl.core.DocumentFormat
import io.github.tikkaaa3.simpl.core.LayoutOptions
import io.github.tikkaaa3.simpl.core.LayoutStatus
import io.github.tikkaaa3.simpl.core.ReadingFont
import io.github.tikkaaa3.simpl.core.atlas
import io.github.tikkaaa3.simpl.core.buildInfo
import io.github.tikkaaa3.simpl.core.importDocument
import io.github.tikkaaa3.simpl.core.inspectDocument
import io.github.tikkaaa3.simpl.core.openBook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.File
import java.security.MessageDigest
import java.util.zip.CRC32
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/** The packaged Rust core and PDFium open each format inside the app process. */
@RunWith(AndroidJUnit4::class)
class CoreSmokeTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val app = context.applicationContext as SimplApplication
    private val scratch = File(context.cacheDir, "core-smoke").apply { mkdirs() }

    @Test
    fun coreStartsWithTheAppProfile() {
        assertNull(app.coreFailure)
        assertEquals("android", buildInfo().os)
    }

    @Test
    fun epubChaptersAndFingerprint() {
        val file = File(scratch, "harbour.epub").apply { writeBytes(sampleEpub()) }
        val summary = inspectDocument(file.absolutePath)
        assertEquals(DocumentFormat.EPUB, summary.format)
        assertEquals("Harbour Lights", summary.title)
        assertEquals("simPl test", summary.author)
        assertEquals(2u, summary.chapters)
        assertEquals(sha256(file), summary.fingerprint)
    }

    @Test
    fun pdfPagesThroughPackagedPdfium() {
        val file = File(scratch, "tides.pdf").apply { writeBytes(samplePdf(listOf("High tide", "Low tide"))) }
        val summary = inspectDocument(file.absolutePath)
        assertEquals(DocumentFormat.PDF, summary.format)
        assertEquals(2u, summary.sourcePages)
        assertEquals(sha256(file), summary.fingerprint)
    }

    @Test
    fun importedTextBecomesAManagedTextDocument() {
        val file = File(scratch, "Notes.txt").apply { writeText("A short note.\n\nSecond paragraph.\n") }
        val managed = File(importDocument(file.absolutePath))
        assertTrue(managed.canonicalPath.startsWith(File(context.filesDir, "simPl/documents").canonicalPath))
        val summary = inspectDocument(managed.absolutePath)
        assertEquals(DocumentFormat.TEXT, summary.format)
        assertEquals(1u, summary.chapters)
    }

    @Test
    fun unsupportedDocumentsAreReportedNotCrashed() {
        try {
            inspectDocument(File(scratch, "missing.epub").absolutePath)
            fail("a missing EPUB must be an error")
        } catch (error: CoreException.Failed) {
            assertTrue(error.reason, error.reason.isNotBlank())
        }
    }

    @Test
    fun pageLayoutAndAdaptationThroughKotlinBindings() {
        val file = File(scratch, "long-chapter.html").apply {
            writeText("<h1>Harbour</h1><p>" + "The boats came in before the evening lights. ".repeat(300) + "</p>")
        }
        openBook(file.absolutePath).use { task ->
            awaitLayout(task::status)
            requireNotNull(task.result()).use { book ->
                val canonical = book.atlas()
                assertTrue(canonical.total > 1u)
                assertEquals(canonical.total, atlas(book.fingerprint()).total)
                val first = book.page(1u).first()
                assertEquals("Harbour", first.rows.first().text)
                assertTrue(canonical.sections.flatMap { it.pages }.any { it.endCut != null })
                assertTrue(File(context.cacheDir, "simPl/page-maps/v1-${book.fingerprint()}.json").isFile)
                book.adapt("soft", LayoutOptions(ReadingFont.FIRA_SANS, 26u, 64u, 180u)).use { adaptation ->
                    awaitLayout(adaptation::status)
                    requireNotNull(adaptation.result()).use { adapted ->
                        val layout = adapted.atlas()
                        assertEquals(canonical.total, layout.total)
                        assertEquals(
                            canonical.sections.flatMap { it.pages }.map { it.number to it.label },
                            layout.sections.flatMap { it.pages }.map { it.number to it.label },
                        )
                        assertEquals(first.rows.first().id, adapted.page(1u).first().rows.first().id)
                    }
                }
            }
        }
    }

    @Test
    fun pdfBookAtlasPreservesPhysicalPages() {
        val file = File(scratch, "book-tides.pdf").apply { writeBytes(samplePdf(listOf("High tide", "Low tide"))) }
        openBook(file.absolutePath).use { task ->
            awaitLayout(task::status)
            requireNotNull(task.result()).use { book ->
                val atlas = book.atlas()
                assertEquals(2u, atlas.total)
                assertTrue(atlas.sourcePages)
                assertEquals(2u, book.page(2u).first().layout.number)
            }
        }
    }

    private fun awaitLayout(status: () -> LayoutStatus) {
        val deadline = android.os.SystemClock.uptimeMillis() + 30_000
        while (status() == LayoutStatus.RUNNING) {
            assertTrue("layout worker timed out", android.os.SystemClock.uptimeMillis() < deadline)
            Thread.sleep(10)
        }
        assertEquals(LayoutStatus.COMPLETE, status())
    }

    private fun sha256(file: File): String =
        MessageDigest.getInstance("SHA-256").digest(file.readBytes()).joinToString("") { "%02x".format(it) }
}

/** A two-chapter EPUB 3 package; `mimetype` comes first and uncompressed. */
private fun sampleEpub(): ByteArray {
    val chapter = { title: String, text: String ->
        """<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>$title</title></head><body><h1>$title</h1><p>$text</p></body></html>"""
    }
    val files = linkedMapOf(
        "META-INF/container.xml" to
            """<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>""",
        "OPS/book.opf" to
            """<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Harbour Lights</dc:title><dc:creator>simPl test</dc:creator></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/></spine></package>""",
        "OPS/one.xhtml" to chapter("Evening", "The lamps were lit along the quay."),
        "OPS/two.xhtml" to chapter("Morning", "The boats went out before dawn."),
    )
    val bytes = ByteArrayOutputStream()
    ZipOutputStream(bytes).use { zip ->
        val mimetype = "application/epub+zip".toByteArray()
        zip.putNextEntry(
            ZipEntry("mimetype").apply {
                method = ZipEntry.STORED
                size = mimetype.size.toLong()
                crc = CRC32().apply { update(mimetype) }.value
            },
        )
        zip.write(mimetype)
        zip.closeEntry()
        for ((name, text) in files) {
            zip.putNextEntry(ZipEntry(name))
            zip.write(text.toByteArray())
            zip.closeEntry()
        }
    }
    return bytes.toByteArray()
}

/** One Helvetica line per page with a correct cross-reference table. */
private fun samplePdf(pages: List<String>): ByteArray {
    val objects = mutableListOf(
        "<</Type/Catalog/Pages 2 0 R>>",
        "<</Type/Pages/Kids[${pages.indices.joinToString(" ") { "${4 + it * 2} 0 R" }}]/Count ${pages.size}>>",
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>",
    )
    pages.forEachIndexed { index, text ->
        val content = "BT /F1 18 Tf 20 150 Td ($text) Tj ET"
        objects += "<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents ${5 + index * 2} 0 R/Resources<</Font<</F1 3 0 R>>>>>>"
        objects += "<</Length ${content.length}>>\nstream\n$content\nendstream"
    }
    val pdf = StringBuilder("%PDF-1.4\n")
    val offsets = objects.mapIndexed { index, body ->
        pdf.length.also { pdf.append("${index + 1} 0 obj\n$body\nendobj\n") }
    }
    val xref = pdf.length
    pdf.append("xref\n0 ${objects.size + 1}\n0000000000 65535 f \n")
    offsets.forEach { pdf.append("%010d 00000 n \n".format(it)) }
    pdf.append("trailer\n<</Root 1 0 R/Size ${objects.size + 1}>>\nstartxref\n$xref\n%%EOF\n")
    return pdf.toString().toByteArray(Charsets.US_ASCII)
}
