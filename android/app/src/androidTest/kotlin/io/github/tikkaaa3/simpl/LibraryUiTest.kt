package io.github.tikkaaa3.simpl

import android.content.ClipData
import android.content.Intent
import android.graphics.Bitmap
import android.provider.DocumentsContract
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class LibraryUiTest {
    @get:Rule val ui = createAndroidComposeRule<MainActivity>()
    private lateinit var book: LibraryBook
    private val added = mutableListOf<String>()
    private var baseline: Set<String> = emptySet()
    private var shelf: ULong? = null

    @Before fun seedLibrary() {
        grantFixtureDocuments()
        baseline = loadLibrary().books.map { it.fingerprint }.toSet()
        val file = File(ui.activity.cacheDir, "M3 library.html").apply {
            writeText("<title>M3 Library Journey</title><meta name=author content='Test Author'><p>${UUID.randomUUID()}</p>")
        }
        book = importLibraryBook(file.absolutePath)
        added += book.fingerprint
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].apply { appearance(Appearance.Light); query(""); filter("all"); reload() } }
        ui.waitUntil(10_000) { ui.onAllNodesWithText(book.title).fetchSemanticsNodes().isNotEmpty() }
    }

    @After fun cleanLibrary() {
        shelf?.let(::deleteLibraryShelf)
        added.distinct().filter { it !in baseline }.forEach { if (loadLibrary().books.any { book -> book.fingerprint == it }) removeLibraryBook(it) }
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].appearance(Appearance.System) }
    }

    @Test fun searchFavoritesShelvesAndDeleteConfirmation() {
        openLibraryBook(book.fingerprint)
        ui.activityRule.scenario.onActivity { ViewModelProvider(it)[LibraryViewModel::class.java].reload() }
        ui.waitUntil(10_000) { ui.onAllNodesWithText("CONTINUE").fetchSemanticsNodes().isNotEmpty() }
        // Removing Continue must not replace the focused text field after its first letter.
        ui.onNodeWithTag("search").performTextInput("T")
        ui.onNodeWithTag("search").assertIsFocused()
        ui.onNodeWithTag("search").performTextInput("est Author")
        ui.onNodeWithTag("book:${book.fingerprint}").assertExists()
        ui.onNodeWithContentDescription("Favorite ${book.title}").performClick()
        ui.waitUntil(10_000) { loadLibrary().books.first { it.fingerprint == book.fingerprint }.favourite }
        ui.onNodeWithText("Favorites").performClick()
        ui.onNodeWithTag("search").performTextClearance()
        ui.onNodeWithText("New shelf").performClick()
        ui.onNodeWithTag("shelfName").performTextInput("M3 Shelf")
        ui.onNodeWithText("Save").performClick()
        ui.waitUntil(10_000) { loadLibrary().shelves.any { it.name == "M3 Shelf" } }
        shelf = loadLibrary().shelves.first { it.name == "M3 Shelf" }.id
        ui.onNodeWithContentDescription("More options for ${book.title}").performScrollTo().performClick()
        ui.onNodeWithText("Add to shelves").performClick()
        ui.onNode(hasText("M3 Shelf") and hasAnyAncestor(isDialog())).performClick()
        ui.onNodeWithText("Done").performClick()
        ui.waitUntil(10_000) { book.fingerprint in loadLibrary().shelves.first { it.id == shelf }.books }
        ui.onNodeWithText("M3 Shelf").performClick()
        ui.onNodeWithTag("book:${book.fingerprint}").assertExists()
        // Both the selected shelf and query survive Activity recreation.
        ui.onNodeWithTag("search").performTextInput("Journey")
        ui.activityRule.scenario.recreate()
        ui.onNodeWithTag("search").assertTextContains("Journey")
        ui.onNodeWithTag("book:${book.fingerprint}").assertExists()
        ui.onNodeWithContentDescription("Settings").performClick()
        ui.onNodeWithContentDescription("Manage M3 Shelf").performClick()
        ui.onNodeWithText("Rename shelf").performClick()
        ui.onNodeWithTag("shelfName").performTextClearance()
        ui.onNodeWithTag("shelfName").performTextInput("M3 Renamed")
        ui.onNodeWithText("Save").performClick()
        ui.waitUntil(10_000) { loadLibrary().shelves.any { it.name == "M3 Renamed" } }
        ui.onNodeWithContentDescription("Manage M3 Renamed").performClick()
        ui.onNodeWithText("Delete shelf").performClick()
        ui.onNodeWithText("Cancel").performClick()
        assertTrue(loadLibrary().shelves.any { it.id == shelf })
        ui.onNodeWithContentDescription("Manage M3 Renamed").performClick()
        ui.onNodeWithText("Delete shelf").performClick()
        ui.onNodeWithText("Delete").performClick()
        ui.waitUntil(10_000) { loadLibrary().shelves.none { it.id == shelf } }
        shelf = null
        ui.onNodeWithContentDescription("Back").performClick()
        ui.onNodeWithContentDescription("More options for ${book.title}").performScrollTo().performClick()
        ui.onNodeWithText("Remove book").performClick()
        ui.onNodeWithText("Cancel").performClick()
        assertTrue(File(book.path).isFile)
        ui.onNodeWithContentDescription("More options for ${book.title}").performScrollTo().performClick()
        ui.onNodeWithText("Remove book").performClick()
        ui.onNodeWithText("Remove", useUnmergedTree = true).performClick()
        ui.waitUntil(10_000) { loadLibrary().books.none { it.fingerprint == book.fingerprint } }
        assertFalse(File(book.path).exists())
    }

    @Test fun readerSettingsNavigationAndThemeSurviveRecreation() {
        screenshot("library-light")
        ui.onNodeWithTag("open:${book.fingerprint}").performClick()
        ui.onNodeWithTag("reader").assertExists()
        ui.onNodeWithContentDescription("Settings").performClick()
        ui.onNodeWithText("Dark theme").performClick()
        screenshot("settings-dark")
        ui.activityRule.scenario.recreate()
        ui.onNodeWithTag("settings").assertExists()
        ui.onNodeWithContentDescription("Back").performClick()
        ui.onNodeWithTag("reader").assertExists()
        ui.onNodeWithContentDescription("Back to library").performClick()
        ui.onNodeWithText("CONTINUE").assertExists()
        screenshot("library-dark")
    }

    @Test fun openWithShareAndMultipleShareStreamsUseTheSameImporter() {
        val activity = ui.activity
        val launchIntent = Intent(activity.intent)
        try {
        val authority = "io.github.tikkaaa3.simpl.test.documents"
        val notes = DocumentsContract.buildDocumentUri(authority, "Notes.txt")
        ui.runOnUiThread {
            ui.activity.startActivity(Intent(Intent.ACTION_VIEW, notes, ui.activity, MainActivity::class.java)
                .setDataAndType(notes, "text/plain")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
        }
        ui.waitUntil(20_000) { ui.onAllNodesWithTag("reader").fetchSemanticsNodes().isNotEmpty() }
        added += loadLibrary().books.first { it.format == DocumentFormat.TEXT }.fingerprint
        ui.onNodeWithContentDescription("Back to library").performClick()
        val guide = DocumentsContract.buildDocumentUri(authority, "Guide.md")
        val pdf = DocumentsContract.buildDocumentUri(authority, "Tides.pdf")
        ui.runOnUiThread {
            ui.activity.startActivity(Intent(ui.activity, MainActivity::class.java).apply {
                action = Intent.ACTION_SEND_MULTIPLE; type = "*/*"
                putParcelableArrayListExtra(Intent.EXTRA_STREAM, arrayListOf(guide, pdf))
                clipData = ClipData.newRawUri("Guide", guide).apply { addItem(ClipData.Item(pdf)) }
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            })
        }
        ui.waitUntil(20_000) { loadLibrary().books.any { it.format == DocumentFormat.MARKDOWN } && loadLibrary().books.any { it.format == DocumentFormat.PDF } }
        added += loadLibrary().books.filter { it.format == DocumentFormat.MARKDOWN || it.format == DocumentFormat.PDF }.map { it.fingerprint }
        val epub = DocumentsContract.buildDocumentUri(authority, "Harbour.epub")
        ui.runOnUiThread {
            ui.activity.startActivity(Intent(ui.activity, MainActivity::class.java).apply {
                action = Intent.ACTION_SEND; type = "application/epub+zip"
                putExtra(Intent.EXTRA_STREAM, epub); addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            })
        }
        ui.waitUntil(20_000) { ui.onAllNodesWithTag("reader").fetchSemanticsNodes().isNotEmpty() }
        added += loadLibrary().books.first { it.format == DocumentFormat.EPUB }.fingerprint
        ui.onAllNodesWithText("Harbour Lights")[0].assertExists()
        } finally {
            // ActivityScenario identifies its activity by the original intent.
            ui.runOnUiThread { activity.intent = launchIntent }
        }
    }

    private fun screenshot(name: String) {
        ui.waitForIdle()
        val directory = File(ui.activity.getExternalFilesDir(null), "m3-ui").apply { mkdirs() }
        File(directory, "$name.png").outputStream().use {
            ui.onRoot().captureToImage().asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}
