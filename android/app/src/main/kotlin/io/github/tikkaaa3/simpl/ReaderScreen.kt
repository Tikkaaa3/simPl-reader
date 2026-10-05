@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.ui.draw.drawBehind
import androidx.compose.foundation.gestures.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.pointer.*
import androidx.compose.ui.platform.*
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.*
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlin.math.roundToInt

@Composable
internal fun ReaderScreen(book: LibraryBook, model: ReaderViewModel, back: () -> Unit, settings: () -> Unit, document: (() -> Unit)? = null) {
    val state by model.state.collectAsStateWithLifecycle()
    val appDark = MaterialTheme.colorScheme.surface.luminance() < 0.5f
    val dark = when (state.paperAppearance) { "light" -> false; "dark" -> true; else -> appDark }
    // Sheets, menus and bars follow the paper, as the desktop chrome follows its reading theme.
    SimplColors(dark) { ReaderScreenBody(book, model, back, settings, document, appDark, dark) }
}

@Composable
private fun ReaderScreenBody(book: LibraryBook, model: ReaderViewModel, back: () -> Unit, settings: () -> Unit, document: (() -> Unit)?,
    appDark: Boolean, dark: Boolean) {
    val state by model.state.collectAsStateWithLifecycle()
    LaunchedEffect(book.path) { model.open(book) }
    val lifecycle = LocalLifecycleOwner.current
    val activity = requireNotNull(LocalActivity.current)
    var toolbar by rememberSaveable { mutableStateOf(true) }
    var zoom by rememberSaveable { mutableFloatStateOf(1f) }
    var fitWidth by rememberSaveable { mutableStateOf(false) }
    var previousZoom by rememberSaveable { mutableFloatStateOf(1f) }
    var sideVisible by rememberSaveable { mutableStateOf(false) }
    var paneAvailable by remember { mutableStateOf(false) }
    var help by rememberSaveable { mutableStateOf(false) }
    var pageEditing by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val scrollKeys = remember { kotlinx.coroutines.channels.Channel<Int>(kotlinx.coroutines.channels.Channel.CONFLATED) }
    val context = LocalContext.current
    val selectionMeasurer = rememberTextMeasurer()
    val quickSwitch = LocalQuickSwitch.current
    var editing by remember { mutableStateOf<AnnotationEntry?>(null) }
    LaunchedEffect(state.selection) { if (state.selection != null) toolbar = true }
    var panel by rememberSaveable { mutableStateOf<String?>(null) }
    val findState = rememberFindState()
    var dictionary by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(state.selection) { if (state.selection == null) dictionary = false }
    val theme = state.themes.firstOrNull { it.id == state.theme }
    val palette = if (dark) theme?.dark else theme?.light
    val paper = palette?.let { rgbaColor(it.paper) } ?: MaterialTheme.colorScheme.surface
    // On a phone the whole screen is paper: the canonical page keeps its boundaries but no longer floats on a desk.
    val compact = LocalConfiguration.current.screenWidthDp < 600
    val desk = if (compact) paper else palette?.let { rgbaColor(it.background) } ?: MaterialTheme.colorScheme.background
    val text = palette?.let { rgbaColor(it.text) } ?: MaterialTheme.colorScheme.onSurface
    val accent = palette?.let { rgbaColor(it.accent) } ?: MaterialTheme.colorScheme.primary

    DisposableEffect(lifecycle, model) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) model.stop() }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer); model.stop() }
    }
    DisposableEffect(toolbar, dark) {
        val controller = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (toolbar) controller.show(WindowInsetsCompat.Type.systemBars()) else controller.hide(WindowInsetsCompat.Type.systemBars())
        controller.isAppearanceLightStatusBars = !dark
        controller.isAppearanceLightNavigationBars = !dark
        onDispose { controller.show(WindowInsetsCompat.Type.systemBars()); controller.isAppearanceLightStatusBars = !appDark; controller.isAppearanceLightNavigationBars = !appDark }
    }
    fun escape() {
        when { pageEditing -> { focusManager.clearFocus(); keyboard?.hide(); panel = null }
            state.selection != null -> model.select(null); state.note != null -> model.dismissNote(); panel != null -> panel = null;
            state.canReturn -> model.returnFromLink(); !toolbar -> toolbar = true; else -> { model.stop(); back() } }
    }
    BackHandler { escape() }
    ReaderKeys(!state.loading && !state.adapting) { command ->
        when (command) {
            ReaderCommand.Previous -> model.turn(-1)
            ReaderCommand.Next -> model.turn(1)
            ReaderCommand.ScrollUp -> scrollKeys.trySend(-1)
            ReaderCommand.ScrollDown -> scrollKeys.trySend(1)
            ReaderCommand.First -> model.jump("1") {}
            ReaderCommand.Last -> model.jump(state.total.toString()) {}
            ReaderCommand.Find -> { panel = "find"; toolbar = true }
            ReaderCommand.Jump -> { toolbar = true; panel = "jump" }
            ReaderCommand.ZoomIn -> zoom = (zoom * 1.1f).coerceAtMost(3f)
            ReaderCommand.ZoomOut -> zoom = (zoom / 1.1f).coerceAtLeast(.5f)
            ReaderCommand.ResetZoom -> zoom = 1f
            ReaderCommand.Fit -> if (fitWidth) { fitWidth = false; zoom = previousZoom }
                else { previousZoom = zoom; fitWidth = true; zoom = 1f }
            ReaderCommand.Contents -> { panel = "contents"; sideVisible = true }
            ReaderCommand.PreviousChapter -> if (book.format == DocumentFormat.EPUB) model.chapter(-1)
            ReaderCommand.NextChapter -> if (book.format == DocumentFormat.EPUB) model.chapter(1)
            ReaderCommand.Return -> model.returnFromLink()
            ReaderCommand.Copy -> model.copy { (context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(android.content.ClipData.newPlainText("Book text", it)) }
            ReaderCommand.SelectAll -> {
                val rows = measurePage(state.pages, state.options, selectionMeasurer, text, accent).filter { it.text != null && it.lastLine > it.firstLine }
                val first = rows.firstOrNull(); val last = rows.lastOrNull()
                if (first != null && last != null) model.select(ReflowSelection(
                    SourcePoint(first.section, first.row.index, sourceByte(first.row.text.orEmpty(), first.text!!.getLineStart(first.firstLine))),
                    SourcePoint(last.section, last.row.index, sourceByte(last.row.text.orEmpty(), last.text!!.getLineEnd(last.lastLine - 1)))))
            }
            ReaderCommand.Speech -> if (ReadAloud.state.value.active) ReadAloud.stop() else model.readAloud()
            ReaderCommand.Highlight -> model.highlight(AnnotationColor.YELLOW, null)
            ReaderCommand.Bookmark -> model.bookmark()
            ReaderCommand.Panel -> {
                if (paneAvailable) { sideVisible = !sideVisible; panel = null }
                else { panel = if (panel == "annotations") null else "annotations"; sideVisible = true }
            }
            ReaderCommand.Toolbar -> toolbar = !toolbar
            ReaderCommand.Help -> help = true
            ReaderCommand.Close -> { model.stop(); back() }
            ReaderCommand.Escape -> escape()
        }
    }
    CompositionLocalProvider(LocalReaderScroll provides scrollKeys) {
    AdaptivePanes(modifier = Modifier.background(desk).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top)),
        sideVisible = (toolbar && sideVisible) || panel == "find" || panel == "contents",
        idle = { ReaderContentsPanel(state, null) { model.go(it) } }, side = {
        if (panel == "find") {
            if (state.loading || state.adapting) CircularProgressIndicator()
            else FindPanel(findState, model::find, model::searchHit) { panel = null; model.select(null) }
        }
        else if (panel == "contents") ReaderContentsPanel(state, { panel = null; sideVisible = false }, { model.go(it) })
        else AnnotationPanel(state.annotations, { panel = null; sideVisible = false }, model::annotation, model::edit, model::remove, read = { model.readPassage(it) }, book = book)
    }) { wide ->
    SideEffect { paneAvailable = wide }
    Column(Modifier.fillMaxSize().background(desk).testTag("reader").semantics { paneTitle = book.title }) {
        if (state.loading) Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) { Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            if (document != null) {
                val progress = state.preparation
                Text(if ((progress?.total ?: 0u) > 0u) "Preparing Book: ${progress?.completed} of ${progress?.total} pages" else "Preparing Book…", Modifier.testTag("bookPreparation"))
                TextButton(onClick = { model.cancelOpening(); document() }) { Text("Cancel preparation") }
            }
        } }
        else Box(Modifier.weight(1f).fillMaxWidth()) {
            if (state.pages.isNotEmpty()) ReaderViewport(state, model, zoom, fitWidth, compact, { zoom = it }, { toolbar = !toolbar }, paper, text, accent,
                dictionary = { model.selectWord(it) { dictionary = true } }) { id -> editing = state.annotations.highlights.firstOrNull { it.id == id } }
            if (state.canReturn) FilledTonalIconButton(onClick = model::returnFromLink, modifier = Modifier.align(Alignment.TopStart).padding(8.dp)) {
                Icon(AppIcons.Back, "Return from link")
            }
            state.selection?.let { selection -> SelectionSurface(Modifier.align(Alignment.BottomCenter)) {
                key(selection) { Column {
                    SelectionMenu("readerSelection", model::copy, model::highlight, read = { model.readSelection() }, dictionary = { dictionary = true }) { model.select(null) }
                    DictionarySelection(model::copy, selection, dictionary, expand = { dictionary = true }) { dictionary = false }
                } }
            } }
            if (state.adapting) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter).testTag("layoutProgress"))
            if (state.pages.isEmpty() && state.error != null) Column(Modifier.padding(24.dp)) {
                Text(state.error!!, color = text); TextButton(onClick = back) { Text("Return to library") }
            }
        }
        val progress = if (state.total > 0u) state.page.toFloat() / state.total.toFloat() else 0f
        if (toolbar) {
            if (panel != "tools") ReaderSpeechBar(book.fingerprint)
            val chapter = remember(state.contents, state.page) { currentContents(state.contents, state.page)?.label }
            ReaderBottomBar(state.page, state.total, state.pages.firstOrNull()?.layout?.label ?: state.page.toString(),
                caption = listOfNotNull(book.title, chapter?.takeIf { it != book.title }).joinToString(" · "), progress = progress,
                enabled = !state.loading && !state.adapting && state.total > 0u, color = paper, text = text, loading = state.loading,
                numeric = book.format != DocumentFormat.EPUB, requestPageFocus = panel == "jump", pageFocusHandled = { panel = null },
                editingChanged = { pageEditing = it },
                back = { model.stop(); back() }, previous = { model.turn(-1) }, next = { model.turn(1) }, jump = model::jump,
                contents = { panel = "contents"; sideVisible = true }, tools = { panel = "tools" })
        } else if (state.total > 1u) ThinProgress(progress, Modifier.fillMaxWidth().testTag("readingProgress"), height = 2.dp)
    }
    if (panel == "annotations" && !wide) AnnotationSheet(state.annotations, { panel = null }, model::annotation, model::edit, model::remove, read = { model.readPassage(it) }, book = book)
    if (panel == "find" && !wide) ModalBottomSheet(onDismissRequest = { panel = null; model.select(null) }) {
        if (state.loading || state.adapting) CircularProgressIndicator()
        else FindPanel(findState, model::find, model::searchHit) { panel = null; model.select(null) }
    }
    editing?.let { entry -> NoteEditor(entry, { editing = null }) { color, note -> model.edit(entry.id, color, note); editing = null } }
    if (panel == "contents" && !wide) ModalBottomSheet(onDismissRequest = { panel = null }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        ReaderContentsPanel(state, { panel = null }) { model.go(it); panel = null }
    }
    if (panel == "options") ModalBottomSheet(onDismissRequest = { panel = null }) { ReadingOptions(state, model, dark) }
    if (panel == "tools") ReaderToolsSheet(book.title, book.fingerprint, zoom, buildList {
        add(ReaderTool("Contents", AppIcons.Contents, { panel = "contents"; sideVisible = true }))
        add(ReaderTool("Find in book", AppIcons.Search, { panel = "find" }))
        add(ReaderTool("Annotations", AppIcons.Notes, { panel = "annotations"; sideVisible = true }))
        add(ReaderTool("Reading options", AppIcons.Tune, { panel = "options" }))
        val bookmarked = state.annotations.bookmarks.any { it.pageNumber == state.page }
        add(ReaderTool("Bookmark page", if (bookmarked) AppIcons.Bookmark else AppIcons.BookmarkOutline, model::bookmark, selected = bookmarked))
        add(ReaderTool("Switch book", AppIcons.Switch, quickSwitch))
        add(ReaderTool("Settings", AppIcons.Settings, settings))
        if (document != null) add(ReaderTool("Document", AppIcons.Contents, document))
        add(ReaderTool("Hide controls", AppIcons.Hide, { toolbar = false }))
    }, dismiss = { panel = null }, zoomOut = { zoom = (zoom / 1.2f).coerceAtLeast(.5f) },
        zoomIn = { zoom = (zoom * 1.2f).coerceAtMost(3f) }, fitPage = { fitWidth = false; zoom = 1f },
        fitWidth = { fitWidth = true; zoom = 1f }, read = model::readAloud)
    } }
    if (help) KeyboardHelp { help = false }
    ReadAloudError()
    state.note?.let { note ->
        val measurer = rememberTextMeasurer()
        val rows = remember(note, state.options, text, accent) { measureRows(note.section, note.noteRows, state.options, measurer, text, accent) }
        ModalBottomSheet(onDismissRequest = model::dismissNote, containerColor = paper) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text("Note", color = text, style = MaterialTheme.typography.titleLarge)
                TextButton(onClick = model::dismissNote) { Text("Return to reading") }
            }
            BoxWithConstraints(Modifier.fillMaxWidth().heightIn(max = 480.dp).verticalScroll(rememberScrollState()).padding(20.dp)) {
                val scale = constraints.maxWidth / (720f - state.options.margin.toInt() * 2)
                Column { rows.forEach { row -> PaperRow(row, scale, text, accent, model::image, model::follow, {}) } }
            }
        }
    }
    if (state.error != null && state.pages.isNotEmpty()) AlertDialog(onDismissRequest = model::dismissError,
        title = { Text("Reader") }, text = { Text(state.error!!) }, confirmButton = { TextButton(onClick = model::dismissError) { Text("OK") } })
}

@Composable
private fun ReaderContentsPanel(state: ReaderState, dismiss: (() -> Unit)?, go: (ReaderLocation) -> Unit) {
    val accentBar = MaterialTheme.colorScheme.primary
    Column(Modifier.fillMaxWidth().testTag("contentsPanel")) {
        Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Contents", Modifier.weight(1f).padding(vertical = 12.dp), style = MaterialTheme.typography.headlineSmall)
            if (dismiss != null) IconButton(onClick = dismiss, modifier = Modifier.size(48.dp)) { Icon(AppIcons.Close, "Close contents") }
        }
        val current = remember(state.contents, state.page) { currentContents(state.contents, state.page) }
        val list = rememberLazyListState(initialFirstVisibleItemIndex = state.contents.indexOf(current).coerceAtLeast(0))
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 520.dp), state = list) {
            if (state.contents.isEmpty()) item { Text("No contents entries in this book", Modifier.padding(20.dp)) }
            items(state.contents) { entry ->
                // Desktop marks the current chapter with a slim accent bar and brighter text.
                val active = entry === current
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable { go(entry.location) }
                    .semantics { if (active) selected = true }.drawBehind {
                        if (active) drawRect(accentBar, size = androidx.compose.ui.geometry.Size(2.dp.toPx(), size.height))
                    }.padding(start = 20.dp + (entry.depth.toInt().coerceAtMost(8) * 14).dp, end = 20.dp),
                    verticalAlignment = Alignment.CenterVertically) {
                    Text(entry.label, Modifier.weight(1f).padding(vertical = 10.dp), style = MaterialTheme.typography.bodyMedium,
                        color = if (active) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(entry.location.page.toString(), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable
private fun ReaderViewport(state: ReaderState, model: ReaderViewModel, zoom: Float, fitWidth: Boolean, topAligned: Boolean, zoomTo: (Float) -> Unit, toggle: () -> Unit,
    paper: Color, text: Color, accent: Color, dictionary: (SourcePoint) -> Unit, editMark: (ULong) -> Unit) {
    val measurer = rememberTextMeasurer(cacheSize = 64)
    val measured = remember(state.pages, state.options, text, accent) { measurePage(state.pages, state.options, measurer, text, accent) }
    val spoken = state.spoken
    val spokenRow = state.speechRow
    val spokenLayout = remember(spokenRow, state.options, text, accent) {
        spokenRow?.let { measureRows(0u, listOf(it), state.options, measurer, text, accent).firstOrNull()?.text }
    }
    LaunchedEffect(spoken, spokenLayout, state.revision, state.adapting) {
        if (spoken != null && spokenRow?.index == spoken.from.row && spokenLayout != null) {
            val line = spokenLayout.getLineForOffset(byteIndex(spokenRow.text.orEmpty(), spoken.from.byte))
            model.speechFollow(spoken.from, line.toUInt(), spokenLayout.lineCount.toUInt())
        }
    }
    val density = LocalDensity.current.density
    val vertical = rememberScrollState()
    val horizontal = rememberScrollState()
    val placement = remember { PagePlacement() }
    var gestureRevision by remember { mutableIntStateOf(0) }
    val currentToggle by rememberUpdatedState(toggle)
    val currentZoom by rememberUpdatedState(zoom)
    val currentZoomTo by rememberUpdatedState(zoomTo)
    val scrollKeys = LocalReaderScroll.current
    var viewportHeight by remember { mutableIntStateOf(0) }
    LaunchedEffect(scrollKeys) { if (scrollKeys != null) for (delta in scrollKeys) vertical.animateScrollBy(viewportHeight * .8f * delta) }
    BoxWithConstraints(Modifier.fillMaxSize().testTag("paperViewport").semantics { stateDescription = "Zoom $zoom; page ${state.page}" }
        .pointerInput(state.page, zoom <= 1.01f, state.selection != null) {
            if (zoom <= 1.01f && state.selection == null) {
                var distance = 0f
                detectHorizontalDragGestures(onDragStart = { distance = 0f }, onDragEnd = {
                    if (kotlin.math.abs(distance) > 48 * density) model.turn(if (distance < 0) 1 else -1)
                }) { change, amount -> change.consume(); distance += amount }
            }
        }
        .pointerInput(Unit) {
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false)
                var gestureZoom = currentZoom
                do {
                    val event = awaitPointerEvent()
                    if (event.changes.count { it.pressed } >= 2) {
                        val focus = event.calculateCentroid()
                        placement.zoomAt(if (event.changes.count { it.previousPressed } >= 2) event.calculateCentroid(useCurrent = false) else focus, focus)
                        gestureZoom = (gestureZoom * event.calculateZoom()).coerceIn(.5f, 3f)
                        currentZoomTo(gestureZoom)
                        gestureRevision++
                        event.changes.forEach { it.consume() }
                    }
                } while (event.changes.any { it.pressed })
            }
        }) {
        val heights = remember(measured) {
            val starts = mutableListOf(42f)
            measured.forEach { starts += starts.last() + it.height }
            starts
        }
        val logicalHeight = maxOf(720f * 1.414f, heights.last() + 42f)
        val geometry = PageGeometry.fitted(constraints.maxWidth.toFloat(), constraints.maxHeight.toFloat(), 720f, logicalHeight, zoom, fitWidth, topAligned)
        val scale = geometry.scale
        SideEffect { viewportHeight = constraints.maxHeight }
        TrackPageScroll(placement, geometry, state.revision, horizontal, vertical)
        val viewportWidth = constraints.maxWidth
        val paperWidth = (geometry.width / density).dp
        fun hit(point: Offset): SourcePoint? {
            val sourcePoint = geometry.source(point, Offset(horizontal.value.toFloat(), vertical.value.toFloat()))
            val y = sourcePoint.y
            val index = (heights.indexOfLast { it <= y }).coerceIn(0, measured.lastIndex.coerceAtLeast(0))
            val row = measured.getOrNull(index) ?: return null
            val layout = row.text ?: return null
            val source = Offset(sourcePoint.x - state.options.margin.toInt() - row.row.presentation.left,
                (y - heights[index] - row.top + row.textTop).coerceIn(row.textTop, row.textBottom - .01f))
            val offset = layout.getOffsetForPosition(source).coerceIn(layout.getLineStart(row.firstLine), layout.getLineEnd(row.lastLine - 1))
            return SourcePoint(row.section, row.row.index, sourceByte(row.row.text.orEmpty(), offset))
        }
        fun handle(point: SourcePoint?): Offset? {
            val index = measured.indexOfFirst { it.section == point?.section && it.row.index == point.row }
            if (index < 0) return null
            val row = measured[index]; val layout = row.text ?: return null
            val offset = byteIndex(row.row.text.orEmpty(), point!!.byte)
            val line = layout.getLineForOffset(offset)
            if (line !in row.firstLine until row.lastLine) return null
            val rect = layout.getCursorRect(offset)
            val result = geometry.screen(Offset(state.options.margin.toInt() + row.row.presentation.left + rect.left,
                heights[index] + row.top - row.textTop + rect.bottom), Offset(horizontal.value.toFloat(), vertical.value.toFloat()))
            return result.takeIf { it.y in 0f..constraints.maxHeight.toFloat() }
        }
        val handles = handle(state.selection?.from) to handle(state.selection?.to)
        fun lift(point: SourcePoint?): Float {
            val row = measured.firstOrNull { it.section == point?.section && it.row.index == point.row } ?: return 0f
            val layout = row.text ?: return 0f
            val line = layout.getLineForOffset(byteIndex(row.row.text.orEmpty(), point!!.byte))
            return (layout.getLineBottom(line) - layout.getLineTop(line)) * scale / 2
        }
        var dragging by remember { mutableStateOf<Offset?>(null) }
        var draggingStart by remember { mutableStateOf(false) }
        val gestures = selectionGesture(handles, { point ->
            hit(point)?.let { source -> model.selectWord(source); true } ?: false
        }, { point, start -> draggingStart = start; hit(point)?.let { model.extend(it, start) } }, { dragging = it }, lift(state.selection?.from) to lift(state.selection?.to))
        SelectionEdgeDrag(dragging, constraints.maxHeight.toFloat(), { vertical.scrollBy(it) }, { model.selectionTurn(it, draggingStart) }) { point -> hit(point)?.let { model.extend(it, draggingStart) } }
        var positionReady by remember(state.revision) { mutableStateOf(false) }
        LaunchedEffect(state.revision, geometry, gestureRevision) {
            positionReady = false
            val target = if (placement.revision == state.revision) placement.resized(geometry) else {
                val location = state.location
                val index = measured.indexOfFirst { it.section == location?.section && it.row.index == location.row }
                val y = if (index < 0) 0f else {
                    val row = measured[index]
                    val part = ((location!!.within - row.startFraction) / (row.endFraction - row.startFraction).coerceAtLeast(0.001f)).coerceIn(0f, 1f)
                    (heights[index] + row.height * part) * scale + geometry.inset.y
                }
                geometry.clamp(Offset(geometry.maximum.x / 2, y))
            }
            withFrameNanos { }
            horizontal.scrollTo(target.x.roundToInt())
            vertical.scrollTo(target.y.roundToInt())
            placement.geometry = geometry; placement.revision = state.revision
            placement.scroll = Offset(horizontal.value.toFloat(), vertical.value.toFloat()); placement.pending = null
            state.selectionEdge?.let { start ->
                hit(Offset(constraints.maxWidth * .5f, if (start) 0f else constraints.maxHeight.toFloat()))?.let(model::completeSelectionEdge)
            }
            positionReady = true
        }
        LaunchedEffect(measured, state.revision, geometry) {
            snapshotFlow { vertical.value }.distinctUntilChanged().collect { offset ->
                if (positionReady && measured.isNotEmpty()) {
                    val y = (offset - geometry.inset.y) / scale
                    val index = (heights.indexOfLast { it <= y }).coerceIn(0, measured.lastIndex)
                    val row = measured[index]
                    val part = ((y - heights[index]) / row.height.coerceAtLeast(1f)).coerceIn(0f, 1f)
                    model.record(ReaderLocation(state.page, row.section, row.row.index, row.startFraction + part * (row.endFraction - row.startFraction)))
                }
            }
        }
        LaunchedEffect(state.spoken, measured, scale) {
            val word = state.spoken ?: return@LaunchedEffect
            val index = measured.indexOfFirst { it.section == word.from.section && it.row.index == word.from.row }
            val row = measured.getOrNull(index) ?: return@LaunchedEffect
            val layout = row.text ?: return@LaunchedEffect
            val line = layout.getLineForOffset(byteIndex(row.row.text.orEmpty(), word.from.byte))
            if (line !in row.firstLine until row.lastLine) return@LaunchedEffect
            val top = (heights[index] + row.top - row.textTop + layout.getLineTop(line)) * scale + geometry.inset.y
            val bottom = top + row.row.presentation.lineHeight * scale
            if (top < vertical.value || bottom > vertical.value + constraints.maxHeight) {
                vertical.scrollTo((top - constraints.maxHeight * .25f).roundToInt().coerceAtLeast(0))
            }
        }
        LaunchedEffect(state.searchPoint, measured, scale, state.revision) {
            val point = state.searchPoint ?: return@LaunchedEffect
            val index = measured.indexOfFirst { it.section == point.section && it.row.index == point.row }
            val row = measured.getOrNull(index) ?: return@LaunchedEffect
            val layout = row.text ?: return@LaunchedEffect
            val line = layout.getLineForOffset(byteIndex(row.row.text.orEmpty(), point.byte))
            if (line !in row.firstLine until row.lastLine) return@LaunchedEffect
            withFrameNanos { }
            val top = (heights[index] + row.top - row.textTop + layout.getLineTop(line)) * scale + geometry.inset.y
            vertical.scrollTo((top - constraints.maxHeight * .25f).roundToInt().coerceAtLeast(0))
        }
        val tap: (Offset) -> Unit = { point ->
            // Row offsets are within the text column; margins count as edge zones too.
            val x = point.x + geometry.inset.x + state.options.margin.toInt() * scale - horizontal.value
            when { state.selection != null -> model.select(null); zoom <= 1.01f && x < constraints.maxWidth * 0.18f -> model.turn(-1);
                zoom <= 1.01f && x > constraints.maxWidth * 0.82f -> model.turn(1); else -> currentToggle() }
        }
        Box(Modifier.fillMaxSize().then(gestures).pointerInput(state.page, zoom, geometry.viewportWidth, geometry.viewportHeight) {
            detectTapGestures { point ->
                when { state.selection != null -> model.select(null)
                    zoom <= 1.01f && point.x < geometry.viewportWidth * .18f -> model.turn(-1)
                    zoom <= 1.01f && point.x > geometry.viewportWidth * .82f -> model.turn(1)
                    else -> currentToggle() }
            }
        }) {
          Box(Modifier.fillMaxSize().testTag("pageScroll").verticalScroll(vertical, enabled = dragging == null)
              .horizontalScroll(horizontal, enabled = geometry.maximum.x > 1f && dragging == null)) {
            Box(Modifier.width((maxOf(geometry.width, geometry.viewportWidth) / density).dp)
                .height((maxOf(geometry.height, geometry.viewportHeight) / density).dp)) {
            Column(Modifier.align(if (topAligned) Alignment.TopCenter else Alignment.Center).requiredWidth(paperWidth).requiredHeight((geometry.height / density).dp).testTag("bookPage")
                .background(paper).pointerInput(scale) { detectTapGestures { point ->
                    when { state.selection != null -> model.select(null); zoom <= 1.01f && point.x + geometry.inset.x - horizontal.value < viewportWidth * 0.18f -> model.turn(-1);
                        zoom <= 1.01f && point.x + geometry.inset.x - horizontal.value > viewportWidth * 0.82f -> model.turn(1); else -> currentToggle() }
                } }
                .padding(horizontal = (state.options.margin.toInt() * scale / density).dp, vertical = (42f * scale / density).dp)) {
                measured.forEach { row -> key(row.section, row.row.index) { PaperRow(row, scale, text, accent, model::image, model::follow, tap,
                    selection = state.selection, spoken = state.spoken, marks = state.marks.filter { it.section == row.section && it.row == row.row.index }, editMark = editMark,
                    extend = { model.extend(it, false) }, dictionary = dictionary) } }
            }
            }
          }
          PageScrollIndicator(geometry, { Offset(horizontal.value.toFloat(), vertical.value.toFloat()) }, accent)
          SelectionHandle(handles.first, true)
          SelectionHandle(handles.second, false)
        }
    }
}

@Composable
private fun ReadingOptions(state: ReaderState, model: ReaderViewModel, dark: Boolean) {
    val options = state.options
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Reading options", style = MaterialTheme.typography.headlineSmall)
        Text("Theme", Modifier.padding(top = 4.dp), style = MaterialTheme.typography.titleSmall)
        ReadingThemePicker(state.theme, dark, Modifier.fillMaxWidth(), edge = 0.dp) {
            model.change(theme = it)
        }
        Text("Paper", Modifier.padding(top = 4.dp), style = MaterialTheme.typography.titleSmall)
        ChoiceRow(listOf("auto" to "Device", "light" to "Light paper", "dark" to "Dark paper"), state.paperAppearance,
            Modifier.fillMaxWidth(), model::paperAppearance)
        Text("Font", Modifier.padding(top = 4.dp), style = MaterialTheme.typography.titleSmall)
        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(ReadingFont.entries) { font -> FilterChip(options.font == font, onClick = { model.change(options.copy(font = font)) },
                label = { Text(when (font) { ReadingFont.THEME -> "Theme font"; ReadingFont.LITERATA -> "Literata"; ReadingFont.SPECTRAL -> "Spectral"; ReadingFont.FIRA_SANS -> "Fira Sans" }) }) }
        }
        OptionStepper("Font size", options.size.toInt().toString(), "font size", options.size.toInt() > 12, options.size.toInt() < 36,
            { model.change(options.copy(size = (options.size.toInt() - 2).coerceAtLeast(12).toUShort())) },
            { model.change(options.copy(size = (options.size.toInt() + 2).coerceAtMost(36).toUShort())) })
        OptionStepper("Line spacing", if (options.spacing == 0.toUShort()) "Theme" else "${options.spacing}%", "line spacing", options.spacing.toInt() != 110, options.spacing.toInt() < 220,
            { model.change(options.copy(spacing = ((if (options.spacing == 0.toUShort()) 160 else options.spacing.toInt()) - 10).coerceAtLeast(110).toUShort())) },
            { model.change(options.copy(spacing = ((if (options.spacing == 0.toUShort()) 160 else options.spacing.toInt()) + 10).coerceAtMost(220).toUShort())) })
        OptionStepper("Margins", options.margin.toInt().toString(), "margins", options.margin.toInt() > 16, options.margin.toInt() < 96,
            { model.change(options.copy(margin = (options.margin.toInt() - 8).coerceAtLeast(16).toUShort())) },
            { model.change(options.copy(margin = (options.margin.toInt() + 8).coerceAtMost(96).toUShort())) })
        TextButton(onClick = { model.change(LayoutOptions(ReadingFont.THEME, 20u, 48u, 0u)) }, colors = quietButtonColors(), shape = ControlShape) { Text("Reset typography") }
        Text("A theme sets the reading font, spacing and colors. Page numbers stay the same in every theme. Pages fit the screen by default. Pinch to zoom and drag to move. Swipe or tap a page edge to turn. Tap the center to hide controls.", style = MaterialTheme.typography.bodySmall)
        if (state.warnings.isNotEmpty()) Text(state.warnings.joinToString("\n"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(16.dp))
    }
}

@Composable
private fun OptionStepper(title: String, value: String, label: String, decrease: Boolean, increase: Boolean, less: () -> Unit, more: () -> Unit) {
    val outline = androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.outline)
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f))
        OutlinedIconButton(onClick = less, enabled = decrease, shape = ControlShape, border = outline,
            modifier = Modifier.semantics { contentDescription = "Decrease $label" }) { Text("−", style = MaterialTheme.typography.titleMedium) }
        Text(value, Modifier.widthIn(min = 64.dp), textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        OutlinedIconButton(onClick = more, enabled = increase, shape = ControlShape, border = outline,
            modifier = Modifier.semantics { contentDescription = "Increase $label" }) { Text("+", style = MaterialTheme.typography.titleMedium) }
    }
}

/** The contents entry the reader is in: the last one starting at or before this page. */
internal fun currentContents(contents: List<ReaderContents>, page: UInt): ReaderContents? =
    contents.withIndex().filter { it.value.location.page <= page }.maxWithOrNull(compareBy({ it.value.location.page }, { it.index }))?.value
