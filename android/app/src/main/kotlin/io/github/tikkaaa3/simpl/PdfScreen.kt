@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.*
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlin.math.*

/** Build a bounded spatial index once; pointer movement does not scan every glyph. */
internal class PdfTextGeometry(private val layer: PdfPageText) {
    private val rows = Array(128) { ArrayList<Int>() }
    private val tall = ArrayList<Int>()
    init {
        layer.glyphs.forEachIndexed { index, glyph -> glyph.bounds?.let { r ->
            val first = (r.top * 128).toInt().coerceIn(0, 127)
            val last = (r.bottom * 128).toInt().coerceIn(first, 127)
            if (last - first > 4) tall += index else for (row in first..last) rows[row] += index
        } }
    }
    fun closest(point: Offset, width: Float, height: Float, distance: Float = Float.POSITIVE_INFINITY): Int? {
        val x = point.x / width; val y = point.y / height
        var best: Int? = null; var score = Float.POSITIVE_INFINITY
        fun consider(index: Int) {
            val r = layer.glyphs[index].bounds!!
            val dx = max(max(r.left - x, x - r.right), 0f) * width
            val dy = max(max(r.top - y, y - r.bottom), 0f) * height
            val candidate = dx * dx + dy * dy * 4
            if (candidate < score && dx <= distance && dy <= distance) { best = index; score = candidate }
        }
        tall.forEach(::consider)
        val center = (y * 128).toInt().coerceIn(0, 127)
        for (step in 0..127) {
            var searched = false
            for (row in if (step == 0) listOf(center) else listOf(center - step, center + step)) {
                if (row !in 0..127) continue
                val dy = max(max(row / 128f - y, y - (row + 1) / 128f), 0f) * height * 2
                if (dy * dy <= score) { rows[row].forEach(::consider); searched = true }
            }
            if (!searched) break
        }
        return best
    }
    fun word(index: Int): IntRange {
        val bytes = layer.text.toByteArray(Charsets.UTF_8)
        fun isWord(i: Int): Boolean {
            val g = layer.glyphs[i]
            return String(bytes, g.start.toInt(), (g.end - g.start).toInt(), Charsets.UTF_8).any { it.isLetterOrDigit() || it == '\'' || it == '’' }
        }
        var first = index; var last = index
        if (isWord(index)) {
            while (first > 0 && isWord(first - 1)) first--
            while (last < layer.glyphs.lastIndex && isWord(last + 1)) last++
        }
        return first..last
    }
}

@Composable
internal fun PdfScreen(book: LibraryBook, model: PdfViewModel, back: () -> Unit, settings: () -> Unit) {
    val state by model.state.collectAsStateWithLifecycle()
    LaunchedEffect(book.path) { model.open(book) }
    var toolbar by rememberSaveable { mutableStateOf(true) }
    var jump by rememberSaveable { mutableStateOf(false) }
    var annotations by rememberSaveable { mutableStateOf(false) }
    var editing by remember { mutableStateOf<AnnotationEntry?>(null) }
    LaunchedEffect(state.selection) { if (state.selection != null) toolbar = true }
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(state.selection, state.location.page) { copied = false }
    val activity = requireNotNull(LocalActivity.current)
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current
    val dark = MaterialTheme.colorScheme.surface.luminance() < .5f
    DisposableEffect(lifecycle, model) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) model.stop() }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer); model.stop() }
    }
    DisposableEffect(toolbar, dark) {
        val controller = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (toolbar) controller.show(WindowInsetsCompat.Type.systemBars()) else controller.hide(WindowInsetsCompat.Type.systemBars())
        controller.isAppearanceLightStatusBars = !dark; controller.isAppearanceLightNavigationBars = !dark
        onDispose { controller.show(WindowInsetsCompat.Type.systemBars()) }
    }
    BackHandler {
        when { annotations -> annotations = false; state.selection != null -> model.select(null); jump -> jump = false;
            !toolbar -> toolbar = true; else -> { model.stop(); back() } }
    }
    Column(Modifier.fillMaxSize().testTag("reader")) {
        if (toolbar) TopAppBar(title = { Text(book.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = { IconButton(onClick = { model.stop(); back() }) { Icon(AppIcons.Back, "Back to library") } },
            actions = {
                IconButton(onClick = model::bookmark, enabled = !state.loading) { Text(if (state.annotations.bookmarks.any { it.pageNumber == state.location.page }) "★" else "☆", Modifier.semantics { contentDescription = "Bookmark page" }) }
                IconButton(onClick = { annotations = true }) { Text("☰", Modifier.semantics { contentDescription = "Annotations" }) }
                IconButton(onClick = settings) { Icon(AppIcons.Settings, "Settings") } })
        if (toolbar && !state.loading && state.info != null) FlowRow(Modifier.fillMaxWidth().padding(horizontal = 8.dp)) {
            Text("PDF", style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 8.dp))
            ReadAloudControls(book.fingerprint, model::readAloud, enabled = state.info?.canCopy == true)
            TextButton(onClick = model::fit) { Text("Fit width") }
            TextButton(onClick = model::selectAll, enabled = !state.text?.glyphs.isNullOrEmpty()) { Text("Select page text") }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (state.loading) CircularProgressIndicator(Modifier.align(Alignment.Center))
            else if (state.info != null) PdfViewport(state, model, { toolbar = !toolbar; copied = false }) { id -> editing = state.annotations.highlights.firstOrNull { it.id == id } }
            else Column(Modifier.padding(24.dp)) { Text(state.error ?: "Could not open this PDF"); TextButton(onClick = back) { Text("Return to library") } }
            state.selection?.let { selection -> Surface(Modifier.align(Alignment.BottomCenter), tonalElevation = 3.dp) {
                key(selection) { SelectionMenu("pdfSelection", model::copy, model::highlight, read = { model.readSelection() }) { model.select(null) } }
            } }
            if (state.rendering) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter).testTag("pdfRendering"))
        }
        if (toolbar && state.info != null) Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
            TextButton(onClick = { model.turn(-1) }, enabled = state.location.page > 1u) { Text("Previous") }
            TextButton(onClick = { jump = true }, modifier = Modifier.weight(1f).testTag("pageLabel")) { Text("Page ${state.location.page} of ${state.info!!.pages.size}", maxLines = 2, overflow = TextOverflow.Ellipsis) }
            TextButton(onClick = { model.turn(1) }, enabled = state.location.page < state.info!!.pages.size.toUInt()) { Text("Next") }
        }
    }
    if (annotations) AnnotationSheet(state.annotations, { annotations = false }, model::annotation, model::edit, model::remove, read = { model.readPassage(it) })
    editing?.let { entry -> NoteEditor(entry, { editing = null }) { color, note -> model.edit(entry.id, color, note); editing = null } }
    if (jump) {
        var value by rememberSaveable { mutableStateOf("") }
        AlertDialog(onDismissRequest = { jump = false }, title = { Text("Go to page") },
            text = { OutlinedTextField(value, { value = it }, singleLine = true, label = { Text("Page number") }, modifier = Modifier.testTag("jumpPage")) },
            confirmButton = { TextButton(onClick = { if (model.jump(value)) jump = false }, enabled = value.isNotBlank()) { Text("Go") } },
            dismissButton = { TextButton(onClick = { jump = false }) { Text("Cancel") } })
    }
    if (state.error != null && state.info != null) AlertDialog(onDismissRequest = model::dismissError, title = { Text("PDF reader") },
        text = { Text(state.error!!) }, confirmButton = { TextButton(onClick = model::dismissError) { Text("OK") } })
}

@Composable
private fun PdfViewport(state: PdfState, model: PdfViewModel, toggle: () -> Unit, editMark: (ULong) -> Unit) {
    val density = LocalDensity.current.density
    val vertical = rememberScrollState(); val horizontal = rememberScrollState()
    val location = state.location
    val pageSize = state.info!!.pages[location.page.toInt() - 1]
    val toggleNow by rememberUpdatedState(toggle)
    val locationNow by rememberUpdatedState(location)
    val selectionNow by rememberUpdatedState(state.selection)
    val editNow by rememberUpdatedState(editMark)
    val marksNow by rememberUpdatedState(state.marks)
    val geometry = remember(state.text) { state.text?.let(::PdfTextGeometry) }
    val geometryNow by rememberUpdatedState(geometry)
    val background = MaterialTheme.colorScheme.surfaceContainer
    BoxWithConstraints(Modifier.fillMaxSize().background(background).testTag("pdfViewport")
        .pointerInput(location.page, location.fitWidth, state.selection != null) {
            if (location.fitWidth && state.selection == null) {
                var distance = 0f
                detectHorizontalDragGestures(onDragStart = { distance = 0f }, onDragEnd = {
                    if (abs(distance) > 48 * density) model.turn(if (distance < 0) 1 else -1)
                }) { change, amount -> change.consume(); distance += amount }
            }
        }) {
        val fitScale = constraints.maxWidth / density / pageSize.width / (4f / 3f)
        val zoom = if (location.fitWidth) 1f else location.zoom / fitScale
        val width = constraints.maxWidth * zoom
        val height = width * pageSize.height / pageSize.width
        fun hit(point: Offset): Int? = geometry?.closest(Offset(point.x + horizontal.value, point.y + vertical.value), width, height)
        fun handle(point: PdfSelectionPoint?, end: Boolean): Offset? {
            if (point?.page != location.page) return null
            val r = state.text?.glyphs?.getOrNull(point.index.toInt())?.bounds ?: return null
            return Offset((if (end) r.right else r.left) * width - horizontal.value, r.bottom * height - vertical.value)
                .takeIf { it.y in 0f..constraints.maxHeight.toFloat() }
        }
        val handles = handle(state.selection?.from, false) to handle(state.selection?.to, true)
        fun lift(point: PdfSelectionPoint?): Float = if (point?.page != location.page) 0f else
            state.text?.glyphs?.getOrNull(point.index.toInt())?.bounds?.let { (it.bottom - it.top) * height / 2 } ?: 0f
        var dragging by remember { mutableStateOf<Offset?>(null) }
        var draggingStart by remember { mutableStateOf(false) }
        val gestures = selectionGesture(handles, { point ->
            val index = geometry?.closest(Offset(point.x + horizontal.value, point.y + vertical.value), width, height, 24 * density)
            if (index != null) { model.select(geometry.word(index)); true } else false
        }, { point, start -> draggingStart = start; hit(point)?.let { model.extend(it, start) } }, { dragging = it }, lift(state.selection?.from) to lift(state.selection?.to))
        SelectionEdgeDrag(dragging, constraints.maxHeight.toFloat(), { vertical.scrollBy(it) }, { model.selectionTurn(it, draggingStart) }) { point -> hit(point)?.let { model.extend(it, draggingStart) } }
        LaunchedEffect(state.spoken, state.text, height) {
            val word = state.spoken?.takeIf { it.pdfPage == location.page } ?: return@LaunchedEffect
            val bounds = state.text?.glyphs?.firstOrNull { it.end > word.from.byte && it.start < word.to.byte }?.bounds ?: return@LaunchedEffect
            val top = bounds.top * height; val bottom = bounds.bottom * height
            if (top < vertical.value || bottom > vertical.value + constraints.maxHeight) vertical.scrollTo((top - constraints.maxHeight * .25f).roundToInt().coerceAtLeast(0))
        }
        val zoomNow by rememberUpdatedState(zoom)
        LaunchedEffect(location.page, width) { model.render(width) }
        var ready by remember(state.revision) { mutableStateOf(false) }
        LaunchedEffect(state.revision, width, height) {
            ready = false
            withFrameNanos { }
            vertical.scrollTo((locationNow.within * height).roundToInt())
            horizontal.scrollTo((locationNow.horizontal * horizontal.maxValue).roundToInt())
            ready = true
        }
        LaunchedEffect(state.revision, width, height) {
            snapshotFlow { vertical.value to horizontal.value }.distinctUntilChanged().collect { (y, x) ->
                if (ready) model.record(locationNow.copy(within = (y / height).coerceIn(0f, 1f),
                    horizontal = if (horizontal.maxValue > 0) (x.toFloat() / horizontal.maxValue).coerceIn(0f, 1f) else 0f))
            }
        }
        Box(Modifier.fillMaxSize().then(gestures)) {
          Box(Modifier.fillMaxSize().semantics { stateDescription = "Zoom $zoom; page ${location.page}" }
            .pointerInput(fitScale) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    var scale = zoomNow
                    do {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } >= 2) {
                            scale = (scale * event.calculateZoom()).coerceIn(.5f, 4f)
                            model.zoom(scale * fitScale)
                            event.changes.forEach { it.consume() }
                        }
                    } while (event.changes.any { it.pressed })
                }
            }.verticalScroll(vertical, enabled = dragging == null).horizontalScroll(horizontal, enabled = !location.fitWidth && dragging == null)) {
            val image = remember(state.raster) { state.raster?.asImageBitmap() }
            Canvas(Modifier.requiredWidth((width / density).dp).requiredHeight((height / density).dp).background(Color.White).testTag("pdfPage")
                .semantics {
                    contentDescription = "PDF page ${location.page}"
                    stateDescription = if (image == null) "Loading page" else if (state.spoken?.pdfPage == location.page) "Reading aloud" else "Page ready"
                    if (state.text != null) text = androidx.compose.ui.text.AnnotatedString(state.text.text)
                    if (!state.text?.glyphs.isNullOrEmpty()) customActions = listOf(CustomAccessibilityAction("Select page text") { model.selectAll(); true })
                }
                .pointerInput(location.page) {
                    detectTapGestures { point ->
                        val index = geometryNow?.closest(point, size.width.toFloat(), size.height.toFloat(), 12 * density)
                        val mark = index?.let { glyph -> marksNow.lastOrNull { glyph in (it.range(locationNow.page, Int.MAX_VALUE) ?: IntRange.EMPTY) } }
                        if (selectionNow != null && index != null) model.extend(index, false)
                        else if (selectionNow != null) model.select(null)
                        else if (mark != null) editNow(mark.id)
                        else if (locationNow.fitWidth && point.x < size.width * .18f) model.turn(-1)
                        else if (locationNow.fitWidth && point.x > size.width * .82f) model.turn(1)
                        else toggleNow()
                    }
                }) {
                image?.let { drawImage(it, dstSize = IntSize(size.width.roundToInt(), size.height.roundToInt())) }
                fun paint(range: IntRange, color: Color, underline: Boolean = false) {
                    range.forEach { index -> state.text?.glyphs?.getOrNull(index)?.bounds?.let { r ->
                        if (underline) drawLine(color.copy(alpha = .9f), Offset(r.left * size.width, r.bottom * size.height), Offset(r.right * size.width, r.bottom * size.height), 1.5f * density)
                        else drawRect(color, Offset(r.left * size.width, r.top * size.height), Size((r.right - r.left) * size.width, (r.bottom - r.top) * size.height))
                    } }
                }
                val count = state.text?.glyphs?.size ?: 0
                coloredRanges(state.marks.mapNotNull { mark -> mark.range(location.page, count)?.let { it to mark.color } }).forEach { (range, color) -> paint(range, color.tint()) }
                state.marks.filter { it.note }.forEach { mark -> mark.range(location.page, count)?.let { paint(it, mark.color.tint(), true) } }
                state.spoken?.takeIf { it.pdfPage == location.page }?.let { word ->
                    state.text?.glyphs?.forEachIndexed { index, glyph ->
                        if (glyph.start < word.to.byte && glyph.end > word.from.byte) paint(index..index, Color(0x6657a8ef))
                    }
                }
                state.selection?.range(location.page, count)?.let { paint(it, Color(0x6657a8ef)) }
            }
        }
          SelectionHandle(handles.first, true)
          SelectionHandle(handles.second, false)
        }
        if (state.raster == null && state.error == null) CircularProgressIndicator(Modifier.align(Alignment.Center))
    }
}

internal fun PdfSelectionPoint.compare(other: PdfSelectionPoint): Int = compareValuesBy(this, other, { it.page }, { it.index })
internal fun PdfSelection.range(page: UInt, count: Int): IntRange? {
    if (count <= 0) return null
    val (a, b) = if (from.compare(to) <= 0) from to to else to to from
    if (page !in a.page..b.page) return null
    val start = if (page == a.page) a.index.toLong().coerceAtMost(count.toLong()).toInt() else 0
    val end = if (page == b.page) b.index.toLong().coerceAtMost((count - 1).toLong()).toInt() else count - 1
    return (start..end).takeUnless { it.isEmpty() }
}
internal fun PdfMark.range(page: UInt, count: Int): IntRange? = PdfSelection(from, to).range(page, count)
