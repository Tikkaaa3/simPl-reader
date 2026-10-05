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
    val searchPoint: SourcePoint? = null,
    val preparation: BookPreparation? = null,
    val openMs: Long = 0,
    val spoken: SpeechRange? = null,
    val speechRow: BookRow? = null,
    val selection: ReflowSelection? = null,
    val selectionEdge: Boolean? = null,
    val annotations: AnnotationCollection = emptyAnnotations,
    val marks: List<ReaderMark> = emptyList(),
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
    private var fingerprint: String = ""
    private var selectionJob: Job? = null
    private var path: String? = null
    private var current: ReaderLocation? = null
    private val history = ArrayDeque<ReaderLocation>()
    private var cleared = false
    private var speechStart: Job? = null
    private var speechRowKey: Pair<UInt, UInt>? = null
    private var speechSheetsKey: Triple<UInt, UInt, UInt>? = null
    private var speechSheets: List<SpeechSheet> = emptyList()
    private var title = ""
    private var requestedTheme: String? = null

    init {
        // A theme chosen in Settings while this book stays open re-themes it on return.
        viewModelScope.launch {
            ReadingThemes.selected.collect { id -> if (id != requestedTheme && id != mutable.value.theme && source != null) change(theme = id) }
        }
        viewModelScope.launch {
            ReadAloud.state.collect { speech ->
                val range = speech.range.takeIf { speech.active && !speech.passage && speech.fingerprint == fingerprint }
                mutable.value = mutable.value.copy(spoken = range)
                val point = range?.from ?: speech.chunk?.source.takeIf { speech.active && !speech.passage && speech.fingerprint == fingerprint }
                if (point != null && source != null && speechRowKey != (point.section to point.row)) {
                    speechRowKey = point.section to point.row
                    mutable.value = mutable.value.copy(speechRow = null)
                    try {
                        val row = withContext(Dispatchers.IO) { source!!.speechRow(point, mutable.value.theme, mutable.value.options) }
                        mutable.value = mutable.value.copy(speechRow = row)
                    } catch (error: CancellationException) { throw error }
                    catch (error: Exception) { speechRowKey = null; fail(error) }
                }
            }
        }
    }
    fun readAloud() {
        speechStart?.cancel()
        speechStart = launch {
            val location = current ?: firstLocation(mutable.value.pages) ?: return@launch
            val row = mutable.value.pages.firstOrNull { it.section == location.section }?.rows?.firstOrNull { it.index == location.row }
            val text = row?.text.orEmpty()
            val at = sourceByte(text, (text.length * location.within).toInt())
            val plan = withContext(Dispatchers.IO) { source!!.speechPlan(SourcePoint(location.section, location.row, at)) }
            ReadAloud.start(getApplication(), fingerprint, title, plan)
        }
    }
    fun readSelection() = copy(::readPassage)
    fun readPassage(text: String) = launch {
        val plan = withContext(Dispatchers.IO) { speechPassage(text) }
        ReadAloud.start(getApplication(), fingerprint, title, plan, passage = true)
    }
    suspend fun speechFollow(point: SourcePoint, line: UInt, lines: UInt) {
        try {
            if (mutable.value.loading || mutable.value.adapting) return
            val key = Triple(point.section, point.row, lines)
            if (speechSheetsKey != key) {
                speechSheets = withContext(Dispatchers.IO) { adapted?.speechSheets(point, lines).orEmpty() }
                speechSheetsKey = key
            }
            val page = speechFollowPage(line, lines, speechSheets) ?: return
            if (mutable.value.spoken?.from != point) return
            if (page != mutable.value.page) show(page, ReaderLocation(page, point.section, point.row, line.toFloat() / lines.coerceAtLeast(1u).toFloat()), speech = true)
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) { fail(error) }
    }
    fun open(book: LibraryBook) {
        if (path == book.path) return
        if (ReadAloud.state.value.active && ReadAloud.state.value.fingerprint != book.fingerprint) ReadAloud.stop()
        title = book.title
        path = book.path
        fingerprint = book.fingerprint
        loadJob = viewModelScope.launch {
            val started = android.os.SystemClock.elapsedRealtime()
            var localTask: OpenBookTask? = null
            try {
                val task = withContext(Dispatchers.IO) { openBook(book.path) }
                localTask = task
                opening = task
                while (withContext(Dispatchers.IO) { task.status() } == LayoutStatus.RUNNING) {
                    mutable.value = mutable.value.copy(preparation = withContext(Dispatchers.IO) { task.progress() }); delay(50)
                }
                val opened = withContext(Dispatchers.IO) { task.result() } ?: error("Opening was cancelled")
                source = opened
                mutable.value = mutable.value.copy(preparation = withContext(Dispatchers.IO) { task.progress() }, openMs = android.os.SystemClock.elapsedRealtime() - started)
                mutable.value = mutable.value.copy(selection = saved.get<LongArray>("selection")?.takeIf { it.size == 6 }?.let {
                    ReflowSelection(SourcePoint(it[0].toUInt(), it[1].toUInt(), it[2].toUInt()), SourcePoint(it[3].toUInt(), it[4].toUInt(), it[5].toUInt()))
                })
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
            finally { localTask?.cancel(); localTask?.close(); if (opening === localTask) opening = null }
        }
    }

    fun cancelOpening() {
        opening?.cancel(); loadJob?.cancel(); path = null
        adaptation?.cancel(); adapted?.close(); adapted = null; source?.close(); source = null
        mutable.value = mutable.value.copy(loading = false, adapting = false, error = "Book preparation cancelled. Return to Document or retry.")
    }

    fun change(options: LayoutOptions = mutable.value.options, theme: String = mutable.value.theme) {
        requestedTheme = theme
        settingsJob?.cancel()
        adaptation?.cancel()
        mutable.value = mutable.value.copy(adapting = true)
        settingsJob = viewModelScope.launch {
            try {
                // Serialize persistence with the forced onStop position write.
                val valid = withContext(Dispatchers.IO) { writes.withLock { source!!.saveOptions(options) } }
                ReadingThemes.select(getApplication(), theme)
                adapt(valid, theme, current)
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { fail(error, FailureAction.Save) }
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
            speechSheetsKey = null; speechRowKey = null
            mutable.value = mutable.value.copy(loading = false, adapting = false, options = options, theme = theme,
                page = page, pages = content, location = location, revision = mutable.value.revision + 1)
            refreshAnnotations()
            current = location ?: firstLocation(content)
            current?.let(::record)
            val speech = ReadAloud.state.value
            val point = speech.range?.from ?: speech.chunk?.source
            if (speech.active && !speech.passage && speech.fingerprint == fingerprint && point != null) {
                val row = withContext(Dispatchers.IO) { opened.speechRow(point, theme, options) }
                speechRowKey = point.section to point.row
                mutable.value = mutable.value.copy(spoken = speech.range, speechRow = row)
            }
        } finally { task.cancel(); task.close(); if (adaptation === task) adaptation = null }
    }

    fun turn(delta: Int) {
        val page = (mutable.value.page.toLong() + delta).coerceIn(1, mutable.value.total.toLong().coerceAtLeast(1)).toUInt()
        if (page != mutable.value.page) show(page)
    }
    fun selectionTurn(delta: Int, start: Boolean) {
        val page = (mutable.value.page.toLong() + delta).coerceIn(1, mutable.value.total.toLong().coerceAtLeast(1)).toUInt()
        if (page != mutable.value.page) show(page, edge = start)
    }
    fun completeSelectionEdge(point: SourcePoint) {
        val start = mutable.value.selectionEdge ?: return
        extend(point, start)
        mutable.value = mutable.value.copy(selectionEdge = null)
    }
    fun go(location: ReaderLocation) = show(location.page, location)
    fun find(query: String): FindTask = checkNotNull(source).find(query)
    fun searchHit(hit: SearchHit) { hit.location?.let { select(hit.reflow); mutable.value = mutable.value.copy(searchPoint = hit.reflow?.from); go(it) } }
    fun chapter(delta: Int) = launch {
        val section = mutable.value.location?.section ?: mutable.value.pages.firstOrNull()?.section ?: 0u
        go(withContext(Dispatchers.IO) { source!!.adjacentChapter(section, delta) })
    }
    fun jump(value: String, done: () -> Unit) = launch {
        val page = withContext(Dispatchers.IO) { source!!.jump(value) }
        show(page); done()
    }
    private fun show(page: UInt, location: ReaderLocation? = null, edge: Boolean? = null, speech: Boolean = false) {
        if (mutable.value.loading || mutable.value.adapting) return
        flush()
        pageJob?.cancel()
        pageJob = launch {
            val content = withContext(Dispatchers.IO) {
                android.os.Trace.beginSection("simPl.pageContent")
                try { adapted!!.page(page) } finally { android.os.Trace.endSection() }
            }
            mutable.value = mutable.value.copy(page = page, pages = content, location = location, note = null, selectionEdge = edge,
                revision = mutable.value.revision + 1, error = null)
            refreshAnnotations()
            current = location ?: firstLocation(content)
            current?.let(::record)
            if (!speech && ReadAloud.state.value.active && !ReadAloud.state.value.passage && ReadAloud.state.value.fingerprint == fingerprint) readAloud()
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
    private suspend fun refreshAnnotations() {
        val sections = mutable.value.pages.map { it.section }.distinct()
        val result = withContext(Dispatchers.IO) { source!!.readerAnnotations() to sections.flatMap { source!!.readerMarks(it) } }
        mutable.value = mutable.value.copy(annotations = result.first, marks = result.second)
    }
    fun select(selection: ReflowSelection?) {
        selectionJob?.cancel()
        mutable.value = mutable.value.copy(selection = selection, searchPoint = null)
        saved["selection"] = selection?.let { longArrayOf(it.from.section.toLong(), it.from.row.toLong(), it.from.byte.toLong(), it.to.section.toLong(), it.to.row.toLong(), it.to.byte.toLong()) }
    }
    fun selectWord(point: SourcePoint, done: () -> Unit = {}) {
        selectionJob?.cancel()
        selectionJob = launch {
            val selection = withContext(Dispatchers.IO) { source!!.selectionWord(point) }
            // Do not cancel the coroutine that delivered the word.
            selectionJob = null
            select(selection)
            done()
        }
    }
    fun extend(point: SourcePoint, start: Boolean) {
        val selection = mutable.value.selection ?: return
        select(if (start) selection.copy(from = point) else selection.copy(to = point))
    }
    fun copy(done: (String) -> Unit) = launch {
        val selection = mutable.value.selection ?: return@launch
        done(withContext(Dispatchers.IO) { source!!.selectionText(selection) })
    }
    fun highlight(color: AnnotationColor, note: String?) = launch(FailureAction.Save) {
        val selection = mutable.value.selection ?: return@launch
        withContext(Dispatchers.IO) { source!!.highlightSelection(selection, color, note) }
        select(null); refreshAnnotations()
    }
    fun bookmark() = launch(FailureAction.Save) { withContext(Dispatchers.IO) { source!!.toggleBookmark(mutable.value.page) }; refreshAnnotations() }
    fun edit(id: ULong, color: AnnotationColor, note: String) = launch(FailureAction.Save) {
        withContext(Dispatchers.IO) { editAnnotation(fingerprint, id, color, note) }; refreshAnnotations()
    }
    fun remove(id: ULong, bookmark: Boolean) = launch(FailureAction.Save) {
        withContext(Dispatchers.IO) { removeAnnotation(fingerprint, id, bookmark) }; refreshAnnotations()
    }
    fun annotation(id: ULong, bookmark: Boolean) = launch {
        val target = withContext(Dispatchers.IO) { source!!.annotationTarget(id, bookmark) }
        select(target.selection); go(target.location)
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
            writes.withLock { runCatching { opened.saveLocation(location, size) }.onFailure { fail(it, FailureAction.Save) } }
        }
    }
    private suspend fun persist(location: ReaderLocation) {
        val opened = source ?: return
        val size = mutable.value.options.size
        withContext(Dispatchers.IO) { writes.withLock { runCatching { opened.saveLocation(location, size) }.onFailure { fail(it, FailureAction.Save) } } }
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
    private fun launch(action: FailureAction = FailureAction.Read, block: suspend () -> Unit): Job = viewModelScope.launch {
        try { block() } catch (error: CancellationException) { throw error } catch (error: Exception) { fail(error, action) }
    }
    private fun fail(error: Throwable, action: FailureAction = FailureAction.Read) {
        val message = userError(error, action)
        mutable.value = mutable.value.copy(loading = false, adapting = false, error = message)
    }
    override fun onCleared() {
        speechStart?.cancel(); stop(); cleared = true
        opening?.cancel(); adaptation?.cancel()
        adapted?.close(); source?.close()
    }
}
