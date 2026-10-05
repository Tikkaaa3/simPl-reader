@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class, androidx.compose.foundation.ExperimentalFoundationApi::class)

package io.github.tikkaaa3.simpl

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun LibraryScreen(state: LibraryState, model: LibraryViewModel, importBooks: () -> Unit,
    importFolder: () -> Unit, settings: () -> Unit, navigationToMissing: () -> Unit, remove: (LibraryBook) -> Unit,
    membership: (LibraryBook) -> Unit, createShelf: () -> Unit) {
    val books = remember(state.books, state.shelves, state.filter, state.query, state.sort) {
        val members = state.shelves.find { "shelf:${it.id}" == state.filter }?.books
        val query = state.query.trim()
        val matching = state.books.filter { book ->
            (state.filter != "favorites" || book.favourite) && (members == null || book.fingerprint in members) &&
                (query.isEmpty() || book.title.contains(query, ignoreCase = true) || book.author?.contains(query, ignoreCase = true) == true)
        }
        when (state.sort) {
            LibrarySort.Recent -> matching
            LibrarySort.Title -> matching.sortedBy { it.title.lowercase() }
            LibrarySort.Author -> matching.sortedWith(compareBy({ it.author?.lowercase() ?: "￿" }, { it.title.lowercase() }))
            LibrarySort.Format -> matching.sortedWith(compareBy({ formatLabel(it.format) }, { it.title.lowercase() }))
        }
    }
    val resume = state.books.firstOrNull { it.openedAt > 0uL && it.progress < 1f }
    var importMenu by rememberSaveable { mutableStateOf(false) }
    var sortMenu by rememberSaveable { mutableStateOf(false) }
    val switchBook = LocalQuickSwitch.current
    val focusNow by rememberUpdatedState(LocalLibraryFocus.current)
    DisposableEffect(Unit) { onDispose { focusNow(false) } }
    val open = { book: LibraryBook -> if (book.missing) navigationToMissing() else model.open(book) }
    AdaptivePanes(sideAtStart = true, modifier = Modifier.onFocusChanged { focusNow(it.hasFocus) }.focusGroup(), side = {
        LibrarySide(state, model, createShelf, switchBook, settings)
    }) { wide ->
    Column(Modifier.fillMaxSize().testTag("library")) {
        TopAppBar(title = { Text("Library", style = MaterialTheme.typography.headlineSmall) },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
            actions = {
                IconButton(onClick = switchBook) { Icon(AppIcons.Search, "Switch book") }
                Box {
                    IconButton(onClick = { importMenu = true }) { Icon(AppIcons.Plus, "Import books") }
                    DropdownMenu(expanded = importMenu, onDismissRequest = { importMenu = false }) {
                        DropdownMenuItem(text = { Text("Import books") }, onClick = { importMenu = false; importBooks() })
                        DropdownMenuItem(text = { Text("Import HTML folder") }, onClick = { importMenu = false; importFolder() })
                    }
                }
                IconButton(onClick = settings) { Icon(AppIcons.Settings, "Settings") }
            })
        LazyVerticalGrid(columns = GridCells.Adaptive(if (wide) 140.dp else 108.dp), state = rememberLazyGridState(),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 4.dp,
                bottom = 24.dp + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()),
            horizontalArrangement = Arrangement.spacedBy(14.dp), verticalArrangement = Arrangement.spacedBy(20.dp),
            modifier = Modifier.weight(1f)) {
            if (state.importing > 0) item(key = "header:import", span = { GridItemSpan(maxLineSpan) }) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Importing ${state.importing} ${if (state.importing == 1) "document" else "documents"}…", style = MaterialTheme.typography.bodySmall)
                    LinearProgressIndicator(Modifier.fillMaxWidth().height(3.dp), trackColor = MaterialTheme.colorScheme.outline, gapSize = 0.dp)
                }
            }
            if (resume != null && state.filter == "all" && state.query.isBlank()) item(key = "header:continue", span = { GridItemSpan(maxLineSpan) }) {
                ContinueCard(resume) { open(resume) }
            }
            item(key = "header:search", span = { GridItemSpan(maxLineSpan) }) {
                OutlinedTextField(value = state.query, onValueChange = model::query, modifier = Modifier.fillMaxWidth().testTag("search"),
                    placeholder = { Text("Search title or author") }, singleLine = true,
                    leadingIcon = { Icon(AppIcons.Search, null) },
                    trailingIcon = { if (state.query.isNotEmpty()) IconButton(onClick = { model.query("") }) { Icon(AppIcons.Close, "Clear search") } })
            }
            if (!wide) item(key = "header:filters", span = { GridItemSpan(maxLineSpan) }) {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    item { FilterChip(selected = state.filter == "all", onClick = { model.filter("all") }, label = { Text("All books") }) }
                    item { FilterChip(selected = state.filter == "favorites", onClick = { model.filter("favorites") }, label = { Text("Favorites") }) }
                    items(state.shelves, key = { it.id.toString() }) { shelf ->
                        FilterChip(selected = state.filter == "shelf:${shelf.id}", onClick = { model.filter("shelf:${shelf.id}") }, label = { Text(shelf.name) })
                    }
                    item { AssistChip(onClick = createShelf, label = { Text("New shelf") }, leadingIcon = { Icon(AppIcons.Plus, null, Modifier.size(18.dp)) }) }
                }
            }
            item(key = "header:sort", span = { GridItemSpan(maxLineSpan) }) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(if (books.size == state.books.size) countLabel(books.size) else "${books.size} of ${countLabel(state.books.size)}",
                        Modifier.weight(1f), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Box {
                        TextButton(onClick = { sortMenu = true }, colors = quietButtonColors(), shape = ControlShape, modifier = Modifier.testTag("sort")) {
                            Text(state.sort.label, style = MaterialTheme.typography.labelLarge)
                            Icon(AppIcons.ChevronRight, null, Modifier.padding(start = 4.dp).size(18.dp).rotate(90f))
                        }
                        DropdownMenu(expanded = sortMenu, onDismissRequest = { sortMenu = false }) {
                            LibrarySort.entries.forEach { sort ->
                                DropdownMenuItem(text = { Text(sort.label) }, onClick = { sortMenu = false; model.sort(sort) },
                                    trailingIcon = if (sort == state.sort) ({ Icon(AppIcons.Check, "Selected") }) else null)
                            }
                        }
                    }
                }
            }
            if (state.loading) item(key = "header:loading", span = { GridItemSpan(maxLineSpan) }) {
                Box(Modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            } else if (books.isEmpty()) item(key = "header:empty", span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(vertical = 32.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(when { state.query.isNotBlank() -> "No matching books"; state.filter == "favorites" -> "No favorites yet";
                        state.filter.startsWith("shelf:") -> "This shelf is empty"; else -> "Make room for a good book" }, style = MaterialTheme.typography.headlineSmall)
                    Text(when { state.query.isNotBlank() -> "Try another title or author."; state.filter == "favorites" -> "Use a book’s menu to add it to Favorites.";
                        state.filter.startsWith("shelf:") -> "Use a book’s menu to add it to this shelf.";
                        else -> "Bring your EPUB, PDF, HTML, TXT or Markdown files." }, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (state.books.isEmpty()) FilledTonalButton(onClick = importBooks, shape = ControlShape) { Text("Import books") }
                    if (state.error != null) TextButton(onClick = model::reload) { Text("Try again") }
                }
            }
            items(books, key = { it.fingerprint }) { book ->
                BookCard(book, open = { open(book) }, favourite = { model.favourite(book) },
                    remove = { remove(book) }, membership = { membership(book) })
            }
        }
    }
    }
}

/** Wide-window shelf rail; the same choices as the chips on a phone. */
@Composable
private fun LibrarySide(state: LibraryState, model: LibraryViewModel, createShelf: () -> Unit, switchBook: () -> Unit, settings: () -> Unit) {
    Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars.only(WindowInsetsSides.Vertical)).verticalScroll(rememberScrollState()).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text("Shelves", Modifier.padding(start = 12.dp, top = 8.dp, bottom = 8.dp), style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        @Composable fun entry(label: String, selected: Boolean, click: () -> Unit) = NavigationDrawerItem(label = { Text(label) }, selected = selected,
            onClick = click, shape = ControlShape, modifier = Modifier.height(44.dp))
        entry("All books", state.filter == "all") { model.filter("all") }
        entry("Favorites", state.filter == "favorites") { model.filter("favorites") }
        state.shelves.forEach { shelf -> entry(shelf.name, state.filter == "shelf:${shelf.id}") { model.filter("shelf:${shelf.id}") } }
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        TextButton(onClick = createShelf, colors = quietButtonColors(), shape = ControlShape) { Text("New shelf") }
        TextButton(onClick = switchBook, colors = quietButtonColors(), shape = ControlShape) { Text("Switch book") }
        TextButton(onClick = settings, colors = quietButtonColors(), shape = ControlShape) { Text("Settings") }
    }
}

@Composable
private fun ContinueCard(book: LibraryBook, open: () -> Unit) {
    Surface(onClick = open, shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainer,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline), modifier = Modifier.fillMaxWidth().testTag("continue")) {
        Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            BookCover(book, Modifier.width(56.dp).aspectRatio(2f / 3f), badge = false)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Continue reading", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                Text(book.title, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                book.author?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1) }
                ThinProgress(book.progress, Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 2.dp))
                Text(progressLabel(book), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Icon(AppIcons.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun BookCard(book: LibraryBook, open: () -> Unit, favourite: () -> Unit, remove: () -> Unit, membership: () -> Unit) {
    var menu by rememberSaveable(book.fingerprint) { mutableStateOf(false) }
    Column(Modifier.testTag("book:${book.fingerprint}")) {
        Box {
            BookCover(book, Modifier.fillMaxWidth().aspectRatio(2f / 3f).testTag("open:${book.fingerprint}")
                .combinedClickable(onClickLabel = "Open ${book.title}", onLongClickLabel = "Book options", onLongClick = { menu = true }, onClick = open))
            if (book.favourite) Icon(AppIcons.Heart, "Favorite", Modifier.align(Alignment.TopEnd).padding(6.dp).size(26.dp)
                .background(Color(0xcc0d1117), CircleShape).padding(5.dp), tint = Color(0xff58a6ff))
        }
        Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f).clickable(onClick = open)) {
                Text(book.title, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(book.author ?: formatLabel(book.format), style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Box {
                IconButton(onClick = { menu = true }, modifier = Modifier.size(28.dp).offset(x = 6.dp)) {
                    Icon(AppIcons.More, "More options for ${book.title}", Modifier.size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text(if (book.favourite) "Remove from favorites" else "Add to favorites") },
                        leadingIcon = { Icon(if (book.favourite) AppIcons.Heart else AppIcons.HeartOutline, null) }, onClick = { menu = false; favourite() })
                    DropdownMenuItem(text = { Text("Add to shelves") }, onClick = { menu = false; membership() })
                    DropdownMenuItem(text = { Text("Remove book") }, onClick = { menu = false; remove() })
                }
            }
        }
        if (book.total > 0u && book.openedAt > 0uL) {
            ThinProgress(book.progress, Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 4.dp), height = 2.dp)
            Text(progressLabel(book), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** Embedded cover, or a quiet generated one in the desktop style: framed title on a muted color. */
@Composable
internal fun BookCover(book: LibraryBook, modifier: Modifier = Modifier, badge: Boolean = true) {
    val image by produceState<ImageBitmap?>(null, book.fingerprint, book.cover) {
        if (book.cover) value = withContext(Dispatchers.IO) {
            runCatching {
                libraryCover(book.fingerprint)?.let { cover ->
                    // Explicit channel packing also works on big-endian hosts.
                    val rgba = cover.rgba
                    val pixels = IntArray(rgba.size / 4) { i ->
                        ((rgba[4 * i + 3].toInt() and 255) shl 24) or ((rgba[4 * i].toInt() and 255) shl 16) or
                            ((rgba[4 * i + 1].toInt() and 255) shl 8) or (rgba[4 * i + 2].toInt() and 255)
                    }
                    android.graphics.Bitmap.createBitmap(pixels, cover.width.toInt(), cover.height.toInt(), android.graphics.Bitmap.Config.ARGB_8888).asImageBitmap()
                }
            }.getOrNull()
        }
    }
    val colors = listOf(Color(0xff223c36), Color(0xff2f4a5c), Color(0xff5a3a3f), Color(0xff4b435e), Color(0xff5a4d36))
    val shape = RoundedCornerShape(4.dp)
    Box(modifier.clip(shape).background(colors[(book.fingerprint.firstOrNull()?.digitToIntOrNull(16) ?: 0) % colors.size])
        .border(1.dp, MaterialTheme.colorScheme.outline, shape)) {
        if (image != null) Image(image!!, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        else BoxWithConstraints(Modifier.fillMaxSize().padding(6.dp).border(1.dp, Color.White.copy(alpha = .28f), RoundedCornerShape(2.dp))) {
            // Scale the title with the cover so thumbnails stay readable.
            val size = (maxWidth.value / 6.2f).coerceIn(8f, 22f)
            // Leave the bottom row to the format badge.
            Column(Modifier.fillMaxSize().padding(maxWidth * .09f).padding(bottom = if (badge) 20.dp else 0.dp), verticalArrangement = Arrangement.SpaceBetween) {
                Text(book.title, style = MaterialTheme.typography.headlineSmall.copy(fontSize = size.sp, lineHeight = (size * 1.25f).sp),
                    color = Color(0xfff2ead8), maxLines = 5, overflow = TextOverflow.Ellipsis)
                book.author?.let { Text(it.uppercase(), style = MaterialTheme.typography.labelSmall.copy(fontSize = (size * .5f).coerceAtLeast(6f).sp),
                    color = Color(0xfff2ead8).copy(alpha = .75f), maxLines = 1) }
            }
        }
        if (badge) Text(formatLabel(book.format), Modifier.align(Alignment.BottomStart).padding(6.dp)
            .background(Color(0xe00d1117), RoundedCornerShape(3.dp)).padding(horizontal = 6.dp, vertical = 2.dp),
            style = MaterialTheme.typography.labelSmall, color = Color(0xffd4deea))
    }
}

private fun countLabel(count: Int) = "$count ${if (count == 1) "book" else "books"}"
