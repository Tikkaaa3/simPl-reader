@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
package io.github.tikkaaa3.simpl

import android.app.Application
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.core.content.edit
import androidx.core.net.toUri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.work.WorkManager
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

internal data class BackupState(val busy: Boolean = false, val summary: BackupSummary? = null,
    val restore: String? = null, val message: String? = null, val restored: Boolean = false)

/** Rotation retains jobs; saved provider requests restart incomplete transfers. */
internal class BackupViewModel(application: Application, private val saved: SavedStateHandle) : AndroidViewModel(application) {
    private val mutable = MutableStateFlow(BackupState(restore = saved["restore"]))
    val state = mutable.asStateFlow()
    init { saved.get<String>("transferUri")?.let { transfer(it.toUri(), saved["transferKind"] ?: "inspect") } }
    fun dismiss() { mutable.value = mutable.value.copy(message = null) }
    fun abandon() {
        mutable.value.restore?.let { File(it).delete() }; saved["restore"] = null
        mutable.value = mutable.value.copy(restore = null, summary = null)
    }
    fun transfer(uri: Uri, kind: String, documents: Boolean = saved["documents"] ?: true, dictionaries: Boolean = saved["dictionaries"] ?: false) {
        if (mutable.value.busy) return
        saved["transferUri"] = uri.toString(); saved["transferKind"] = kind
        saved["documents"] = documents; saved["dictionaries"] = dictionaries
        val resolver = getApplication<Application>().contentResolver
        val flags = if (kind == "create") Intent.FLAG_GRANT_WRITE_URI_PERMISSION else Intent.FLAG_GRANT_READ_URI_PERMISSION
        runCatching { resolver.takePersistableUriPermission(uri, flags) }
        run {
            val context = getApplication<Application>()
            val directory = File(context.cacheDir, "backups").apply { check(mkdirs() || isDirectory) }
            val stage = File(directory, "${UUID.randomUUID()}.zip")
            try {
                if (kind == "create") {
                    val interfacePrefs = context.getSharedPreferences("interface", 0)
                    val reading = context.getSharedPreferences("reader", 0)
                    val options = OfflineDictionary.options.value
                    val dark = when (interfacePrefs.getString("appearance", "System")) {
                        "Dark" -> true; "Light" -> false
                        else -> context.resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES
                    }
                    savePortablePreferences(PortablePreferences(dark, reading.getString("theme", "default").orEmpty(), options.source, options.target, options.automatic))
                    val summary = createBackup(stage.absolutePath, documents, dictionaries)
                    context.contentResolver.openOutputStream(uri, "wt")?.use { output -> stage.inputStream().use { copyBounded(it, output) } }
                        ?: error("Cannot write this backup. Choose a destination again.")
                    mutable.value = mutable.value.copy(message = "Backup saved: ${summary.files} files")
                } else {
                    abandon()
                    context.contentResolver.openInputStream(uri)?.use { input -> stage.outputStream().use { copyBounded(input, it) } }
                        ?: error("Cannot read this backup. Choose it again.")
                    val summary = inspectBackup(stage.absolutePath)
                    saved["restore"] = stage.absolutePath
                    mutable.value = mutable.value.copy(summary = summary, restore = stage.absolutePath)
                }
            } finally {
                if (mutable.value.restore != stage.absolutePath) stage.delete()
                saved["transferUri"] = null; saved["transferKind"] = null
                runCatching { resolver.releasePersistableUriPermission(uri, flags) }
            }
        }
    }
    fun restore() = run {
        val context = getApplication<Application>()
        val jobs = WorkManager.getInstance(context).getWorkInfosByTag(DictionaryJobs.TAG).get()
        check(jobs.none { !it.state.isFinished }) { "Finish or cancel dictionary transfers before restoring." }
        ReadAloud.stop()
        val file = mutable.value.restore ?: error("Choose a backup first.")
        val previous = restoreBackup(file)
        val p = loadPortablePreferences()
        context.getSharedPreferences("interface", 0).edit(commit = true) { putString("appearance", if (p.dark) "Dark" else "Light") }
        context.getSharedPreferences("reader", 0).edit(commit = true) { putString("theme", p.theme) }
        OfflineDictionary.configure(DictionaryOptions(p.source, p.target, p.automatic))
        OfflineDictionary.resetCache()
        abandon()
        mutable.value = mutable.value.copy(restored = true, message = if (previous.isBlank()) "Backup restored" else "Backup restored. The previous profile is retained in private storage.")
    }
    private fun run(operation: suspend () -> Unit) {
        if (mutable.value.busy) return
        mutable.value = mutable.value.copy(busy = true, message = null)
        viewModelScope.launch {
            try { withContext(Dispatchers.IO) { operation() } }
            catch (error: CancellationException) { throw error }
            catch (error: Exception) { mutable.value = mutable.value.copy(message = error.message ?: "Transfer failed. Try again.") }
            finally { mutable.value = mutable.value.copy(busy = false) }
        }
    }
    private suspend fun copyBounded(input: java.io.InputStream, output: java.io.OutputStream) {
        val buffer = ByteArray(64 * 1024); var bytes = 0L
        while (true) {
            currentCoroutineContext().ensureActive()
            val count = input.read(buffer); if (count < 0) break
            bytes += count; check(bytes <= 16L * 1024 * 1024 * 1024 + 64 * 1024 * 1024) { "Backup exceeds its size limit." }
            output.write(buffer, 0, count)
        }
    }
}

@Composable
internal fun BackupScreen(library: LibraryState, locate: (LibraryBook, Uri) -> Unit, back: () -> Unit, restored: () -> Unit,
    model: BackupViewModel = viewModel()) {
    val state by model.state.collectAsStateWithLifecycle()
    var documents by rememberSaveable { mutableStateOf(true) }
    var dictionaries by rememberSaveable { mutableStateOf(false) }
    var missing by rememberSaveable { mutableStateOf<String?>(null) }
    val create = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/zip")) { it?.let { uri -> model.transfer(uri, "create", documents, dictionaries) } }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { it?.let { uri -> model.transfer(uri, "inspect") } }
    val find = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) library.books.firstOrNull { it.fingerprint == missing }?.let { locate(it, uri) }; missing = null
    }
    BackHandler(enabled = state.busy) { }
    LaunchedEffect(state.restored) { if (state.restored) restored() }
    Column(Modifier.fillMaxSize().testTag("backupScreen")) {
        TopAppBar(title = { Text("Backup and export") }, navigationIcon = { IconButton(onClick = back, enabled = !state.busy) { Icon(AppIcons.Back, "Back") } })
        if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth().testTag("backupProgress"))
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text("Portable backup", style = MaterialTheme.typography.headlineSmall)
                Text("Save reading positions, notes, shelves and preferences in a ZIP compatible with simPl on Windows.") }
            item { Row { Checkbox(documents, { documents = it }, enabled = !state.busy); Text("Include private books", Modifier.padding(top = 12.dp)) } }
            item { Row { Checkbox(dictionaries, { dictionaries = it }, enabled = !state.busy); Text("Include dictionaries", Modifier.padding(top = 12.dp)) } }
            item { Button(onClick = { create.launch("simPl-backup.zip") }, enabled = !state.busy) { Text("Create backup") }
                OutlinedButton(onClick = { open.launch(arrayOf("application/zip", "application/octet-stream")) }, enabled = !state.busy && library.importing == 0) { Text("Restore backup") } }
            item { Text("A restore replaces the profile after verification and retains the previous profile. Books omitted from a backup can be located below.") }
            item { HorizontalDivider(); Text("Missing books", style = MaterialTheme.typography.headlineSmall) }
            if (library.books.none { it.missing }) item { Text("All library books are available.") }
            items(library.books.filter { it.missing }, key = { it.fingerprint }) { book ->
                Row { Text(book.title, Modifier.weight(1f)); TextButton(onClick = { missing = book.fingerprint; find.launch(arrayOf("*/*")) }, enabled = !state.busy) { Text("Locate") } }
            }
            item { Text("Export notes as Markdown, Text or JSON from a book's Annotations panel.") }
            state.message?.let { message -> item { Text(message, Modifier.testTag("backupMessage")) } }
        }
    }
    state.restore?.let {
        AlertDialog(onDismissRequest = { if (!state.busy) model.abandon() }, title = { Text("Restore this backup?") },
            text = { Text("${state.summary?.files ?: "Verified"} files. This replaces library, positions, notes, shelves and preferences. Your current profile will be retained.") },
            confirmButton = { TextButton(onClick = model::restore, enabled = !state.busy) { Text("Restore") } },
            dismissButton = { TextButton(onClick = model::abandon, enabled = !state.busy) { Text("Cancel") } })
    }
}

@Composable
internal fun NotesExport(book: LibraryBook) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var format by rememberSaveable { mutableStateOf("Markdown") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun mime() = when (format) { "JSON" -> "application/json"; "Markdown" -> "text/markdown"; else -> "text/plain" }
    fun extension() = when (format) { "JSON" -> "json"; "Markdown" -> "md"; else -> "txt" }
    fun export(uri: Uri?) {
        if (busy) return
        busy = true
        val selected = when (format) { "JSON" -> NotesFormat.JSON; "Text" -> NotesFormat.TEXT; else -> NotesFormat.MARKDOWN }
        val selectedMime = mime(); val suffix = extension()
        scope.launch {
            try {
                val file = withContext(Dispatchers.IO) {
                    val folder = File(context.cacheDir, "exports").apply { check(mkdirs() || isDirectory) }
                    // Keep a small recent share history for recipients reading later.
                    folder.listFiles()?.filter { System.currentTimeMillis() - it.lastModified() > 24 * 60 * 60 * 1000L }?.forEach { it.delete() }
                    val file = File(folder, "notes-${UUID.randomUUID()}.$suffix")
                    exportNotes(file.absolutePath, book.fingerprint, book.title, selected)
                    if (uri != null) {
                        try { context.contentResolver.openOutputStream(uri, "wt")?.use { output -> file.inputStream().use { it.copyTo(output) } } ?: error("Cannot write notes. Choose another destination.") }
                        finally { file.delete() }
                    }
                    file
                }
                if (uri == null) {
                    val shared = FileProvider.getUriForFile(context, "${context.packageName}.exports", file)
                    context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                        type = selectedMime; putExtra(Intent.EXTRA_STREAM, shared)
                        clipData = android.content.ClipData.newUri(context.contentResolver, "Notes", shared)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }, "Share notes"))
                } else error = "Notes saved"
            } catch (failure: CancellationException) { throw failure }
            catch (failure: Exception) { error = failure.message ?: "Could not export notes." }
            finally { busy = false }
        }
    }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(mime())) { if (it != null) export(it) }
    FlowRow(Modifier.fillMaxWidth().padding(horizontal = 12.dp).testTag("notesExport")) {
        listOf("Markdown", "Text", "JSON").forEach { label -> FilterChip(format == label, { format = label }, label = { Text(label) }, enabled = !busy) }
        TextButton(onClick = { save.launch("notes.${extension()}") }, enabled = !busy) { Text("Export notes") }
        TextButton(onClick = { export(null) }, enabled = !busy) { Text("Share notes") }
        if (busy) CircularProgressIndicator(Modifier.size(24.dp))
    }
    error?.let { AlertDialog(onDismissRequest = { error = null }, text = { Text(it) }, confirmButton = { TextButton(onClick = { error = null }) { Text("OK") } }) }
}
