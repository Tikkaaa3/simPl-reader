package io.github.tikkaaa3.simpl

import android.app.Application
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.work.WorkInfo
import androidx.work.WorkManager
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

internal class DictionaryViewModel(application: Application) : AndroidViewModel(application) {
    private val mutable = MutableStateFlow<List<DictionaryPackage>>(emptyList())
    val packages = mutable.asStateFlow()
    private val mutableJobs = MutableStateFlow<List<WorkInfo>>(emptyList())
    val jobs = mutableJobs.asStateFlow()
    var error by mutableStateOf<String?>(null)
        private set
    init {
        viewModelScope.launch {
            var completed: Set<Pair<java.util.UUID, WorkInfo.State>>? = null
            WorkManager.getInstance(application).getWorkInfosByTagFlow(DictionaryJobs.TAG).collect {
                mutableJobs.value = it
                val next = it.filter { job -> job.state.isFinished }.map { job -> job.id to job.state }.toSet()
                if (completed != next) { refresh(); completed = next }
            }
        }
    }
    private suspend fun refresh() { mutable.value = withContext(Dispatchers.IO) { OfflineDictionary.store.inventory() } }
    fun remove(id: UInt) = viewModelScope.launch {
        try { withContext(Dispatchers.IO) { OfflineDictionary.store.removePackage(id) }; refresh() }
        catch (error: CancellationException) { throw error }
        catch (failure: Exception) { error = failure.message ?: "Could not remove dictionary." }
    }
    fun import(uri: android.net.Uri) {
        try { DictionaryJobs.import(getApplication(), uri); error = null }
        catch (failure: Exception) { error = "Cannot access this ZIP. Choose it again from a storage provider." }
    }
}

@Composable
internal fun DictionaryOptionsControls() {
    val options by OfflineDictionary.options.collectAsStateWithLifecycle()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("Automatic lookup on selection", Modifier.weight(1f))
        Switch(options.automatic, { OfflineDictionary.configure(options.copy(automatic = it)) }, Modifier.testTag("dictionaryAutomatic"))
    }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        LanguagePicker("From", options.source, OfflineDictionary.languages.map { it.code }) { OfflineDictionary.configure(options.copy(source = it)) }
        LanguagePicker("To", options.target, OfflineDictionary.languages.first { it.code == options.source }.targets) { OfflineDictionary.configure(options.copy(target = it)) }
    }
}

@Composable
private fun LanguagePicker(label: String, code: String, choices: List<String>, change: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }, modifier = Modifier.testTag("dictionary$label")) {
            Text("$label: ${OfflineDictionary.languages.first { it.code == code }.label}")
        }
        DropdownMenu(expanded, { expanded = false }) {
            choices.forEach { value -> DropdownMenuItem(text = { Text(OfflineDictionary.languages.first { it.code == value }.label) },
                onClick = { expanded = false; change(value) }, modifier = Modifier.testTag("language:$value")) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun DictionaryScreen(back: () -> Unit, model: DictionaryViewModel = viewModel()) {
    val context = LocalContext.current
    val packages by model.packages.collectAsStateWithLifecycle()
    val jobs by model.jobs.collectAsStateWithLifecycle()
    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> uri?.let(model::import) }
    var notices by remember { mutableStateOf<UInt?>(null) }
    Column(Modifier.fillMaxSize().testTag("dictionaries")) {
        TopAppBar(title = { Text("Offline dictionaries") }, navigationIcon = { IconButton(onClick = back) { Icon(AppIcons.Back, "Back") } })
        LazyColumn(Modifier.weight(1f).testTag("dictionaryList"), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item {
                DictionaryOptionsControls()
                Text("Lookups stay on this device. Only dictionary packages are downloaded; book text is never sent.", Modifier.padding(vertical = 12.dp))
                OutlinedButton(onClick = { importer.launch(arrayOf("application/zip", "application/x-zip-compressed", "application/octet-stream")) }) { Text("Import dictionary ZIP") }
                model.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                (jobs.firstOrNull { "dictionary-import-job" in it.tags && !it.state.isFinished }
                    ?: jobs.filter { "dictionary-import-job" in it.tags }.maxByOrNull(::created))?.let { JobStatus(it) }
            }
            items(packages, key = { it.id.toString() }) { pack ->
                val job = jobs.filter { "dictionary-package-${pack.id}" in it.tags }.maxByOrNull(::created)
                // Active work always takes priority over older completed attempts.
                val current = jobs.firstOrNull { "dictionary-package-${pack.id}" in it.tags && !it.state.isFinished } ?: job
                Card(Modifier.fillMaxWidth().testTag("dictionaryPackage:${pack.id}")) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(pack.label, style = MaterialTheme.typography.titleMedium)
                        Text("${pack.provider} · ${pack.entries} entries · ${"%.2f".format(pack.bytes.toDouble() / 1_000_000)} MB")
                        Text(when (pack.state) { "installed" -> "Ready offline"; "invalid" -> "Damaged package — download or import again"; else -> "Not installed" })
                        current?.let { JobStatus(it) }
                        if (current?.state?.isFinished != false) Row {
                            TextButton(onClick = { DictionaryJobs.download(context, pack.id) }) { Text(if (current?.state == WorkInfo.State.FAILED || current?.state == WorkInfo.State.CANCELLED || pack.state == "invalid") "Retry download" else "Download") }
                            if (pack.state != "missing") TextButton(onClick = { model.remove(pack.id) }) { Text("Remove") }
                        }
                        if (pack.state == "installed") TextButton(onClick = { notices = pack.id }) { Text("Dictionary licenses") }
                    }
                }
            }
            item { Text("Data: WikDict / Wiktionary (CC BY-SA); Chinese also CC-CEDICT (CC BY-SA), Korean from Kaikki / Wiktionary. Provider attribution and license notices are included in each verified ZIP.", style = MaterialTheme.typography.bodySmall) }
        }
    }
    notices?.let { id ->
        val text by produceState<String?>(null, id) {
            value = withContext(Dispatchers.IO) { try { OfflineDictionary.store.packageNotices(id) } catch (error: Exception) { error.message ?: "Could not read dictionary notices." } }
        }
        ModalBottomSheet(onDismissRequest = { notices = null }) {
            Text(text ?: "Loading licenses…", Modifier.fillMaxWidth().heightIn(max = 520.dp).verticalScroll(rememberScrollState()).padding(20.dp).testTag("dictionaryNotices"))
        }
    }
}

private fun created(info: WorkInfo): Long = info.tags.firstOrNull { it.startsWith("dictionary-created-") }?.substringAfterLast('-')?.toLongOrNull() ?: 0L

@Composable
private fun JobStatus(info: WorkInfo) {
    val context = LocalContext.current
    if (!info.state.isFinished) {
        val total = info.progress.getLong("total", 0)
        val bytes = info.progress.getLong("bytes", 0)
        Text(info.progress.getString("message") ?: "Waiting for connection or download slot")
        if (total > 0) LinearProgressIndicator(progress = { (bytes.toFloat() / total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
        else LinearProgressIndicator(Modifier.fillMaxWidth())
        TextButton(onClick = { WorkManager.getInstance(context).cancelWorkById(info.id) }) { Text(if ("dictionary-import-job" in info.tags) "Cancel import" else "Cancel download") }
    } else if (info.state == WorkInfo.State.FAILED) Text(info.outputData.getString("message") ?: "Dictionary installation failed.", color = MaterialTheme.colorScheme.error)
    else if (info.state == WorkInfo.State.CANCELLED) Text("Cancelled. You can retry.")
    else if ("dictionary-import-job" in info.tags) Text(info.outputData.getString("message").orEmpty())
}

/** The request is scoped to this selection; replies to a dismissed selection are discarded. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun DictionarySelection(copy: ((String) -> Unit) -> Unit, identity: Any, expanded: Boolean, dismiss: () -> Unit) {
    val context = LocalContext.current
    val options by OfflineDictionary.options.collectAsStateWithLifecycle()
    val work by remember(context) { WorkManager.getInstance(context).getWorkInfosByTagFlow(DictionaryJobs.TAG) }.collectAsStateWithLifecycle(emptyList())
    val revision = work.filter { it.state.isFinished }.map { it.id to it.state }
    var result by remember(identity, options, revision) { mutableStateOf<DictionaryResult?>(null) }
    var query by remember(identity, options) { mutableStateOf("") }
    var error by remember(identity, options, revision) { mutableStateOf<String?>(null) }
    if (options.automatic || expanded) LaunchedEffect(identity, options, expanded, revision) {
        if (!expanded) delay(350)
        try {
            val reply = CompletableDeferred<String>(); copy { reply.complete(it) }
            val text = reply.await()
            val normalized = withContext(Dispatchers.IO) { dictionaryQuery(text, options.source) }
            if (normalized == null) {
                if (expanded) error = "Select a word or short phrase (up to 4 words)."
                return@LaunchedEffect
            }
            query = text.trim()
            result = withContext(Dispatchers.IO) { OfflineDictionary.store.lookup(text, options.source, options.target) }
        } catch (cancel: CancellationException) { throw cancel }
        catch (failure: Exception) { if (expanded) error = failure.message ?: "Cannot look up this selection." }
    }
    @Composable fun Content() {
        Column(Modifier.fillMaxWidth().padding(16.dp).testTag("dictionaryCard"), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (query.isNotEmpty()) Text(query, style = MaterialTheme.typography.titleMedium)
            result?.let { value ->
                value.headword?.let { Text(it + if (value.baseForm) " · Base form" else "", style = MaterialTheme.typography.titleLarge) }
                if (expanded) value.meanings.forEach { Text(it) } else value.meanings.firstOrNull()?.let { Text(it, maxLines = 2) }
                value.provider?.let { Text("$it · Offline", style = MaterialTheme.typography.labelSmall) }
                value.message?.let { Text(it, maxLines = if (expanded) Int.MAX_VALUE else 2) }
                value.missingPackage?.let { id ->
                    val job = work.filter { "dictionary-package-$id" in it.tags }.maxByOrNull(::created)
                    if (job?.state?.isFinished == false) JobStatus(job)
                    else TextButton(onClick = { DictionaryJobs.download(context, id) }) { Text(if (job?.state == WorkInfo.State.FAILED) "Retry download" else "Download dictionary") }
                }
            }
            error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            if (expanded && result == null && error == null) CircularProgressIndicator()
            if (expanded) { DictionaryOptionsControls(); TextButton(onClick = dismiss) { Text("Close dictionary") } }
        }
    }
    if (expanded) ModalBottomSheet(onDismissRequest = dismiss) {
        Column(Modifier.heightIn(max = 600.dp).verticalScroll(rememberScrollState())) { Content() }
    } else if (options.automatic && result != null) Content()
}
