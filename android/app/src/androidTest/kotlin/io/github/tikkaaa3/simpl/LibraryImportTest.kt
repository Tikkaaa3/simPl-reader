package io.github.tikkaaa3.simpl

import android.provider.DocumentsContract
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModelStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.Before
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class LibraryImportTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val importer = DocumentImport(context)
    private val authority = "io.github.tikkaaa3.simpl.test.documents"
    private fun document(name: String) = DocumentsContract.buildDocumentUri(authority, name)
    private var baseline: Set<String> = emptySet()
    @Before fun grantDocuments() { grantFixtureDocuments(); baseline = loadLibrary().books.map { it.fingerprint }.toSet() }

    @Test fun providerStreamsImportEveryFormatAndGeneratePdfCover() = runBlocking {
        val imported = mutableListOf<LibraryBook>()
        try {
            for ((name, format) in listOf("Notes.txt" to DocumentFormat.TEXT, "Guide.md" to DocumentFormat.MARKDOWN,
                "Harbour.epub" to DocumentFormat.EPUB, "Tides.pdf" to DocumentFormat.PDF, "chapter.html" to DocumentFormat.HTML)) {
                val id = UUID.randomUUID().toString()
                val book = importer.import(document(name), false, id).also(imported::add)
                assertEquals(format, book.format)
                assertTrue(File(book.path).isFile)
                assertFalse(File(context.cacheDir, "imports/$id").exists())
                assertTrue(loadLibrary().books.any { it.fingerprint == book.fingerprint })
            }
            val pdf = imported.first { it.format == DocumentFormat.PDF }
            assertTrue(pdf.cover)
            val cover = requireNotNull(libraryCover(pdf.fingerprint))
            assertTrue(cover.width in 1u..240u && cover.height in 1u..360u)
            assertEquals((cover.width * cover.height * 4u).toInt(), cover.rgba.size)
        } finally { imported.filter { it.fingerprint !in baseline }.forEach { removeLibraryBook(it.fingerprint) } }
    }

    @Test fun htmlTreeKeepsImagesAndInvalidImportLeavesCatalogUnchanged() = runBlocking {
        val tree = DocumentsContract.buildTreeDocumentUri(authority, "root")
        val book = importer.import(tree, true, UUID.randomUUID().toString())
        try {
            assertEquals(DocumentFormat.HTML, book.format)
            assertTrue(book.path.endsWith(".epub"))
            // A bundled HTML image is still available inside the EPUB chapter.
            openBook(book.path).use { task ->
                val deadline = android.os.SystemClock.uptimeMillis() + 30_000
                while (task.status() == LayoutStatus.RUNNING) {
                    assertTrue(android.os.SystemClock.uptimeMillis() < deadline); Thread.sleep(10)
                }
                assertEquals(LayoutStatus.COMPLETE, task.status())
                requireNotNull(task.result()).use { opened ->
                    assertTrue(opened.page(1u).flatMap { it.rows }.any { it.imageAsset != null })
                }
            }
            val before = loadLibrary().books.map { it.fingerprint }
            val id = UUID.randomUUID().toString()
            try { importer.import(document("bad.epub"), false, id); fail("Corrupt EPUB must fail") } catch (_: CoreException.Failed) { }
            assertEquals(before, loadLibrary().books.map { it.fingerprint })
            assertFalse(File(context.cacheDir, "imports/$id").exists())
        } finally { if (book.fingerprint !in baseline) removeLibraryBook(book.fingerprint) }
    }

    @Test fun savedHandleRoundTripRestoresSearchFilterAndPendingImports() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        lateinit var model: LibraryViewModel
        val store = ViewModelStore()
        instrumentation.runOnMainSync {
            val original = SavedStateHandle(mapOf("query" to "Harbour", "filter" to "favorites", "openRequest" to "a".repeat(64),
                "imports" to arrayListOf("{\"id\":\"${UUID.randomUUID()}\",\"uri\":\"${document("Notes.txt")}\",\"tree\":false,\"open\":false}")))
            val restored = SavedStateHandle.createHandle(original.savedStateProvider().saveState(), null)
            assertEquals("Harbour", restored.get<String>("query"))
            assertEquals("favorites", restored.get<String>("filter"))
            assertEquals("a".repeat(64), restored.get<String>("openRequest"))
            assertEquals(1, restored.get<ArrayList<String>>("imports")!!.size)
            model = LibraryViewModel(context.applicationContext as SimplApplication, restored)
            store.put("restored", model)
        }
        try {
            val deadline = android.os.SystemClock.uptimeMillis() + 20_000
            while (loadLibrary().books.none { it.format == DocumentFormat.TEXT }) {
                assertTrue("Restored import did not finish", android.os.SystemClock.uptimeMillis() < deadline)
                Thread.sleep(20)
            }
            assertEquals("Harbour", model.state.value.query)
            assertEquals("favorites", model.state.value.filter)
            assertNull(model.state.value.error)
        } finally {
            instrumentation.runOnMainSync { store.clear() }
            loadLibrary().books.filter { it.format == DocumentFormat.TEXT && it.fingerprint !in baseline }.forEach { removeLibraryBook(it.fingerprint) }
        }
    }
}
