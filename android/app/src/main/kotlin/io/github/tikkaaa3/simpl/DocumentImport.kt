package io.github.tikkaaa3.simpl

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import io.github.tikkaaa3.simpl.core.LibraryBook
import io.github.tikkaaa3.simpl.core.importLibraryBook
import java.io.File
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

/** Copies provider streams into private staging; Rust owns the permanent copy. */
class DocumentImport(private val context: Context) {
    companion object {
        const val MAX_BYTES = 512L * 1024 * 1024
        private const val MAX_RESOURCE_BYTES = 32L * 1024 * 1024
        private val resources = setOf("html", "htm", "xhtml", "png", "jpg", "jpeg", "gif", "webp", "svg", "css", "woff", "woff2", "ttf")
        private val formats = setOf("epub", "pdf", "html", "htm", "xhtml", "txt", "text", "md", "markdown")
    }

    private val resolver = context.contentResolver

    suspend fun import(uri: Uri, tree: Boolean, jobId: String): LibraryBook {
        require(uri.scheme == "content") { "Choose a document from a storage provider." }
        require(jobId.matches(Regex("[a-f0-9-]{36}"))) { "Invalid import request." }
        val stage = File(context.cacheDir, "imports/$jobId")
        // A restored request restarts its incomplete copy from the provider.
        stage.deleteRecursively()
        check(stage.mkdirs()) { "Cannot prepare the import." }
        return try {
            val source = if (tree) {
                val folder = File(stage, "HTML book").apply { check(mkdir()) }
                copyTree(uri, DocumentsContract.getTreeDocumentId(uri), folder, Budget(), 0)
                folder
            } else {
                val name = documentName(uri)
                require(name.substringAfterLast('.', "").lowercase() in formats) {
                    "Choose an EPUB, PDF, HTML, TXT or Markdown file."
                }
                File(stage, name).also { copyStream(uri, it, MAX_BYTES, Budget()) }
            }
            currentCoroutineContext().ensureActive()
            importLibraryBook(source.absolutePath)
        } finally {
            stage.deleteRecursively()
        }
    }

    private fun documentName(uri: Uri): String {
        val display = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst() && !it.isNull(0)) it.getString(0) else null
        } ?: "Imported book"
        var name = safeName(display)
        if (name.substringAfterLast('.', "").lowercase() !in formats) {
            val extension = when (resolver.getType(uri)?.lowercase()) {
                "application/epub+zip" -> "epub"
                "application/pdf" -> "pdf"
                "text/html", "application/xhtml+xml" -> "html"
                "text/plain" -> "txt"
                "text/markdown", "text/x-markdown" -> "md"
                else -> null
            }
            if (extension != null) name = "$name.$extension"
        }
        return name
    }

    private fun safeName(name: String): String {
        require(name.isNotBlank() && name != "." && name != ".." && name.length <= 200 &&
            name.none { it == '/' || it == '\\' || it.code < 32 }) { "The provider returned an invalid filename." }
        return name
    }

    private class Budget(var bytes: Long = 0, var files: Int = 0)

    private suspend fun copyStream(uri: Uri, file: File, limit: Long, budget: Budget) {
        resolver.openInputStream(uri)?.use { input ->
            file.outputStream().use { output ->
                val buffer = ByteArray(64 * 1024)
                var size = 0L
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val count = input.read(buffer)
                    if (count < 0) break
                    size += count
                    budget.bytes += count
                    require(size <= limit && budget.bytes <= MAX_BYTES) { "This document exceeds the import size limit." }
                    output.write(buffer, 0, count)
                }
            }
        } ?: error("Cannot read this document. Choose it again to grant access.")
    }

    private suspend fun copyTree(tree: Uri, id: String, folder: File, budget: Budget, depth: Int) {
        require(depth <= 32) { "The HTML folder is nested too deeply." }
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
        val columns = arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE)
        resolver.query(children, columns, null, null, null)?.use { cursor ->
            val names = mutableSetOf<String>()
            while (cursor.moveToNext()) {
                currentCoroutineContext().ensureActive()
                require(++budget.files <= 19000) { "The HTML folder contains too many files." }
                val name = safeName(cursor.getString(1))
                require(names.add(name.lowercase())) { "The HTML folder contains conflicting filenames." }
                val childId = cursor.getString(0)
                if (cursor.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR) {
                    val child = File(folder, name).apply { check(mkdir()) }
                    copyTree(tree, childId, child, budget, depth + 1)
                } else if (name.substringAfterLast('.', "").lowercase() in resources) {
                    copyStream(DocumentsContract.buildDocumentUriUsingTree(tree, childId),
                        File(folder, name), MAX_RESOURCE_BYTES, budget)
                }
            }
        } ?: error("Cannot read this folder. Choose it again to grant access.")
    }
}
