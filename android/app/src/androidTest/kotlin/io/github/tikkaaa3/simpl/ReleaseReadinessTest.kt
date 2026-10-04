package io.github.tikkaaa3.simpl

import androidx.activity.compose.setContent
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.unit.Density
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.test.ext.junit.runners.AndroidJUnit4
import android.provider.DocumentsContract
import android.system.ErrnoException
import android.system.OsConstants
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReleaseReadinessTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()

    @Test fun allFiveFormatsReachTheReaderAndReopenSavedPositions() = runBlocking {
        grantFixtureDocuments()
        val baseline = loadLibrary().books.map { it.fingerprint }.toSet()
        val added = mutableListOf<LibraryBook>()
        try {
            for (name in listOf("Notes.txt", "Guide.md", "chapter.html", "Harbour.epub", "Tides.pdf")) {
                val book = DocumentImport(ui.activity).import(DocumentsContract.buildDocumentUri(
                    "io.github.tikkaaa3.simpl.test.documents", name), false, UUID.randomUUID().toString())
                added += book
                repeat(2) {
                    ui.activityRule.scenario.onActivity { activity ->
                        ViewModelProvider(activity)[LibraryViewModel::class.java].open(book)
                    }
                    ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().isNotEmpty() }
                    if (book.format == DocumentFormat.PDF) ui.waitUntil(30_000) {
                        ui.onAllNodesWithText("Select page text").fetchSemanticsNodes().any {
                            !it.config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled)
                        }
                    }
                    ui.onNodeWithTag("pageLabel").assertTextContains("Page 1", substring = true)
                    ui.onNodeWithContentDescription("Back to library").performClick()
                }
                assertTrue(loadLibrary().books.first { it.fingerprint == book.fingerprint }.total > 0u)
            }
        } finally { added.filter { it.fingerprint !in baseline }.forEach { removeLibraryBook(it.fingerprint) } }
    }

    @Test fun licenseAssetsIncludeEveryRuntimeAndFullTexts() {
        val assets = ui.activity.assets
        val entries = JSONArray(assets.open("licenses/index.json").bufferedReader().use { it.readText() })
        val categories = mutableSetOf<String>()
        repeat(entries.length()) { index ->
            val entry = entries.getJSONObject(index)
            categories += entry.getString("category")
            assets.open(entry.getString("path")).use { assertTrue(it.read() >= 0) }
        }
        assertEquals(setOf("simPl", "Rust", "PDFium", "Fonts", "Android runtime"), categories)
        assertTrue(entries.toString().contains("core-splashscreen"))
        assertTrue(entries.toString().contains("profileinstaller"))
    }

    @Test fun settingsAndLicenseNavigationRemainUsableAtLargeFontScale() {
        largeFonts()
        ui.onNodeWithContentDescription("Settings").performClick()
        ui.onNodeWithTag("settingsList").performScrollToNode(hasText("Licenses"))
        ui.onNodeWithText("Licenses").performClick()
        ui.onNodeWithTag("licenses").assertExists()
        ui.onNodeWithTag("licenseSearch").performTextInput("Literata-OFL")
        val back = ui.onNodeWithContentDescription("Back").fetchSemanticsNode().boundsInRoot
        val pixels = 48f * ui.activity.resources.displayMetrics.density
        assertTrue("Back target must be at least 48 dp: $back, minimum $pixels px", back.width >= pixels - 1 && back.height >= pixels - 1)
        ui.onNodeWithText("fonts · Literata-OFL.txt").performClick()
        ui.waitUntil(10_000) { ui.onAllNodesWithText("SIL OPEN FONT LICENSE", substring = true).fetchSemanticsNodes().isNotEmpty() }
        ui.onNodeWithTag("licenseText").assertIsDisplayed()
        screenshot("licenses-large-font")
    }

    @Test fun readerControlsRemainVisibleAtLargeFontScale() {
        val source = File(ui.activity.cacheDir, "M7 large font.html").apply {
            writeText("<title>M7 Large Font</title>" + (0..100).joinToString("") { "<p>Paragraph $it ${UUID.randomUUID()} The lighthouse keeper watches the tide from the harbour.</p>" })
        }
        val book = importLibraryBook(source.absolutePath)
        try {
            largeFonts()
            ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].open(book) }
            ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().isNotEmpty() }
            ui.onNodeWithText("Contents").assertIsDisplayed()
            ui.onNodeWithText("Reading options").assertIsDisplayed()
            ui.onNodeWithText("Fit width").assertIsDisplayed()
            ui.onNodeWithText("Next").assertIsDisplayed().performClick()
            ui.waitUntil(10_000) { ui.onAllNodesWithText("Page 2", substring = true).fetchSemanticsNodes().isNotEmpty() }
            ui.onNodeWithTag("pageLabel").assertIsDisplayed()
            screenshot("reader-large-font")
            ui.onNodeWithContentDescription("Back to library").performClick()
        } finally { removeLibraryBook(book.fingerprint); source.delete() }
    }

    private fun largeFonts() {
        ui.runOnUiThread {
            val model = ViewModelProvider(ui.activity)[LibraryViewModel::class.java]
            ui.activity.setContent {
                val state by model.state.collectAsStateWithLifecycle()
                CompositionLocalProvider(LocalDensity provides Density(ui.activity.resources.displayMetrics.density, 1.8f)) {
                    SimplTheme(state.appearance) { SimplApp(state, model) }
                }
            }
        }
    }

    private fun screenshot(name: String) {
        ui.waitForIdle()
        if (android.os.Build.VERSION.SDK_INT < 29) return
        val uri = requireNotNull(ui.activity.contentResolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
            android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, "$name.png")
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "image/png")
                put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, "Pictures/simPl-M7")
            }))
        ui.activity.contentResolver.openOutputStream(uri)!!.use {
            ui.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    @Test fun corruptImportsCleanStagingAndKeepTheCatalog() = runBlocking {
        grantFixtureDocuments()
        val before = loadLibrary().books.map { it.fingerprint }
        val id = UUID.randomUUID().toString()
        val error = runCatching {
            DocumentImport(ui.activity).import(DocumentsContract.buildDocumentUri(
                "io.github.tikkaaa3.simpl.test.documents", "bad.epub"), false, id)
        }.exceptionOrNull()
        assertNotNull(error)
        assertEquals("This document is damaged or cannot be read. Try another copy of the file.", userError(error!!, FailureAction.Import))
        assertEquals(before, loadLibrary().books.map { it.fingerprint })
        assertFalse(File(ui.activity.cacheDir, "imports/$id").exists())
        assertTrue(userError(IllegalArgumentException("Choose an EPUB, PDF, HTML, TXT or Markdown file."), FailureAction.Import).contains("not supported"))
    }

    @Test fun diskFullAndSaveFailuresHaveRecoveryActions() {
        val diskFull = IOException("write failed", ErrnoException("write", OsConstants.ENOSPC))
        assertTrue(userError(diskFull, FailureAction.Save).startsWith("Storage is full."))
        assertTrue(userError(CoreException.Failed("No space left on device (os error 28)"), FailureAction.Import).contains("Free some space"))
        assertTrue(userError(IOException("read-only destination"), FailureAction.Save).contains("Could not save"))
        assertTrue(userError(SecurityException("revoked"), FailureAction.Import).contains("grant access"))
    }

    @Test fun failedAnnotationSaveIsVisibleAndDoesNotReportSuccess() {
        val source = File(ui.activity.cacheDir, "M7 save failure.html").apply {
            writeText("<title>M7 Save Failure</title><p>${UUID.randomUUID()} A release failure fixture.</p>")
        }
        val book = importLibraryBook(source.absolutePath)
        val path = File(ui.activity.filesDir, "simPl/annotations/${book.fingerprint}.json")
        try {
            ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { query(""); filter("all"); reload() } }
            ui.waitUntil(10_000) { ui.onAllNodesWithTag("open:${book.fingerprint}").fetchSemanticsNodes().isNotEmpty() }
            ui.onNodeWithTag("open:${book.fingerprint}").performClick()
            ui.waitUntil(30_000) { ui.onAllNodesWithTag("pageLabel").fetchSemanticsNodes().isNotEmpty() }
            path.parentFile!!.mkdirs()
            assertTrue(path.mkdir())
            ui.onNodeWithContentDescription("Bookmark page").performClick()
            ui.waitUntil(10_000) { ui.onAllNodesWithText("Could not save your changes.", substring = true).fetchSemanticsNodes().isNotEmpty() }
            assertTrue(path.isDirectory)
            ui.onNodeWithText("OK").performClick()
            assertTrue(path.delete())
            assertTrue(loadAnnotations(book.fingerprint).bookmarks.isEmpty())
            ui.onNodeWithContentDescription("Bookmark page").performClick()
            ui.waitUntil(10_000) { loadAnnotations(book.fingerprint).bookmarks.size == 1 }
            ui.onNodeWithContentDescription("Back to library").performClick()
        } finally {
            path.delete()
            removeLibraryBook(book.fingerprint)
            source.delete()
        }
    }
}
