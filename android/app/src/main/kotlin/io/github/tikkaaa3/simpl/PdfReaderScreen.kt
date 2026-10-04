package io.github.tikkaaa3.simpl

import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.compose.material3.*
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Both views retain their model; only explicit transitions move the source page. */
@Composable
internal fun PdfReaderScreen(book: LibraryBook, back: () -> Unit, settings: () -> Unit,
    pdf: PdfViewModel = viewModel(), reader: ReaderViewModel = viewModel()) {
    var mode by rememberSaveable(book.fingerprint) { mutableStateOf<Boolean?>(null) }
    var target by rememberSaveable(book.fingerprint) { mutableStateOf<Long?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val pdfState by pdf.state.collectAsStateWithLifecycle()
    val readerState by reader.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    LaunchedEffect(book.path) {
        pdf.open(book)
        if (mode == null) mode = withContext(Dispatchers.IO) { runCatching { pdfBookMode(book.path, book.fingerprint) }.getOrDefault(false) }
    }
    LaunchedEffect(mode, pdfState.loading) {
        if (mode == true && !pdfState.loading && readerState.loading && target == null) target = pdfState.location.page.toLong()
    }
    LaunchedEffect(mode, readerState.loading, readerState.adapting, target) {
        if (mode == true && !readerState.loading && !readerState.adapting) target?.let { page ->
            if (readerState.page.toLong() == page) target = null
            else reader.jump(page.toString()) { target = null }
        }
    }
    fun switch(bookView: Boolean) {
        ReadAloud.stop()
        if (bookView) { pdf.stop(); pdf.select(null); target = pdfState.location.page.toLong() }
        else {
            if (!readerState.loading && readerState.total > 0u) { reader.stop(); pdf.jump(readerState.page.toString()) }
            reader.select(null); target = null
        }
        scope.launch {
            try {
                withContext(Dispatchers.IO) { savePdfBookMode(book.path, book.fingerprint, bookView) }
                mode = bookView
            } catch (failure: kotlinx.coroutines.CancellationException) { throw failure }
            catch (failure: Exception) { error = failure.message ?: "Could not save the PDF view. Try again." }
        }
    }
    if (mode == true) ReaderScreen(book, reader, back, settings, document = { switch(false) })
    else PdfScreen(book, pdf, back, settings, bookMode = { switch(true) })
    error?.let { AlertDialog(onDismissRequest = { error = null }, text = { Text(it) }, confirmButton = { TextButton(onClick = { error = null }) { Text("OK") } }) }
}
