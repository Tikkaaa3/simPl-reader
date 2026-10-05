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
    Scaffold(snackbarHost = { SnackbarHost(snackbars, if (route?.startsWith("reader") == true)
        Modifier.navigationBarsPadding().imePadding().padding(bottom = 56.dp) else if (route == "library") Modifier.navigationBarsPadding() else Modifier) },
        contentWindowInsets = if (route?.startsWith("reader") == true) WindowInsets(0, 0, 0, 0)
            // The library draws behind the navigation bar and pads its own scrolling content.
            else if (route == "library") WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal)
            else WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom)) { padding ->
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

internal fun progressLabel(book: LibraryBook) = if (book.total == 0u) "Recently opened" else "Page ${book.current} of ${book.total} · ${(book.progress * 100).toInt()}%"
