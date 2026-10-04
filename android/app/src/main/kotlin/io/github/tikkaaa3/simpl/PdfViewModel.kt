package io.github.tikkaaa3.simpl

import android.app.Application
import android.graphics.Bitmap
import android.os.SystemClock
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlin.math.ceil
import kotlin.math.sqrt

internal data class PdfRasterKey(val page: UInt, val width: UInt)

/** Software rasters only; eviction releases ownership after the UI's last frame. */
internal class PdfBitmapCache(val limit: Long = 24L * 1024 * 1024) {
    private val entries = LinkedHashMap<PdfRasterKey, Bitmap>(8, .75f, true)
    var bytes = 0L; private set
    var hits = 0; private set
    var evictions = 0; private set
    fun get(key: PdfRasterKey): Bitmap? = entries[key]?.also { hits++ }
    fun put(key: PdfRasterKey, bitmap: Bitmap) {
        entries.remove(key)?.let { bytes -= it.allocationByteCount }
        if (bitmap.allocationByteCount > limit) return
        while (bytes + bitmap.allocationByteCount > limit && entries.isNotEmpty()) {
            val first = entries.entries.iterator()
            bytes -= first.next().value.allocationByteCount; first.remove(); evictions++
        }
        entries[key] = bitmap; bytes += bitmap.allocationByteCount
    }
    fun clear() { entries.clear(); bytes = 0 }
}

internal data class PdfState(
    val loading: Boolean = true,
    val rendering: Boolean = false,
    val info: PdfInfo? = null,
    val location: PdfLocation = PdfLocation(1u, 0f, 0f, 1f, true),
    val revision: Int = 0,
    val raster: Bitmap? = null,
    val rasterKey: PdfRasterKey? = null,
    val text: PdfPageText? = null,
    val selection: PdfSelection? = null,
    val annotations: AnnotationCollection = emptyAnnotations,
    val marks: List<PdfMark> = emptyList(),
    val error: String? = null,
    val firstOpenMs: Long = 0,
    val firstRasterMs: Long = 0,
    val renderMs: Long = 0,
    val cacheBytes: Long = 0,
    val cacheHits: Int = 0,
    val cacheEvictions: Int = 0,
)

internal class PdfViewModel(application: Application, private val saved: SavedStateHandle) : AndroidViewModel(application) {
    private val mutable = MutableStateFlow(PdfState())
    val state = mutable.asStateFlow()
    private var source: PdfDocument? = null
    private var fingerprint = ""
    private var path: String? = null
    private var rendering: Job? = null
    private var saving: Job? = null
    private val native = Mutex()
    private val writes = Mutex()
    private val cache = PdfBitmapCache()
    private var requested: PdfRasterKey? = null
    private var cleared = false
    private var openedAt = 0L

    fun open(book: LibraryBook) {
        if (path == book.path) return
        path = book.path
        fingerprint = book.fingerprint
        viewModelScope.launch {
            val start = SystemClock.elapsedRealtime()
            openedAt = start
            try {
                // A cancelled route still closes a native handle returned by open.
                val opened = withContext(Dispatchers.IO + NonCancellable) { openPdfDocument(book.path) }
                if (cleared || !isActive) { opened.close(); return@launch }
                source = opened
                val info = withContext(Dispatchers.IO) { opened.info() }
                val location = saved.get<Long>("pdfPage")?.let { page ->
                    PdfLocation(page.toUInt(), saved["pdfWithin"] ?: 0f, saved["pdfHorizontal"] ?: 0f,
                        saved["pdfZoom"] ?: 1f, saved["pdfFit"] ?: true)
                }?.takeIf { it.page in 1u..info.pages.size.toUInt() } ?: info.restored
                mutable.value = mutable.value.copy(loading = false, info = info, location = location,
                    firstOpenMs = SystemClock.elapsedRealtime() - start, revision = 1)
                saved.get<LongArray>("pdfSelection")?.takeIf { it.size == 4 }?.let {
                    selectSource(PdfSelection(PdfSelectionPoint(it[0].toUInt(), it[1].toUInt()), PdfSelectionPoint(it[2].toUInt(), it[3].toUInt())))
                }
                refreshAnnotations()
                record(location)
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { fail(error) }
        }
    }

    /** Quantized widths avoid a bitmap for each pinch event; cap at two megapixels. */
    fun render(displayWidth: Float) {
        val current = mutable.value
        val page = current.location.page
        val size = current.info?.pages?.getOrNull(page.toInt() - 1) ?: return
        val capped = displayWidth.coerceAtLeast(1f).coerceAtMost(sqrt(2_000_000f * size.width / size.height))
        val width = (ceil(capped / 128f) * 128f).toInt().coerceIn(1, 4096).toUInt()
        val key = PdfRasterKey(page, width)
        if (requested == key) return
        requested = key
        rendering?.cancel()
        rendering = launch {
            delay(80) // Coalesce pinch events before invoking the serial PDFium worker.
            val opened = source ?: return@launch
            mutable.value = mutable.value.copy(rendering = true)
            val start = SystemClock.elapsedRealtime()
            val bitmap = withContext(Dispatchers.IO) {
                native.withLock {
                    cache.get(key) ?: opened.render(page, width).let { raster ->
                        val rgba = raster.rgba
                        val pixels = IntArray(rgba.size / 4) { i ->
                            ((rgba[4 * i + 3].toInt() and 255) shl 24) or ((rgba[4 * i].toInt() and 255) shl 16) or
                                ((rgba[4 * i + 1].toInt() and 255) shl 8) or (rgba[4 * i + 2].toInt() and 255)
                        }
                        Bitmap.createBitmap(pixels, raster.width.toInt(), raster.height.toInt(), Bitmap.Config.ARGB_8888).also { cache.put(key, it) }
                    }
                }
            }
            ensureActive()
            mutable.value = mutable.value.copy(raster = bitmap, rasterKey = key, rendering = false,
                renderMs = SystemClock.elapsedRealtime() - start,
                firstRasterMs = mutable.value.firstRasterMs.takeIf { it > 0 } ?: (SystemClock.elapsedRealtime() - openedAt),
                cacheBytes = cache.bytes, cacheHits = cache.hits, cacheEvictions = cache.evictions)
            if (mutable.value.text == null && mutable.value.info?.canCopy == true) {
                val layer = withContext(Dispatchers.IO) { native.withLock { opened.text(page, width) } }
                ensureActive()
                mutable.value = mutable.value.copy(text = layer)
            }
        }
    }

    fun turn(delta: Int) {
        val total = mutable.value.info?.pages?.size ?: return
        show((mutable.value.location.page.toLong() + delta).coerceIn(1, total.toLong()).toUInt())
    }
    fun selectionTurn(delta: Int, start: Boolean) {
        val page = mutable.value.location.page
        turn(delta)
        if (mutable.value.location.page != page) extend(0, start)
    }
    fun jump(value: String): Boolean {
        val number = value.trim().toUIntOrNull()
        if (number == null || number !in 1u..(mutable.value.info?.pages?.size?.toUInt() ?: 0u)) {
            mutable.value = mutable.value.copy(error = "Enter a page number from 1 to ${mutable.value.info?.pages?.size ?: 0}")
            return false
        }
        show(number); return true
    }
    private fun show(page: UInt) {
        if (page == mutable.value.location.page) return
        flush()
        rendering?.cancel(); requested = null
        val location = mutable.value.location.copy(page = page, within = 0f, horizontal = 0f)
        mutable.value = mutable.value.copy(location = location, raster = null, rasterKey = null, text = null,
            rendering = true, error = null, revision = mutable.value.revision + 1)
        record(location)
    }
    fun zoom(scale: Float) {
        record(mutable.value.location.copy(zoom = scale.coerceIn(.25f, 4f), fitWidth = false))
    }
    fun fit() {
        record(mutable.value.location.copy(zoom = 1f, fitWidth = true, horizontal = 0f))
        mutable.value = mutable.value.copy(revision = mutable.value.revision + 1)
    }
    private suspend fun refreshAnnotations() {
        val result = withContext(Dispatchers.IO) { native.withLock { loadAnnotations(fingerprint) to source!!.pdfMarks() } }
        mutable.value = mutable.value.copy(annotations = result.first, marks = result.second)
    }
    fun selectSource(selection: PdfSelection?) {
        mutable.value = mutable.value.copy(selection = selection)
        saved["pdfSelection"] = selection?.let { longArrayOf(it.from.page.toLong(), it.from.index.toLong(), it.to.page.toLong(), it.to.index.toLong()) }
    }
    fun select(range: IntRange?) = selectSource(range?.let { PdfSelection(PdfSelectionPoint(mutable.value.location.page, it.first.toUInt()), PdfSelectionPoint(mutable.value.location.page, it.last.toUInt())) })
    fun extend(index: Int, start: Boolean) {
        val selection = mutable.value.selection ?: return
        val point = PdfSelectionPoint(mutable.value.location.page, index.toUInt())
        selectSource(if (start) selection.copy(from = point) else selection.copy(to = point))
    }
    fun selectAll() { mutable.value.text?.glyphs?.takeIf { it.isNotEmpty() }?.let { select(it.indices) } }
    fun copy(done: (String) -> Unit) = launch {
        val selection = mutable.value.selection ?: return@launch
        done(withContext(Dispatchers.IO) { native.withLock { source!!.selectionText(selection) } })
    }
    fun highlight(color: AnnotationColor, note: String?) = launch(FailureAction.Save) {
        val selection = mutable.value.selection ?: return@launch
        withContext(Dispatchers.IO) { native.withLock { source!!.highlightSelection(selection, color, note) } }
        select(null); refreshAnnotations()
    }
    fun bookmark() = launch(FailureAction.Save) { withContext(Dispatchers.IO) { native.withLock { source!!.toggleBookmark(mutable.value.location.page) } }; refreshAnnotations() }
    fun edit(id: ULong, color: AnnotationColor, note: String) = launch(FailureAction.Save) {
        withContext(Dispatchers.IO) { editAnnotation(fingerprint, id, color, note) }; refreshAnnotations()
    }
    fun remove(id: ULong, bookmark: Boolean) = launch(FailureAction.Save) {
        withContext(Dispatchers.IO) { removeAnnotation(fingerprint, id, bookmark) }; refreshAnnotations()
    }
    fun annotation(id: ULong, bookmark: Boolean) = launch {
        val target = withContext(Dispatchers.IO) { native.withLock { source!!.annotationTarget(id, bookmark) } }
        selectSource(target.selection); show(target.page)
        val location = mutable.value.location.copy(within = target.within, horizontal = 0f)
        mutable.value = mutable.value.copy(location = location, revision = mutable.value.revision + 1)
        record(location)
    }
    fun dismissError() { mutable.value = mutable.value.copy(error = null) }
    fun record(location: PdfLocation) {
        if (cleared) return
        mutable.value = mutable.value.copy(location = location)
        saved["pdfPage"] = location.page.toLong(); saved["pdfWithin"] = location.within
        saved["pdfHorizontal"] = location.horizontal; saved["pdfZoom"] = location.zoom; saved["pdfFit"] = location.fitWidth
        saving?.cancel(); saving = viewModelScope.launch { delay(350); persist(location) }
    }
    private suspend fun persist(location: PdfLocation) {
        val opened = source ?: return
        withContext(Dispatchers.IO) { writes.withLock { runCatching { opened.saveLocation(location) }.onFailure { fail(it, FailureAction.Save) } } }
    }
    private fun flush() {
        saving?.cancel(); val location = mutable.value.location
        saving = viewModelScope.launch { persist(location) }
    }
    fun stop() {
        saving?.cancel(); val opened = source ?: return
        val location = mutable.value.location
        runBlocking(Dispatchers.IO) { writes.withLock { runCatching { opened.saveLocation(location) }.onFailure { fail(it, FailureAction.Save) } } }
    }
    private fun launch(action: FailureAction = FailureAction.Read, block: suspend CoroutineScope.() -> Unit) = viewModelScope.launch {
        try { block() } catch (error: CancellationException) { throw error } catch (error: Exception) { requested = null; fail(error, action) }
    }
    private fun fail(error: Throwable, action: FailureAction = FailureAction.Read) {
        mutable.value = mutable.value.copy(loading = false, rendering = false,
            error = userError(error, action))
    }
    override fun onCleared() {
        stop(); cleared = true; rendering?.cancel(); source?.close(); source = null
        runBlocking(Dispatchers.IO) { native.withLock { cache.clear() } }
    }
}
