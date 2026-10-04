package io.github.tikkaaa3.simpl

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

data class ReaderState(
    val loading: Boolean = true,
    val adapting: Boolean = false,
    val total: UInt = 0u,
    val page: UInt = 1u,
    val pages: List<PageContent> = emptyList(),
    val contents: List<ReaderContents> = emptyList(),
    val options: LayoutOptions = LayoutOptions(ReadingFont.THEME, 20u, 48u, 0u),
    val theme: String = "default",
    val paperAppearance: String = "auto",
    val themes: List<ReaderTheme> = emptyList(),
    val location: ReaderLocation? = null,
    val revision: Int = 0,
    val note: LinkDestination? = null,
    val canReturn: Boolean = false,
    val error: String? = null,
    val warnings: List<String> = emptyList(),
)

class ReaderViewModel(application: Application, private val saved: SavedStateHandle) : AndroidViewModel(application) {
    private val preferences = application.getSharedPreferences("reader", 0)
    private val mutable = MutableStateFlow(ReaderState(theme = preferences.getString("theme", "default")!!,
        paperAppearance = preferences.getString("paperAppearance", "auto")!!))
    val state = mutable.asStateFlow()
    private var source: OpenBook? = null
    private var adapted: AdaptedBook? = null
    private var opening: OpenBookTask? = null
    private var adaptation: AdaptBookTask? = null
    private var loadJob: Job? = null
    private var pageJob: Job? = null
    private var settingsJob: Job? = null
    private var saveJob: Job? = null
    private val writes = Mutex()
    private var path: String? = null
    private var current: ReaderLocation? = null
    private val history = ArrayDeque<ReaderLocation>()
    private var cleared = false

    fun open(book: LibraryBook) {
        if (path == book.path) return
        path = book.path
        loadJob = viewModelScope.launch {
            try {
                val task = withContext(Dispatchers.IO) { openBook(book.path) }
                opening = task
                while (withContext(Dispatchers.IO) { task.status() } == LayoutStatus.RUNNING) delay(16)
                val opened = withContext(Dispatchers.IO) { task.result() } ?: error("Opening was cancelled")
                source = opened
                val info = withContext(Dispatchers.IO) { opened.readerInfo() }
                val themes = withContext(Dispatchers.IO) { readerThemes() }
                val theme = mutable.value.theme.takeIf { id -> themes.any { it.id == id } } ?: "default"
                mutable.value = mutable.value.copy(total = info.total, options = info.options, themes = themes, theme = theme, warnings = info.warnings)
                val location = savedLocation()?.takeIf { it.page in 1u..info.total } ?: info.restored
                current = location
                adapt(info.options, theme, location)
                // Contents may load chapters; keep this work off the UI thread.
                mutable.value = mutable.value.copy(contents = withContext(Dispatchers.IO) { opened.contents() })
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { fail(error) }
            finally { opening?.cancel(); opening?.close(); opening = null }
        }
    }

    fun change(options: LayoutOptions = mutable.value.options, theme: String = mutable.value.theme) {
        settingsJob?.cancel()
        adaptation?.cancel()
        mutable.value = mutable.value.copy(adapting = true)
        settingsJob = viewModelScope.launch {
            try {
                // Serialize persistence with the forced onStop position write.
                val valid = withContext(Dispatchers.IO) { writes.withLock { source!!.saveOptions(options) } }
                preferences.edit().putString("theme", theme).apply()
                adapt(valid, theme, current)
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { fail(error) }
        }
    }

    private suspend fun adapt(options: LayoutOptions, theme: String, location: ReaderLocation?) {
        pageJob?.cancel()
        mutable.value = mutable.value.copy(adapting = true, error = null)
        val opened = source ?: return
        val task = withContext(Dispatchers.IO) { opened.adapt(theme, options) }
        adaptation = task
        try {
            while (withContext(Dispatchers.IO) { task.status() } == LayoutStatus.RUNNING) delay(16)
            val next = withContext(Dispatchers.IO) { task.result() } ?: error("Layout was cancelled")
            val page = (location?.page ?: mutable.value.page).coerceIn(1u, mutable.value.total.coerceAtLeast(1u))
            val content = withContext(Dispatchers.IO) { next.page(page) }
            adapted?.close()
            adapted = next
            mutable.value = mutable.value.copy(loading = false, adapting = false, options = options, theme = theme,
                page = page, pages = content, location = location, revision = mutable.value.revision + 1)
            current = location ?: firstLocation(content)
            current?.let(::record)
        } finally { task.cancel(); task.close(); if (adaptation === task) adaptation = null }
    }

    fun turn(delta: Int) {
        val page = (mutable.value.page.toLong() + delta).coerceIn(1, mutable.value.total.toLong().coerceAtLeast(1)).toUInt()
        if (page != mutable.value.page) show(page)
    }
    fun go(location: ReaderLocation) = show(location.page, location)
    fun jump(value: String, done: () -> Unit) = launch {
        val page = withContext(Dispatchers.IO) { source!!.jump(value) }
        show(page); done()
    }
    private fun show(page: UInt, location: ReaderLocation? = null) {
        if (mutable.value.loading || mutable.value.adapting) return
        flush()
        pageJob?.cancel()
        pageJob = launch {
            val content = withContext(Dispatchers.IO) { adapted!!.page(page) }
            mutable.value = mutable.value.copy(page = page, pages = content, location = location, note = null,
                revision = mutable.value.revision + 1, error = null)
            current = location ?: firstLocation(content)
            current?.let(::record)
        }
    }
    fun follow(section: UInt, link: BookLink) = launch {
        if (link.kind == BookLinkKind.BACKLINK && (history.isNotEmpty() || mutable.value.note != null)) { returnFromLink(); return@launch }
        val target = withContext(Dispatchers.IO) { source!!.followLink(section, link.href, mutable.value.theme, mutable.value.options) }
        if (target.location != null) {
            current?.let { if (history.size == 32) history.removeFirst(); history.addLast(it) }
            mutable.value = mutable.value.copy(canReturn = history.isNotEmpty())
            go(target.location!!)
        } else mutable.value = mutable.value.copy(note = target)
    }
    fun returnFromLink() {
        if (mutable.value.note != null) { dismissNote(); return }
        if (history.isNotEmpty()) { go(history.removeLast()); mutable.value = mutable.value.copy(canReturn = history.isNotEmpty()) }
    }
    fun dismissNote() { mutable.value = mutable.value.copy(note = null) }
    fun dismissError() { mutable.value = mutable.value.copy(error = null) }
    fun paperAppearance(value: String) {
        preferences.edit().putString("paperAppearance", value).apply()
        mutable.value = mutable.value.copy(paperAppearance = value)
    }

    fun record(location: ReaderLocation) {
        if (cleared) return
        current = location
        saved["page"] = location.page.toLong(); saved["section"] = location.section.toLong()
        saved["row"] = location.row.toLong(); saved["within"] = location.within
        saveJob?.cancel()
        saveJob = viewModelScope.launch { delay(350); persist(location) }
    }
    fun flush() {
        saveJob?.cancel()
        current?.let { location -> saveJob = viewModelScope.launch { persist(location) } }
    }
    // Lifecycle STOP is the final checkpoint before Android may kill the process.
    fun stop() {
        saveJob?.cancel()
        val opened = source ?: return
        val location = current ?: return
        val size = mutable.value.options.size
        runBlocking(Dispatchers.IO) {
            writes.withLock { runCatching { opened.saveLocation(location, size) }.onFailure { fail(it) } }
        }
    }
    private suspend fun persist(location: ReaderLocation) {
        val opened = source ?: return
        val size = mutable.value.options.size
        withContext(Dispatchers.IO) { writes.withLock { runCatching { opened.saveLocation(location, size) }.onFailure { fail(it) } } }
    }
    private fun savedLocation(): ReaderLocation? = saved.get<Long>("page")?.let {
        ReaderLocation(it.toUInt(), (saved.get<Long>("section") ?: 0).toUInt(), (saved.get<Long>("row") ?: 0).toUInt(), saved["within"] ?: 0f)
    }
    private fun firstLocation(pages: List<PageContent>): ReaderLocation? = pages.firstOrNull()?.let { fragment ->
        fragment.rows.firstOrNull()?.let { row ->
            val cut = fragment.layout.startCut
            ReaderLocation(fragment.layout.number, fragment.section, row.index,
                if (cut?.row == row.index) cut.line.toFloat() / cut.lines.coerceAtLeast(1u).toFloat() else 0f)
        }
    }
    suspend fun image(section: UInt, asset: String): ReaderImage? = withContext(Dispatchers.IO) { source?.image(section, asset) }
    private fun launch(block: suspend () -> Unit): Job = viewModelScope.launch {
        try { block() } catch (error: CancellationException) { throw error } catch (error: Exception) { fail(error) }
    }
    private fun fail(error: Throwable) {
        val message = if (error is CoreException.Failed) error.reason else error.message ?: "Could not open this page"
        mutable.value = mutable.value.copy(loading = false, adapting = false, error = message)
    }
    override fun onCleared() {
        stop(); cleared = true
        opening?.cancel(); adaptation?.cancel()
        adapted?.close(); source?.close()
    }
}
