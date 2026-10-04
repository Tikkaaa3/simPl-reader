package io.github.tikkaaa3.simpl

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import io.github.tikkaaa3.simpl.core.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject

enum class Appearance { System, Light, Dark }

data class LibraryState(
    val books: List<LibraryBook> = emptyList(),
    val shelves: List<LibraryShelf> = emptyList(),
    val query: String = "",
    val filter: String = "all",
    val appearance: Appearance = Appearance.System,
    val loading: Boolean = true,
    val importing: Int = 0,
    val error: String? = null,
    val notice: String? = null,
    val openRequest: String? = null,
)

class LibraryViewModel(application: Application, private val saved: SavedStateHandle) : AndroidViewModel(application) {
    private val preferences = application.getSharedPreferences("interface", 0)
    private val importer = DocumentImport(application)
    private val mutations = Mutex()
    private var importing = false
    private val mutable = MutableStateFlow(LibraryState(
        query = saved["query"] ?: "", filter = saved["filter"] ?: "all",
        appearance = runCatching { Appearance.valueOf(preferences.getString("appearance", "System")!!) }.getOrDefault(Appearance.System),
        openRequest = saved["openRequest"],
    ))
    val state = mutable.asStateFlow()

    init {
        val failure = (application as SimplApplication).coreFailure
        if (failure != null) mutable.value = mutable.value.copy(loading = false, error = "Cannot open the library: $failure")
        else {
            mutate { refresh() }
            drainImports()
        }
    }

    fun query(value: String) { saved["query"] = value; mutable.value = mutable.value.copy(query = value) }
    fun filter(value: String) { saved["filter"] = value; mutable.value = mutable.value.copy(filter = value) }
    fun appearance(value: Appearance) {
        preferences.edit().putString("appearance", value.name).apply()
        mutable.value = mutable.value.copy(appearance = value)
    }
    fun dismissMessage() { mutable.value = mutable.value.copy(error = null, notice = null) }
    fun navigated() { saved["openRequest"] = null; mutable.value = mutable.value.copy(openRequest = null) }
    fun reload() = mutate { refresh() }
    fun favourite(book: LibraryBook) = mutate { setLibraryFavourite(book.fingerprint, !book.favourite); refresh() }
    fun remove(book: LibraryBook) = mutate { removeLibraryBook(book.fingerprint); refresh() }
    fun createShelf(name: String) = mutate { createLibraryShelf(name); refresh() }
    fun renameShelf(id: ULong, name: String) = mutate { renameLibraryShelf(id, name); refresh() }
    fun deleteShelf(id: ULong) = mutate {
        deleteLibraryShelf(id); refresh()
        if (mutable.value.filter == "shelf:$id") withContext(Dispatchers.Main) { filter("all") }
    }
    fun toggleShelf(id: ULong, book: LibraryBook) = mutate { toggleLibraryShelf(id, book.fingerprint); refresh() }
    fun open(book: LibraryBook) = mutate {
        openLibraryBook(book.fingerprint); refresh()
        withContext(Dispatchers.Main) { requestOpen(book.fingerprint) }
    }

    private fun requestOpen(fingerprint: String) {
        saved["openRequest"] = fingerprint
        mutable.value = mutable.value.copy(openRequest = fingerprint)
    }

    fun enqueue(uris: List<Uri>, tree: Boolean = false, openAfter: Boolean = false) {
        if ((getApplication<Application>() as SimplApplication).coreFailure != null || uris.isEmpty()) return
        if (pending().size + uris.size > 64 || uris.any { it.toString().length > 4096 }) {
            mutable.value = mutable.value.copy(error = "Choose up to 64 documents at a time.")
            return
        }
        val queue = pending().toMutableList()
        for (uri in uris.distinct()) {
            queue += JSONObject().put("id", UUID.randomUUID().toString()).put("uri", uri.toString())
                .put("tree", tree).put("open", openAfter).toString()
        }
        saved["imports"] = ArrayList(queue)
        drainImports()
    }

    private fun pending(): List<String> = saved.get<ArrayList<String>>("imports") ?: emptyList()

    private fun drainImports() {
        if (importing || pending().isEmpty()) return
        importing = true
        viewModelScope.launch {
            var succeeded = 0
            try {
                while (pending().isNotEmpty()) {
                    mutable.value = mutable.value.copy(importing = pending().size)
                    val job = JSONObject(pending().first())
                    try {
                        val book = mutations.withLock {
                            withContext(Dispatchers.IO) {
                                importer.import(Uri.parse(job.getString("uri")), job.getBoolean("tree"), job.getString("id"))
                                    .also { if (job.getBoolean("open")) openLibraryBook(it.fingerprint); refresh() }
                            }
                        }
                        succeeded++
                        if (job.getBoolean("open")) requestOpen(book.fingerprint)
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Exception) {
                        mutable.value = mutable.value.copy(error = error.message ?: "Could not import this document.")
                    }
                    saved["imports"] = ArrayList(pending().drop(1))
                    // Keep grants only while a process-restorable copy is pending.
                    if (pending().none { JSONObject(it).getString("uri") == job.getString("uri") }) {
                        runCatching {
                            getApplication<Application>().contentResolver.releasePersistableUriPermission(
                                Uri.parse(job.getString("uri")), android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        }
                    }
                }
            } finally {
                importing = false
                mutable.value = mutable.value.copy(importing = 0,
                    notice = if (succeeded > 0) "$succeeded ${if (succeeded == 1) "book" else "books"} imported" else null)
            }
        }
    }

    private suspend fun refresh() {
        val snapshot = loadLibrary()
        withContext(Dispatchers.Main) {
            val filter = mutable.value.filter.takeUnless { value ->
                value.startsWith("shelf:") && snapshot.shelves.none { "shelf:${it.id}" == value }
            } ?: "all"
            saved["filter"] = filter
            mutable.value = mutable.value.copy(books = snapshot.books, shelves = snapshot.shelves, filter = filter, loading = false)
        }
    }

    private fun mutate(operation: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                mutations.withLock { withContext(Dispatchers.IO) { operation() } }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                mutable.value = mutable.value.copy(loading = false, error = error.message ?: "Could not update the library.")
            }
        }
    }
}
