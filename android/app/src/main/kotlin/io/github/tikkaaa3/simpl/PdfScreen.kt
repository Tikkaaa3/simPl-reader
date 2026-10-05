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
internal fun PdfScreen(book: LibraryBook, model: PdfViewModel, back: () -> Unit, settings: () -> Unit, bookMode: (() -> Unit)? = null) {
    val state by model.state.collectAsStateWithLifecycle()
    LaunchedEffect(book.path) { model.open(book) }
    var toolbar by rememberSaveable { mutableStateOf(true) }
    var jump by rememberSaveable { mutableStateOf(false) }
    var tools by rememberSaveable { mutableStateOf(false) }
    var pageEditing by remember { mutableStateOf(false) }
    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current
    val keyboard = androidx.compose.ui.platform.LocalSoftwareKeyboardController.current
    var annotations by rememberSaveable { mutableStateOf(false) }
    var finding by rememberSaveable { mutableStateOf(false) }
    val findState = rememberFindState()
    var sideVisible by rememberSaveable { mutableStateOf(false) }
    var paneAvailable by remember { mutableStateOf(false) }
    var help by rememberSaveable { mutableStateOf(false) }
    var previousZoom by rememberSaveable { mutableFloatStateOf(1.5f) }
    val scrollKeys = remember { kotlinx.coroutines.channels.Channel<Int>(kotlinx.coroutines.channels.Channel.CONFLATED) }
    val quickSwitch = LocalQuickSwitch.current
    var dictionary by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(state.selection) { if (state.selection == null) dictionary = false }
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
    fun escape() {
        when { pageEditing -> { focusManager.clearFocus(); keyboard?.hide(); jump = false }
            tools -> tools = false; finding -> { finding = false; model.select(null) }; annotations -> annotations = false; state.selection != null -> model.select(null); jump -> jump = false;
            !toolbar -> toolbar = true; else -> { model.stop(); back() } }
    }
    BackHandler { escape() }
    ReaderKeys(!state.loading && state.info != null) { command ->
        when (command) {
            ReaderCommand.Previous -> model.turn(-1)
            ReaderCommand.Next -> model.turn(1)
            ReaderCommand.ScrollUp -> scrollKeys.trySend(-1)
            ReaderCommand.ScrollDown -> scrollKeys.trySend(1)
            ReaderCommand.First -> model.jump("1")
            ReaderCommand.Last -> model.jump(state.info!!.pages.size.toString())
            ReaderCommand.Find -> if (state.info?.canCopy == true) { finding = true; toolbar = true }
            ReaderCommand.Jump -> { toolbar = true; jump = true }
            ReaderCommand.ZoomIn -> model.stepZoom(1.1f)
            ReaderCommand.ZoomOut -> model.stepZoom(1 / 1.1f)
            ReaderCommand.ResetZoom -> model.zoom(1f)
            ReaderCommand.Fit -> if (state.location.fitWidth) model.zoom(previousZoom) else { previousZoom = state.location.zoom; model.fit() }
            ReaderCommand.Copy -> model.copy { (context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(android.content.ClipData.newPlainText("Book text", it)) }
            ReaderCommand.SelectAll -> model.selectAll()
            ReaderCommand.Speech -> if (ReadAloud.state.value.active) ReadAloud.stop() else model.readAloud()
            ReaderCommand.Highlight -> model.highlight(AnnotationColor.YELLOW, null)
            ReaderCommand.Bookmark -> model.bookmark()
            ReaderCommand.Panel -> {
                if (paneAvailable) { sideVisible = !sideVisible; annotations = false }
                else { annotations = !annotations; sideVisible = true }
                finding = false
            }
            ReaderCommand.Toolbar -> toolbar = !toolbar
            ReaderCommand.Help -> help = true
            ReaderCommand.Close -> { model.stop(); back() }
            ReaderCommand.Escape -> escape()
            else -> Unit
        }
    }
    CompositionLocalProvider(LocalReaderScroll provides scrollKeys) {
    AdaptivePanes(modifier = Modifier.background(MaterialTheme.colorScheme.surfaceContainer).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top)),
        sideVisible = (toolbar && sideVisible) || finding, side = {
        if (finding) {
            if (state.loading) CircularProgressIndicator()
            else FindPanel(findState, model::find, model::searchHit) { finding = false; model.select(null) }
        }
        else AnnotationPanel(state.annotations, { annotations = false; sideVisible = false }, model::annotation, model::edit, model::remove, read = { model.readPassage(it) }, book = book)
    }) { wide ->
    SideEffect { paneAvailable = wide }
    Column(Modifier.fillMaxSize().testTag("reader").semantics { paneTitle = book.title }) {
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (state.loading) CircularProgressIndicator(Modifier.align(Alignment.Center))
            else if (state.info != null) PdfViewport(state, model, { toolbar = !toolbar; copied = false }, dictionary = { dictionary = true }) { id -> editing = state.annotations.highlights.firstOrNull { it.id == id } }
            else Column(Modifier.padding(24.dp)) { Text(state.error ?: "Could not open this PDF"); TextButton(onClick = back) { Text("Return to library") } }
            state.selection?.let { selection -> SelectionSurface(Modifier.align(Alignment.BottomCenter)) {
                key(selection) { Column {
                    SelectionMenu("pdfSelection", model::copy, model::highlight, read = { model.readSelection() }, dictionary = if (state.info?.canCopy == true) ({ dictionary = true }) else null) { model.select(null) }
                    if (state.info?.canCopy == true) DictionarySelection(model::copy, selection, dictionary, expand = { dictionary = true }) { dictionary = false }
                } }
            } }
            if (state.rendering) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter).testTag("pdfRendering"))
        }
        if (toolbar) {
            if (!tools) ReaderSpeechBar(book.fingerprint)
            ReaderBottomBar(state.location.page, state.info?.pages?.size?.toUInt() ?: 0u,
                enabled = !state.loading && state.info != null, color = MaterialTheme.colorScheme.surface,
                text = MaterialTheme.colorScheme.onSurface, loading = state.loading, requestPageFocus = jump, pageFocusHandled = { jump = false },
                editingChanged = { pageEditing = it },
                back = { model.stop(); back() }, previous = { model.turn(-1) }, next = { model.turn(1) },
                jump = { value, done -> if (model.jump(value)) done() }, tools = { tools = true })
        }
    }
    if (annotations && !wide) AnnotationSheet(state.annotations, { annotations = false }, model::annotation, model::edit, model::remove, read = { model.readPassage(it) }, book = book)
    if (finding && !wide) ModalBottomSheet(onDismissRequest = { finding = false; model.select(null) }) {
        if (state.loading) CircularProgressIndicator()
        else FindPanel(findState, model::find, model::searchHit) { finding = false; model.select(null) }
    }
    editing?.let { entry -> NoteEditor(entry, { editing = null }) { color, note -> model.edit(entry.id, color, note); editing = null } }
    if (tools) ReaderToolsSheet(book.title, book.fingerprint, state.location.zoom, buildList {
        if (bookMode != null) add(ReaderTool("Book", AppIcons.Contents, bookMode, state.info?.canCopy == true))
        add(ReaderTool("Find in book", AppIcons.Search, { finding = true }, state.info?.canCopy == true))
        add(ReaderTool("Annotations", AppIcons.Notes, { annotations = true; finding = false; sideVisible = true }))
        val bookmarked = state.annotations.bookmarks.any { it.pageNumber == state.location.page }
        add(ReaderTool("Bookmark page", if (bookmarked) AppIcons.Bookmark else AppIcons.BookmarkOutline, model::bookmark, selected = bookmarked))
        add(ReaderTool("Switch book", AppIcons.Switch, quickSwitch))
        add(ReaderTool("Settings", AppIcons.Settings, settings))
        add(ReaderTool("Select page text", AppIcons.Notes, model::selectAll, !state.text?.glyphs.isNullOrEmpty()))
        add(ReaderTool("Hide controls", AppIcons.Hide, { toolbar = false }))
    }, dismiss = { tools = false }, zoomOut = { model.stepZoom(1 / 1.2f) }, zoomIn = { model.stepZoom(1.2f) },
        fitWidth = model::fit, read = model::readAloud, canRead = state.info?.canCopy == true)
    } }
    if (help) KeyboardHelp { help = false }
    ReadAloudError()
    if (state.error != null && state.info != null) AlertDialog(onDismissRequest = model::dismissError, title = { Text("PDF reader") },
        text = { Text(state.error!!) }, confirmButton = { TextButton(onClick = model::dismissError) { Text("OK") } })
}

@Composable
private fun PdfViewport(state: PdfState, model: PdfViewModel, toggle: () -> Unit, dictionary: () -> Unit, editMark: (ULong) -> Unit) {
    val density = LocalDensity.current.density
    val vertical = rememberScrollState(); val horizontal = rememberScrollState()
    val placement = remember { PagePlacement() }
    var gestureRevision by remember { mutableIntStateOf(0) }
    val scrollKeys = LocalReaderScroll.current
    var viewportHeight by remember { mutableIntStateOf(0) }
    LaunchedEffect(scrollKeys) { if (scrollKeys != null) for (delta in scrollKeys) vertical.animateScrollBy(viewportHeight * .8f * delta) }
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
        SideEffect { viewportHeight = constraints.maxHeight; model.viewportScale(fitScale) }
        val zoom = if (location.fitWidth) 1f else location.zoom / fitScale
        val width = constraints.maxWidth * zoom
        val height = width * pageSize.height / pageSize.width
        val pageGeometry = PageGeometry(constraints.maxWidth.toFloat(), constraints.maxHeight.toFloat(), pageSize.width, pageSize.height, width / pageSize.width)
        TrackPageScroll(placement, pageGeometry, state.revision, horizontal, vertical)
        fun hit(point: Offset): Int? = geometry?.closest(point + Offset(horizontal.value.toFloat(), vertical.value.toFloat()) - pageGeometry.inset, width, height)
        fun handle(point: PdfSelectionPoint?, end: Boolean): Offset? {
            if (point?.page != location.page) return null
            val r = state.text?.glyphs?.getOrNull(point.index.toInt())?.bounds ?: return null
            return (Offset((if (end) r.right else r.left) * width - horizontal.value, r.bottom * height - vertical.value) + pageGeometry.inset)
                .takeIf { it.y in 0f..constraints.maxHeight.toFloat() }
        }
        val handles = handle(state.selection?.from, false) to handle(state.selection?.to, true)
        fun lift(point: PdfSelectionPoint?): Float = if (point?.page != location.page) 0f else
            state.text?.glyphs?.getOrNull(point.index.toInt())?.bounds?.let { (it.bottom - it.top) * height / 2 } ?: 0f
        var dragging by remember { mutableStateOf<Offset?>(null) }
        var draggingStart by remember { mutableStateOf(false) }
        val gestures = selectionGesture(handles, { point ->
            val index = geometry?.closest(point + Offset(horizontal.value.toFloat(), vertical.value.toFloat()) - pageGeometry.inset, width, height, 24 * density)
            if (index != null) { model.select(geometry.word(index)); true } else false
        }, { point, start -> draggingStart = start; hit(point)?.let { model.extend(it, start) } }, { dragging = it }, lift(state.selection?.from) to lift(state.selection?.to))
        SelectionEdgeDrag(dragging, constraints.maxHeight.toFloat(), { vertical.scrollBy(it) }, { model.selectionTurn(it, draggingStart) }) { point -> hit(point)?.let { model.extend(it, draggingStart) } }
        LaunchedEffect(state.spoken, state.text, height) {
            val word = state.spoken?.takeIf { it.pdfPage == location.page } ?: return@LaunchedEffect
            val bounds = state.text?.glyphs?.firstOrNull { it.end > word.from.byte && it.start < word.to.byte }?.bounds ?: return@LaunchedEffect
            val top = bounds.top * height + pageGeometry.inset.y; val bottom = bounds.bottom * height + pageGeometry.inset.y
            if (top < vertical.value || bottom > vertical.value + constraints.maxHeight) vertical.scrollTo((top - constraints.maxHeight * .25f).roundToInt().coerceAtLeast(0))
        }
    val zoomNow by rememberUpdatedState(zoom)
        val dictionaryNow by rememberUpdatedState(dictionary)
        LaunchedEffect(location.page, width) { model.render(width) }
        var ready by remember(state.revision) { mutableStateOf(false) }
        LaunchedEffect(state.revision, pageGeometry, gestureRevision) {
            ready = false
            val target = if (placement.revision == state.revision) placement.resized(pageGeometry) else
                pageGeometry.clamp(Offset(locationNow.horizontal * pageGeometry.maximum.x, locationNow.within * height + pageGeometry.inset.y))
            withFrameNanos { }
            vertical.scrollTo(target.y.roundToInt())
            horizontal.scrollTo(target.x.roundToInt())
            placement.geometry = pageGeometry; placement.revision = state.revision
            placement.scroll = Offset(horizontal.value.toFloat(), vertical.value.toFloat()); placement.pending = null
            ready = true
        }
        LaunchedEffect(state.revision, pageGeometry) {
            snapshotFlow { vertical.value to horizontal.value }.distinctUntilChanged().collect { (y, x) ->
                if (ready) model.record(locationNow.copy(within = ((y - pageGeometry.inset.y) / height).coerceIn(0f, 1f),
                    horizontal = if (horizontal.maxValue > 0) (x.toFloat() / horizontal.maxValue).coerceIn(0f, 1f) else 0f))
            }
        }
        Box(Modifier.fillMaxSize().then(gestures).pointerInput(location.page, pageGeometry.viewportWidth, pageGeometry.viewportHeight) {
            detectTapGestures { point ->
                when {
                    selectionNow != null -> model.select(null)
                    locationNow.fitWidth && point.x < pageGeometry.viewportWidth * .18f -> model.turn(-1)
                    locationNow.fitWidth && point.x > pageGeometry.viewportWidth * .82f -> model.turn(1)
                    else -> toggleNow()
                }
            }
        }) {
          Box(Modifier.fillMaxSize().semantics { stateDescription = "Zoom $zoom; page ${location.page}" }
            .pointerInput(fitScale) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    var scale = zoomNow
                    do {
                        val event = awaitPointerEvent()
                        if (event.changes.count { it.pressed } >= 2) {
                            val focus = event.calculateCentroid()
                            placement.zoomAt(if (event.changes.count { it.previousPressed } >= 2) event.calculateCentroid(useCurrent = false) else focus, focus)
                            scale = (scale * event.calculateZoom()).coerceIn(.5f, 4f)
                            model.zoom(scale * fitScale)
                            gestureRevision++
                            event.changes.forEach { it.consume() }
                        }
                    } while (event.changes.any { it.pressed })
                }
            }.verticalScroll(vertical, enabled = dragging == null).horizontalScroll(horizontal, enabled = !location.fitWidth && dragging == null)) {
            val image = remember(state.raster) { state.raster?.asImageBitmap() }
            Box(Modifier.width((max(width, pageGeometry.viewportWidth) / density).dp).height((max(height, pageGeometry.viewportHeight) / density).dp)) {
            Canvas(Modifier.align(Alignment.Center).requiredWidth((width / density).dp).requiredHeight((height / density).dp).background(Color.White).testTag("pdfPage")
                .semantics {
                    contentDescription = "PDF page ${location.page}"
                    stateDescription = if (image == null) "Loading page" else if (state.spoken?.pdfPage == location.page) "Reading aloud" else "Page ready"
                    if (state.text != null) text = androidx.compose.ui.text.AnnotatedString(state.text.text)
                    if (!state.text?.glyphs.isNullOrEmpty()) customActions = listOf(CustomAccessibilityAction("Select page text") { model.selectAll(); true })
                }
                .pointerInput(location.page) {
                    var selectedTap = false
                    detectTapGestures(onPress = { point ->
                        selectedTap = selectionNow != null
                        if (selectedTap && tryAwaitRelease()) {
                            val index = geometryNow?.closest(point, size.width.toFloat(), size.height.toFloat(), 12 * density)
                            if (index != null) model.extend(index, false) else model.select(null)
                        }
                    }, onDoubleTap = { point ->
                        if (state.info.canCopy) geometryNow?.closest(point, size.width.toFloat(), size.height.toFloat(), 12 * density)?.let { index ->
                            model.select(geometryNow!!.word(index)); dictionaryNow()
                        }
                    }, onTap = tap@ { point ->
                        if (selectedTap) return@tap
                        val index = geometryNow?.closest(point, size.width.toFloat(), size.height.toFloat(), 12 * density)
                        val mark = index?.let { glyph -> marksNow.lastOrNull { glyph in (it.range(locationNow.page, Int.MAX_VALUE) ?: IntRange.EMPTY) } }
                        if (selectionNow != null && index != null) model.extend(index, false)
                        else if (selectionNow != null) model.select(null)
                        else if (mark != null) editNow(mark.id)
                        else if (locationNow.fitWidth && point.x < size.width * .18f) model.turn(-1)
                        else if (locationNow.fitWidth && point.x > size.width * .82f) model.turn(1)
                        else toggleNow()
                    })
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
        }
          PageScrollIndicator(pageGeometry, { Offset(horizontal.value.toFloat(), vertical.value.toFloat()) }, MaterialTheme.colorScheme.primary)
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
