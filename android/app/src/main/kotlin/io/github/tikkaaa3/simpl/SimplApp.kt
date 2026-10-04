@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package io.github.tikkaaa3.simpl

import android.content.Intent
import androidx.core.net.toUri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.navigation.compose.*
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
fun SimplApp(state: LibraryState, model: LibraryViewModel) {
    val navigation = rememberNavController()
    val snackbars = remember { SnackbarHostState() }
    val context = LocalContext.current
    var quick by rememberSaveable { mutableStateOf(false) }
    var recent by rememberSaveable { mutableStateOf(false) }
    var help by rememberSaveable { mutableStateOf(false) }
    var libraryFocused by remember { mutableStateOf(false) }
    val documents = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        uris.forEach { uri -> runCatching { context.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) } }
        model.enqueue(uris)
    }
    val folder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        if (uri != null) {
            runCatching { context.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
            model.enqueue(listOf(uri), tree = true)
        }
    }
    LaunchedEffect(state.openRequest) {
        state.openRequest?.let { fingerprint ->
            navigation.navigate("reader/$fingerprint") { launchSingleTop = true; popUpTo("library") }
            model.navigated()
        }
    }
    LaunchedEffect(state.error, state.notice) {
        val message = state.error ?: state.notice
        if (message != null) {
            snackbars.showSnackbar(message, withDismissAction = true,
                duration = if (state.error != null) SnackbarDuration.Long else SnackbarDuration.Short)
            model.dismissMessage()
        }
    }
    var remove by rememberSaveable { mutableStateOf<String?>(null) }
    var membership by rememberSaveable { mutableStateOf<String?>(null) }
    var editShelf by rememberSaveable { mutableStateOf<String?>(null) }
    var deleteShelf by rememberSaveable { mutableStateOf<String?>(null) }
    val settings = { navigation.navigate("settings") { launchSingleTop = true } }
    val activity = androidx.activity.compose.LocalActivity.current as MainActivity
    val currentRoute by navigation.currentBackStackEntryAsState()
    val route = currentRoute?.destination?.route
    val keys by rememberUpdatedState<(android.view.KeyEvent) -> Boolean> { event ->
        when {
            event.isCtrlPressed && event.keyCode in listOf(android.view.KeyEvent.KEYCODE_K, android.view.KeyEvent.KEYCODE_R) -> { if (event.repeatCount == 0) { recent = event.keyCode == android.view.KeyEvent.KEYCODE_R; quick = true }; true }
            event.isCtrlPressed && event.keyCode == android.view.KeyEvent.KEYCODE_O -> { if (event.repeatCount == 0) documents.launch(arrayOf("*/*")); true }
            event.isCtrlPressed && event.keyCode == android.view.KeyEvent.KEYCODE_W && route?.startsWith("reader") != true -> { navigation.popBackStack("library", false); true }
            event.keyCode == android.view.KeyEvent.KEYCODE_F1 && route?.startsWith("reader") != true -> { help = true; true }
            event.keyCode == android.view.KeyEvent.KEYCODE_SPACE && route == "library" && !libraryFocused && !quick -> {
                val resume = state.books.firstOrNull { it.openedAt > 0uL && it.progress < 1f }
                if (resume != null && event.repeatCount == 0) { if (resume.missing) navigation.navigate("backups") else model.open(resume) }
                resume != null
            }
            else -> false
        }
    }
    DisposableEffect(activity) { val handler: (android.view.KeyEvent) -> Boolean = { keys(it) }; activity.appKeys = handler; onDispose { if (activity.appKeys === handler) activity.appKeys = null } }
    CompositionLocalProvider(LocalQuickSwitch provides { recent = false; quick = true }, LocalLibraryFocus provides { libraryFocused = it }) {
    Scaffold(snackbarHost = { SnackbarHost(snackbars) },
        contentWindowInsets = WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom)) { padding ->
        NavHost(navigation, startDestination = "library", modifier = Modifier.padding(padding)) {
            composable("library") {
                LibraryScreen(state, model,
                    importBooks = { documents.launch(arrayOf("*/*")) },
                    importFolder = { folder.launch(null) }, settings = settings,
                    navigationToMissing = { navigation.navigate("backups") { popUpTo("library") } },
                    remove = { remove = it.fingerprint }, membership = { membership = it.fingerprint },
                    createShelf = { editShelf = "new" })
            }
            composable("settings") {
                SettingsScreen(state, model, back = { navigation.popBackStack() },
                    dictionaries = { navigation.navigate("dictionaries") { launchSingleTop = true } },
                    backups = { navigation.navigate("backups") { launchSingleTop = true; popUpTo("library") } },
                    licenses = { navigation.navigate("licenses") { launchSingleTop = true } },
                    createShelf = { editShelf = "new" }, editShelf = { editShelf = it.id.toString() },
                    deleteShelf = { deleteShelf = it.id.toString() })
            }
            composable("licenses") { LicenseScreen { navigation.popBackStack() } }
            composable("dictionaries") { DictionaryScreen(back = { navigation.popBackStack() }) }
            composable("backups") { BackupScreen(state, model::locate, back = { navigation.popBackStack() }, restored = {
                model.restored(); navigation.navigate("library") { popUpTo("library") { inclusive = true }; launchSingleTop = true }
            }) }
            composable("reader/{fingerprint}") { entry ->
                val fingerprint = entry.arguments?.getString("fingerprint")
                val book = state.books.find { it.fingerprint == fingerprint }
                if (book != null && book.format == DocumentFormat.PDF) PdfReaderScreen(book,
                    back = { navigation.popBackStack(); model.reload() }, settings = settings)
                else if (book != null) ReaderScreen(book, viewModel(viewModelStoreOwner = entry),
                    back = { navigation.popBackStack(); model.reload() }, settings = settings)
                else BookOverview(book,
                    back = { navigation.popBackStack() }, settings = settings,
                    membership = { membership = it.fingerprint }, model = model)
            }
        }
    }
    }
    if (quick) QuickSwitch(state.books, recent, { quick = false }) { book ->
        quick = false; ReadAloud.stop()
        if (book.missing) navigation.navigate("backups") { popUpTo("library") } else model.open(book)
    }
    if (help) KeyboardHelp { help = false }
    state.books.find { it.fingerprint == remove }?.let { book ->
        AlertDialog(onDismissRequest = { remove = null }, title = { Text("Remove book?") },
            text = { Text("Remove “${book.title}” and its private copy from your library? Your original file and saved annotations are kept.") },
            confirmButton = { TextButton(onClick = { model.remove(book); remove = null }) { Text("Remove") } },
            dismissButton = { TextButton(onClick = { remove = null }) { Text("Cancel") } })
    }
    state.books.find { it.fingerprint == membership }?.let { book ->
        AlertDialog(onDismissRequest = { membership = null }, title = { Text("Add to shelves") },
            text = {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    if (state.shelves.isEmpty()) Text("Create a shelf to organize your books.")
                    state.shelves.forEach { shelf ->
                        Row(Modifier.fillMaxWidth().clickable { model.toggleShelf(shelf.id, book) }.padding(vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(checked = book.fingerprint in shelf.books, onCheckedChange = { model.toggleShelf(shelf.id, book) })
                            Text(shelf.name, Modifier.weight(1f))
                        }
                    }
                    TextButton(onClick = { editShelf = "new" }) { Text("New shelf") }
                }
            }, confirmButton = { TextButton(onClick = { membership = null }) { Text("Done") } })
    }
    editShelf?.let { id ->
        val shelf = state.shelves.find { it.id.toString() == id }
        if (id == "new" || shelf != null) ShelfEditor(id, shelf?.name.orEmpty(),
            dismiss = { editShelf = null }, save = { name ->
                if (shelf == null) model.createShelf(name) else model.renameShelf(shelf.id, name)
                editShelf = null
            })
    }
    state.shelves.find { it.id.toString() == deleteShelf }?.let { shelf ->
        AlertDialog(onDismissRequest = { deleteShelf = null }, title = { Text("Delete shelf?") },
            text = { Text("Delete “${shelf.name}”? Its books stay in your library.") },
            confirmButton = { TextButton(onClick = { model.deleteShelf(shelf.id); deleteShelf = null }) { Text("Delete") } },
            dismissButton = { TextButton(onClick = { deleteShelf = null }) { Text("Cancel") } })
    }
}

@Composable
private fun LibraryScreen(state: LibraryState, model: LibraryViewModel, importBooks: () -> Unit,
    importFolder: () -> Unit, settings: () -> Unit, navigationToMissing: () -> Unit, remove: (LibraryBook) -> Unit,
    membership: (LibraryBook) -> Unit, createShelf: () -> Unit) {
    val books = remember(state.books, state.shelves, state.filter, state.query) {
        val members = state.shelves.find { "shelf:${it.id}" == state.filter }?.books
        state.books.filter { book ->
            (state.filter != "favorites" || book.favourite) && (members == null || book.fingerprint in members) &&
                (state.query.isBlank() || book.title.contains(state.query.trim(), ignoreCase = true) ||
                    book.author?.contains(state.query.trim(), ignoreCase = true) == true)
        }
    }
    val resume = state.books.firstOrNull { it.openedAt > 0uL && it.progress < 1f }
    var importMenu by rememberSaveable { mutableStateOf(false) }
    val switchBook = LocalQuickSwitch.current
    val focusNow by rememberUpdatedState(LocalLibraryFocus.current)
    DisposableEffect(Unit) { onDispose { focusNow(false) } }
    AdaptivePanes(sideAtStart = true, modifier = Modifier.onFocusChanged { focusNow(it.hasFocus) }.focusGroup(), side = {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp)) {
            Text("Library", style = MaterialTheme.typography.headlineSmall)
            FilterChip(state.filter == "all", { model.filter("all") }, label = { Text("All books") })
            FilterChip(state.filter == "favorites", { model.filter("favorites") }, label = { Text("Favorites") })
            state.shelves.forEach { shelf -> FilterChip(state.filter == "shelf:${shelf.id}", { model.filter("shelf:${shelf.id}") }, label = { Text(shelf.name) }) }
            TextButton(onClick = createShelf) { Text("New shelf") }
            TextButton(onClick = switchBook) { Text("Switch book") }
            TextButton(onClick = settings) { Text("Settings") }
        }
    }) { wide ->
    Column(Modifier.fillMaxSize().testTag("library")) {
        TopAppBar(title = { Text("simPl", style = MaterialTheme.typography.headlineSmall) }, actions = {
            IconButton(onClick = switchBook) { Icon(AppIcons.Search, "Switch book") }
            IconButton(onClick = settings) { Icon(AppIcons.Settings, "Settings") }
        })
        LazyVerticalGrid(columns = GridCells.Adaptive(148.dp), state = rememberLazyGridState(),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 24.dp),
            horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier.weight(1f)) {
            item(key = "header:title", span = { GridItemSpan(maxLineSpan) }) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Your library", style = MaterialTheme.typography.headlineLarge)
                        Text("${state.books.size} ${if (state.books.size == 1) "book" else "books"}", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Box {
                        FilledTonalIconButton(onClick = { importMenu = true }) { Icon(AppIcons.Plus, "Import books") }
                        DropdownMenu(expanded = importMenu, onDismissRequest = { importMenu = false }) {
                            DropdownMenuItem(text = { Text("Import books") }, onClick = { importMenu = false; importBooks() })
                            DropdownMenuItem(text = { Text("Import HTML folder") }, onClick = { importMenu = false; importFolder() })
                        }
                    }
                }
            }
            if (state.importing > 0) item(key = "header:import", span = { GridItemSpan(maxLineSpan) }) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Importing ${state.importing} ${if (state.importing == 1) "document" else "documents"}…")
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                }
            }
            if (resume != null && state.filter == "all" && state.query.isBlank()) item(key = "header:continue", span = { GridItemSpan(maxLineSpan) }) {
                Card(onClick = { if (resume.missing) navigationToMissing() else model.open(resume) }, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer)) {
                    Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        BookCover(resume, Modifier.width(62.dp).aspectRatio(2f / 3f))
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text("CONTINUE", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                            Text(resume.title, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                            Text(progressLabel(resume), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
            item(key = "header:search", span = { GridItemSpan(maxLineSpan) }) {
                OutlinedTextField(value = state.query, onValueChange = model::query, modifier = Modifier.fillMaxWidth().testTag("search"),
                    placeholder = { Text("Search title or author") }, singleLine = true,
                    leadingIcon = { Icon(AppIcons.Search, null) },
                    trailingIcon = { if (state.query.isNotEmpty()) TextButton(onClick = { model.query("") }) { Text("Clear") } })
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
            if (state.loading) item(key = "header:loading", span = { GridItemSpan(maxLineSpan) }) {
                Box(Modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            } else if (books.isEmpty()) item(key = "header:empty", span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(vertical = 32.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(when { state.query.isNotBlank() -> "No matching books"; state.filter == "favorites" -> "No favorites yet";
                        state.filter.startsWith("shelf:") -> "This shelf is empty"; else -> "Make room for a good book" }, style = MaterialTheme.typography.headlineSmall)
                    Text(when { state.query.isNotBlank() -> "Try another title or author."; state.filter == "favorites" -> "Tap a heart to keep a book here.";
                        state.filter.startsWith("shelf:") -> "Use a book’s menu to add it to this shelf.";
                        else -> "Bring your EPUB, PDF, HTML, TXT or Markdown files." }, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (state.books.isEmpty()) Button(onClick = importBooks) { Text("Import books") }
                    if (state.error != null) TextButton(onClick = model::reload) { Text("Try again") }
                }
            }
            items(books, key = { it.fingerprint }) { book ->
                BookCard(book, open = { if (book.missing) navigationToMissing() else model.open(book) }, favourite = { model.favourite(book) },
                    remove = { remove(book) }, membership = { membership(book) })
            }
        }
    }
    }
}

@Composable
private fun BookCard(book: LibraryBook, open: () -> Unit, favourite: () -> Unit, remove: () -> Unit, membership: () -> Unit) {
    var menu by rememberSaveable(book.fingerprint) { mutableStateOf(false) }
    Column(Modifier.testTag("book:${book.fingerprint}")) {
        Box {
            BookCover(book, Modifier.fillMaxWidth().aspectRatio(2f / 3f).testTag("open:${book.fingerprint}").clickable(onClickLabel = "Open ${book.title}", onClick = open))
            FilledTonalIconButton(onClick = favourite, modifier = Modifier.align(Alignment.TopEnd).padding(4.dp)) {
                Icon(AppIcons.Heart, if (book.favourite) "Unfavorite ${book.title}" else "Favorite ${book.title}",
                    tint = if (book.favourite) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f).clickable(onClick = open).padding(top = 10.dp)) {
                Text(book.title, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(book.author ?: formatLabel(book.format), style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Box {
                IconButton(onClick = { menu = true }) { Icon(AppIcons.More, "More options for ${book.title}") }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text("Add to shelves") }, onClick = { menu = false; membership() })
                    DropdownMenuItem(text = { Text("Remove book") }, onClick = { menu = false; remove() })
                }
            }
        }
        if (book.total > 0u) {
            LinearProgressIndicator(progress = { book.progress.coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth().padding(top = 6.dp))
            Text(progressLabel(book), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
internal fun BookCover(book: LibraryBook, modifier: Modifier = Modifier) {
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
    val colors = listOf(Color(0xff193858), Color(0xff384c3d), Color(0xff543d43), Color(0xff4b435e), Color(0xff594c37))
    Box(modifier.clip(RoundedCornerShape(8.dp)).background(colors[(book.fingerprint.firstOrNull()?.digitToIntOrNull(16) ?: 0) % colors.size])) {
        if (image != null) Image(image!!, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        else Column(Modifier.fillMaxSize().padding(14.dp), verticalArrangement = Arrangement.SpaceBetween) {
            Text(formatLabel(book.format), style = MaterialTheme.typography.labelSmall, color = Color(0xffd4deea))
            Text(book.title, style = MaterialTheme.typography.headlineSmall, color = Color.White, maxLines = 4, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun SettingsScreen(state: LibraryState, model: LibraryViewModel, back: () -> Unit,
    dictionaries: () -> Unit,
    backups: () -> Unit,
    licenses: () -> Unit,
    createShelf: () -> Unit, editShelf: (LibraryShelf) -> Unit, deleteShelf: (LibraryShelf) -> Unit) {
    val context = LocalContext.current
    val controls by ReadingControls.state.collectAsState()
    var keyboardHelp by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().testTag("settings")) {
        TopAppBar(title = { Text("Settings") }, navigationIcon = { IconButton(onClick = back) { Icon(AppIcons.Back, "Back") } })
        LazyColumn(Modifier.weight(1f).testTag("settingsList"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text("Reading controls", style = MaterialTheme.typography.headlineSmall) }
            item { Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Text("Volume keys turn pages"); Text("While reading: up goes back, down goes forward", style = MaterialTheme.typography.bodySmall) }
                Switch(controls.volumeTurns, { ReadingControls.update(context, volumeTurns = it) }, Modifier.testTag("volumeTurns"))
            } }
            item { Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("Keep screen on while reading", Modifier.weight(1f))
                Switch(controls.keepScreenOn, { ReadingControls.update(context, keepScreenOn = it) }, Modifier.testTag("keepScreenOn"))
            } }
            item { TextButton(onClick = { keyboardHelp = true }) { Text("Keyboard shortcuts") }; HorizontalDivider() }
            item { Text("Appearance", style = MaterialTheme.typography.headlineSmall) }
            items(Appearance.entries) { appearance ->
                Row(Modifier.fillMaxWidth().clickable { model.appearance(appearance) }, verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(selected = state.appearance == appearance, onClick = { model.appearance(appearance) })
                    Text(if (appearance == Appearance.System) "Use device theme" else "${appearance.name} theme")
                }
            }
            item { HorizontalDivider(Modifier.padding(vertical = 12.dp)); Text("Shelves", style = MaterialTheme.typography.headlineSmall) }
            items(state.shelves, key = { it.id.toString() }) { shelf ->
                var menu by rememberSaveable { mutableStateOf(false) }
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(shelf.name, style = MaterialTheme.typography.titleMedium)
                        Text("${state.books.count { it.fingerprint in shelf.books }} books", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Box {
                        IconButton(onClick = { menu = true }) { Icon(AppIcons.More, "Manage ${shelf.name}") }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            DropdownMenuItem(text = { Text("Rename shelf") }, onClick = { menu = false; editShelf(shelf) })
                            DropdownMenuItem(text = { Text("Delete shelf") }, onClick = { menu = false; deleteShelf(shelf) })
                        }
                    }
                }
            }
            item { OutlinedButton(onClick = createShelf) { Text("New shelf") } }
            item { HorizontalDivider(); TextButton(onClick = dictionaries) { Text("Offline dictionaries") } }
            item { TextButton(onClick = backups) { Text("Backup and export") } }
            item {
                HorizontalDivider(Modifier.padding(vertical = 12.dp))
                TextButton(onClick = licenses) { Text("Licenses") }
                TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, "https://github.com/Tikkaaa3/simPl-reader/blob/main/docs/privacy.md".toUri())) }) { Text("Privacy policy") }
                Text("simPl ${BuildConfig.VERSION_NAME}", style = MaterialTheme.typography.labelMedium)
                Text("Your books stay on this device", style = MaterialTheme.typography.titleMedium)
                Text("Imports are copied into private storage. Removing a book never deletes its original file.",
                    style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    if (keyboardHelp) KeyboardHelp { keyboardHelp = false }
}

/** M3's reader destination. Page rendering and reader controls belong to M4/M5. */
@Composable
private fun BookOverview(book: LibraryBook?, back: () -> Unit, settings: () -> Unit,
    membership: (LibraryBook) -> Unit, model: LibraryViewModel) {
    Column(Modifier.fillMaxSize().testTag("reader")) {
        TopAppBar(title = { Text("Book details") }, navigationIcon = { IconButton(onClick = back) { Icon(AppIcons.Back, "Back to library") } },
            actions = { IconButton(onClick = settings) { Icon(AppIcons.Settings, "Settings") } })
        if (book == null) Column(Modifier.padding(24.dp)) { Text("This book is no longer in your library."); TextButton(onClick = back) { Text("Return to library") } }
        else Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp)) {
            BookCover(book, Modifier.widthIn(max = 240.dp).fillMaxWidth().aspectRatio(2f / 3f))
            Text(book.title, style = MaterialTheme.typography.headlineMedium)
            book.author?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            Text("${formatLabel(book.format)} · ${android.text.format.Formatter.formatShortFileSize(LocalContext.current, book.byteLen.toLong())}",
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (book.total > 0u) Text(progressLabel(book))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { model.favourite(book) }) { Text(if (book.favourite) "Unfavorite" else "Favorite") }
                OutlinedButton(onClick = { membership(book) }) { Text("Add to shelves") }
            }
        }
    }
}

@Composable
private fun ShelfEditor(id: String, initial: String, dismiss: () -> Unit, save: (String) -> Unit) {
    var name by rememberSaveable(id) { mutableStateOf(initial) }
    AlertDialog(onDismissRequest = dismiss, title = { Text(if (id == "new") "New shelf" else "Rename shelf") },
        text = { OutlinedTextField(name, onValueChange = { name = it }, label = { Text("Shelf name") }, singleLine = true,
            supportingText = { Text("Up to 40 characters") }, isError = name.codePointCount(0, name.length) > 40, modifier = Modifier.testTag("shelfName")) },
        confirmButton = { TextButton(onClick = { save(name) }, enabled = name.isNotBlank() && name.codePointCount(0, name.length) <= 40) { Text("Save") } },
        dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } })
}

internal fun formatLabel(format: DocumentFormat): String = when (format) {
    DocumentFormat.EPUB -> "EPUB"; DocumentFormat.PDF -> "PDF"; DocumentFormat.HTML -> "HTML";
    DocumentFormat.TEXT -> "TXT"; DocumentFormat.MARKDOWN -> "Markdown"
}

private fun progressLabel(book: LibraryBook) = if (book.total == 0u) "Recently opened" else "Page ${book.current} of ${book.total} · ${(book.progress * 100).toInt()}%"
